use super::{canonical::canonical_digest, CplResult, CPL_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordInput {
    pub record_type: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CplRecord {
    pub schema_version: String,
    pub record_id: String,
    pub record_type: String,
    pub project_id: String,
    pub intent_id: String,
    pub client_action_id: String,
    pub created_at: String,
    pub payload: Value,
    pub record_sha256: String,
}
impl CplRecord {
    pub fn identity(&self) -> Value {
        json!({"schema_version":self.schema_version,"record_id":self.record_id,"record_type":self.record_type,"project_id":self.project_id,"intent_id":self.intent_id,"client_action_id":self.client_action_id,"created_at":self.created_at,"payload":self.payload})
    }
    pub fn verify_digest(&self) -> CplResult<bool> {
        Ok(canonical_digest(&self.identity())? == self.record_sha256)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordReference {
    pub record_id: String,
    pub record_type: String,
    pub path: String,
    pub record_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CplEvent {
    pub schema_version: String,
    pub event_id: String,
    pub project_id: String,
    pub event_sequence: u64,
    pub timestamp: String,
    pub event_type: String,
    pub actor: String,
    pub client_action_id: String,
    pub command_sha256: String,
    pub record_references: Vec<RecordReference>,
    pub metadata: Value,
    pub previous_event_sha256: Option<String>,
    pub event_sha256: String,
}
impl CplEvent {
    pub fn identity(&self) -> Value {
        json!({"schema_version":self.schema_version,"event_id":self.event_id,"project_id":self.project_id,"event_sequence":self.event_sequence,"timestamp":self.timestamp,"event_type":self.event_type,"actor":self.actor,"client_action_id":self.client_action_id,"command_sha256":self.command_sha256,"record_references":self.record_references,"metadata":self.metadata,"previous_event_sha256":self.previous_event_sha256})
    }
    pub fn verify_digest(&self) -> CplResult<bool> {
        Ok(canonical_digest(&self.identity())? == self.event_sha256)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChainHead {
    pub schema_version: String,
    pub project_id: String,
    pub segment_number: u64,
    pub segment_file: String,
    pub event_id: String,
    pub event_sequence: u64,
    pub event_sha256: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SegmentManifest {
    pub schema_version: String,
    pub segment_number: u64,
    pub previous_segment_file_sha256: Option<String>,
    pub first_event_sha256: String,
    pub final_event_sha256: String,
    pub first_event_sequence: u64,
    pub final_event_sequence: u64,
    pub event_count: u64,
    pub byte_length: u64,
    pub segment_file_sha256: String,
    pub sealed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteCommand {
    pub client_action_id: String,
    pub project_id: String,
    pub event_type: String,
    pub actor: String,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub records: Vec<RecordInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operational_state: Option<Value>,
}
impl WriteCommand {
    pub fn digest(&self) -> CplResult<String> {
        canonical_digest(&serde_json::to_value(self).map_err(|error| {
            super::CplError::new("CPL_COMMAND_INVALID", error.to_string(), false)
        })?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WriteResult {
    pub idempotent_replay: bool,
    pub intent_id: String,
    pub event: CplEvent,
    pub records: Vec<RecordReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VerificationStatus {
    Verified,
    VerifiedWithWarnings,
    Incomplete,
    Failed,
    Unsafe,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationFinding {
    pub code: String,
    pub severity: String,
    pub scope: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationReport {
    pub schema_version: String,
    pub report_id: String,
    pub project_id: String,
    pub status: VerificationStatus,
    pub verified_at: String,
    pub chain_head: Option<ChainHead>,
    pub event_count: u64,
    pub record_count: u64,
    pub findings: Vec<VerificationFinding>,
}
impl VerificationReport {
    pub fn empty(project_id: &str, report_id: String, verified_at: String) -> Self {
        Self {
            schema_version: CPL_SCHEMA_VERSION.to_owned(),
            report_id,
            project_id: project_id.to_owned(),
            status: VerificationStatus::Incomplete,
            verified_at,
            chain_head: None,
            event_count: 0,
            record_count: 0,
            findings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecoveryClassification {
    Clean,
    RecoverableAutomatically,
    RequiresUserConfirmation,
    IntegrityFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveryReport {
    pub classification: RecoveryClassification,
    pub actions: Vec<String>,
    pub quarantined_paths: Vec<String>,
    pub verification: VerificationReport,
}
