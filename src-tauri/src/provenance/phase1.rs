//! Typed Phase 1 commands and deterministic projections.
//!
//! `phase1-operation` records are the replay authority for the ideation UI.
//! Specific companion records preserve the evidentiary meaning of each action.

use super::{
    canonical::{canonical_digest, canonicalize},
    ledger::{self, LedgerPaths},
    records::{CplEvent, CplRecord, RecordInput, WriteCommand, WriteResult},
    writer::{init_database, CplService},
    CplError, CplResult, CPL_SCHEMA_VERSION,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase1Turn {
    pub id: String,
    pub speaker: String,
    pub text: String,
    pub created_at: String,
    pub input_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase1Idea {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub detail: String,
    pub status: String,
    #[serde(default)]
    pub source_turn_ids: Vec<String>,
    #[serde(default)]
    pub parent_idea_ids: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub pinned: bool,
    pub selected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub created_by: String,
    pub revision_number: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase1Provider {
    pub kind: String,
    pub name: String,
    pub endpoint: String,
    pub model: String,
    pub mode: String,
    pub connected: bool,
}

impl Default for Phase1Provider {
    fn default() -> Self {
        Self {
            kind: "ollama".to_owned(),
            name: "Ollama".to_owned(),
            endpoint: "http://127.0.0.1:11434".to_owned(),
            model: "llama3.2".to_owned(),
            mode: "local".to_owned(),
            connected: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase1Session {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub persona: String,
    pub challenge: String,
    pub genre: String,
    pub lore: String,
    #[serde(default)]
    pub turns: Vec<Phase1Turn>,
    pub workspace: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase1Seed {
    pub active_session_id: String,
    pub session_title: String,
    pub session_created_at: String,
    #[serde(default)]
    pub sessions: Vec<Phase1Session>,
    #[serde(default)]
    pub turns: Vec<Phase1Turn>,
    #[serde(default)]
    pub ideas: Vec<Phase1Idea>,
    pub persona: String,
    pub challenge: String,
    pub genre: String,
    pub lore: String,
    pub workspace: String,
    pub summary: String,
    pub provider: Phase1Provider,
    pub cloud_approved: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInvocation {
    pub invocation_id: String,
    pub purpose: String,
    pub session_id: String,
    pub provider: Phase1Provider,
    pub prompt_template_sha256: String,
    pub input_sha256: String,
    pub context_sha256: String,
    pub requested_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct InvocationProjection {
    pub purpose: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub enum Phase1Operation {
    Initialize {
        seed: Phase1Seed,
    },
    SessionCreated {
        session_id: String,
        title: String,
        created_at: String,
    },
    SessionActivated {
        session_id: String,
        activated_at: String,
    },
    SessionTitleRevised {
        session_id: String,
        title: String,
        revised_at: String,
    },
    PersonaChanged {
        session_id: String,
        persona: String,
    },
    ChallengeChanged {
        session_id: String,
        challenge: String,
    },
    GenreChanged {
        session_id: String,
        genre: String,
    },
    LoreChanged {
        session_id: String,
        lore: String,
        input_kind: String,
    },
    ProviderContextChanged {
        provider: Phase1Provider,
    },
    CloudApprovalChanged {
        approved: bool,
    },
    HumanTurnCreated {
        session_id: String,
        turn: Phase1Turn,
    },
    AssistantTurnCreated {
        session_id: String,
        turn: Phase1Turn,
    },
    IdeasChanged {
        ideas: Vec<Phase1Idea>,
        #[serde(default)]
        created_ids: Vec<String>,
    },
    DraftingPaperTurnAppended {
        session_id: String,
        turn_id: String,
    },
    DraftingPaperRevised {
        session_id: String,
        text: String,
        input_kind: String,
        #[serde(default)]
        clear_summary: bool,
    },
    ProviderInvocationRequested {
        invocation: ProviderInvocation,
    },
    ProviderInvocationResponded {
        invocation_id: String,
        retained_text: String,
        content_sha256: String,
        created_at: String,
    },
    ProviderInvocationFailed {
        invocation_id: String,
        code: String,
        bounded_summary: String,
        recoverable: bool,
        occurred_at: String,
    },
    DistillationDisposed {
        invocation_id: String,
        disposition: String,
        summary: String,
        decided_at: String,
    },
    ExternalContentDeclared {
        source_id: String,
        source_kind: String,
        target: String,
        retained_text: String,
        resulting_text: String,
        declaration: String,
    },
}

impl Phase1Operation {
    fn event_type(&self) -> &'static str {
        match self {
            Self::Initialize { .. } => "PHASE1_INITIALIZED",
            Self::SessionCreated { .. } => "SESSION_CREATED",
            Self::SessionActivated { .. } => "SESSION_ACTIVATED",
            Self::SessionTitleRevised { .. } => "SESSION_REVISED",
            Self::PersonaChanged { .. } => "PERSONA_CHANGED",
            Self::ChallengeChanged { .. } => "CHALLENGE_CHANGED",
            Self::GenreChanged { .. } => "GENRE_CHANGED",
            Self::LoreChanged { .. } => "LORE_CHANGED",
            Self::ProviderContextChanged { .. } => "PROVIDER_CONTEXT_CHANGED",
            Self::CloudApprovalChanged { .. } => "CLOUD_APPROVAL_CHANGED",
            Self::HumanTurnCreated { turn, .. } if turn.input_mode == "voice_transcription" => {
                "VOICE_TRANSCRIPTION_COMMITTED"
            }
            Self::HumanTurnCreated { .. } => "HUMAN_TURN_CREATED",
            Self::AssistantTurnCreated { .. } => "ASSISTANT_RESPONSE_RETAINED",
            Self::IdeasChanged { created_ids, .. } if !created_ids.is_empty() => "IDEAS_CREATED",
            Self::IdeasChanged { .. } => "IDEAS_REVISED",
            Self::DraftingPaperTurnAppended { .. } => "TURN_APPENDED_TO_DRAFTING_PAPER",
            Self::DraftingPaperRevised { .. } => "DRAFTING_PAPER_REVISED",
            Self::ProviderInvocationRequested { .. } => "PROVIDER_INVOCATION_REQUESTED",
            Self::ProviderInvocationResponded { .. } => "PROVIDER_INVOCATION_RESPONDED",
            Self::ProviderInvocationFailed { .. } => "PROVIDER_INVOCATION_FAILED",
            Self::DistillationDisposed { .. } => "DISTILLATION_DISPOSED",
            Self::ExternalContentDeclared { source_kind, .. } if source_kind == "paste" => {
                "PASTED_CONTENT_DECLARED"
            }
            Self::ExternalContentDeclared { source_kind, .. } if source_kind == "import" => {
                "IMPORTED_CONTENT_DECLARED"
            }
            Self::ExternalContentDeclared { .. } => "EXTERNAL_SOURCE_DECLARED",
        }
    }

    fn evidence_records(&self) -> CplResult<Vec<RecordInput>> {
        let value = serde_json::to_value(self).map_err(|error| {
            CplError::new("PHASE1_SERIALIZATION_FAILED", error.to_string(), false)
        })?;
        let mut records = Vec::new();
        let mut push = |record_type: &str, payload: Value| {
            records.push(RecordInput {
                record_type: record_type.to_owned(),
                payload,
            });
        };
        match self {
            Self::Initialize { seed } => {
                push("phase1-initialization", serde_json::to_value(seed).unwrap());
                push(
                    "conversation-session",
                    json!({"session_id": seed.active_session_id, "title": seed.session_title, "created_at": seed.session_created_at}),
                );
                for session in &seed.sessions {
                    push(
                        "conversation-session",
                        serde_json::to_value(session).unwrap(),
                    );
                }
                for turn in seed
                    .turns
                    .iter()
                    .chain(seed.sessions.iter().flat_map(|session| &session.turns))
                {
                    push("transcript-turn", serde_json::to_value(turn).unwrap());
                    if turn.input_mode == "voice_transcription" {
                        push(
                            "voice-transcription",
                            json!({"turn": turn, "audio_retained": false, "audio_reference": Value::Null}),
                        );
                    }
                }
                for idea in &seed.ideas {
                    push(
                        "idea",
                        json!({"idea_id": idea.id, "status": idea.status, "created_at": idea.created_at}),
                    );
                    push("idea-revision", serde_json::to_value(idea).unwrap());
                }
                push(
                    "provider-context-revision",
                    serde_json::to_value(&seed.provider).unwrap(),
                );
                if !seed.lore.is_empty() {
                    push(
                        "lore-context-revision",
                        json!({"session_id": seed.active_session_id, "lore": seed.lore, "input_kind": "initialization"}),
                    );
                }
                if !seed.workspace.is_empty() {
                    push(
                        "drafting-paper-revision",
                        json!({"session_id": seed.active_session_id, "text": seed.workspace, "input_kind": "initialization"}),
                    );
                }
            }
            Self::SessionCreated { .. }
            | Self::SessionActivated { .. }
            | Self::SessionTitleRevised { .. } => push("conversation-session", value),
            Self::PersonaChanged { .. } => push("persona-context-revision", value),
            Self::ChallengeChanged { .. } => push("challenge-context-revision", value),
            Self::GenreChanged { .. } => push("genre-context-revision", value),
            Self::LoreChanged { .. } => push("lore-context-revision", value),
            Self::ProviderContextChanged { .. } | Self::CloudApprovalChanged { .. } => {
                push("provider-context-revision", value)
            }
            Self::HumanTurnCreated { turn, .. } => {
                push("transcript-turn", serde_json::to_value(turn).unwrap());
                if turn.input_mode == "voice_transcription" {
                    push(
                        "voice-transcription",
                        json!({"turn": turn, "audio_retained": false, "audio_reference": Value::Null}),
                    );
                }
            }
            Self::AssistantTurnCreated { turn, .. } => {
                push("transcript-turn", serde_json::to_value(turn).unwrap())
            }
            Self::IdeasChanged { ideas, created_ids } => {
                for idea in ideas {
                    push("idea-revision", serde_json::to_value(idea).unwrap());
                    if created_ids.contains(&idea.id) {
                        push(
                            "idea",
                            json!({"idea_id": idea.id, "status": idea.status, "created_at": idea.created_at}),
                        );
                    }
                }
            }
            Self::DraftingPaperTurnAppended { .. } => push("drafting-paper-append", value),
            Self::DraftingPaperRevised { .. } => push("drafting-paper-revision", value),
            Self::ProviderInvocationRequested { invocation } => push(
                "invocation-request",
                serde_json::to_value(invocation).unwrap(),
            ),
            Self::ProviderInvocationResponded { .. } => push("invocation-response", value),
            Self::ProviderInvocationFailed { .. } => push("invocation-failure", value),
            Self::DistillationDisposed { .. } => push("disposition-revision", value),
            Self::ExternalContentDeclared { .. } => push("source-declaration", value),
        }
        Ok(records)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase1Command {
    pub client_action_id: String,
    pub actor: String,
    pub summary: String,
    pub occurred_at: String,
    pub operation: Phase1Operation,
}

impl Phase1Command {
    fn validate(&self) -> CplResult<()> {
        for (label, value) in [
            ("client_action_id", self.client_action_id.as_str()),
            ("actor", self.actor.as_str()),
            ("summary", self.summary.as_str()),
            ("occurred_at", self.occurred_at.as_str()),
        ] {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(CplError::new(
                    "PHASE1_COMMAND_INVALID",
                    format!("{label} is empty or contains control characters."),
                    false,
                ));
            }
        }
        if !matches!(self.actor.as_str(), "user" | "assistant" | "system") {
            return Err(CplError::new(
                "PHASE1_ACTOR_INVALID",
                "Phase 1 actor must be user, assistant, or system.",
                false,
            ));
        }
        Ok(())
    }

    fn into_write_command(self, project_id: &str) -> CplResult<WriteCommand> {
        self.validate()?;
        let client_action_id = self.client_action_id.clone();
        let event_type = self.operation.event_type().to_owned();
        let actor = self.actor.clone();
        let summary = self.summary.clone();
        let evidence = self.operation.evidence_records()?;
        let command = serde_json::to_value(&self)
            .map_err(|error| CplError::new("PHASE1_COMMAND_INVALID", error.to_string(), false))?;
        let mut records = vec![RecordInput {
            record_type: "phase1-operation".to_owned(),
            payload: json!({"schema_version": CPL_SCHEMA_VERSION, "command": command}),
        }];
        records.extend(evidence);
        Ok(WriteCommand {
            client_action_id,
            project_id: project_id.to_owned(),
            event_type,
            actor,
            metadata: json!({"summary": summary, "phase": "ideation", "typed_command": true}),
            records,
            operational_state: None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase1Activity {
    pub id: String,
    pub r#type: String,
    pub actor: String,
    pub at: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Phase1Projection {
    pub schema_version: String,
    pub project_id: String,
    pub initialized: bool,
    pub active_session_id: String,
    pub session_title: String,
    pub session_created_at: String,
    pub sessions: Vec<Phase1Session>,
    pub turns: Vec<Phase1Turn>,
    pub ideas: Vec<Phase1Idea>,
    pub persona: String,
    pub challenge: String,
    pub genre: String,
    pub lore: String,
    pub workspace: String,
    pub summary: String,
    pub provider: Phase1Provider,
    pub cloud_approved: bool,
    pub activity: Vec<Phase1Activity>,
    pub invocations: BTreeMap<String, InvocationProjection>,
    pub updated_at: String,
}

impl Phase1Projection {
    fn empty(project_id: &str) -> Self {
        Self {
            schema_version: CPL_SCHEMA_VERSION.to_owned(),
            project_id: project_id.to_owned(),
            initialized: false,
            active_session_id: String::new(),
            session_title: String::new(),
            session_created_at: String::new(),
            sessions: Vec::new(),
            turns: Vec::new(),
            ideas: Vec::new(),
            persona: "Supportive Coach".to_owned(),
            challenge: "Balanced".to_owned(),
            genre: "Fiction".to_owned(),
            lore: String::new(),
            workspace: String::new(),
            summary: String::new(),
            provider: Phase1Provider::default(),
            cloud_approved: false,
            activity: Vec::new(),
            invocations: BTreeMap::new(),
            updated_at: String::new(),
        }
    }

    pub fn current_session_snapshot(&self) -> Phase1Session {
        Phase1Session {
            id: self.active_session_id.clone(),
            title: self.session_title.clone(),
            created_at: self.session_created_at.clone(),
            updated_at: self.updated_at.clone(),
            persona: self.persona.clone(),
            challenge: self.challenge.clone(),
            genre: self.genre.clone(),
            lore: self.lore.clone(),
            turns: self.turns.clone(),
            workspace: self.workspace.clone(),
            summary: self.summary.clone(),
        }
    }

    fn store_current_session(&mut self) {
        if self.active_session_id.is_empty() {
            return;
        }
        let snapshot = self.current_session_snapshot();
        if let Some(existing) = self.sessions.iter_mut().find(|item| item.id == snapshot.id) {
            *existing = snapshot;
        } else {
            self.sessions.push(snapshot);
        }
    }

    fn apply(&mut self, command: &Phase1Command, event: &CplEvent) -> CplResult<()> {
        match &command.operation {
            Phase1Operation::Initialize { seed } => {
                self.initialized = true;
                self.active_session_id = seed.active_session_id.clone();
                self.session_title = seed.session_title.clone();
                self.session_created_at = seed.session_created_at.clone();
                self.sessions = seed.sessions.clone();
                self.turns = seed.turns.clone();
                self.ideas = seed.ideas.clone();
                self.persona = seed.persona.clone();
                self.challenge = seed.challenge.clone();
                self.genre = seed.genre.clone();
                self.lore = seed.lore.clone();
                self.workspace = seed.workspace.clone();
                self.summary = seed.summary.clone();
                self.provider = seed.provider.clone();
                self.cloud_approved = seed.cloud_approved;
                self.updated_at = seed.updated_at.clone();
            }
            Phase1Operation::SessionCreated {
                session_id,
                title,
                created_at,
            } => {
                self.store_current_session();
                self.active_session_id = session_id.clone();
                self.session_title = title.clone();
                self.session_created_at = created_at.clone();
                self.turns.clear();
                self.workspace.clear();
                self.summary.clear();
            }
            Phase1Operation::SessionActivated { session_id, .. } => {
                self.store_current_session();
                let target = self
                    .sessions
                    .iter()
                    .find(|session| &session.id == session_id)
                    .cloned()
                    .ok_or_else(|| {
                        CplError::new(
                            "PHASE1_SESSION_UNKNOWN",
                            format!("Session {session_id} cannot be activated before it exists."),
                            false,
                        )
                    })?;
                self.active_session_id = target.id;
                self.session_title = target.title;
                self.session_created_at = target.created_at;
                self.persona = target.persona;
                self.challenge = target.challenge;
                self.genre = target.genre;
                self.lore = target.lore;
                self.turns = target.turns;
                self.workspace = target.workspace;
                self.summary = target.summary;
                self.sessions.retain(|session| session.id != *session_id);
            }
            Phase1Operation::SessionTitleRevised {
                session_id, title, ..
            } => {
                require_active(self, session_id)?;
                self.session_title = title.clone();
            }
            Phase1Operation::PersonaChanged {
                session_id,
                persona,
            } => {
                require_active(self, session_id)?;
                self.persona = persona.clone();
            }
            Phase1Operation::ChallengeChanged {
                session_id,
                challenge,
            } => {
                require_active(self, session_id)?;
                self.challenge = challenge.clone();
            }
            Phase1Operation::GenreChanged { session_id, genre } => {
                require_active(self, session_id)?;
                self.genre = genre.clone();
            }
            Phase1Operation::LoreChanged {
                session_id, lore, ..
            } => {
                require_active(self, session_id)?;
                self.lore = lore.clone();
            }
            Phase1Operation::ProviderContextChanged { provider } => {
                self.provider = provider.clone()
            }
            Phase1Operation::CloudApprovalChanged { approved } => self.cloud_approved = *approved,
            Phase1Operation::HumanTurnCreated { session_id, turn }
            | Phase1Operation::AssistantTurnCreated { session_id, turn } => {
                require_active(self, session_id)?;
                if !self.turns.iter().any(|existing| existing.id == turn.id) {
                    self.turns.push(turn.clone());
                }
            }
            Phase1Operation::IdeasChanged { ideas, .. } => {
                for idea in ideas {
                    if let Some(existing) = self.ideas.iter_mut().find(|item| item.id == idea.id) {
                        *existing = idea.clone();
                    } else {
                        self.ideas.push(idea.clone());
                    }
                }
            }
            Phase1Operation::DraftingPaperTurnAppended {
                session_id,
                turn_id,
            } => {
                require_active(self, session_id)?;
                let text = self
                    .turns
                    .iter()
                    .find(|turn| &turn.id == turn_id)
                    .map(|turn| turn.text.clone())
                    .ok_or_else(|| {
                        CplError::new(
                            "PHASE1_TURN_UNKNOWN",
                            format!("Turn {turn_id} cannot be appended before it exists."),
                            false,
                        )
                    })?;
                if !self.workspace.trim().is_empty() {
                    self.workspace.push_str("\n\n");
                }
                self.workspace.push_str(&text);
            }
            Phase1Operation::DraftingPaperRevised {
                session_id,
                text,
                clear_summary,
                ..
            } => {
                require_active(self, session_id)?;
                self.workspace = text.clone();
                if *clear_summary {
                    self.summary.clear();
                }
            }
            Phase1Operation::ProviderInvocationRequested { invocation } => {
                self.provider = invocation.provider.clone();
                self.invocations.insert(
                    invocation.invocation_id.clone(),
                    InvocationProjection {
                        purpose: invocation.purpose.clone(),
                        status: "requested".to_owned(),
                        retained_text: None,
                        failure_code: None,
                    },
                );
            }
            Phase1Operation::ProviderInvocationResponded {
                invocation_id,
                retained_text,
                ..
            } => {
                let invocation = self.invocations.get_mut(invocation_id).ok_or_else(|| {
                    CplError::new(
                        "PHASE1_INVOCATION_UNKNOWN",
                        "A provider response cannot precede its request record.",
                        false,
                    )
                })?;
                invocation.status = "responded".to_owned();
                invocation.retained_text = Some(retained_text.clone());
                self.provider.connected = true;
            }
            Phase1Operation::ProviderInvocationFailed {
                invocation_id,
                code,
                ..
            } => {
                let invocation = self.invocations.get_mut(invocation_id).ok_or_else(|| {
                    CplError::new(
                        "PHASE1_INVOCATION_UNKNOWN",
                        "A provider failure cannot precede its request record.",
                        false,
                    )
                })?;
                invocation.status = "failed".to_owned();
                invocation.failure_code = Some(code.clone());
                self.provider.connected = false;
            }
            Phase1Operation::DistillationDisposed {
                invocation_id,
                disposition,
                summary,
                ..
            } => {
                let invocation = self.invocations.get_mut(invocation_id).ok_or_else(|| {
                    CplError::new(
                        "PHASE1_INVOCATION_UNKNOWN",
                        "A distillation disposition cannot precede its invocation.",
                        false,
                    )
                })?;
                if invocation.purpose != "distillation" || invocation.status != "responded" {
                    return Err(CplError::new(
                        "PHASE1_DISTILLATION_INVALID",
                        "Only a retained distillation response can be disposed.",
                        false,
                    ));
                }
                invocation.status = disposition.clone();
                if matches!(disposition.as_str(), "accepted" | "replaced") {
                    self.summary = summary.clone();
                }
            }
            Phase1Operation::ExternalContentDeclared {
                target,
                resulting_text,
                ..
            } => match target.as_str() {
                "drafting_paper" => self.workspace = resulting_text.clone(),
                "lore" => self.lore = resulting_text.clone(),
                _ => {}
            },
        }
        self.updated_at = command.occurred_at.clone();
        self.activity.push(Phase1Activity {
            id: command.client_action_id.clone(),
            r#type: event.event_type.clone(),
            actor: command.actor.clone(),
            at: command.occurred_at.clone(),
            summary: command.summary.clone(),
        });
        Ok(())
    }
}

fn require_active(projection: &Phase1Projection, session_id: &str) -> CplResult<()> {
    if projection.active_session_id == session_id {
        Ok(())
    } else {
        Err(CplError::new(
            "PHASE1_SESSION_MISMATCH",
            format!(
                "Command targets session {session_id}, but {} is active.",
                projection.active_session_id
            ),
            false,
        ))
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Phase1CommandResult {
    pub write: WriteResult,
    pub projection: Phase1Projection,
}

pub fn apply_command(
    root: &Path,
    project_id: &str,
    command: Phase1Command,
) -> CplResult<Phase1CommandResult> {
    let write = CplService::new(root, project_id).write(command.into_write_command(project_id)?)?;
    let projection = reconstruct(root, project_id)?;
    cache_projection(root, &projection)?;
    Ok(Phase1CommandResult { write, projection })
}

pub fn reconstruct(root: &Path, project_id: &str) -> CplResult<Phase1Projection> {
    let events = ledger::read_all_events(&LedgerPaths::new(root))?;
    reconstruct_from_events(root, project_id, &events)
}

pub(crate) fn reconstruct_from_events(
    root: &Path,
    project_id: &str,
    events: &[CplEvent],
) -> CplResult<Phase1Projection> {
    let mut projection = Phase1Projection::empty(project_id);
    for event in events {
        let operations = event
            .record_references
            .iter()
            .filter(|reference| reference.record_type == "phase1-operation")
            .collect::<Vec<_>>();
        if operations.is_empty() {
            continue;
        }
        if operations.len() != 1 {
            return Err(CplError::new(
                "PHASE1_OPERATION_AMBIGUOUS",
                "A typed Phase 1 event must bind exactly one phase1-operation record.",
                false,
            ));
        }
        let path = root.join(
            operations[0]
                .path
                .split('/')
                .collect::<std::path::PathBuf>(),
        );
        let bytes = fs::read(&path)
            .map_err(|error| CplError::io("Could not read a Phase 1 operation record", error))?;
        let record: CplRecord = serde_json::from_slice(&bytes)
            .map_err(|error| CplError::new("PHASE1_RECORD_INVALID", error.to_string(), false))?;
        if canonicalize(
            &serde_json::to_value(&record).map_err(|error| {
                CplError::new("PHASE1_RECORD_INVALID", error.to_string(), false)
            })?,
        )? != bytes
        {
            return Err(CplError::new(
                "PHASE1_RECORD_NONCANONICAL",
                format!("{} is not canonical JSON.", path.display()),
                false,
            ));
        }
        let command: Phase1Command =
            serde_json::from_value(record.payload.get("command").cloned().ok_or_else(|| {
                CplError::new(
                    "PHASE1_COMMAND_MISSING",
                    "A phase1-operation record has no typed command.",
                    false,
                )
            })?)
            .map_err(|error| CplError::new("PHASE1_COMMAND_INVALID", error.to_string(), false))?;
        projection.apply(&command, event)?;
    }
    Ok(projection)
}

pub(crate) fn rebuild_projection_cache(
    root: &Path,
    project_id: &str,
    events: &[CplEvent],
) -> CplResult<()> {
    let projection = reconstruct_from_events(root, project_id, events)?;
    cache_projection(root, &projection)
}

fn cache_projection(root: &Path, projection: &Phase1Projection) -> CplResult<()> {
    let database = init_database(root)?;
    database
        .execute(
            "CREATE TABLE IF NOT EXISTS phase1_projection (id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL, source_event_count INTEGER NOT NULL, updated_at TEXT NOT NULL)",
            [],
        )
        .map_err(super::writer::database_error)?;
    let json = serde_json::to_string(projection)
        .map_err(|error| CplError::new("PHASE1_SERIALIZATION_FAILED", error.to_string(), false))?;
    database
        .execute(
            "INSERT INTO phase1_projection(id,json,source_event_count,updated_at) VALUES(1,?1,?2,?3) ON CONFLICT(id) DO UPDATE SET json=excluded.json,source_event_count=excluded.source_event_count,updated_at=excluded.updated_at",
            params![json, projection.activity.len() as u64, projection.updated_at],
        )
        .map_err(super::writer::database_error)?;
    Ok(())
}

pub fn canonical_text_digest(text: &str) -> CplResult<String> {
    canonical_digest(&json!({"text": text}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::{identifiers::timestamp_millis, writer::rebuild_indexes};
    use tempfile::tempdir;

    fn command(id: &str, operation: Phase1Operation) -> Phase1Command {
        Phase1Command {
            client_action_id: id.to_owned(),
            actor: "user".to_owned(),
            summary: id.to_owned(),
            occurred_at: timestamp_millis(),
            operation,
        }
    }

    fn seed() -> Phase1Seed {
        Phase1Seed {
            active_session_id: "session_one".to_owned(),
            session_title: "One".to_owned(),
            session_created_at: timestamp_millis(),
            sessions: vec![],
            turns: vec![],
            ideas: vec![],
            persona: "Supportive Coach".to_owned(),
            challenge: "Balanced".to_owned(),
            genre: "Fiction".to_owned(),
            lore: String::new(),
            workspace: String::new(),
            summary: String::new(),
            provider: Phase1Provider::default(),
            cloud_approved: false,
            updated_at: timestamp_millis(),
        }
    }

    #[test]
    fn typed_records_reconstruct_phase1_without_an_application_state_snapshot() {
        let temp = tempdir().unwrap();
        let project_id = "project_test";
        apply_command(
            temp.path(),
            project_id,
            command("initialize", Phase1Operation::Initialize { seed: seed() }),
        )
        .unwrap();
        let turn = Phase1Turn {
            id: "turn_one".to_owned(),
            speaker: "user".to_owned(),
            text: "A human thought".to_owned(),
            created_at: timestamp_millis(),
            input_mode: "typed".to_owned(),
            invocation_id: None,
        };
        apply_command(
            temp.path(),
            project_id,
            command(
                "turn",
                Phase1Operation::HumanTurnCreated {
                    session_id: "session_one".to_owned(),
                    turn: turn.clone(),
                },
            ),
        )
        .unwrap();
        apply_command(
            temp.path(),
            project_id,
            command(
                "append",
                Phase1Operation::DraftingPaperTurnAppended {
                    session_id: "session_one".to_owned(),
                    turn_id: turn.id.clone(),
                },
            ),
        )
        .unwrap();
        let idea = Phase1Idea {
            id: "idea_one".to_owned(),
            title: "Idea".to_owned(),
            summary: "Summary".to_owned(),
            detail: String::new(),
            status: "accepted".to_owned(),
            source_turn_ids: vec!["turn_one".to_owned()],
            parent_idea_ids: vec![],
            tags: vec!["test".to_owned()],
            pinned: true,
            selected: true,
            group: None,
            created_by: "user".to_owned(),
            revision_number: 1,
            created_at: timestamp_millis(),
            updated_at: timestamp_millis(),
        };
        apply_command(
            temp.path(),
            project_id,
            command(
                "idea",
                Phase1Operation::IdeasChanged {
                    ideas: vec![idea.clone()],
                    created_ids: vec![idea.id.clone()],
                },
            ),
        )
        .unwrap();

        let events = ledger::read_all_events(&LedgerPaths::new(temp.path())).unwrap();
        assert!(events
            .iter()
            .flat_map(|event| &event.record_references)
            .all(|reference| reference.record_type != "application-state-snapshot"));
        let rebuilt = reconstruct(temp.path(), project_id).unwrap();
        assert_eq!(rebuilt.turns, vec![turn]);
        assert_eq!(rebuilt.workspace, "A human thought");
        assert_eq!(rebuilt.ideas, vec![idea]);

        let database = init_database(temp.path()).unwrap();
        database
            .execute("DELETE FROM phase1_projection", [])
            .unwrap();
        drop(database);
        rebuild_indexes(temp.path(), &events).unwrap();
        let cached: String = init_database(temp.path())
            .unwrap()
            .query_row("SELECT json FROM phase1_projection WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Phase1Projection>(&cached).unwrap(),
            rebuilt
        );
    }

    #[test]
    fn provider_request_is_durable_before_simulated_io_and_outcome_follows() {
        let temp = tempdir().unwrap();
        let project_id = "project_test";
        apply_command(
            temp.path(),
            project_id,
            command("initialize", Phase1Operation::Initialize { seed: seed() }),
        )
        .unwrap();
        let invocation = ProviderInvocation {
            invocation_id: "invocation_one".to_owned(),
            purpose: "conversation".to_owned(),
            session_id: "session_one".to_owned(),
            provider: Phase1Provider::default(),
            prompt_template_sha256: "sha256:prompt".to_owned(),
            input_sha256: "sha256:input".to_owned(),
            context_sha256: "sha256:context".to_owned(),
            requested_at: timestamp_millis(),
        };
        apply_command(
            temp.path(),
            project_id,
            command(
                "request",
                Phase1Operation::ProviderInvocationRequested {
                    invocation: invocation.clone(),
                },
            ),
        )
        .unwrap();

        // This write represents work performed while provider I/O is in flight.
        // It succeeds because the request write released the OS writer lock.
        apply_command(
            temp.path(),
            project_id,
            command(
                "context-during-io",
                Phase1Operation::ChallengeChanged {
                    session_id: "session_one".to_owned(),
                    challenge: "Rigorous".to_owned(),
                },
            ),
        )
        .unwrap();
        apply_command(
            temp.path(),
            project_id,
            command(
                "response",
                Phase1Operation::ProviderInvocationResponded {
                    invocation_id: invocation.invocation_id.clone(),
                    retained_text: "Response".to_owned(),
                    content_sha256: canonical_text_digest("Response").unwrap(),
                    created_at: timestamp_millis(),
                },
            ),
        )
        .unwrap();
        let projection = reconstruct(temp.path(), project_id).unwrap();
        assert_eq!(projection.challenge, "Rigorous");
        assert_eq!(
            projection.invocations[&invocation.invocation_id].status,
            "responded"
        );
        let events = ledger::read_all_events(&LedgerPaths::new(temp.path())).unwrap();
        assert_eq!(events[1].event_type, "PROVIDER_INVOCATION_REQUESTED");
        assert_eq!(events[3].event_type, "PROVIDER_INVOCATION_RESPONDED");
    }

    #[test]
    fn voice_transcription_retains_text_but_no_audio_reference_or_digest() {
        let temp = tempdir().unwrap();
        let project_id = "project_test";
        apply_command(
            temp.path(),
            project_id,
            command("initialize", Phase1Operation::Initialize { seed: seed() }),
        )
        .unwrap();
        apply_command(
            temp.path(),
            project_id,
            command(
                "voice",
                Phase1Operation::HumanTurnCreated {
                    session_id: "session_one".to_owned(),
                    turn: Phase1Turn {
                        id: "turn_voice".to_owned(),
                        speaker: "user".to_owned(),
                        text: "Spoken expression".to_owned(),
                        created_at: timestamp_millis(),
                        input_mode: "voice_transcription".to_owned(),
                        invocation_id: None,
                    },
                },
            ),
        )
        .unwrap();
        let events = ledger::read_all_events(&LedgerPaths::new(temp.path())).unwrap();
        let voice = events[1]
            .record_references
            .iter()
            .find(|reference| reference.record_type == "voice-transcription")
            .unwrap();
        let bytes = fs::read(
            temp.path()
                .join(voice.path.replace('/', std::path::MAIN_SEPARATOR_STR)),
        )
        .unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("Spoken expression"));
        assert!(text.contains("\"audio_retained\":false"));
        assert!(!text.contains("audio_digest"));
        assert!(!text.contains("audio_path"));
    }
    #[test]
    fn every_visible_phase1_field_replays_from_typed_operations() {
        let temp = tempdir().unwrap();
        let project_id = "project_complete";
        apply_command(
            temp.path(),
            project_id,
            command("initialize", Phase1Operation::Initialize { seed: seed() }),
        )
        .unwrap();
        let human = Phase1Turn {
            id: "human".into(),
            speaker: "user".into(),
            text: "Human text".into(),
            created_at: timestamp_millis(),
            input_mode: "typed".into(),
            invocation_id: None,
        };
        let assistant = Phase1Turn {
            id: "assistant".into(),
            speaker: "assistant".into(),
            text: "Assistant text".into(),
            created_at: timestamp_millis(),
            input_mode: "typed".into(),
            invocation_id: Some("conversation".into()),
        };
        let provider = Phase1Provider {
            kind: "compatible".into(),
            name: "Local endpoint".into(),
            endpoint: "http://127.0.0.1:1234/v1".into(),
            model: "writer".into(),
            mode: "local".into(),
            connected: false,
        };
        let idea_v1 = Phase1Idea {
            id: "idea".into(),
            title: "Initial".into(),
            summary: "Summary".into(),
            detail: String::new(),
            status: "suggested".into(),
            source_turn_ids: vec![human.id.clone()],
            parent_idea_ids: vec![],
            tags: vec![],
            pinned: false,
            selected: false,
            group: None,
            created_by: "user".into(),
            revision_number: 1,
            created_at: timestamp_millis(),
            updated_at: timestamp_millis(),
        };
        let idea_v2 = Phase1Idea {
            title: "Revised".into(),
            status: "accepted".into(),
            selected: true,
            revision_number: 2,
            updated_at: timestamp_millis(),
            ..idea_v1.clone()
        };
        let distillation = ProviderInvocation {
            invocation_id: "distill".into(),
            purpose: "distillation".into(),
            session_id: "session_one".into(),
            provider: provider.clone(),
            prompt_template_sha256: "sha256:prompt".into(),
            input_sha256: "sha256:input".into(),
            context_sha256: "sha256:context".into(),
            requested_at: timestamp_millis(),
        };
        let failed = ProviderInvocation {
            invocation_id: "failed".into(),
            purpose: "conversation".into(),
            ..distillation.clone()
        };
        let operations = vec![
            Phase1Operation::SessionTitleRevised {
                session_id: "session_one".into(),
                title: "Revised session".into(),
                revised_at: timestamp_millis(),
            },
            Phase1Operation::PersonaChanged {
                session_id: "session_one".into(),
                persona: "Creative Partner".into(),
            },
            Phase1Operation::ChallengeChanged {
                session_id: "session_one".into(),
                challenge: "Rigorous".into(),
            },
            Phase1Operation::GenreChanged {
                session_id: "session_one".into(),
                genre: "Poetry".into(),
            },
            Phase1Operation::LoreChanged {
                session_id: "session_one".into(),
                lore: "Manual lore".into(),
                input_kind: "manual".into(),
            },
            Phase1Operation::ProviderContextChanged {
                provider: provider.clone(),
            },
            Phase1Operation::CloudApprovalChanged { approved: true },
            Phase1Operation::HumanTurnCreated {
                session_id: "session_one".into(),
                turn: human.clone(),
            },
            Phase1Operation::AssistantTurnCreated {
                session_id: "session_one".into(),
                turn: assistant.clone(),
            },
            Phase1Operation::DraftingPaperTurnAppended {
                session_id: "session_one".into(),
                turn_id: human.id.clone(),
            },
            Phase1Operation::DraftingPaperTurnAppended {
                session_id: "session_one".into(),
                turn_id: assistant.id.clone(),
            },
            Phase1Operation::DraftingPaperRevised {
                session_id: "session_one".into(),
                text: "Manual paper".into(),
                input_kind: "manual".into(),
                clear_summary: false,
            },
            Phase1Operation::ExternalContentDeclared {
                source_id: "draft_source".into(),
                source_kind: "paste".into(),
                target: "drafting_paper".into(),
                retained_text: "Pasted".into(),
                resulting_text: "Final paper".into(),
                declaration: "Declared paste".into(),
            },
            Phase1Operation::ExternalContentDeclared {
                source_id: "lore_source".into(),
                source_kind: "import".into(),
                target: "lore".into(),
                retained_text: "Imported".into(),
                resulting_text: "Final lore".into(),
                declaration: "Declared import".into(),
            },
            Phase1Operation::IdeasChanged {
                ideas: vec![idea_v1],
                created_ids: vec!["idea".into()],
            },
            Phase1Operation::IdeasChanged {
                ideas: vec![idea_v2.clone()],
                created_ids: vec![],
            },
            Phase1Operation::ProviderInvocationRequested {
                invocation: distillation.clone(),
            },
            Phase1Operation::ProviderInvocationResponded {
                invocation_id: distillation.invocation_id.clone(),
                retained_text: "Distilled".into(),
                content_sha256: canonical_text_digest("Distilled").unwrap(),
                created_at: timestamp_millis(),
            },
            Phase1Operation::DistillationDisposed {
                invocation_id: distillation.invocation_id.clone(),
                disposition: "accepted".into(),
                summary: "Accepted summary".into(),
                decided_at: timestamp_millis(),
            },
            Phase1Operation::ProviderInvocationRequested {
                invocation: failed.clone(),
            },
            Phase1Operation::ProviderInvocationFailed {
                invocation_id: failed.invocation_id.clone(),
                code: "OFFLINE".into(),
                bounded_summary: "Unavailable".into(),
                recoverable: true,
                occurred_at: timestamp_millis(),
            },
            Phase1Operation::SessionCreated {
                session_id: "session_two".into(),
                title: "Two".into(),
                created_at: timestamp_millis(),
            },
            Phase1Operation::SessionTitleRevised {
                session_id: "session_two".into(),
                title: "Revised two".into(),
                revised_at: timestamp_millis(),
            },
            Phase1Operation::SessionActivated {
                session_id: "session_one".into(),
                activated_at: timestamp_millis(),
            },
        ];
        for (index, operation) in operations.into_iter().enumerate() {
            apply_command(
                temp.path(),
                project_id,
                command(&format!("operation_{index}"), operation),
            )
            .unwrap();
        }
        let projection = reconstruct(temp.path(), project_id).unwrap();
        assert_eq!(
            (
                projection.active_session_id.as_str(),
                projection.session_title.as_str()
            ),
            ("session_one", "Revised session")
        );
        assert_eq!(
            (
                projection.persona.as_str(),
                projection.challenge.as_str(),
                projection.genre.as_str()
            ),
            ("Creative Partner", "Rigorous", "Poetry")
        );
        assert_eq!(
            (
                projection.lore.as_str(),
                projection.workspace.as_str(),
                projection.summary.as_str()
            ),
            ("Final lore", "Final paper", "Accepted summary")
        );
        assert_eq!(projection.provider, provider);
        assert!(projection.cloud_approved);
        assert_eq!(projection.turns, vec![human, assistant]);
        assert_eq!(projection.ideas, vec![idea_v2]);
        assert_eq!(projection.invocations["distill"].status, "accepted");
        assert_eq!(projection.invocations["failed"].status, "failed");
        assert_eq!(projection.sessions[0].title, "Revised two");
    }
}
