pub mod project_format;
pub mod provenance;

use chrono::Utc;
use rusqlite::Connection;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;
use walkdir::WalkDir;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

const SCHEMA_VERSION: &str = "1.0";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const CONVERSATION_PROMPT_DEFAULT: &str = include_str!("../prompts/conversation.json");
const DRAFTING_PROMPT_DEFAULT: &str = include_str!("../prompts/drafting.json");
const PROMPT_GUIDE: &str = include_str!("../../PROMPTS.md");

#[derive(Default)]
struct RuntimeState {
    active_project: Mutex<Option<PathBuf>>,
    read_only_project: Mutex<Option<ReadOnlyProject>>,
}

#[derive(Debug, Clone)]
struct ReadOnlyProject {
    path: PathBuf,
    classification: project_format::ProjectClassification,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandError {
    code: String,
    message: String,
    recoverable: bool,
}
type CommandResult<T> = Result<T, CommandError>;

impl From<provenance::CplError> for CommandError {
    fn from(error: provenance::CplError) -> Self {
        Self::new(&error.code, error.message, error.recoverable)
    }
}

impl CommandError {
    fn new(code: &str, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverable,
        }
    }
    fn io(context: &str, error: impl std::fmt::Display) -> Self {
        Self::new("IO_ERROR", format!("{context}: {error}"), true)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ProjectManifest {
    #[serde(rename = "project_format")]
    project_format: String,
    #[serde(rename = "project_format_version")]
    project_format_version: String,
    #[serde(rename = "provenance_conformance")]
    provenance_conformance: String,
    schema_version: String,
    application_version: String,
    project_id: String,
    title: String,
    description: String,
    created_at: String,
    updated_at: String,
    current_phase: String,
    publication_status: String,
    provenance_policy_id: String,
    audio_retained: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderProfile {
    kind: String,
    name: String,
    endpoint: String,
    model: String,
    mode: String,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConversationPromptConfig {
    schema_version: u32,
    system_prompt: String,
    user_prompt_template: String,
    challenge_guidance: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DraftingPromptConfig {
    schema_version: u32,
    system_prompt: String,
    draft_prompt_template: String,
    editorial_prompt_template: String,
    distillation_prompt_template: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PromptConfigInfo {
    directory: String,
    files: Vec<String>,
    reload_behavior: String,
}

#[derive(Debug, Serialize)]
struct ConnectionResult {
    ok: bool,
    message: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    path: String,
    manifest: Option<ProjectManifest>,
    classification: project_format::ProjectClassification,
    editable: bool,
    provenance_valid: bool,
    recovery_available: bool,
    message: String,
}

fn active_path(state: &State<RuntimeState>) -> CommandResult<PathBuf> {
    state
        .active_project
        .lock()
        .map_err(|_| {
            CommandError::new(
                "STATE_LOCKED",
                "Project state is temporarily unavailable.",
                true,
            )
        })?
        .clone()
        .ok_or_else(|| CommandError::new("NO_PROJECT", "Create or open a project first.", true))
}
fn atomic_write(path: &Path, bytes: &[u8]) -> CommandResult<()> {
    let parent = path.parent().ok_or_else(|| {
        CommandError::new(
            "INVALID_PATH",
            "The destination has no parent folder.",
            false,
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|e| CommandError::io("Could not create project folder", e))?;
    let temp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("write"),
        Uuid::new_v4()
    ));
    {
        let mut file = fs::File::create(&temp)
            .map_err(|e| CommandError::io("Could not create temporary file", e))?;
        file.write_all(bytes)
            .map_err(|e| CommandError::io("Could not write temporary file", e))?;
        file.sync_all()
            .map_err(|e| CommandError::io("Could not flush temporary file", e))?;
    }
    provenance::ledger::atomic_replace(&temp, path)?;
    provenance::ledger::sync_directory(parent)?;
    Ok(())
}
fn write_json(path: &Path, value: &impl Serialize) -> CommandResult<()> {
    let value = serde_json::to_value(value)
        .map_err(|e| CommandError::new("SERIALIZE_ERROR", e.to_string(), false))?;
    atomic_write(path, &provenance::canonical::canonicalize(&value)?)
}
fn prompt_config_dir(app: &AppHandle) -> CommandResult<PathBuf> {
    app.path()
        .app_config_dir()
        .map(|path| path.join("prompts"))
        .map_err(|error| {
            CommandError::io(
                "Could not locate the application configuration folder",
                error,
            )
        })
}

fn merge_missing_prompt_fields(current: &mut Value, defaults: &Value) -> bool {
    let (Some(current_object), Some(default_object)) =
        (current.as_object_mut(), defaults.as_object())
    else {
        return false;
    };
    let mut changed = false;
    for (key, default_value) in default_object {
        if let Some(current_value) = current_object.get_mut(key) {
            changed |= merge_missing_prompt_fields(current_value, default_value);
        } else {
            current_object.insert(key.clone(), default_value.clone());
            changed = true;
        }
    }
    changed
}
fn ensure_prompt_files_at(app: &AppHandle) -> CommandResult<PathBuf> {
    let directory = prompt_config_dir(app)?;
    fs::create_dir_all(&directory).map_err(|error| {
        CommandError::io("Could not create the prompt configuration folder", error)
    })?;
    for (name, contents) in [
        ("conversation.json", CONVERSATION_PROMPT_DEFAULT),
        ("drafting.json", DRAFTING_PROMPT_DEFAULT),
        ("README.md", PROMPT_GUIDE),
    ] {
        let path = directory.join(name);
        if !path.exists() {
            atomic_write(&path, contents.as_bytes())?;
        } else if name.ends_with(".json") {
            let existing = fs::read_to_string(&path).map_err(|error| {
                CommandError::io("Could not read prompt configuration for migration", error)
            })?;
            if let (Ok(mut current), Ok(defaults)) = (
                serde_json::from_str::<Value>(&existing),
                serde_json::from_str::<Value>(contents),
            ) {
                let mut changed = merge_missing_prompt_fields(&mut current, &defaults);
                if name == "conversation.json" {
                    let legacy_system = current
                        .get("systemPrompt")
                        .and_then(Value::as_str)
                        .filter(|prompt| !prompt.contains("{{persona_instruction}}"))
                        .map(str::to_owned);
                    if let Some(legacy_system) = legacy_system {
                        current["systemPrompt"] = Value::String(format!(
                            "{{{{persona_instruction}}}}\n\n{{{{genre_instruction}}}}\n\nProject lore and context:\n{{{{lore_context}}}}\n\n{{{{web_search_instruction}}}}\n\n{legacy_system}"
                        ));
                        changed = true;
                    }
                }
                if changed {
                    write_json(&path, &current)?;
                }
            }
        }
    }
    Ok(directory)
}

fn load_prompt_config<T: DeserializeOwned>(path: &Path) -> CommandResult<T> {
    let raw = fs::read_to_string(path).map_err(|error| {
        CommandError::io(
            &format!("Could not read prompt configuration {}", path.display()),
            error,
        )
    })?;
    serde_json::from_str(&raw).map_err(|error| {
        CommandError::new(
            "PROMPT_CONFIG_INVALID",
            format!("Invalid prompt configuration {}: {error}", path.display()),
            true,
        )
    })
}

fn required_prompt_variable<'a>(
    variables: &'a HashMap<String, String>,
    name: &str,
) -> CommandResult<&'a str> {
    variables
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CommandError::new(
                "PROMPT_VARIABLE_MISSING",
                format!("The prompt variable '{{{{{name}}}}}' is missing."),
                true,
            )
        })
}

fn render_prompt_template(
    template: &str,
    variables: &HashMap<String, String>,
) -> CommandResult<String> {
    let mut rendered = template.to_owned();
    for (name, value) in variables {
        rendered = rendered.replace(&format!("{{{{{name}}}}}"), value);
    }
    if let Some(start) = rendered.find("{{") {
        if let Some(end) = rendered[start + 2..].find("}}") {
            let name = &rendered[start + 2..start + 2 + end];
            return Err(CommandError::new(
                "PROMPT_VARIABLE_MISSING",
                format!("The prompt template references unknown or unavailable variable '{{{{{name}}}}}'."),
                true,
            ));
        }
    }
    Ok(rendered)
}

fn prompts_for_request(
    app: &AppHandle,
    purpose: &str,
    mut variables: HashMap<String, String>,
) -> CommandResult<(String, String)> {
    let directory = ensure_prompt_files_at(app)?;
    match purpose {
        "conversation" => {
            let path = directory.join("conversation.json");
            let config: ConversationPromptConfig = load_prompt_config(&path)?;
            if config.schema_version != 1
                || config.system_prompt.trim().is_empty()
                || config.user_prompt_template.trim().is_empty()
            {
                return Err(CommandError::new(
                    "PROMPT_CONFIG_INVALID",
                    format!(
                        "{} must use schemaVersion 1 and non-empty required prompt fields.",
                        path.display()
                    ),
                    true,
                ));
            }
            let challenge = required_prompt_variable(&variables, "challenge")?;
            let guidance = config.challenge_guidance.get(challenge).ok_or_else(|| {
                CommandError::new(
                    "PROMPT_CONFIG_INVALID",
                    format!(
                        "{} has no challengeGuidance entry for '{challenge}'.",
                        path.display()
                    ),
                    true,
                )
            })?;
            variables.insert("challenge_guidance".into(), guidance.clone());
            Ok((
                render_prompt_template(&config.system_prompt, &variables)?,
                render_prompt_template(&config.user_prompt_template, &variables)?,
            ))
        }
        "drafting" => {
            let path = directory.join("drafting.json");
            let config: DraftingPromptConfig = load_prompt_config(&path)?;
            if config.schema_version != 1
                || config.system_prompt.trim().is_empty()
                || config.draft_prompt_template.trim().is_empty()
                || config.editorial_prompt_template.trim().is_empty()
                || config.distillation_prompt_template.trim().is_empty()
            {
                return Err(CommandError::new(
                    "PROMPT_CONFIG_INVALID",
                    format!(
                        "{} must use schemaVersion 1 and non-empty required prompt fields.",
                        path.display()
                    ),
                    true,
                ));
            }
            let template = match required_prompt_variable(&variables, "action")? {
                "draft" => &config.draft_prompt_template,
                "distill" => &config.distillation_prompt_template,
                _ => &config.editorial_prompt_template,
            };
            Ok((
                config.system_prompt,
                render_prompt_template(template, &variables)?,
            ))
        }
        _ => Err(CommandError::new(
            "PROMPT_PURPOSE_INVALID",
            format!("No prompt configuration is registered for '{purpose}'."),
            false,
        )),
    }
}

#[tauri::command]
fn ensure_prompt_files(app: AppHandle) -> CommandResult<PromptConfigInfo> {
    let directory = ensure_prompt_files_at(&app)?;
    Ok(PromptConfigInfo {
        directory: directory.to_string_lossy().into_owned(),
        files: vec![
            "conversation.json".into(),
            "drafting.json".into(),
            "README.md".into(),
        ],
        reload_behavior: "Prompt files reload before every model request.".into(),
    })
}

#[tauri::command]
fn open_prompt_folder(app: AppHandle) -> CommandResult<String> {
    let directory = ensure_prompt_files_at(&app)?;
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    command.arg(&directory).spawn().map_err(|error| {
        CommandError::io("Could not open the prompt configuration folder", error)
    })?;
    Ok(directory.to_string_lossy().into_owned())
}

fn init_db(root: &Path) -> CommandResult<Connection> {
    let state_dir = root.join(".app");
    fs::create_dir_all(&state_dir)
        .map_err(|e| CommandError::io("Could not create live state folder", e))?;
    let db = Connection::open(state_dir.join("state.sqlite"))
        .map_err(|e| CommandError::new("DATABASE_ERROR", e.to_string(), true))?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS project_state (id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL, updated_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS staged_generations (id TEXT PRIMARY KEY, state TEXT NOT NULL, json TEXT NOT NULL, updated_at TEXT NOT NULL);").map_err(|e| CommandError::new("MIGRATION_ERROR", e.to_string(), false))?;
    Ok(db)
}
fn run_git(root: &Path, args: &[&str]) -> CommandResult<()> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| {
            CommandError::new(
                "HISTORY_UNAVAILABLE",
                format!("Version service could not start: {e}"),
                true,
            )
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(CommandError::new(
            "HISTORY_UNAVAILABLE",
            String::from_utf8_lossy(&output.stderr),
            true,
        ))
    }
}
fn init_history(root: &Path) -> CommandResult<()> {
    if !root.join(".git").exists() {
        run_git(root, &["init", "--quiet"])?;
        run_git(root, &["config", "user.name", "Thinkloom"])?;
        run_git(root, &["config", "user.email", "history@thinkloom.local"])?;
    }
    Ok(())
}

fn append_event(
    root: &Path,
    project_id: &str,
    event_type: &str,
    actor: &str,
    summary: &str,
    metadata: Value,
) -> CommandResult<provenance::WriteResult> {
    let client_action_id = provenance::identifiers::sortable_id("action")?;
    Ok(provenance::CplService::new(root, project_id).write(provenance::WriteCommand {
        client_action_id,
        project_id: project_id.to_owned(),
        event_type: event_type.to_owned(),
        actor: actor.to_owned(),
        metadata: json!({"summary": summary, "details": metadata}),
        records: vec![provenance::RecordInput {
            record_type: "native-action".to_owned(),
            payload: json!({"event_type": event_type, "actor": actor, "summary": summary, "details": metadata}),
        }],
        operational_state: None,
    })?)
}
fn project_layout(root: &Path) -> CommandResult<()> {
    project_format::initialize_conforming_layout(root)?;
    for folder in ["style", "exports"] {
        fs::create_dir_all(root.join(folder))
            .map_err(|error| CommandError::io("Could not create project structure", error))?;
    }
    atomic_write(
        &root.join(".gitignore"),
        b".app/\nreports/\nexports/*.pdf\nexports/*.zip\n*.tmp\n*.wav\n*.mp3\n",
    )?;
    Ok(())
}

fn read_manifest(root: &Path) -> CommandResult<ProjectManifest> {
    serde_json::from_slice(
        &fs::read(root.join("project.json"))
            .map_err(|error| CommandError::io("Could not read project", error))?,
    )
    .map_err(|error| CommandError::new("PROJECT_INVALID", error.to_string(), false))
}

fn set_read_only_project(
    state: &State<RuntimeState>,
    root: &Path,
    classification: project_format::ProjectClassification,
) -> CommandResult<()> {
    *state
        .active_project
        .lock()
        .map_err(|_| CommandError::new("STATE_LOCKED", "Project state is unavailable.", true))? =
        None;
    *state
        .read_only_project
        .lock()
        .map_err(|_| CommandError::new("STATE_LOCKED", "Project state is unavailable.", true))? =
        Some(ReadOnlyProject {
            path: root.to_path_buf(),
            classification,
        });
    Ok(())
}

fn set_editable_project(state: &State<RuntimeState>, root: &Path) -> CommandResult<()> {
    *state
        .read_only_project
        .lock()
        .map_err(|_| CommandError::new("STATE_LOCKED", "Project state is unavailable.", true))? =
        None;
    *state
        .active_project
        .lock()
        .map_err(|_| CommandError::new("STATE_LOCKED", "Project state is unavailable.", true))? =
        Some(root.to_path_buf());
    Ok(())
}

fn read_only_summary(root: &Path, inspection: project_format::ProjectInspection) -> ProjectSummary {
    ProjectSummary {
        path: root.to_string_lossy().into_owned(),
        manifest: None,
        classification: inspection.classification,
        editable: false,
        provenance_valid: false,
        recovery_available: false,
        message: inspection.message,
    }
}

fn snapshot(root: &Path) -> CommandResult<PathBuf> {
    let db = root.join(".app/state.sqlite");
    if !db.exists() {
        return Err(CommandError::new(
            "NO_SNAPSHOT",
            "There is no live database to snapshot yet.",
            true,
        ));
    }
    let snapshots = root.join(".app/snapshots");
    fs::create_dir_all(&snapshots)
        .map_err(|error| CommandError::io("Could not create snapshot folder", error))?;
    let destination = snapshots.join(format!(
        "state-{}.sqlite",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    fs::copy(&db, &destination)
        .map_err(|error| CommandError::io("Could not create database snapshot", error))?;
    let mut existing: Vec<_> = fs::read_dir(&snapshots)
        .map_err(|error| CommandError::io("Could not inspect snapshots", error))?
        .filter_map(Result::ok)
        .collect();
    existing.sort_by_key(|entry| entry.file_name());
    if existing.len() > 7 {
        for entry in &existing[..existing.len() - 7] {
            let _ = fs::remove_file(entry.path());
        }
    }
    Ok(destination)
}

#[tauri::command]
fn choose_project_folder() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("Choose a Thinkloom project folder")
        .pick_folder()
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
fn create_project(
    path: String,
    title: String,
    state: State<RuntimeState>,
) -> CommandResult<ProjectSummary> {
    let base = PathBuf::from(path);
    let title = title.trim().to_owned();
    if title.is_empty() {
        return Err(CommandError::new(
            "PROJECT_TITLE_REQUIRED",
            "Enter a project title before creating its CPL boundary.",
            true,
        ));
    }
    let safe_name: String = title
        .chars()
        .map(|character| {
            if character.is_alphanumeric()
                || character == '-'
                || character == '_'
                || character == ' '
            {
                character
            } else {
                '-'
            }
        })
        .collect();
    let root = if base.join("project.json").exists() {
        base
    } else {
        base.join(safe_name.trim().replace(' ', "-"))
    };
    if root.join("project.json").exists() {
        return Err(CommandError::new(
            "PROJECT_EXISTS",
            "A Thinkloom project already exists in that folder.",
            true,
        ));
    }
    project_layout(&root)?;
    let timestamp = provenance::identifiers::timestamp_millis();
    let manifest = ProjectManifest {
        project_format: project_format::PROJECT_FORMAT.into(),
        project_format_version: project_format::PROJECT_FORMAT_VERSION.into(),
        provenance_conformance: project_format::PROVENANCE_CONFORMANCE.into(),
        schema_version: SCHEMA_VERSION.into(),
        application_version: APP_VERSION.into(),
        project_id: provenance::identifiers::sortable_id("project")?,
        description: String::new(),
        title: title.clone(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        current_phase: "ideation".into(),
        publication_status: "working".into(),
        provenance_policy_id: provenance::identifiers::sortable_id("policy")?,
        audio_retained: false,
    };
    write_json(&root.join("project.json"), &manifest)?;
    atomic_write(
        &root.join("manuscript/manuscript.md"),
        format!("# {title}\n").as_bytes(),
    )?;
    write_json(&root.join("ideas/ideas.json"), &json!([]))?;
    write_json(&root.join("ideas/archived.json"), &json!([]))?;
    write_json(&root.join("conversations/sessions.json"), &json!([]))?;
    write_json(
        &root.join("style/profile.json"),
        &json!({"schemaVersion":SCHEMA_VERSION,"traits":[],"disallowedHabits":[],"confidence":"limited"}),
    )?;
    write_json(&root.join("style/sample-references.json"), &json!([]))?;
    init_db(&root)?;
    let history_ok = init_history(&root).is_ok();
    provenance::CplService::new(&root, &manifest.project_id).write(provenance::WriteCommand {
        client_action_id: provenance::identifiers::sortable_id("action")?,
        project_id: manifest.project_id.clone(),
        event_type: "PROJECT_CREATED".to_owned(),
        actor: "user".to_owned(),
        metadata: json!({
            "summary": format!("Created {title}"),
            "historyAvailable": history_ok,
            "projectFormat": project_format::PROJECT_FORMAT,
            "projectFormatVersion": project_format::PROJECT_FORMAT_VERSION,
            "provenanceConformance": project_format::PROVENANCE_CONFORMANCE
        }),
        records: vec![
            provenance::RecordInput {
                record_type: "project-manifest".to_owned(),
                payload: serde_json::to_value(&manifest).map_err(|error| {
                    CommandError::new("SERIALIZE_ERROR", error.to_string(), false)
                })?,
            },
            provenance::RecordInput {
                record_type: "provenance-policy".to_owned(),
                payload: json!({
                    "schema_version": SCHEMA_VERSION,
                    "policy_id": manifest.provenance_policy_id,
                    "project_id": manifest.project_id,
                    "retention_mode": "minimal",
                    "encryption_mode": "none",
                    "default_export_profile": "sanitized",
                    "effective_at": manifest.created_at
                }),
            },
        ],
        operational_state: None,
    })?;
    if history_ok {
        let _ = run_git(
            &root,
            &[
                "add",
                "project.json",
                ".gitignore",
                "manuscript",
                "ideas",
                "conversations",
                "records",
                "provenance",
                "style",
            ],
        );
        let _ = run_git(&root, &["commit", "--quiet", "-m", "Project created"]);
    }
    set_editable_project(&state, &root)?;
    Ok(ProjectSummary {
        path: root.to_string_lossy().into_owned(),
        manifest: Some(manifest),
        classification: project_format::ProjectClassification::CplConforming,
        editable: true,
        provenance_valid: true,
        recovery_available: false,
        message: "Created a CPL 1.0-conforming project boundary.".to_owned(),
    })
}

#[tauri::command]
fn open_project(path: String, state: State<RuntimeState>) -> CommandResult<ProjectSummary> {
    let root = PathBuf::from(path);
    let inspection = project_format::inspect_project(&root)?;
    if inspection.classification != project_format::ProjectClassification::CplConforming {
        set_read_only_project(&state, &root, inspection.classification)?;
        return Ok(read_only_summary(&root, inspection));
    }

    set_read_only_project(
        &state,
        &root,
        project_format::ProjectClassification::CplBlocked,
    )?;
    let manifest = match read_manifest(&root) {
        Ok(manifest) => manifest,
        Err(error) => {
            return Ok(read_only_summary(
                &root,
                project_format::ProjectInspection {
                    classification: project_format::ProjectClassification::UnsupportedReadOnly,
                    message: format!(
                        "The exact marker is present, but the project manifest is incompatible: {}. No recovery was attempted.",
                        error.message
                    ),
                },
            ));
        }
    };
    if manifest.schema_version != SCHEMA_VERSION
        || manifest.project_format != project_format::PROJECT_FORMAT
        || manifest.project_format_version != project_format::PROJECT_FORMAT_VERSION
        || manifest.provenance_conformance != project_format::PROVENANCE_CONFORMANCE
    {
        let inspection = project_format::ProjectInspection {
            classification: project_format::ProjectClassification::CplBlocked,
            message: format!(
                "The project marker is recognized, but schema {} is incompatible with supported schema {SCHEMA_VERSION}. No recovery was attempted.",
                manifest.schema_version
            ),
        };
        set_read_only_project(&state, &root, inspection.classification)?;
        return Ok(read_only_summary(&root, inspection));
    }

    let recovery_report = match provenance::CplService::new(&root, &manifest.project_id).recover() {
        Ok(report) => report,
        Err(error) => {
            return Ok(ProjectSummary {
                path: root.to_string_lossy().into_owned(),
                manifest: Some(manifest),
                classification: project_format::ProjectClassification::CplBlocked,
                editable: false,
                provenance_valid: false,
                recovery_available: true,
                message: format!(
                    "CPL recovery was unable to establish a safe editable state: {}",
                    error.message
                ),
            });
        }
    };
    let valid = matches!(
        recovery_report.verification.status,
        provenance::VerificationStatus::Verified
            | provenance::VerificationStatus::VerifiedWithWarnings
    );
    let recovery_available = !matches!(
        recovery_report.classification,
        provenance::RecoveryClassification::Clean
    ) || root.join(".app/snapshots").exists();
    if !valid {
        let inspection = project_format::ProjectInspection {
            classification: project_format::ProjectClassification::CplBlocked,
            message: format!(
                "CPL recovery completed with verification status {:?}; the project remains read-only.",
                recovery_report.verification.status
            ),
        };
        set_read_only_project(&state, &root, inspection.classification)?;
        return Ok(ProjectSummary {
            path: root.to_string_lossy().into_owned(),
            manifest: Some(manifest),
            classification: inspection.classification,
            editable: false,
            provenance_valid: false,
            recovery_available,
            message: inspection.message,
        });
    }

    set_editable_project(&state, &root)?;
    Ok(ProjectSummary {
        path: root.to_string_lossy().into_owned(),
        manifest: Some(manifest),
        classification: project_format::ProjectClassification::CplConforming,
        editable: true,
        provenance_valid: true,
        recovery_available,
        message: "CPL recovery and native verification passed; the project is editable.".to_owned(),
    })
}

fn selected_project_path(state: &State<RuntimeState>) -> CommandResult<PathBuf> {
    if let Some(selection) = state
        .read_only_project
        .lock()
        .map_err(|_| CommandError::new("STATE_LOCKED", "Project state is unavailable.", true))?
        .as_ref()
    {
        return Ok(selection.path.clone());
    }
    active_path(state)
}

#[tauri::command]
fn show_project_folder(state: State<RuntimeState>) -> CommandResult<String> {
    let directory = selected_project_path(&state)?;
    #[cfg(target_os = "windows")]
    let mut command = Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = Command::new("xdg-open");
    command
        .arg(&directory)
        .spawn()
        .map_err(|error| CommandError::io("Could not show the project folder", error))?;
    Ok(directory.to_string_lossy().into_owned())
}

#[tauri::command]
fn create_legacy_preservation_archive(state: State<RuntimeState>) -> CommandResult<Option<String>> {
    let selection = state
        .read_only_project
        .lock()
        .map_err(|_| CommandError::new("STATE_LOCKED", "Project state is unavailable.", true))?
        .clone()
        .ok_or_else(|| {
            CommandError::new(
                "NO_LEGACY_PROJECT",
                "Select an unmarked legacy preview project first.",
                true,
            )
        })?;
    if selection.classification != project_format::ProjectClassification::LegacyPreviewReadOnly {
        return Err(CommandError::new(
            "LEGACY_ARCHIVE_NOT_PERMITTED",
            "Only unmarked legacy preview projects can receive a preservation archive.",
            false,
        ));
    }
    let default_name = selection
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("legacy-preview-project");
    let Some(destination) = rfd::FileDialog::new()
        .set_title("Save legacy preview-project preservation archive")
        .set_file_name(format!("{default_name}-preservation.zip"))
        .add_filter("ZIP archive", &["zip"])
        .save_file()
    else {
        return Ok(None);
    };
    project_format::create_legacy_preservation_archive(&selection.path, &destination)?;
    Ok(Some(destination.to_string_lossy().into_owned()))
}
fn refresh_phase1_files(
    root: &Path,
    projection: &provenance::phase1::Phase1Projection,
) -> CommandResult<()> {
    write_json(&root.join("ideas/ideas.json"), &projection.ideas)?;
    let mut sessions = projection.sessions.clone();
    sessions.retain(|session| session.id != projection.active_session_id);
    if !projection.active_session_id.is_empty() {
        sessions.push(projection.current_session_snapshot());
    }
    write_json(&root.join("conversations/sessions.json"), &sessions)?;
    Ok(())
}

#[tauri::command]
fn apply_phase1_command(
    command: provenance::phase1::Phase1Command,
    state: State<RuntimeState>,
) -> CommandResult<provenance::phase1::Phase1CommandResult> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    let result = provenance::phase1::apply_command(&root, &manifest.project_id, command)?;
    refresh_phase1_files(&root, &result.projection)?;
    Ok(result)
}

#[tauri::command]
fn load_phase1_projection(
    state: State<RuntimeState>,
) -> CommandResult<provenance::phase1::Phase1Projection> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    Ok(provenance::phase1::reconstruct(
        &root,
        &manifest.project_id,
    )?)
}

#[tauri::command]
fn apply_composition_command(
    command: provenance::composition::CompositionCommand,
    state: State<RuntimeState>,
) -> CommandResult<provenance::composition::CompositionCommandResult> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    let result = provenance::composition::apply_command(&root, &manifest.project_id, command)?;
    atomic_write(
        &root.join("manuscript/manuscript.md"),
        result.projection.manuscript.as_bytes(),
    )?;
    Ok(result)
}

#[tauri::command]
fn load_composition_projection(
    state: State<RuntimeState>,
) -> CommandResult<provenance::composition::CompositionProjection> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    Ok(provenance::composition::reconstruct(
        &root,
        &manifest.project_id,
    )?)
}

#[tauri::command]
fn ensure_composition_projection(
    state: State<RuntimeState>,
) -> CommandResult<provenance::composition::CompositionProjection> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    let projection = provenance::composition::reconstruct(&root, &manifest.project_id)?;
    if projection.initialized {
        return Ok(projection);
    }
    let manuscript =
        fs::read_to_string(root.join("manuscript/manuscript.md")).map_err(|error| {
            CommandError::io(
                "Could not read the existing manuscript for unattested initialization",
                error,
            )
        })?;
    let digest = provenance::canonical::sha256_digest(manuscript.as_bytes());
    let result = provenance::composition::apply_command(
        &root,
        &manifest.project_id,
        provenance::composition::CompositionCommand {
            client_action_id: format!(
                "composition_initialize_{}",
                digest.trim_start_matches("sha256:")
            ),
            actor: "system".to_owned(),
            summary: "Initialized existing manuscript as unattested expression".to_owned(),
            occurred_at: provenance::identifiers::timestamp_millis(),
            action: provenance::composition::CompositionAction::Initialize {
                text: manuscript,
                origin: provenance::composition::RecordedOrigin::Unattested,
            },
        },
    )?;
    Ok(result.projection)
}

#[tauri::command]
fn refresh_non_phase1_files(app_state: Value, state: State<RuntimeState>) -> CommandResult<()> {
    let root = active_path(&state)?;
    write_json(
        &root.join("style/profile.json"),
        &json!({"schemaVersion":SCHEMA_VERSION,"traits":app_state.get("styleTraits"),"disallowedHabits":app_state.get("disallowedHabits")}),
    )?;
    Ok(())
}
#[tauri::command]
fn freeze_contribution_map(
    request: Option<provenance::contribution_map::ContributionMapRequest>,
    state: State<RuntimeState>,
) -> CommandResult<provenance::contribution_map::ContributionMapProjection> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    Ok(provenance::contribution_map::freeze_current(
        &root,
        &manifest.project_id,
        request.unwrap_or_default(),
    )?)
}

#[tauri::command]
fn load_contribution_map(
    state: State<RuntimeState>,
) -> CommandResult<Option<provenance::contribution_map::ContributionMapProjection>> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    Ok(provenance::contribution_map::load_latest(
        &root,
        &manifest.project_id,
    )?)
}
#[tauri::command]
fn load_cpl_explorer(
    state: State<RuntimeState>,
) -> CommandResult<provenance::explorer::CplExplorerProjection> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    Ok(provenance::explorer::load(&root, &manifest.project_id)?)
}

#[tauri::command]
fn prepare_harp(state: State<RuntimeState>) -> CommandResult<provenance::harp::HarpPreparation> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    Ok(provenance::harp::prepare_current(
        &root,
        &manifest.project_id,
    )?)
}
#[tauri::command]
fn export_harp_artifacts(
    request: provenance::export::HarpExportRequest,
    state: State<RuntimeState>,
) -> CommandResult<provenance::export::HarpExportResult> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    Ok(provenance::export::create_harp_exports(
        &root,
        &manifest.project_id,
        request,
    )?)
}

#[tauri::command]
fn verify_harp_sanitized_archive(
    archive_path: String,
    state: State<RuntimeState>,
) -> CommandResult<provenance::export::SanitizedArchiveVerification> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    let harp = provenance::harp::load_latest(&root, &manifest.project_id)?.ok_or_else(|| {
        CommandError::new(
            "HARP_EXPORT_REQUIRED",
            "Generate HARP before verifying its sanitized archive.",
            true,
        )
    })?;
    let chain = harp.harp["cpl_binding"]["chain_head"]
        .as_str()
        .ok_or_else(|| {
            CommandError::new(
                "HARP_BINDING_INVALID",
                "HARP has no CPL chain binding.",
                false,
            )
        })?;
    let harp_sha256 = harp.harp["harp_sha256"]
        .as_str()
        .ok_or_else(|| CommandError::new("HARP_BINDING_INVALID", "HARP has no digest.", false))?;
    let deposit_sha256 = harp.harp["deposit"]["deposit_sha256"]
        .as_str()
        .ok_or_else(|| {
            CommandError::new("HARP_BINDING_INVALID", "HARP has no deposit digest.", false)
        })?;
    let requested = PathBuf::from(archive_path);
    let target = if requested.is_absolute() {
        requested
    } else {
        root.join(requested)
    };
    let canonical_root = fs::canonicalize(&root)
        .map_err(|error| CommandError::io("Could not resolve the project folder", error))?;
    let canonical_target = fs::canonicalize(&target)
        .map_err(|error| CommandError::io("Could not resolve the sanitized archive", error))?;
    if !canonical_target.starts_with(canonical_root.join("exports").join("harp")) {
        return Err(CommandError::new(
            "HARP_ARCHIVE_PATH_REFUSED",
            "Only sanitized HARP archives inside this project's export folder can be verified here.",
            false,
        ));
    }
    Ok(provenance::export::verify_sanitized_archive(
        &canonical_target,
        Some((chain, harp_sha256, deposit_sha256)),
    )?)
}
#[tauri::command]
fn generate_harp(
    request: provenance::harp::HarpGenerationRequest,
    state: State<RuntimeState>,
) -> CommandResult<provenance::harp::HarpProjection> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    Ok(provenance::harp::generate_current(
        &root,
        &manifest.project_id,
        request,
    )?)
}

#[tauri::command]
fn load_harp(
    state: State<RuntimeState>,
) -> CommandResult<Option<provenance::harp::HarpProjection>> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    Ok(provenance::harp::load_latest(&root, &manifest.project_id)?)
}
#[tauri::command]
fn verify_provenance(state: State<RuntimeState>) -> CommandResult<provenance::VerificationReport> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    Ok(provenance::verify_project(&root, &manifest.project_id)?)
}

#[tauri::command]
fn recover_provenance(state: State<RuntimeState>) -> CommandResult<provenance::RecoveryReport> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    Ok(provenance::recover_project(&root, &manifest.project_id)?)
}

#[tauri::command]
fn create_checkpoint(name: String, state: State<RuntimeState>) -> CommandResult<()> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    append_event(
        &root,
        &manifest.project_id,
        "CHECKPOINT_CREATED",
        "user",
        &format!("Saved version {name}"),
        json!({"name":name}),
    )?;
    run_git(
        &root,
        &[
            "add",
            "project.json",
            "manuscript",
            "ideas",
            "conversations",
            "deposits",
            "provenance",
            "style",
        ],
    )?;
    run_git(
        &root,
        &[
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            &format!("Version: {name}"),
        ],
    )
}

#[tauri::command]
fn store_provider_secret(profile_id: String, secret: String) -> CommandResult<()> {
    if secret.trim().is_empty() {
        return Err(CommandError::new(
            "SECRET_EMPTY",
            "Enter a credential before saving.",
            true,
        ));
    }
    keyring::Entry::new("com.app.desktop", &profile_id)
        .map_err(|e| CommandError::new("CREDENTIAL_ERROR", e.to_string(), true))?
        .set_password(&secret)
        .map_err(|e| CommandError::new("CREDENTIAL_ERROR", e.to_string(), true))
}
#[tauri::command]
fn delete_provider_secret(profile_id: String) -> CommandResult<()> {
    keyring::Entry::new("com.app.desktop", &profile_id)
        .map_err(|e| CommandError::new("CREDENTIAL_ERROR", e.to_string(), true))?
        .delete_credential()
        .map_err(|e| CommandError::new("CREDENTIAL_ERROR", e.to_string(), true))
}

#[tauri::command]
fn test_provider(
    profile: ProviderProfile,
    client_action_id: String,
    invocation_id: String,
    session_id: String,
    state: State<RuntimeState>,
) -> CommandResult<ConnectionResult> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    let requested_at = provenance::identifiers::timestamp_millis();
    let provider = provenance::phase1::Phase1Provider {
        kind: profile.kind.clone(),
        name: profile.name.clone(),
        endpoint: profile.endpoint.clone(),
        model: profile.model.clone(),
        mode: profile.mode.clone(),
        connected: false,
    };
    provenance::phase1::apply_command(
        &root,
        &manifest.project_id,
        provenance::phase1::Phase1Command {
            client_action_id: format!("{client_action_id}:request"),
            actor: "user".to_owned(),
            summary: "Requested provider connectivity test".to_owned(),
            occurred_at: requested_at.clone(),
            operation: provenance::phase1::Phase1Operation::ProviderInvocationRequested {
                invocation: provenance::phase1::ProviderInvocation {
                    invocation_id: invocation_id.clone(),
                    purpose: "provider_test".to_owned(),
                    session_id,
                    provider,
                    prompt_template_sha256: provenance::phase1::canonical_text_digest(
                        "provider-connectivity-test",
                    )?,
                    input_sha256: provenance::phase1::canonical_text_digest(&profile.model)?,
                    context_sha256: provenance::phase1::canonical_text_digest(&profile.endpoint)?,
                    requested_at,
                },
            },
        },
    )?;

    // The connectivity request runs after the durable request event and without a writer lock.
    let outcome = (|| -> CommandResult<ConnectionResult> {
        if profile.mode == "cloud" && profile.kind != "openai" {
            return Ok(ConnectionResult {
                ok: false,
                message: "Cloud processing must be approved in the project before testing.".into(),
            });
        }
        let endpoint = profile.endpoint.trim_end_matches('/');
        let url = if profile.kind == "ollama" {
            format!("{endpoint}/api/tags")
        } else {
            format!("{endpoint}/models")
        };
        let mut request = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .map_err(|e| CommandError::new("PROVIDER_ERROR", e.to_string(), true))?
            .get(url);
        if profile.kind == "openai" {
            let secret = keyring::Entry::new("com.app.desktop", "openai")
                .ok()
                .and_then(|entry| entry.get_password().ok())
                .ok_or_else(|| {
                    CommandError::new(
                        "CREDENTIAL_MISSING",
                        "Save the OpenAI credential in the system vault first.",
                        true,
                    )
                })?;
            request = request.bearer_auth(secret);
        }
        let response = match request.send() {
            Ok(response) => response,
            Err(error) => {
                return Ok(ConnectionResult {
                    ok: false,
                    message: format!("Could not reach {}: {}", profile.name, error),
                });
            }
        };
        if !response.status().is_success() {
            return Ok(ConnectionResult {
                ok: false,
                message: format!(
                    "{} responded with status {}.",
                    profile.name,
                    response.status()
                ),
            });
        }
        if profile.kind == "ollama" {
            let value: Value = response
                .json()
                .map_err(|error| CommandError::new("PROVIDER_RESPONSE", error.to_string(), true))?;
            let requested = profile.model.trim();
            let installed = value
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|model| model.get("name").and_then(Value::as_str))
                .collect::<Vec<_>>();
            let available = installed.iter().any(|name| {
                *name == requested
                    || name.strip_suffix(":latest") == Some(requested)
                    || requested.strip_suffix(":latest") == Some(*name)
            });
            if !available {
                return Ok(ConnectionResult {
                    ok: false,
                    message: format!(
                        "Ollama is running, but model '{}' is not installed. Available: {}.",
                        requested,
                        installed
                            .iter()
                            .take(5)
                            .copied()
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
            }
        }
        Ok(ConnectionResult {
            ok: true,
            message: format!("{} is ready with {}.", profile.name, profile.model),
        })
    })();
    match outcome {
        Ok(result) if result.ok => {
            let created_at = provenance::identifiers::timestamp_millis();
            provenance::phase1::apply_command(
                &root,
                &manifest.project_id,
                provenance::phase1::Phase1Command {
                    client_action_id: format!("{client_action_id}:response"),
                    actor: "system".to_owned(),
                    summary: "Provider connectivity test succeeded".to_owned(),
                    occurred_at: created_at.clone(),
                    operation: provenance::phase1::Phase1Operation::ProviderInvocationResponded {
                        invocation_id,
                        retained_text: result.message.clone(),
                        content_sha256: provenance::phase1::canonical_text_digest(&result.message)?,
                        created_at,
                    },
                },
            )?;
            Ok(result)
        }
        Ok(result) => {
            let occurred_at = provenance::identifiers::timestamp_millis();
            provenance::phase1::apply_command(
                &root,
                &manifest.project_id,
                provenance::phase1::Phase1Command {
                    client_action_id: format!("{client_action_id}:failure"),
                    actor: "system".to_owned(),
                    summary: "Provider connectivity test failed".to_owned(),
                    occurred_at: occurred_at.clone(),
                    operation: provenance::phase1::Phase1Operation::ProviderInvocationFailed {
                        invocation_id,
                        code: "PROVIDER_TEST_FAILED".to_owned(),
                        bounded_summary: result.message.chars().take(512).collect(),
                        recoverable: true,
                        occurred_at,
                    },
                },
            )?;
            Ok(result)
        }
        Err(error) => {
            let occurred_at = provenance::identifiers::timestamp_millis();
            provenance::phase1::apply_command(
                &root,
                &manifest.project_id,
                provenance::phase1::Phase1Command {
                    client_action_id: format!("{client_action_id}:failure"),
                    actor: "system".to_owned(),
                    summary: "Provider connectivity test failed".to_owned(),
                    occurred_at: occurred_at.clone(),
                    operation: provenance::phase1::Phase1Operation::ProviderInvocationFailed {
                        invocation_id,
                        code: error.code.clone(),
                        bounded_summary: error.message.chars().take(512).collect(),
                        recoverable: error.recoverable,
                        occurred_at,
                    },
                },
            )?;
            Err(error)
        }
    }
}
fn pdf_bytes(title: &str, manuscript: &str) -> Vec<u8> {
    let mut lines = vec![title.to_string()];
    lines.extend(
        manuscript
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.trim_start_matches('#').trim().to_string()),
    );
    lines.truncate(48);
    let escape = |text: &str| {
        text.replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)")
    };
    let mut stream = String::from("BT /F1 18 Tf 72 742 Td ");
    for (index, line) in lines.iter().enumerate() {
        if index == 1 {
            stream.push_str("/F1 11 Tf ");
        }
        let safe: String = line.chars().filter(|c| c.is_ascii()).take(92).collect();
        stream.push_str(&format!("({}) Tj 0 -15 Td ", escape(&safe)));
    }
    stream.push_str("ET");
    let objects = ["<< /Type /Catalog /Pages 2 0 R >>".to_string(), "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(), "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(), format!("<< /Length {} >>\nstream\n{}\nendstream", stream.len(), stream), "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string()];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}
fn file_hash(path: &Path) -> CommandResult<String> {
    Ok(provenance::canonical::sha256_digest(
        &fs::read(path).map_err(|e| CommandError::io("Could not hash export", e))?,
    ))
}

#[tauri::command]
fn export_project(
    format: String,
    sanitized: bool,
    state: State<RuntimeState>,
) -> CommandResult<String> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    let markdown = fs::read_to_string(root.join("manuscript/manuscript.md"))
        .map_err(|e| CommandError::io("Could not read manuscript", e))?;
    let exports = root.join("exports");
    fs::create_dir_all(&exports)
        .map_err(|e| CommandError::io("Could not create export folder", e))?;
    let slug = manifest
        .title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let destination = match format.as_str() {
        "markdown" => {
            let p = exports.join(format!("{slug}.md"));
            atomic_write(&p, markdown.as_bytes())?;
            p
        }
        "text" => {
            let text = markdown
                .lines()
                .map(|line| line.trim_start_matches('#').trim())
                .collect::<Vec<_>>()
                .join("\n");
            let p = exports.join(format!("{slug}.txt"));
            atomic_write(&p, text.as_bytes())?;
            p
        }
        "html" => {
            let body = markdown
                .lines()
                .map(|line| {
                    if let Some(value) = line.strip_prefix("# ") {
                        format!("<h1>{}</h1>", escape_html(value))
                    } else if let Some(value) = line.strip_prefix("## ") {
                        format!("<h2>{}</h2>", escape_html(value))
                    } else if line.trim().is_empty() {
                        String::new()
                    } else {
                        format!("<p>{}</p>", escape_html(line))
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            let p = exports.join(format!("{slug}.html"));
            atomic_write(&p,format!("<!doctype html><meta charset=\"utf-8\"><title>{}</title><style>body{{max-width:720px;margin:64px auto;padding:24px;font:18px/1.7 Georgia,serif}}h1,h2{{line-height:1.2}}</style><main>{body}</main>",escape_html(&manifest.title)).as_bytes())?;
            p
        }
        "pdf" => {
            let p = exports.join(format!("{slug}.pdf"));
            atomic_write(&p, &pdf_bytes(&manifest.title, &markdown))?;
            p
        }
        "evidence" => {
            let result = provenance::export::create_harp_exports(
                &root,
                &manifest.project_id,
                provenance::export::HarpExportRequest {
                    redact_personal_identifiers: sanitized,
                },
            )?;
            let role = if sanitized {
                "sanitized_supporting_archive"
            } else {
                "full_private_archive"
            };
            let artifact = result
                .artifacts
                .iter()
                .find(|artifact| artifact.role == role)
                .ok_or_else(|| {
                    CommandError::new(
                        "HARP_EXPORT_MISSING",
                        "The requested HARP archive was not created.",
                        false,
                    )
                })?;
            root.join(artifact.path.replace('/', std::path::MAIN_SEPARATOR_STR))
        }
        _ => {
            return Err(CommandError::new(
                "FORMAT_UNSUPPORTED",
                "Choose Markdown, HTML, PDF, text, or evidence.",
                true,
            ))
        }
    };
    append_event(
        &root,
        &manifest.project_id,
        "EXPORT_CREATED",
        "user",
        &format!("Created {format} export"),
        json!({"format":format,"sanitized":sanitized,"sha256":file_hash(&destination)?}),
    )?;
    Ok(destination.to_string_lossy().into_owned())
}
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
fn add_tree_to_zip(zip: &mut ZipWriter<fs::File>, root: &Path) -> CommandResult<Vec<Value>> {
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path == root || entry.file_type().is_dir() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|e| CommandError::new("BACKUP_ERROR", e.to_string(), false))?;
        let name = relative.to_string_lossy().replace('\\', "/");
        if entry.file_type().is_symlink()
            || name.ends_with(".tmp")
            || name.ends_with("-wal")
            || name.ends_with("-shm")
            || (name.starts_with("exports/") && name.ends_with(".zip"))
        {
            continue;
        }
        let bytes =
            fs::read(path).map_err(|e| CommandError::io("Could not read backup file", e))?;
        zip.start_file(&name, options)
            .map_err(|e| CommandError::new("BACKUP_ERROR", e.to_string(), true))?;
        zip.write_all(&bytes)
            .map_err(|e| CommandError::io("Could not write backup file", e))?;
        files.push(
            json!({"path":name,"sha256":provenance::canonical::sha256_digest(&bytes),"size":bytes.len()}),
        );
    }
    Ok(files)
}

#[tauri::command]
fn create_backup(state: State<RuntimeState>) -> CommandResult<String> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    let _ = snapshot(&root);
    let destination = root
        .join("exports")
        .join(format!("{}-backup.zip", manifest.project_id));
    let temp = destination.with_extension("zip.tmp");
    fs::create_dir_all(destination.parent().unwrap())
        .map_err(|e| CommandError::io("Could not create export folder", e))?;
    let file =
        fs::File::create(&temp).map_err(|e| CommandError::io("Could not create backup", e))?;
    let mut zip = ZipWriter::new(file);
    let files = add_tree_to_zip(&mut zip, &root)?;
    let backup_manifest = json!({"schemaVersion":SCHEMA_VERSION,"applicationVersion":APP_VERSION,"projectId":manifest.project_id,"createdAt":provenance::identifiers::timestamp_millis(),"files":files});
    zip.start_file(
        "backup-manifest.json",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
    )
    .map_err(|e| CommandError::new("BACKUP_ERROR", e.to_string(), true))?;
    zip.write_all(&provenance::canonical::canonicalize(&backup_manifest)?)
        .map_err(|e| CommandError::io("Could not write backup manifest", e))?;
    zip.finish()
        .map_err(|e| CommandError::new("BACKUP_ERROR", e.to_string(), true))?;
    provenance::ledger::atomic_replace(&temp, &destination)?;
    provenance::ledger::sync_directory(destination.parent().unwrap())?;
    append_event(
        &root,
        &manifest.project_id,
        "BACKUP_CREATED",
        "user",
        "Created restorable project backup",
        json!({"sha256":file_hash(&destination)?}),
    )?;
    Ok(destination.to_string_lossy().into_owned())
}

#[tauri::command]
fn import_backup(archive_path: String, destination: String) -> CommandResult<String> {
    let file =
        fs::File::open(&archive_path).map_err(|e| CommandError::io("Could not open backup", e))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| CommandError::new("BACKUP_INVALID", e.to_string(), false))?;
    let mut archived_manifest = Vec::new();
    archive
        .by_name("project.json")
        .map_err(|_| {
            CommandError::new(
                "BACKUP_INVALID",
                "The backup has no root project.json.",
                false,
            )
        })?
        .read_to_end(&mut archived_manifest)
        .map_err(|error| CommandError::io("Could not inspect the backup project marker", error))?;
    if !project_format::manifest_has_supported_marker(&archived_manifest) {
        return Err(CommandError::new(
            "LEGACY_BACKUP_REFUSED",
            "Legacy preview backups cannot be imported or converted before Thinkloom 1.0.0. Preserve the original bytes instead.",
            false,
        ));
    }
    if archive.len() > 10_000 {
        return Err(CommandError::new(
            "BACKUP_LIMIT",
            "The backup contains too many files.",
            false,
        ));
    }
    let target = PathBuf::from(destination);
    fs::create_dir_all(&target)
        .map_err(|e| CommandError::io("Could not create import folder", e))?;
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut item = archive
            .by_index(index)
            .map_err(|e| CommandError::new("BACKUP_INVALID", e.to_string(), false))?;
        let enclosed = item
            .enclosed_name()
            .ok_or_else(|| {
                CommandError::new(
                    "BACKUP_UNSAFE",
                    format!("Unsafe archive path: {}", item.name()),
                    false,
                )
            })?
            .to_path_buf();
        if item
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(CommandError::new(
                "BACKUP_UNSAFE",
                "Symbolic links are not allowed in project backups.",
                false,
            ));
        }
        total = total.saturating_add(item.size());
        if total > 1_073_741_824 {
            return Err(CommandError::new(
                "BACKUP_LIMIT",
                "The expanded backup exceeds 1 GB.",
                false,
            ));
        }
        let output = target.join(enclosed);
        if item.is_dir() {
            fs::create_dir_all(&output)
                .map_err(|e| CommandError::io("Could not create imported folder", e))?;
        } else {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| CommandError::io("Could not create imported folder", e))?;
            }
            let mut out = fs::File::create(&output)
                .map_err(|e| CommandError::io("Could not create imported file", e))?;
            std::io::copy(&mut item, &mut out)
                .map_err(|e| CommandError::io("Could not extract imported file", e))?;
        }
    }
    if !target.join("project.json").exists() || !target.join("backup-manifest.json").exists() {
        return Err(CommandError::new(
            "BACKUP_INVALID",
            "The archive is missing its project or backup manifest.",
            false,
        ));
    }
    let _ = fs::remove_file(target.join("backup-manifest.json"));
    let _ = read_manifest(&target)?;
    Ok(target.to_string_lossy().into_owned())
}

#[tauri::command]
fn rebuild_repository(state: State<RuntimeState>) -> CommandResult<()> {
    let root = active_path(&state)?;
    if !root.join("project.json").is_file() {
        return Err(CommandError::new(
            "PROJECT_INVALID",
            "Repository repair requires a valid Thinkloom project.",
            false,
        ));
    }
    let history = root.join(".git");
    if history.exists() {
        fs::remove_dir_all(&history)
            .map_err(|e| CommandError::io("Could not replace damaged history", e))?;
    }
    init_history(&root)?;
    run_git(
        &root,
        &[
            "add",
            "project.json",
            ".gitignore",
            "manuscript",
            "ideas",
            "conversations",
            "deposits",
            "provenance",
            "style",
        ],
    )?;
    run_git(
        &root,
        &["commit", "--quiet", "-m", "Rebuilt project history"],
    )?;
    let manifest = read_manifest(&root)?;
    append_event(
        &root,
        &manifest.project_id,
        "PROVENANCE_REBUILT",
        "system",
        "Rebuilt project history from canonical files",
        json!({}),
    )?;
    Ok(())
}

#[tauri::command]
fn finalize_release(state: State<RuntimeState>) -> CommandResult<String> {
    let root = active_path(&state)?;
    let mut manifest = read_manifest(&root)?;
    if fs::read_to_string(root.join("manuscript/manuscript.md"))
        .map(|text| text.trim().is_empty())
        .unwrap_or(true)
    {
        return Err(CommandError::new(
            "MANUSCRIPT_EMPTY",
            "Add manuscript text before finalizing.",
            true,
        ));
    }
    let verification = provenance::verify_project(&root, &manifest.project_id)?;
    provenance::verifier::require_release_verification(&verification)?;
    manifest.publication_status = "finalized".into();
    manifest.updated_at = provenance::identifiers::timestamp_millis();
    write_json(&root.join("project.json"), &manifest)?;
    let release = format!("release-{}", Utc::now().format("%Y%m%d-%H%M%S"));
    append_event(
        &root,
        &manifest.project_id,
        "RELEASE_FINALIZED",
        "user",
        &format!("Finalized {release}"),
        json!({"releaseId":release}),
    )?;
    provenance::contribution_map::freeze_current(
        &root,
        &manifest.project_id,
        provenance::contribution_map::ContributionMapRequest::default(),
    )?;
    run_git(
        &root,
        &[
            "add",
            "project.json",
            "manuscript",
            "ideas",
            "conversations",
            "deposits",
            "provenance",
            "style",
        ],
    )?;
    run_git(
        &root,
        &[
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            &format!("Release: {release}"),
        ],
    )?;
    run_git(
        &root,
        &["tag", "-a", &release, "-m", &format!("Thinkloom {release}")],
    )?;
    Ok(release)
}

#[tauri::command]
fn diagnostics(state: State<RuntimeState>) -> Value {
    let project = state.active_project.lock().ok().and_then(|guard| {
        guard
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
    });
    json!({"applicationVersion":APP_VERSION,"schemaVersion":SCHEMA_VERSION,"platform":std::env::consts::OS,"projectOpen":project.is_some(),"projectPath":project,"audioRetention":false,"promptLogging":false,"responseLogging":false})
}

#[tauri::command]
fn generate_text(
    app: AppHandle,
    profile: ProviderProfile,
    prompt_variables: HashMap<String, String>,
    cloud_approved: bool,
    purpose: String,
    client_action_id: String,
    invocation_id: String,
    session_id: String,
    state: State<RuntimeState>,
) -> CommandResult<String> {
    if profile.mode == "cloud" && !cloud_approved {
        return Err(CommandError::new(
            "CLOUD_APPROVAL_REQUIRED",
            "Approve cloud processing for this project before sending context.",
            true,
        ));
    }
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    let context_value = serde_json::to_value(&prompt_variables)
        .map_err(|error| CommandError::new("SERIALIZE_ERROR", error.to_string(), false))?;
    let context_sha256 = provenance::canonical::canonical_digest(&context_value)?;
    let prompt_purpose = if purpose == "distillation" {
        "drafting"
    } else {
        &purpose
    };
    let (system, prompt) = prompts_for_request(&app, prompt_purpose, prompt_variables)?;
    let provider = provenance::phase1::Phase1Provider {
        kind: profile.kind.clone(),
        name: profile.name.clone(),
        endpoint: profile.endpoint.clone(),
        model: profile.model.clone(),
        mode: profile.mode.clone(),
        connected: true,
    };
    let requested_at = provenance::identifiers::timestamp_millis();
    let request = provenance::phase1::Phase1Command {
        client_action_id: format!("{client_action_id}:request"),
        actor: "user".to_owned(),
        summary: format!("Requested {purpose} provider invocation"),
        occurred_at: requested_at.clone(),
        operation: provenance::phase1::Phase1Operation::ProviderInvocationRequested {
            invocation: provenance::phase1::ProviderInvocation {
                invocation_id: invocation_id.clone(),
                purpose: purpose.clone(),
                session_id,
                provider,
                prompt_template_sha256: provenance::phase1::canonical_text_digest(&system)?,
                input_sha256: provenance::phase1::canonical_text_digest(&prompt)?,
                context_sha256,
                requested_at,
            },
        },
    };
    let requested = provenance::phase1::apply_command(&root, &manifest.project_id, request)?;
    if let Some(completed) = requested.projection.invocations.get(&invocation_id) {
        if completed.status == "responded" {
            if let Some(text) = &completed.retained_text {
                return Ok(text.clone());
            }
        }
    }

    // No CPL writer lock is held while provider I/O runs.
    let provider_outcome = (|| -> CommandResult<String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .map_err(|e| CommandError::new("PROVIDER_ERROR", e.to_string(), true))?;
        let (url, body) = if profile.kind == "ollama" {
            (
                format!("{}/api/chat", profile.endpoint.trim_end_matches('/')),
                json!({"model":profile.model,"stream":false,"messages":[{"role":"system","content":system},{"role":"user","content":prompt}]}),
            )
        } else {
            (
                format!(
                    "{}/chat/completions",
                    profile.endpoint.trim_end_matches('/')
                ),
                json!({"model":profile.model,"stream":false,"messages":[{"role":"system","content":system},{"role":"user","content":prompt}]}),
            )
        };
        let mut request = client.post(url).json(&body);
        if profile.kind != "ollama" {
            if let Ok(entry) = keyring::Entry::new("com.app.desktop", &profile.kind) {
                if let Ok(secret) = entry.get_password() {
                    request = request.bearer_auth(secret);
                }
            }
        }
        let response = request.send().map_err(|e| {
            CommandError::new(
                "PROVIDER_UNAVAILABLE",
                format!("The request is preserved and can be retried: {e}"),
                true,
            )
        })?;
        if !response.status().is_success() {
            return Err(CommandError::new(
                "PROVIDER_RESPONSE",
                format!(
                    "The provider returned {}. The request is preserved for retry.",
                    response.status()
                ),
                true,
            ));
        }
        let value: Value = response
            .json()
            .map_err(|e| CommandError::new("STRUCTURED_OUTPUT_INVALID", e.to_string(), true))?;
        value
            .pointer("/message/content")
            .or_else(|| value.pointer("/choices/0/message/content"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                CommandError::new(
                    "STRUCTURED_OUTPUT_INVALID",
                    "The provider returned no usable passage. Retry or switch providers.",
                    true,
                )
            })
    })();

    match provider_outcome {
        Ok(text) => {
            let created_at = provenance::identifiers::timestamp_millis();
            provenance::phase1::apply_command(
                &root,
                &manifest.project_id,
                provenance::phase1::Phase1Command {
                    client_action_id: format!("{client_action_id}:response"),
                    actor: "assistant".to_owned(),
                    summary: format!("Retained {purpose} provider response"),
                    occurred_at: created_at.clone(),
                    operation: provenance::phase1::Phase1Operation::ProviderInvocationResponded {
                        invocation_id,
                        retained_text: text.clone(),
                        content_sha256: provenance::phase1::canonical_text_digest(&text)?,
                        created_at,
                    },
                },
            )?;
            Ok(text)
        }
        Err(error) => {
            let occurred_at = provenance::identifiers::timestamp_millis();
            let bounded_summary = error.message.chars().take(512).collect::<String>();
            provenance::phase1::apply_command(
                &root,
                &manifest.project_id,
                provenance::phase1::Phase1Command {
                    client_action_id: format!("{client_action_id}:failure"),
                    actor: "system".to_owned(),
                    summary: format!("Recorded failed {purpose} provider invocation"),
                    occurred_at: occurred_at.clone(),
                    operation: provenance::phase1::Phase1Operation::ProviderInvocationFailed {
                        invocation_id,
                        code: error.code.clone(),
                        bounded_summary,
                        recoverable: error.recoverable,
                        occurred_at,
                    },
                },
            )?;
            Err(error)
        }
    }
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RuntimeState::default())
        .invoke_handler(tauri::generate_handler![
            choose_project_folder,
            create_project,
            open_project,
            show_project_folder,
            create_legacy_preservation_archive,
            load_phase1_projection,
            apply_phase1_command,
            refresh_non_phase1_files,
            apply_composition_command,
            load_composition_projection,
            ensure_composition_projection,
            freeze_contribution_map,
            load_contribution_map,
            load_cpl_explorer,
            prepare_harp,
            generate_harp,
            export_harp_artifacts,
            verify_harp_sanitized_archive,
            load_harp,
            verify_provenance,
            recover_provenance,
            create_checkpoint,
            store_provider_secret,
            delete_provider_secret,
            test_provider,
            ensure_prompt_files,
            open_prompt_folder,
            generate_text,
            export_project,
            create_backup,
            import_backup,
            rebuild_repository,
            finalize_release,
            diagnostics
        ])
        .run(tauri::generate_context!())
        .expect("Thinkloom desktop runtime failed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_prompt_configs_parse_and_templates_validate() {
        let conversation: ConversationPromptConfig =
            serde_json::from_str(CONVERSATION_PROMPT_DEFAULT).unwrap();
        let drafting: DraftingPromptConfig = serde_json::from_str(DRAFTING_PROMPT_DEFAULT).unwrap();
        assert_eq!(conversation.schema_version, 1);
        assert_eq!(drafting.schema_version, 1);

        let mut variables = HashMap::new();
        variables.insert("context".into(), "A current thought".into());
        variables.insert("challenge_guidance".into(), "Ask one question.".into());
        let rendered =
            render_prompt_template(&conversation.user_prompt_template, &variables).unwrap();
        assert!(rendered.contains("A current thought"));

        variables.remove("context");
        let error = render_prompt_template(&conversation.user_prompt_template, &variables)
            .expect_err("an unresolved placeholder must fail");
        assert_eq!(error.code, "PROMPT_VARIABLE_MISSING");
    }
    #[test]
    fn pdf_is_structurally_complete() {
        let bytes = pdf_bytes("Title", "A short manuscript.");
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF"));
    }
    #[test]
    fn html_is_escaped() {
        assert_eq!(escape_html("<script>&"), "&lt;script&gt;&amp;");
    }
    #[test]
    fn provenance_detects_tampering() {
        let temp = tempfile::tempdir().unwrap();
        project_layout(temp.path()).unwrap();
        let manifest = ProjectManifest {
            project_format: project_format::PROJECT_FORMAT.into(),
            project_format_version: project_format::PROJECT_FORMAT_VERSION.into(),
            provenance_conformance: project_format::PROVENANCE_CONFORMANCE.into(),
            schema_version: SCHEMA_VERSION.into(),
            application_version: APP_VERSION.into(),
            project_id: "test".into(),
            description: String::new(),
            title: "Test".into(),
            created_at: provenance::identifiers::timestamp_millis(),
            updated_at: provenance::identifiers::timestamp_millis(),
            current_phase: "ideation".into(),
            publication_status: "working".into(),
            provenance_policy_id: "policy_test".into(),
            audio_retained: false,
        };
        write_json(&temp.path().join("project.json"), &manifest).unwrap();
        append_event(
            temp.path(),
            "test",
            "PROJECT_CREATED",
            "user",
            "Created",
            json!({}),
        )
        .unwrap();
        assert!(matches!(
            provenance::verify_project(temp.path(), "test")
                .unwrap()
                .status,
            provenance::VerificationStatus::Verified
        ));
        let journal = temp
            .path()
            .join("provenance/ledger/active/segment-000001.jsonl");
        let changed = fs::read_to_string(&journal)
            .unwrap()
            .replace("Created", "Changed");
        fs::write(&journal, changed).unwrap();
        assert_eq!(
            provenance::verify_project(temp.path(), "test")
                .unwrap()
                .status,
            provenance::VerificationStatus::Failed
        );
    }
    #[test]
    fn project_layout_never_creates_audio_files() {
        let temp = tempfile::tempdir().unwrap();
        project_layout(temp.path()).unwrap();
        assert!(!WalkDir::new(temp.path())
            .into_iter()
            .filter_map(Result::ok)
            .any(|entry| matches!(
                entry.path().extension().and_then(|value| value.to_str()),
                Some("wav" | "mp3" | "m4a")
            )));
    }
}
