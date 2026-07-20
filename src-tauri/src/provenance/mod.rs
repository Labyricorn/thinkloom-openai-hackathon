//! Native Composition Provenance Ledger service.
//!
//! This is the only module allowed to create authoritative records, event
//! digests, ledger entries, chain heads, or native verification results.

pub mod assertions;
pub mod canonical;
pub mod composition;
pub mod contribution_map;
pub mod explorer;
pub mod export;
pub mod harp;
pub mod identifiers;
pub mod ledger;
pub mod phase1;
pub mod projections;
pub mod records;
pub mod recovery;
pub mod verifier;
pub mod writer;

use serde::Serialize;
use std::{error::Error, fmt};

pub use records::{
    ChainHead, CplEvent, CplRecord, RecordInput, RecoveryClassification, RecoveryReport,
    VerificationFinding, VerificationReport, VerificationStatus, WriteCommand, WriteResult,
};
pub use recovery::recover_project;
pub use verifier::verify_project;
pub use writer::{CplService, WriterConfig};

pub const CPL_SCHEMA_VERSION: &str = "1.0";
pub type CplResult<T> = Result<T, CplError>;

#[derive(Debug, Clone, Serialize)]
pub struct CplError {
    pub code: String,
    pub message: String,
    pub recoverable: bool,
}

impl CplError {
    pub fn new(code: &str, message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
            recoverable,
        }
    }
    pub fn io(context: &str, error: impl fmt::Display) -> Self {
        Self::new("CPL_IO_ERROR", format!("{context}: {error}"), true)
    }
}

impl fmt::Display for CplError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}
impl Error for CplError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurableBoundary {
    IntentPrepared,
    FirstRecordStaged,
    RecordFlushed,
    RecordMoved,
    RecordDirectorySynced,
    SegmentFlushed,
    SegmentManifestFlushed,
    SegmentMoved,
    SegmentManifestMoved,
    NewActiveSegmentCreated,
    LedgerAppendBeforeFlush,
    LedgerFlushed,
    ChainHeadTemporaryWritten,
    ChainHeadReplaced,
    ChainHeadDirectorySynced,
    SqliteApplied,
    Complete,
}

#[cfg(test)]
pub(crate) fn injected_failure(boundary: DurableBoundary) -> CplError {
    CplError::new(
        "CPL_INJECTED_FAILURE",
        format!("Injected termination after {boundary:?}"),
        true,
    )
}
#[cfg(test)]
mod tests;
