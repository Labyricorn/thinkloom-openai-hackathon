use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    io::Write,
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CommandError {
    code: String,
    message: String,
    recoverable: bool,
}
type CommandResult<T> = Result<T, CommandError>;

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
#[serde(rename_all = "camelCase")]
struct ProjectManifest {
    schema_version: String,
    application_version: String,
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
    current_phase: String,
    publication_status: String,
    audio_retained: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProvenanceEvent {
    schema_version: String,
    event_id: String,
    project_id: String,
    timestamp: String,
    event_type: String,
    actor: Value,
    provider: Option<Value>,
    inputs: Vec<Value>,
    outputs: Vec<Value>,
    metadata: Value,
    previous_event_hash: Option<String>,
    event_hash: String,
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
    manifest: ProjectManifest,
    provenance_valid: bool,
    recovery_available: bool,
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
    if path.exists() {
        fs::remove_file(path)
            .map_err(|e| CommandError::io("Could not replace canonical file", e))?;
    }
    fs::rename(&temp, path).map_err(|e| CommandError::io("Could not finalize canonical file", e))
}
fn write_json(path: &Path, value: &impl Serialize) -> CommandResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| CommandError::new("SERIALIZE_ERROR", e.to_string(), false))?;
    atomic_write(path, &bytes)
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
                config.system_prompt,
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
            let template = if required_prompt_variable(&variables, "action")? == "draft" {
                &config.draft_prompt_template
            } else {
                &config.editorial_prompt_template
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
    let state_dir = root.join(".thinkloom");
    fs::create_dir_all(&state_dir)
        .map_err(|e| CommandError::io("Could not create live state folder", e))?;
    let db = Connection::open(state_dir.join("state.sqlite"))
        .map_err(|e| CommandError::new("DATABASE_ERROR", e.to_string(), true))?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS project_state (id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL, updated_at TEXT NOT NULL); CREATE TABLE IF NOT EXISTS provenance_events (event_id TEXT PRIMARY KEY, event_type TEXT NOT NULL, timestamp TEXT NOT NULL, event_hash TEXT NOT NULL, summary TEXT); CREATE TABLE IF NOT EXISTS staged_generations (id TEXT PRIMARY KEY, state TEXT NOT NULL, json TEXT NOT NULL, updated_at TEXT NOT NULL);").map_err(|e| CommandError::new("MIGRATION_ERROR", e.to_string(), false))?;
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
fn head_hash(root: &Path) -> CommandResult<Option<String>> {
    let path = root.join("provenance/chain-head.json");
    if !path.exists() {
        return Ok(None);
    }
    let value: Value = serde_json::from_slice(
        &fs::read(path).map_err(|e| CommandError::io("Could not read history head", e))?,
    )
    .map_err(|e| CommandError::new("PROVENANCE_INVALID", e.to_string(), true))?;
    Ok(value
        .get("eventHash")
        .and_then(Value::as_str)
        .map(str::to_owned))
}
fn append_event(
    root: &Path,
    project_id: &str,
    event_type: &str,
    actor: &str,
    summary: &str,
    metadata: Value,
) -> CommandResult<ProvenanceEvent> {
    let previous = head_hash(root)?;
    let mut event = ProvenanceEvent {
        schema_version: SCHEMA_VERSION.into(),
        event_id: Uuid::new_v4().to_string(),
        project_id: project_id.into(),
        timestamp: Utc::now().to_rfc3339(),
        event_type: event_type.into(),
        actor: json!({"type": actor}),
        provider: None,
        inputs: vec![],
        outputs: vec![],
        metadata: json!({"summary": summary, "details": metadata}),
        previous_event_hash: previous,
        event_hash: String::new(),
    };
    let canonical = serde_json::to_vec(&json!({"schemaVersion":event.schema_version,"eventId":event.event_id,"projectId":event.project_id,"timestamp":event.timestamp,"eventType":event.event_type,"actor":event.actor,"provider":event.provider,"inputs":event.inputs,"outputs":event.outputs,"metadata":event.metadata,"previousEventHash":event.previous_event_hash})).map_err(|e| CommandError::new("SERIALIZE_ERROR", e.to_string(), false))?;
    event.event_hash = hex::encode(Sha256::digest(canonical));
    let journal = root.join("provenance/events.jsonl");
    fs::create_dir_all(journal.parent().unwrap())
        .map_err(|e| CommandError::io("Could not create history folder", e))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&journal)
        .map_err(|e| CommandError::io("Could not open history journal", e))?;
    serde_json::to_writer(&mut file, &event)
        .map_err(|e| CommandError::new("SERIALIZE_ERROR", e.to_string(), false))?;
    file.write_all(b"\n")
        .map_err(|e| CommandError::io("Could not append history event", e))?;
    file.sync_all()
        .map_err(|e| CommandError::io("Could not flush history event", e))?;
    write_json(
        &root.join("provenance/chain-head.json"),
        &json!({"eventId":event.event_id,"eventHash":event.event_hash,"updatedAt":event.timestamp}),
    )?;
    let db = init_db(root)?;
    db.execute("INSERT OR REPLACE INTO provenance_events(event_id,event_type,timestamp,event_hash,summary) VALUES(?1,?2,?3,?4,?5)", params![event.event_id,event.event_type,event.timestamp,event.event_hash,summary]).map_err(|e| CommandError::new("DATABASE_ERROR", e.to_string(), true))?;
    Ok(event)
}

fn verify_chain_at(root: &Path) -> CommandResult<bool> {
    let journal = root.join("provenance/events.jsonl");
    if !journal.exists() {
        return Ok(false);
    }
    let contents =
        fs::read_to_string(&journal).map_err(|e| CommandError::io("Could not read history", e))?;
    let mut previous: Option<String> = None;
    let mut last = None;
    for (index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let event: ProvenanceEvent = serde_json::from_str(line).map_err(|e| {
            CommandError::new(
                "PROVENANCE_INVALID",
                format!("Event {} is malformed: {e}", index + 1),
                true,
            )
        })?;
        if event.previous_event_hash != previous {
            return Ok(false);
        }
        let canonical = serde_json::to_vec(&json!({"schemaVersion":event.schema_version,"eventId":event.event_id,"projectId":event.project_id,"timestamp":event.timestamp,"eventType":event.event_type,"actor":event.actor,"provider":event.provider,"inputs":event.inputs,"outputs":event.outputs,"metadata":event.metadata,"previousEventHash":event.previous_event_hash})).map_err(|e| CommandError::new("SERIALIZE_ERROR", e.to_string(), false))?;
        if hex::encode(Sha256::digest(canonical)) != event.event_hash {
            return Ok(false);
        }
        previous = Some(event.event_hash.clone());
        last = previous.clone();
    }
    Ok(last == head_hash(root)?)
}
fn project_layout(root: &Path) -> CommandResult<()> {
    for folder in [
        "manuscript/sections",
        "ideas",
        "conversations/transcripts",
        "provenance/reports",
        "style",
        "assets",
        "exports",
        ".thinkloom",
    ] {
        fs::create_dir_all(root.join(folder))
            .map_err(|e| CommandError::io("Could not create project structure", e))?;
    }
    atomic_write(
        &root.join(".gitignore"),
        b".thinkloom/\nexports/*.pdf\nexports/*.zip\n*.tmp\n*.wav\n*.mp3\n",
    )?;
    Ok(())
}
fn read_manifest(root: &Path) -> CommandResult<ProjectManifest> {
    serde_json::from_slice(
        &fs::read(root.join("project.json"))
            .map_err(|e| CommandError::io("Could not read project", e))?,
    )
    .map_err(|e| CommandError::new("PROJECT_INVALID", e.to_string(), false))
}
fn snapshot(root: &Path) -> CommandResult<PathBuf> {
    let db = root.join(".thinkloom/state.sqlite");
    if !db.exists() {
        return Err(CommandError::new(
            "NO_SNAPSHOT",
            "There is no live database to snapshot yet.",
            true,
        ));
    }
    let snapshots = root.join(".thinkloom/snapshots");
    fs::create_dir_all(&snapshots)
        .map_err(|e| CommandError::io("Could not create snapshot folder", e))?;
    let destination = snapshots.join(format!(
        "state-{}.sqlite",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    fs::copy(&db, &destination)
        .map_err(|e| CommandError::io("Could not create database snapshot", e))?;
    let mut existing: Vec<_> = fs::read_dir(&snapshots)
        .map_err(|e| CommandError::io("Could not inspect snapshots", e))?
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
    let safe_name: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                c
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
    let timestamp = Utc::now().to_rfc3339();
    let manifest = ProjectManifest {
        schema_version: SCHEMA_VERSION.into(),
        application_version: APP_VERSION.into(),
        id: Uuid::new_v4().to_string(),
        title: title.clone(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        current_phase: "ideation".into(),
        publication_status: "working".into(),
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
    append_event(
        &root,
        &manifest.id,
        "PROJECT_CREATED",
        "user",
        &format!("Created {title}"),
        json!({"historyAvailable":history_ok}),
    )?;
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
                "provenance",
                "style",
            ],
        );
        let _ = run_git(&root, &["commit", "--quiet", "-m", "Project created"]);
    }
    *state
        .active_project
        .lock()
        .map_err(|_| CommandError::new("STATE_LOCKED", "Project state is unavailable.", true))? =
        Some(root.clone());
    Ok(ProjectSummary {
        path: root.to_string_lossy().into_owned(),
        manifest,
        provenance_valid: true,
        recovery_available: false,
    })
}

#[tauri::command]
fn open_project(path: String, state: State<RuntimeState>) -> CommandResult<ProjectSummary> {
    let root = PathBuf::from(path);
    let manifest = read_manifest(&root)?;
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(CommandError::new(
            "SCHEMA_UNSUPPORTED",
            format!(
                "This project uses schema {}. Thinkloom supports {SCHEMA_VERSION}.",
                manifest.schema_version
            ),
            false,
        ));
    }
    init_db(&root)?;
    *state
        .active_project
        .lock()
        .map_err(|_| CommandError::new("STATE_LOCKED", "Project state is unavailable.", true))? =
        Some(root.clone());
    let valid = verify_chain_at(&root).unwrap_or(false);
    let recovery = root.join(".thinkloom/snapshots").exists();
    let _ = append_event(
        &root,
        &manifest.id,
        "PROJECT_OPENED",
        "user",
        "Reopened project",
        json!({"chainWasValid":valid}),
    );
    Ok(ProjectSummary {
        path: root.to_string_lossy().into_owned(),
        manifest,
        provenance_valid: valid,
        recovery_available: recovery,
    })
}

#[tauri::command]
fn persist_state(app_state: Value, state: State<RuntimeState>) -> CommandResult<()> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    let serialized = serde_json::to_string(&app_state)
        .map_err(|e| CommandError::new("SERIALIZE_ERROR", e.to_string(), false))?;
    let mut db = init_db(&root)?;
    let transaction = db
        .transaction()
        .map_err(|e| CommandError::new("DATABASE_ERROR", e.to_string(), true))?;
    transaction.execute("INSERT INTO project_state(id,json,updated_at) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET json=excluded.json,updated_at=excluded.updated_at", params![serialized,Utc::now().to_rfc3339()]).map_err(|e| CommandError::new("DATABASE_ERROR", e.to_string(), true))?;
    transaction
        .commit()
        .map_err(|e| CommandError::new("DATABASE_ERROR", e.to_string(), true))?;
    let event = app_state
        .get("events")
        .and_then(Value::as_array)
        .and_then(|events| events.last())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let summary = event
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("Saved project change");
    append_event(
        &root,
        &manifest.id,
        event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("STATE_MUTATED"),
        event.get("actor").and_then(Value::as_str).unwrap_or("user"),
        summary,
        json!({"clientEventId":event.get("id")}),
    )?;
    if let Some(manuscript) = app_state.get("manuscript").and_then(Value::as_str) {
        atomic_write(
            &root.join("manuscript/manuscript.md"),
            manuscript.as_bytes(),
        )?;
    }
    if let Some(ideas) = app_state.get("ideas") {
        write_json(&root.join("ideas/ideas.json"), ideas)?;
    }
    if let Some(turns) = app_state.get("turns") {
        write_json(
            &root.join("conversations/sessions.json"),
            &json!([{"id":"active-session","mode":"typed","turns":turns}]),
        )?;
    }
    write_json(
        &root.join("style/profile.json"),
        &json!({"schemaVersion":SCHEMA_VERSION,"traits":app_state.get("styleTraits"),"disallowedHabits":app_state.get("disallowedHabits")}),
    )?;
    let _ = snapshot(&root);
    Ok(())
}

#[tauri::command]
fn verify_provenance(state: State<RuntimeState>) -> CommandResult<bool> {
    verify_chain_at(&active_path(&state)?)
}

#[tauri::command]
fn create_checkpoint(name: String, state: State<RuntimeState>) -> CommandResult<()> {
    let root = active_path(&state)?;
    let manifest = read_manifest(&root)?;
    append_event(
        &root,
        &manifest.id,
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
    keyring::Entry::new("com.thinkloom.desktop", &profile_id)
        .map_err(|e| CommandError::new("CREDENTIAL_ERROR", e.to_string(), true))?
        .set_password(&secret)
        .map_err(|e| CommandError::new("CREDENTIAL_ERROR", e.to_string(), true))
}
#[tauri::command]
fn delete_provider_secret(profile_id: String) -> CommandResult<()> {
    keyring::Entry::new("com.thinkloom.desktop", &profile_id)
        .map_err(|e| CommandError::new("CREDENTIAL_ERROR", e.to_string(), true))?
        .delete_credential()
        .map_err(|e| CommandError::new("CREDENTIAL_ERROR", e.to_string(), true))
}

#[tauri::command]
fn test_provider(profile: ProviderProfile) -> CommandResult<ConnectionResult> {
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
        let secret = keyring::Entry::new("com.thinkloom.desktop", "openai")
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
    Ok(hex::encode(Sha256::digest(fs::read(path).map_err(
        |e| CommandError::io("Could not hash export", e),
    )?)))
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
        "evidence" => create_evidence(&root, &manifest, sanitized)?,
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
        &manifest.id,
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
fn create_evidence(
    root: &Path,
    manifest: &ProjectManifest,
    sanitized: bool,
) -> CommandResult<PathBuf> {
    let destination = root
        .join("exports")
        .join(format!("{}-evidence.zip", manifest.id));
    let temp = destination.with_extension("zip.tmp");
    let file = fs::File::create(&temp)
        .map_err(|e| CommandError::io("Could not create evidence package", e))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let markdown = fs::read(root.join("manuscript/manuscript.md"))
        .map_err(|e| CommandError::io("Could not read manuscript", e))?;
    zip.start_file("final-manuscript.md", options)
        .map_err(|e| CommandError::new("EXPORT_ERROR", e.to_string(), true))?;
    zip.write_all(&markdown)
        .map_err(|e| CommandError::io("Could not write evidence", e))?;
    let report=format!("# Creative Process Record\n\nProject: {}\n\nThis package records creative-process relationships and is not a legal conclusion.\n\nSanitized subset: {}\n\nHistory events are hash-chained with SHA-256.\n",manifest.title,sanitized);
    zip.start_file("creative-process-report.md", options)
        .map_err(|e| CommandError::new("EXPORT_ERROR", e.to_string(), true))?;
    zip.write_all(report.as_bytes())
        .map_err(|e| CommandError::io("Could not write report", e))?;
    if !sanitized {
        let events = fs::read(root.join("provenance/events.jsonl"))
            .map_err(|e| CommandError::io("Could not read provenance", e))?;
        zip.start_file("provenance-events.jsonl", options)
            .map_err(|e| CommandError::new("EXPORT_ERROR", e.to_string(), true))?;
        zip.write_all(&events)
            .map_err(|e| CommandError::io("Could not write provenance", e))?;
    }
    let export_manifest = json!({"schemaVersion":SCHEMA_VERSION,"projectId":manifest.id,"createdAt":Utc::now().to_rfc3339(),"applicationVersion":APP_VERSION,"sanitized":sanitized,"provenanceChainHead":head_hash(root)?,"files":[{"path":"final-manuscript.md","sha256":hex::encode(Sha256::digest(&markdown))}]});
    zip.start_file("export-manifest.json", options)
        .map_err(|e| CommandError::new("EXPORT_ERROR", e.to_string(), true))?;
    zip.write_all(
        serde_json::to_string_pretty(&export_manifest)
            .unwrap()
            .as_bytes(),
    )
    .map_err(|e| CommandError::io("Could not write manifest", e))?;
    zip.finish()
        .map_err(|e| CommandError::new("EXPORT_ERROR", e.to_string(), true))?;
    if destination.exists() {
        fs::remove_file(&destination)
            .map_err(|e| CommandError::io("Could not replace evidence package", e))?;
    }
    fs::rename(&temp, &destination)
        .map_err(|e| CommandError::io("Could not finalize evidence package", e))?;
    Ok(destination)
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
            json!({"path":name,"sha256":hex::encode(Sha256::digest(&bytes)),"size":bytes.len()}),
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
        .join(format!("{}-backup.zip", manifest.id));
    let temp = destination.with_extension("zip.tmp");
    fs::create_dir_all(destination.parent().unwrap())
        .map_err(|e| CommandError::io("Could not create export folder", e))?;
    let file =
        fs::File::create(&temp).map_err(|e| CommandError::io("Could not create backup", e))?;
    let mut zip = ZipWriter::new(file);
    let files = add_tree_to_zip(&mut zip, &root)?;
    let backup_manifest = json!({"schemaVersion":SCHEMA_VERSION,"applicationVersion":APP_VERSION,"projectId":manifest.id,"createdAt":Utc::now().to_rfc3339(),"files":files});
    zip.start_file(
        "backup-manifest.json",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
    )
    .map_err(|e| CommandError::new("BACKUP_ERROR", e.to_string(), true))?;
    zip.write_all(
        serde_json::to_string_pretty(&backup_manifest)
            .unwrap()
            .as_bytes(),
    )
    .map_err(|e| CommandError::io("Could not write backup manifest", e))?;
    zip.finish()
        .map_err(|e| CommandError::new("BACKUP_ERROR", e.to_string(), true))?;
    if destination.exists() {
        fs::remove_file(&destination)
            .map_err(|e| CommandError::io("Could not replace backup", e))?;
    }
    fs::rename(&temp, &destination)
        .map_err(|e| CommandError::io("Could not finalize backup", e))?;
    append_event(
        &root,
        &manifest.id,
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
        &manifest.id,
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
    manifest.publication_status = "finalized".into();
    manifest.updated_at = Utc::now().to_rfc3339();
    write_json(&root.join("project.json"), &manifest)?;
    let release = format!("release-{}", Utc::now().format("%Y%m%d-%H%M%S"));
    append_event(
        &root,
        &manifest.id,
        "RELEASE_FINALIZED",
        "user",
        &format!("Finalized {release}"),
        json!({"releaseId":release}),
    )?;
    run_git(
        &root,
        &[
            "add",
            "project.json",
            "manuscript",
            "ideas",
            "conversations",
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
fn load_project_state(state: State<RuntimeState>) -> CommandResult<Option<Value>> {
    let root = active_path(&state)?;
    let db = init_db(&root)?;
    let mut statement = db
        .prepare("SELECT json FROM project_state WHERE id=1")
        .map_err(|e| CommandError::new("DATABASE_ERROR", e.to_string(), true))?;
    let mut rows = statement
        .query([])
        .map_err(|e| CommandError::new("DATABASE_ERROR", e.to_string(), true))?;
    if let Some(row) = rows
        .next()
        .map_err(|e| CommandError::new("DATABASE_ERROR", e.to_string(), true))?
    {
        let raw: String = row
            .get(0)
            .map_err(|e| CommandError::new("DATABASE_ERROR", e.to_string(), true))?;
        Ok(Some(serde_json::from_str(&raw).map_err(|e| {
            CommandError::new("PROJECT_INVALID", e.to_string(), true)
        })?))
    } else {
        Ok(None)
    }
}

#[tauri::command]
fn generate_text(
    app: AppHandle,
    profile: ProviderProfile,
    prompt_variables: HashMap<String, String>,
    cloud_approved: bool,
    purpose: String,
) -> CommandResult<String> {
    if profile.mode == "cloud" && !cloud_approved {
        return Err(CommandError::new(
            "CLOUD_APPROVAL_REQUIRED",
            "Approve cloud processing for this project before sending context.",
            true,
        ));
    }
    let (system, prompt) = prompts_for_request(&app, &purpose, prompt_variables)?;
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
        if let Ok(entry) = keyring::Entry::new("com.thinkloom.desktop", &profile.kind) {
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
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RuntimeState::default())
        .invoke_handler(tauri::generate_handler![
            choose_project_folder,
            create_project,
            open_project,
            load_project_state,
            persist_state,
            verify_provenance,
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
            schema_version: SCHEMA_VERSION.into(),
            application_version: APP_VERSION.into(),
            id: "test".into(),
            title: "Test".into(),
            created_at: Utc::now().to_rfc3339(),
            updated_at: Utc::now().to_rfc3339(),
            current_phase: "ideation".into(),
            publication_status: "working".into(),
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
        assert!(verify_chain_at(temp.path()).unwrap());
        let journal = temp.path().join("provenance/events.jsonl");
        let changed = fs::read_to_string(&journal)
            .unwrap()
            .replace("Created", "Changed");
        fs::write(&journal, changed).unwrap();
        assert!(!verify_chain_at(temp.path()).unwrap());
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
