//! Typed manuscript composition capture, lineage preservation, and deterministic replay.

use super::{
    canonical::{canonical_digest, canonicalize},
    ledger::{self, LedgerPaths},
    records::{CplEvent, CplRecord, RecordInput, WriteCommand, WriteResult},
    writer::{init_database, CplService},
    CplError, CplResult, CPL_SCHEMA_VERSION,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompositionOperationKind {
    Insert,
    Delete,
    Replace,
    Move,
    Paste,
    Transcription,
    AiAcceptance,
    Restoration,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecordedOrigin {
    RecordedDirectHumanInput,
    HumanExpressiveInputViaTranscription,
    AcceptedAiOutput,
    ImportedOrPasted,
    SystemRestoration,
    Unattested,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompositionBoundary {
    Initialization,
    Idle,
    FocusLoss,
    SectionChange,
    AiOperation,
    Checkpoint,
    PhaseChange,
    ExplicitSave,
    DocumentClose,
    Paste,
    Restoration,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScalarRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiAcceptanceContext {
    pub invocation_id: String,
    pub response_text: String,
    pub accepted_ranges: Vec<ScalarRange>,
    pub rejected_ranges: Vec<ScalarRange>,
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "SCREAMING_SNAKE_CASE",
    rename_all_fields = "camelCase"
)]
pub enum CompositionAction {
    Initialize {
        text: String,
        origin: RecordedOrigin,
    },
    Edit {
        before_text: String,
        after_text: String,
        boundary: CompositionBoundary,
        origin: RecordedOrigin,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        operation_kind_hint: Option<CompositionOperationKind>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ai_acceptance: Option<AiAcceptanceContext>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompositionCommand {
    pub client_action_id: String,
    pub actor: String,
    pub summary: String,
    pub occurred_at: String,
    pub action: CompositionAction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionSpan {
    pub segment_id: String,
    pub ancestry_segment_id: String,
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub content_sha256: String,
    pub origin: RecordedOrigin,
    pub lineage_reference_ids: Vec<String>,
    pub operation_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LineageUnit {
    character: char,
    origin: RecordedOrigin,
    ancestry_segment_id: String,
    lineage_reference_ids: Vec<String>,
    operation_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompositionProjection {
    pub schema_version: String,
    pub project_id: String,
    pub initialized: bool,
    pub manuscript: String,
    pub revision_id: String,
    pub operation_count: u64,
    pub spans: Vec<ExpressionSpan>,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    units: Vec<LineageUnit>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionCommandResult {
    pub write: WriteResult,
    pub projection: CompositionProjection,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompositionOperationRecord {
    operation_id: String,
    operation_kind: CompositionOperationKind,
    base_revision_id: String,
    result_revision_id: String,
    operation_sequence: u64,
    source_start: usize,
    source_end: usize,
    destination_start: usize,
    inserted_content_sha256: Option<String>,
    deleted_content_sha256: Option<String>,
    recorded_origin: RecordedOrigin,
    transformation_relationships: Vec<String>,
    lineage_reference_ids: Vec<String>,
    invocation_id: Option<String>,
    disposition_id: Option<String>,
    boundary: CompositionBoundary,
}

impl CompositionProjection {
    fn empty(project_id: &str) -> Self {
        Self {
            schema_version: CPL_SCHEMA_VERSION.to_owned(),
            project_id: project_id.to_owned(),
            initialized: false,
            manuscript: String::new(),
            revision_id: String::new(),
            operation_count: 0,
            spans: Vec::new(),
            updated_at: String::new(),
            units: Vec::new(),
        }
    }

    fn apply(
        &mut self,
        command: &CompositionCommand,
    ) -> CplResult<Option<CompositionOperationRecord>> {
        command.validate()?;
        match &command.action {
            CompositionAction::Initialize { text, origin } => {
                if self.initialized {
                    return Err(CplError::new(
                        "COMPOSITION_ALREADY_INITIALIZED",
                        "The manuscript composition stream is already initialized.",
                        false,
                    ));
                }
                let revision_id = stable_id("revision", &command.client_action_id);
                let ancestry =
                    stable_id("segment", &format!("{}:initial", command.client_action_id));
                self.units = text
                    .chars()
                    .map(|character| LineageUnit {
                        character,
                        origin: *origin,
                        ancestry_segment_id: ancestry.clone(),
                        lineage_reference_ids: vec![revision_id.clone()],
                        operation_ids: vec![],
                    })
                    .collect();
                self.initialized = true;
                self.manuscript = text.clone();
                self.revision_id = revision_id;
                self.updated_at = command.occurred_at.clone();
                self.rebuild_spans()?;
                Ok(None)
            }
            CompositionAction::Edit {
                before_text,
                after_text,
                boundary,
                origin,
                operation_kind_hint,
                ai_acceptance,
            } => {
                if !self.initialized {
                    return Err(CplError::new(
                        "COMPOSITION_NOT_INITIALIZED",
                        "Initialize manuscript composition before recording edits.",
                        false,
                    ));
                }
                if &self.manuscript != before_text {
                    return Err(CplError::new("COMPOSITION_BASE_MISMATCH", "The edit preimage does not match the current canonical manuscript revision.", true));
                }
                if before_text == after_text {
                    return Err(CplError::new(
                        "COMPOSITION_NO_CHANGE",
                        "A composition boundary with no manuscript change is not an operation.",
                        true,
                    ));
                }
                let before = before_text.chars().collect::<Vec<_>>();
                let after = after_text.chars().collect::<Vec<_>>();
                let prefix = common_prefix(&before, &after);
                let suffix = common_suffix(&before[prefix..], &after[prefix..]);
                let source_end = before.len() - suffix;
                let result_end = after.len() - suffix;
                let deleted = before[prefix..source_end].iter().collect::<String>();
                let inserted = after[prefix..result_end].iter().collect::<String>();
                let operation_kind =
                    operation_kind_hint.unwrap_or_else(|| infer_kind(&deleted, &inserted, *origin));
                if !origin_allowed_for_operation(operation_kind, *origin) {
                    return Err(CplError::new(
                        "COMPOSITION_ORIGIN_INVALID",
                        "The recorded origin is incompatible with the composition operation.",
                        false,
                    ));
                }
                validate_ai_acceptance(operation_kind, ai_acceptance.as_ref())?;

                let operation_id = stable_id("operation", &command.client_action_id);
                let result_revision_id = stable_id("revision", &command.client_action_id);
                let disposition_id = ai_acceptance
                    .as_ref()
                    .map(|_| stable_id("disposition", &command.client_action_id));
                let deleted_units = self.units[prefix..source_end].to_vec();
                let mut lineage = BTreeSet::new();
                for unit in &deleted_units {
                    lineage.insert(unit.ancestry_segment_id.clone());
                    lineage.extend(unit.lineage_reference_ids.iter().cloned());
                    lineage.extend(unit.operation_ids.iter().cloned());
                }
                if let Some(ai) = ai_acceptance {
                    lineage.insert(ai.invocation_id.clone());
                }
                if lineage.is_empty() {
                    lineage.insert(self.revision_id.clone());
                }
                let ancestry =
                    stable_id("segment", &format!("{}:insert", command.client_action_id));
                let inserted_units = if operation_kind == CompositionOperationKind::Move {
                    moved_units(&deleted_units, &inserted, &operation_id)?
                } else {
                    inserted
                        .chars()
                        .map(|character| LineageUnit {
                            character,
                            origin: *origin,
                            ancestry_segment_id: ancestry.clone(),
                            lineage_reference_ids: lineage.iter().cloned().collect(),
                            operation_ids: vec![operation_id.clone()],
                        })
                        .collect::<Vec<_>>()
                };
                self.units.splice(prefix..source_end, inserted_units);
                self.manuscript = self.units.iter().map(|unit| unit.character).collect();
                if self.manuscript != *after_text {
                    return Err(CplError::new(
                        "COMPOSITION_REPLAY_DIVERGED",
                        "The recorded operation did not reproduce its declared result.",
                        false,
                    ));
                }
                let base_revision_id = self.revision_id.clone();
                self.revision_id = result_revision_id.clone();
                self.operation_count += 1;
                self.updated_at = command.occurred_at.clone();
                self.rebuild_spans()?;
                Ok(Some(CompositionOperationRecord {
                    operation_id,
                    operation_kind,
                    base_revision_id,
                    result_revision_id,
                    operation_sequence: self.operation_count,
                    source_start: prefix,
                    source_end,
                    destination_start: prefix,
                    inserted_content_sha256: (!inserted.is_empty())
                        .then(|| text_digest(&inserted))
                        .transpose()?,
                    deleted_content_sha256: (!deleted.is_empty())
                        .then(|| text_digest(&deleted))
                        .transpose()?,
                    recorded_origin: *origin,
                    transformation_relationships: relationships(
                        operation_kind,
                        !deleted.is_empty(),
                    ),
                    lineage_reference_ids: lineage.into_iter().collect(),
                    invocation_id: ai_acceptance
                        .as_ref()
                        .map(|value| value.invocation_id.clone()),
                    disposition_id,
                    boundary: *boundary,
                }))
            }
        }
    }

    fn rebuild_spans(&mut self) -> CplResult<()> {
        let mut spans = Vec::new();
        let mut start = 0;
        while start < self.units.len() {
            let first = &self.units[start];
            let mut end = start + 1;
            while end < self.units.len() {
                let next = &self.units[end];
                if next.origin != first.origin
                    || next.ancestry_segment_id != first.ancestry_segment_id
                    || next.lineage_reference_ids != first.lineage_reference_ids
                    || next.operation_ids != first.operation_ids
                {
                    break;
                }
                end += 1;
            }
            let text = self.units[start..end]
                .iter()
                .map(|unit| unit.character)
                .collect::<String>();
            spans.push(ExpressionSpan {
                segment_id: stable_id(
                    "segment",
                    &format!("{}:{start}:{}", self.revision_id, first.ancestry_segment_id),
                ),
                ancestry_segment_id: first.ancestry_segment_id.clone(),
                start,
                end,
                content_sha256: text_digest(&text)?,
                text,
                origin: first.origin,
                lineage_reference_ids: first.lineage_reference_ids.clone(),
                operation_ids: first.operation_ids.clone(),
            });
            start = end;
        }
        self.spans = spans;
        Ok(())
    }
}

impl CompositionCommand {
    fn validate(&self) -> CplResult<()> {
        for (label, value) in [
            ("client_action_id", self.client_action_id.as_str()),
            ("actor", self.actor.as_str()),
            ("summary", self.summary.as_str()),
            ("occurred_at", self.occurred_at.as_str()),
        ] {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(CplError::new(
                    "COMPOSITION_COMMAND_INVALID",
                    format!("{label} is empty or contains control characters."),
                    false,
                ));
            }
        }
        if !matches!(self.actor.as_str(), "user" | "assistant" | "system") {
            return Err(CplError::new(
                "COMPOSITION_ACTOR_INVALID",
                "Composition actor must be user, assistant, or system.",
                false,
            ));
        }
        Ok(())
    }
}

pub fn apply_command(
    root: &Path,
    project_id: &str,
    command: CompositionCommand,
) -> CplResult<CompositionCommandResult> {
    let service = CplService::new(root, project_id);
    let client_action_id = command.client_action_id.clone();
    let write = service.write_prepared(&client_action_id, || {
        let events = ledger::read_all_events(&LedgerPaths::new(root))?;
        if let Some(event) = events
            .iter()
            .find(|event| event.client_action_id == command.client_action_id)
        {
            return match composition_command_from_event(root, event)? {
                Some(existing) if existing == command => Ok(None),
                _ => Err(CplError::new("CPL_IDEMPOTENCY_CONFLICT", "The client_action_id was already committed with a different canonical command.", false)),
            };
        }
        let mut projected = reconstruct_from_events(root, project_id, &events)?;
        let operation = projected.apply(&command)?;
        let mut records = vec![RecordInput {
        record_type: "composition-command".into(),
        payload: json!({"schema_version": CPL_SCHEMA_VERSION, "command": command}),
    }];
    if let Some(operation) = &operation {
        records.push(RecordInput {
            record_type: "composition-operation".into(),
            payload: serde_json::to_value(operation).map_err(serialization_error)?,
        });
        let (deleted, inserted) = changed_content(&command.action)?;
        records.push(RecordInput { record_type: "composition-content".into(), payload: json!({"operation_id": operation.operation_id, "deleted_text": deleted, "inserted_text": inserted}) });
        if let CompositionAction::Edit {
            ai_acceptance: Some(ai),
            ..
        } = &command.action
        {
            records.push(RecordInput { record_type: "ai-acceptance-disposition".into(), payload: json!({"operation_id": operation.operation_id, "disposition_id": operation.disposition_id, "invocation_id": ai.invocation_id, "response_text": ai.response_text, "accepted_ranges": ai.accepted_ranges, "rejected_ranges": ai.rejected_ranges, "partial": ai.partial, "result_revision_id": operation.result_revision_id}) });
        }
    } else {
        records.push(RecordInput { record_type: "composition-initialization".into(), payload: json!({"revision_id": projected.revision_id, "origin": projected.spans.first().map(|span| span.origin), "text_sha256": text_digest(&projected.manuscript)?}) });
    }
    records.push(RecordInput { record_type: "manuscript-revision".into(), payload: json!({"revision_id": projected.revision_id, "manuscript": projected.manuscript, "content_sha256": text_digest(&projected.manuscript)?, "operation_count": projected.operation_count}) });
    for span in &projected.spans {
        records.push(RecordInput {
            record_type: "expression-segment".into(),
            payload: serde_json::to_value(span).map_err(serialization_error)?,
        });
    }
    let boundary = match &command.action {
        CompositionAction::Initialize { .. } => CompositionBoundary::Initialization,
        CompositionAction::Edit { boundary, .. } => *boundary,
    };
        Ok(Some(WriteCommand {
            client_action_id: command.client_action_id.clone(),
            project_id: project_id.to_owned(),
            event_type: format!("COMPOSITION_{}", serde_json::to_value(boundary).unwrap().as_str().unwrap().to_ascii_uppercase()),
            actor: command.actor.clone(),
            metadata: json!({"summary": command.summary, "composition_boundary": boundary, "result_revision_id": projected.revision_id}),
            records,
            operational_state: None,
        }))
    })?;
    let projection = reconstruct(root, project_id)?;
    cache_projection(root, &projection)?;
    Ok(CompositionCommandResult { write, projection })
}

pub fn reconstruct(root: &Path, project_id: &str) -> CplResult<CompositionProjection> {
    let events = ledger::read_all_events(&LedgerPaths::new(root))?;
    reconstruct_from_events(root, project_id, &events)
}

fn composition_command_from_event(
    root: &Path,
    event: &CplEvent,
) -> CplResult<Option<CompositionCommand>> {
    let references = event
        .record_references
        .iter()
        .filter(|reference| reference.record_type == "composition-command")
        .collect::<Vec<_>>();
    if references.is_empty() {
        return Ok(None);
    }
    if references.len() != 1 {
        return Err(CplError::new(
            "COMPOSITION_COMMAND_AMBIGUOUS",
            "A composition event must bind exactly one composition-command record.",
            false,
        ));
    }
    let path = references[0]
        .path
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part));
    let bytes = fs::read(&path)
        .map_err(|error| CplError::io("Could not read a composition command record", error))?;
    let record: CplRecord = serde_json::from_slice(&bytes)
        .map_err(|error| CplError::new("COMPOSITION_RECORD_INVALID", error.to_string(), false))?;
    if canonicalize(&serde_json::to_value(&record).map_err(serialization_error)?)? != bytes {
        return Err(CplError::new(
            "COMPOSITION_RECORD_NONCANONICAL",
            format!("{} is not canonical JSON.", path.display()),
            false,
        ));
    }
    let command =
        serde_json::from_value(record.payload.get("command").cloned().ok_or_else(|| {
            CplError::new(
                "COMPOSITION_COMMAND_MISSING",
                "The composition command record has no command.",
                false,
            )
        })?)
        .map_err(|error| CplError::new("COMPOSITION_COMMAND_INVALID", error.to_string(), false))?;
    Ok(Some(command))
}

pub(crate) fn reconstruct_from_events(
    root: &Path,
    project_id: &str,
    events: &[CplEvent],
) -> CplResult<CompositionProjection> {
    let mut projection = CompositionProjection::empty(project_id);
    for event in events {
        if let Some(command) = composition_command_from_event(root, event)? {
            projection.apply(&command)?;
        }
    }
    Ok(projection)
}

pub(crate) fn rebuild_projection_cache(
    root: &Path,
    project_id: &str,
    events: &[CplEvent],
) -> CplResult<()> {
    cache_projection(root, &reconstruct_from_events(root, project_id, events)?)
}

fn cache_projection(root: &Path, projection: &CompositionProjection) -> CplResult<()> {
    let database = init_database(root)?;
    database.execute("CREATE TABLE IF NOT EXISTS composition_projection (id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL, operation_count INTEGER NOT NULL, updated_at TEXT NOT NULL)", []).map_err(super::writer::database_error)?;
    let json = serde_json::to_string(projection).map_err(serialization_error)?;
    database.execute("INSERT INTO composition_projection(id,json,operation_count,updated_at) VALUES(1,?1,?2,?3) ON CONFLICT(id) DO UPDATE SET json=excluded.json,operation_count=excluded.operation_count,updated_at=excluded.updated_at", params![json, projection.operation_count, projection.updated_at]).map_err(super::writer::database_error)?;
    Ok(())
}

pub fn origin_allowed_for_operation(
    operation: CompositionOperationKind,
    origin: RecordedOrigin,
) -> bool {
    match operation {
        CompositionOperationKind::Paste => origin == RecordedOrigin::ImportedOrPasted,
        CompositionOperationKind::Transcription => {
            origin == RecordedOrigin::HumanExpressiveInputViaTranscription
        }
        CompositionOperationKind::AiAcceptance => origin == RecordedOrigin::AcceptedAiOutput,
        CompositionOperationKind::Restoration => origin == RecordedOrigin::SystemRestoration,
        CompositionOperationKind::Delete | CompositionOperationKind::Move => true,
        CompositionOperationKind::Insert | CompositionOperationKind::Replace => matches!(
            origin,
            RecordedOrigin::RecordedDirectHumanInput
                | RecordedOrigin::AcceptedAiOutput
                | RecordedOrigin::Unattested
        ),
    }
}

fn validate_ai_acceptance(
    kind: CompositionOperationKind,
    ai: Option<&AiAcceptanceContext>,
) -> CplResult<()> {
    if (kind == CompositionOperationKind::AiAcceptance) != ai.is_some() {
        return Err(CplError::new("AI_ACCEPTANCE_BINDING_INVALID", "AI acceptance operations require exactly one invocation response and disposition binding.", false));
    }
    if let Some(ai) = ai {
        let length = ai.response_text.chars().count();
        if ai.invocation_id.trim().is_empty()
            || ai.accepted_ranges.is_empty()
            || ai
                .accepted_ranges
                .iter()
                .chain(&ai.rejected_ranges)
                .any(|range| range.start >= range.end || range.end > length)
        {
            return Err(CplError::new("AI_ACCEPTANCE_RANGE_INVALID", "Accepted and rejected AI response ranges must be non-empty scalar ranges within the retained response.", false));
        }
        let mut dispositions = vec![0_u8; length];
        for (value, ranges) in [(1_u8, &ai.accepted_ranges), (2_u8, &ai.rejected_ranges)] {
            for range in ranges {
                for disposition in &mut dispositions[range.start..range.end] {
                    if *disposition != 0 {
                        return Err(CplError::new(
                            "AI_ACCEPTANCE_RANGE_OVERLAP",
                            "Accepted and rejected AI response ranges must not overlap.",
                            false,
                        ));
                    }
                    *disposition = value;
                }
            }
        }
        if dispositions.contains(&0) || ai.partial != !ai.rejected_ranges.is_empty() {
            return Err(CplError::new("AI_ACCEPTANCE_DISPOSITION_INCOMPLETE", "AI acceptance must disposition every Unicode scalar as accepted or rejected and accurately declare whether acceptance was partial.", false));
        }
    }
    Ok(())
}

fn changed_content(action: &CompositionAction) -> CplResult<(String, String)> {
    let CompositionAction::Edit {
        before_text,
        after_text,
        ..
    } = action
    else {
        return Ok((String::new(), String::new()));
    };
    let before = before_text.chars().collect::<Vec<_>>();
    let after = after_text.chars().collect::<Vec<_>>();
    let prefix = common_prefix(&before, &after);
    let suffix = common_suffix(&before[prefix..], &after[prefix..]);
    Ok((
        before[prefix..before.len() - suffix].iter().collect(),
        after[prefix..after.len() - suffix].iter().collect(),
    ))
}

fn infer_kind(deleted: &str, inserted: &str, origin: RecordedOrigin) -> CompositionOperationKind {
    match origin {
        RecordedOrigin::ImportedOrPasted => CompositionOperationKind::Paste,
        RecordedOrigin::HumanExpressiveInputViaTranscription => {
            CompositionOperationKind::Transcription
        }
        RecordedOrigin::AcceptedAiOutput => CompositionOperationKind::AiAcceptance,
        RecordedOrigin::SystemRestoration => CompositionOperationKind::Restoration,
        _ if inserted.is_empty() => CompositionOperationKind::Delete,
        _ if deleted.is_empty() => CompositionOperationKind::Insert,
        _ => CompositionOperationKind::Replace,
    }
}

fn relationships(kind: CompositionOperationKind, replaced: bool) -> Vec<String> {
    let mut values = vec![match kind {
        CompositionOperationKind::Insert => "inserted",
        CompositionOperationKind::Delete => "deleted",
        CompositionOperationKind::Replace => "replaced",
        CompositionOperationKind::Move => "moved",
        CompositionOperationKind::Paste => "pasted_from_external",
        CompositionOperationKind::Transcription => "transcribed_from_human_speech",
        CompositionOperationKind::AiAcceptance => "generated_by_ai",
        CompositionOperationKind::Restoration => "restored_from_revision",
    }
    .to_owned()];
    if replaced
        && matches!(
            kind,
            CompositionOperationKind::Insert | CompositionOperationKind::Replace
        )
    {
        values.push("modified_by_human".into());
    }
    values
}

fn moved_units(
    source: &[LineageUnit],
    destination: &str,
    operation_id: &str,
) -> CplResult<Vec<LineageUnit>> {
    let mut remaining = source.iter().cloned().map(Some).collect::<Vec<_>>();
    let mut result = Vec::with_capacity(remaining.len());
    for character in destination.chars() {
        let index = remaining
            .iter()
            .position(|unit| unit.as_ref().is_some_and(|unit| unit.character == character))
            .ok_or_else(|| {
                CplError::new(
                    "COMPOSITION_MOVE_CONTENT_MISMATCH",
                    "A move may only select and rearrange existing expression without changing its content.",
                    false,
                )
            })?;
        let mut unit = remaining[index].take().expect("matched move unit");
        unit.operation_ids.push(operation_id.to_owned());
        result.push(unit);
    }
    if remaining.iter().any(Option::is_some) {
        return Err(CplError::new(
            "COMPOSITION_MOVE_CONTENT_MISMATCH",
            "A move may only select and rearrange existing expression without changing its content.",
            false,
        ));
    }
    Ok(result)
}
fn common_prefix(before: &[char], after: &[char]) -> usize {
    before
        .iter()
        .zip(after)
        .take_while(|(left, right)| left == right)
        .count()
}
fn common_suffix(before: &[char], after: &[char]) -> usize {
    before
        .iter()
        .rev()
        .zip(after.iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
}
fn text_digest(text: &str) -> CplResult<String> {
    canonical_digest(&json!({"text": text}))
}
fn serialization_error(error: impl std::fmt::Display) -> CplError {
    CplError::new("COMPOSITION_SERIALIZATION_FAILED", error.to_string(), false)
}

fn stable_id(prefix: &str, source: &str) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let digest = Sha256::digest(source.as_bytes());
    let mut value = String::with_capacity(26);
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in digest {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 && value.len() < 26 {
            bits -= 5;
            value.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
        if value.len() == 26 {
            break;
        }
    }
    format!("{prefix}_{value}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::identifiers::timestamp_millis;
    use tempfile::tempdir;

    fn command(id: &str, action: CompositionAction) -> CompositionCommand {
        CompositionCommand {
            client_action_id: id.into(),
            actor: "user".into(),
            summary: id.into(),
            occurred_at: timestamp_millis(),
            action,
        }
    }
    fn initialize(root: &Path) {
        apply_command(
            root,
            "project_test",
            command(
                "initialize",
                CompositionAction::Initialize {
                    text: String::new(),
                    origin: RecordedOrigin::Unattested,
                },
            ),
        )
        .unwrap();
    }
    fn edit(
        root: &Path,
        id: &str,
        before: &str,
        after: &str,
        origin: RecordedOrigin,
        hint: Option<CompositionOperationKind>,
        ai: Option<AiAcceptanceContext>,
    ) {
        apply_command(
            root,
            "project_test",
            command(
                id,
                CompositionAction::Edit {
                    before_text: before.into(),
                    after_text: after.into(),
                    boundary: CompositionBoundary::Idle,
                    origin,
                    operation_kind_hint: hint,
                    ai_acceptance: ai,
                },
            ),
        )
        .unwrap();
    }

    #[test]
    fn replays_manual_paste_ai_revision_and_restoration_with_lineage() {
        let temp = tempdir().unwrap();
        initialize(temp.path());
        edit(
            temp.path(),
            "human",
            "",
            "Human",
            RecordedOrigin::RecordedDirectHumanInput,
            None,
            None,
        );
        edit(
            temp.path(),
            "paste",
            "Human",
            "Human source",
            RecordedOrigin::ImportedOrPasted,
            None,
            None,
        );
        let ai = AiAcceptanceContext {
            invocation_id: "invocation_one".into(),
            response_text: " AI unused".into(),
            accepted_ranges: vec![ScalarRange { start: 0, end: 3 }],
            rejected_ranges: vec![ScalarRange { start: 3, end: 10 }],
            partial: true,
        };
        edit(
            temp.path(),
            "ai",
            "Human source",
            "Human source AI",
            RecordedOrigin::AcceptedAiOutput,
            Some(CompositionOperationKind::AiAcceptance),
            Some(ai),
        );
        edit(
            temp.path(),
            "revise",
            "Human source AI",
            "Human source art",
            RecordedOrigin::RecordedDirectHumanInput,
            None,
            None,
        );
        edit(
            temp.path(),
            "restore",
            "Human source art",
            "Restored",
            RecordedOrigin::SystemRestoration,
            Some(CompositionOperationKind::Restoration),
            None,
        );
        let projection = reconstruct(temp.path(), "project_test").unwrap();
        assert_eq!(projection.manuscript, "Restored");
        assert_eq!(projection.operation_count, 5);
        assert!(projection
            .spans
            .iter()
            .all(|span| !span.lineage_reference_ids.is_empty()));
        assert_eq!(
            projection.spans[0].origin,
            RecordedOrigin::SystemRestoration
        );
    }

    #[test]
    fn unicode_scalar_diff_and_partial_ai_ranges_are_exact() {
        let temp = tempdir().unwrap();
        initialize(temp.path());
        edit(
            temp.path(),
            "emoji",
            "",
            "A🧵B",
            RecordedOrigin::RecordedDirectHumanInput,
            None,
            None,
        );
        let ai = AiAcceptanceContext {
            invocation_id: "invocation_emoji".into(),
            response_text: "é🦀tail".into(),
            accepted_ranges: vec![ScalarRange { start: 0, end: 2 }],
            rejected_ranges: vec![ScalarRange { start: 2, end: 6 }],
            partial: true,
        };
        edit(
            temp.path(),
            "partial",
            "A🧵B",
            "Aé🦀B",
            RecordedOrigin::AcceptedAiOutput,
            Some(CompositionOperationKind::AiAcceptance),
            Some(ai),
        );
        let projection = reconstruct(temp.path(), "project_test").unwrap();
        assert_eq!(projection.manuscript, "Aé🦀B");
        assert_eq!(
            projection
                .spans
                .iter()
                .map(|span| span.end - span.start)
                .sum::<usize>(),
            4
        );
        let events = ledger::read_all_events(&LedgerPaths::new(temp.path())).unwrap();
        assert!(events
            .iter()
            .flat_map(|event| &event.record_references)
            .any(|reference| reference.record_type == "ai-acceptance-disposition"));
    }

    #[test]
    fn refuses_a_stale_preimage_without_committing_an_event() {
        let temp = tempdir().unwrap();
        initialize(temp.path());
        let before = ledger::read_all_events(&LedgerPaths::new(temp.path()))
            .unwrap()
            .len();
        let result = apply_command(
            temp.path(),
            "project_test",
            command(
                "stale",
                CompositionAction::Edit {
                    before_text: "wrong".into(),
                    after_text: "new".into(),
                    boundary: CompositionBoundary::Idle,
                    origin: RecordedOrigin::RecordedDirectHumanInput,
                    operation_kind_hint: None,
                    ai_acceptance: None,
                },
            ),
        );
        assert_eq!(result.unwrap_err().code, "COMPOSITION_BASE_MISMATCH");
        assert_eq!(
            ledger::read_all_events(&LedgerPaths::new(temp.path()))
                .unwrap()
                .len(),
            before
        );
    }
    #[test]
    fn concurrent_edits_against_one_preimage_commit_exactly_once() {
        use std::sync::{Arc, Barrier};

        let temp = tempdir().unwrap();
        initialize(temp.path());
        let root = temp.path().to_path_buf();
        let barrier = Arc::new(Barrier::new(3));
        let handles = ["concurrent_a", "concurrent_b"].map(|id| {
            let root = root.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                apply_command(
                    &root,
                    "project_test",
                    command(
                        id,
                        CompositionAction::Edit {
                            before_text: String::new(),
                            after_text: id.chars().last().unwrap().to_string(),
                            boundary: CompositionBoundary::Idle,
                            origin: RecordedOrigin::RecordedDirectHumanInput,
                            operation_kind_hint: None,
                            ai_acceptance: None,
                        },
                    ),
                )
            })
        });
        barrier.wait();
        let results = handles.map(|handle| handle.join().unwrap());
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .filter(|error| error.code == "COMPOSITION_BASE_MISMATCH")
                .count(),
            1
        );
        assert_eq!(
            ledger::read_all_events(&LedgerPaths::new(temp.path()))
                .unwrap()
                .len(),
            2
        );
        assert!(matches!(
            reconstruct(temp.path(), "project_test")
                .unwrap()
                .manuscript
                .as_str(),
            "a" | "b"
        ));
    }
    #[test]
    fn retries_are_idempotent_and_action_id_reuse_conflicts() {
        let temp = tempdir().unwrap();
        let initialization = command(
            "retry_initialize",
            CompositionAction::Initialize {
                text: "base".into(),
                origin: RecordedOrigin::Unattested,
            },
        );
        assert!(
            !apply_command(temp.path(), "project_test", initialization.clone())
                .unwrap()
                .write
                .idempotent_replay
        );
        assert!(
            apply_command(temp.path(), "project_test", initialization)
                .unwrap()
                .write
                .idempotent_replay
        );

        let change = command(
            "retry_edit",
            CompositionAction::Edit {
                before_text: "base".into(),
                after_text: "based".into(),
                boundary: CompositionBoundary::ExplicitSave,
                origin: RecordedOrigin::RecordedDirectHumanInput,
                operation_kind_hint: None,
                ai_acceptance: None,
            },
        );
        assert!(
            !apply_command(temp.path(), "project_test", change.clone())
                .unwrap()
                .write
                .idempotent_replay
        );
        assert!(
            apply_command(temp.path(), "project_test", change)
                .unwrap()
                .write
                .idempotent_replay
        );
        assert_eq!(
            ledger::read_all_events(&LedgerPaths::new(temp.path()))
                .unwrap()
                .len(),
            2
        );

        let conflict = command(
            "retry_edit",
            CompositionAction::Edit {
                before_text: "based".into(),
                after_text: "different".into(),
                boundary: CompositionBoundary::Idle,
                origin: RecordedOrigin::RecordedDirectHumanInput,
                operation_kind_hint: None,
                ai_acceptance: None,
            },
        );
        assert_eq!(
            apply_command(temp.path(), "project_test", conflict)
                .unwrap_err()
                .code,
            "CPL_IDEMPOTENCY_CONFLICT"
        );
    }
    #[test]
    fn selection_and_arrangement_preserve_each_source_origin() {
        let temp = tempdir().unwrap();
        initialize(temp.path());
        edit(
            temp.path(),
            "move_human",
            "",
            "AB",
            RecordedOrigin::RecordedDirectHumanInput,
            None,
            None,
        );
        edit(
            temp.path(),
            "move_import",
            "AB",
            "ABC",
            RecordedOrigin::ImportedOrPasted,
            None,
            None,
        );
        edit(
            temp.path(),
            "move_arrange",
            "ABC",
            "CAB",
            RecordedOrigin::RecordedDirectHumanInput,
            Some(CompositionOperationKind::Move),
            None,
        );
        let projection = reconstruct(temp.path(), "project_test").unwrap();
        assert_eq!(projection.manuscript, "CAB");
        assert_eq!(projection.spans[0].text, "C");
        assert_eq!(projection.spans[0].origin, RecordedOrigin::ImportedOrPasted);
        assert_eq!(projection.spans[1].text, "AB");
        assert_eq!(
            projection.spans[1].origin,
            RecordedOrigin::RecordedDirectHumanInput
        );
        assert!(projection.spans.iter().all(|span| span
            .operation_ids
            .iter()
            .any(|id| id.starts_with("operation_"))));

        let events_before = ledger::read_all_events(&LedgerPaths::new(temp.path()))
            .unwrap()
            .len();
        let mismatch = apply_command(
            temp.path(),
            "project_test",
            command(
                "move_changes_content",
                CompositionAction::Edit {
                    before_text: "CAB".into(),
                    after_text: "CAD".into(),
                    boundary: CompositionBoundary::Idle,
                    origin: RecordedOrigin::RecordedDirectHumanInput,
                    operation_kind_hint: Some(CompositionOperationKind::Move),
                    ai_acceptance: None,
                },
            ),
        )
        .unwrap_err();
        assert_eq!(mismatch.code, "COMPOSITION_MOVE_CONTENT_MISMATCH");
        assert_eq!(
            ledger::read_all_events(&LedgerPaths::new(temp.path()))
                .unwrap()
                .len(),
            events_before
        );
    }

    #[test]
    fn ai_transformation_human_revision_and_deletion_keep_exact_lineage() {
        let temp = tempdir().unwrap();
        initialize(temp.path());
        edit(
            temp.path(),
            "transform_human",
            "",
            "Human phrase",
            RecordedOrigin::RecordedDirectHumanInput,
            None,
            None,
        );
        edit(
            temp.path(),
            "transform_ai",
            "Human phrase",
            "Machine phrase",
            RecordedOrigin::AcceptedAiOutput,
            Some(CompositionOperationKind::AiAcceptance),
            Some(AiAcceptanceContext {
                invocation_id: "invocation_transform".into(),
                response_text: "Machine phrase".into(),
                accepted_ranges: vec![ScalarRange { start: 0, end: 14 }],
                rejected_ranges: vec![],
                partial: false,
            }),
        );
        let ai_projection = reconstruct(temp.path(), "project_test").unwrap();
        assert!(ai_projection
            .spans
            .iter()
            .any(|span| span.origin == RecordedOrigin::AcceptedAiOutput));
        assert!(ai_projection
            .spans
            .iter()
            .any(|span| span.origin == RecordedOrigin::RecordedDirectHumanInput));
        assert!(ai_projection
            .spans
            .iter()
            .filter(|span| span.origin == RecordedOrigin::AcceptedAiOutput)
            .all(|span| span
                .lineage_reference_ids
                .iter()
                .any(|id| id == "invocation_transform")));

        edit(
            temp.path(),
            "transform_revise",
            "Machine phrase",
            "Reworked phrase",
            RecordedOrigin::RecordedDirectHumanInput,
            None,
            None,
        );
        edit(
            temp.path(),
            "transform_delete",
            "Reworked phrase",
            "Reworked",
            RecordedOrigin::RecordedDirectHumanInput,
            None,
            None,
        );
        let revised = reconstruct(temp.path(), "project_test").unwrap();
        assert_eq!(revised.manuscript, "Reworked");
        assert!(revised
            .spans
            .iter()
            .all(|span| span.origin == RecordedOrigin::RecordedDirectHumanInput));
        assert!(revised
            .spans
            .iter()
            .all(|span| !span.lineage_reference_ids.is_empty()));
    }
}
