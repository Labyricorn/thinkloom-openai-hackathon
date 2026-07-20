use super::{
    assertions,
    canonical::{canonicalize, sha256_digest},
    identifiers::{sortable_id, timestamp_millis},
    ledger::{self, LedgerPaths},
    records::{
        CplRecord, SegmentManifest, VerificationFinding, VerificationReport, VerificationStatus,
    },
    writer::init_database,
    CplError, CplResult,
};
use rusqlite::OptionalExtension;
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path},
};

fn finding(
    code: &str,
    severity: &str,
    scope: impl Into<String>,
    message: impl Into<String>,
) -> VerificationFinding {
    VerificationFinding {
        code: code.to_owned(),
        severity: severity.to_owned(),
        scope: scope.into(),
        message: message.into(),
    }
}

fn validate_relative_path(path: &str) -> bool {
    if path.is_empty()
        || path.contains('\\')
        || path.contains(':')
        || path.chars().any(char::is_control)
    {
        return false;
    }
    Path::new(path)
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
}

pub fn verify_project(root: &Path, project_id: &str) -> CplResult<VerificationReport> {
    let report_id = sortable_id("report")?;
    let verified_at = timestamp_millis();
    let mut report = VerificationReport::empty(project_id, report_id, verified_at);
    let paths = LedgerPaths::new(root);
    if !paths.root.exists() {
        report.findings.push(finding(
            "CPL_LEDGER_MISSING",
            "ERROR",
            "ledger",
            "The native CPL ledger has not been initialized.",
        ));
        return Ok(report);
    }

    let mut errors = 0usize;
    let mut warnings = 0usize;
    let sealed = ledger::sealed_segments(&paths)?;
    let mut previous_segment_digest = None;
    for segment_path in &sealed {
        let number = segment_path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("segment-"))
            .and_then(|name| name.strip_suffix(".jsonl"))
            .and_then(|name| name.parse::<u64>().ok())
            .ok_or_else(|| {
                CplError::new(
                    "CPL_SEGMENT_NAME_INVALID",
                    "A sealed segment filename is invalid.",
                    false,
                )
            })?;
        let manifest_path = paths.sealed.join(ledger::segment_manifest_filename(number));
        let bytes = match fs::read(segment_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                errors += 1;
                report.findings.push(finding(
                    "CPL_SEALED_SEGMENT_UNREADABLE",
                    "CRITICAL",
                    segment_path.display().to_string(),
                    error.to_string(),
                ));
                continue;
            }
        };
        let manifest_bytes = match fs::read(&manifest_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                errors += 1;
                report.findings.push(finding(
                    "CPL_SEGMENT_MANIFEST_INVALID",
                    "CRITICAL",
                    manifest_path.display().to_string(),
                    error.to_string(),
                ));
                continue;
            }
        };
        let manifest: SegmentManifest = match serde_json::from_slice(&manifest_bytes) {
            Ok(manifest) => manifest,
            Err(error) => {
                errors += 1;
                report.findings.push(finding(
                    "CPL_SEGMENT_MANIFEST_INVALID",
                    "CRITICAL",
                    manifest_path.display().to_string(),
                    error.to_string(),
                ));
                continue;
            }
        };
        if canonicalize(&serde_json::to_value(&manifest).map_err(|error| {
            CplError::new("CPL_SERIALIZATION_FAILED", error.to_string(), false)
        })?)?
            != manifest_bytes
        {
            errors += 1;
            report.findings.push(finding(
                "CPL_SEGMENT_MANIFEST_NONCANONICAL",
                "CRITICAL",
                manifest_path.display().to_string(),
                "The sealed segment manifest is not canonical JSON.",
            ));
        }
        let events = match ledger::read_segment(segment_path) {
            Ok(events) => events,
            Err(error) => {
                errors += 1;
                report.findings.push(finding(
                    &error.code,
                    "CRITICAL",
                    segment_path.display().to_string(),
                    error.message,
                ));
                continue;
            }
        };
        let manifest_valid = !events.is_empty()
            && manifest.segment_number == number
            && manifest.previous_segment_file_sha256 == previous_segment_digest
            && manifest.first_event_sha256 == events.first().unwrap().event_sha256
            && manifest.final_event_sha256 == events.last().unwrap().event_sha256
            && manifest.first_event_sequence == events.first().unwrap().event_sequence
            && manifest.final_event_sequence == events.last().unwrap().event_sequence
            && manifest.event_count == events.len() as u64
            && manifest.byte_length == bytes.len() as u64
            && manifest.segment_file_sha256 == sha256_digest(&bytes);
        if !manifest_valid {
            errors += 1;
            report.findings.push(finding(
                "CPL_SEGMENT_MANIFEST_MISMATCH",
                "CRITICAL",
                manifest_path.display().to_string(),
                "The sealed manifest does not bind the exact segment bytes and event range.",
            ));
        }
        previous_segment_digest = Some(sha256_digest(&bytes));
    }

    let events = match ledger::read_all_events(&paths) {
        Ok(events) => events,
        Err(error) => {
            report.status = VerificationStatus::Failed;
            report
                .findings
                .push(finding(&error.code, "CRITICAL", "ledger", error.message));
            return Ok(report);
        }
    };
    report.event_count = events.len() as u64;
    let mut previous = None;
    let mut record_count = 0u64;
    let mut event_ids = HashSet::new();
    let mut action_ids = HashSet::new();
    let mut record_ids = HashSet::new();
    for (index, event) in events.iter().enumerate() {
        if !event_ids.insert(&event.event_id) || !action_ids.insert(&event.client_action_id) {
            errors += 1;
            report.findings.push(finding(
                "CPL_EVENT_ID_DUPLICATE",
                "CRITICAL",
                event.event_id.clone(),
                "Event IDs and client action IDs must each be unique across the ledger.",
            ));
        }
        let expected_sequence = index as u64 + 1;
        if event.project_id != project_id
            || event.event_sequence != expected_sequence
            || event.previous_event_sha256 != previous
            || !event.verify_digest()?
        {
            errors += 1;
            report.findings.push(finding("CPL_EVENT_CHAIN_INVALID", "CRITICAL", event.event_id.clone(), format!("Event sequence {} failed project, sequence, previous-link, or digest verification.", event.event_sequence)));
        }
        let parsed = chrono::DateTime::parse_from_rfc3339(&event.timestamp).ok();
        let timestamp_valid = parsed
            .map(|value| {
                value.offset().local_minus_utc() == 0
                    && value.timestamp_subsec_millis() * 1_000_000 == value.timestamp_subsec_nanos()
                    && event.timestamp.ends_with('Z')
                    && event.timestamp.len() == 24
            })
            .unwrap_or(false);
        if !timestamp_valid {
            errors += 1;
            report.findings.push(finding(
                "CPL_TIMESTAMP_INVALID",
                "ERROR",
                event.event_id.clone(),
                "The event timestamp is not exact RFC 3339 UTC millisecond form.",
            ));
        }
        for reference in &event.record_references {
            if !record_ids.insert(&reference.record_id) {
                errors += 1;
                report.findings.push(finding(
                    "CPL_RECORD_ID_DUPLICATE",
                    "CRITICAL",
                    reference.record_id.clone(),
                    "An immutable record ID is referenced more than once.",
                ));
            }
            record_count += 1;
            if !validate_relative_path(&reference.path) {
                errors += 1;
                report.findings.push(finding(
                    "CPL_RECORD_PATH_UNSAFE",
                    "CRITICAL",
                    reference.record_id.clone(),
                    "The record path is not a safe project-relative forward-slash path.",
                ));
                continue;
            }
            let path = root.join(reference.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    errors += 1;
                    report.findings.push(finding(
                        "CPL_RECORD_MISSING",
                        "CRITICAL",
                        reference.record_id.clone(),
                        error.to_string(),
                    ));
                    continue;
                }
            };
            let record: CplRecord = match serde_json::from_slice(&bytes) {
                Ok(record) => record,
                Err(error) => {
                    errors += 1;
                    report.findings.push(finding(
                        "CPL_RECORD_INVALID",
                        "CRITICAL",
                        reference.record_id.clone(),
                        error.to_string(),
                    ));
                    continue;
                }
            };
            let canonical = canonicalize(&serde_json::to_value(&record).map_err(|error| {
                CplError::new("CPL_SERIALIZATION_FAILED", error.to_string(), false)
            })?)?;
            if canonical != bytes
                || record.record_id != reference.record_id
                || record.record_type != reference.record_type
                || record.record_sha256 != reference.record_sha256
                || !record.verify_digest()?
            {
                errors += 1;
                report.findings.push(finding("CPL_RECORD_DIGEST_INVALID", "CRITICAL", reference.record_id.clone(), "The immutable record bytes, identity, type, or digest do not match the event reference."));
            }
            if let Err(message) = assertions::validate_record(&record) {
                errors += 1;
                report.findings.push(finding(
                    "CPL_ASSERTION_INVALID",
                    "ERROR",
                    reference.record_id.clone(),
                    message,
                ));
            }
        }
        previous = Some(event.event_sha256.clone());
    }
    report.record_count = record_count;

    let head = ledger::read_chain_head(&paths)?;
    report.chain_head = head.clone();
    let expected_head_location = if let Some(last) = events.last() {
        let number = ledger::locate_event_segment(&paths, &last.event_id)?;
        if let Some(number) = number {
            let active = ledger::active_segments(&paths)?.iter().any(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == ledger::segment_filename(number))
            });
            Some((
                number,
                format!(
                    "provenance/ledger/{}/{}",
                    if active { "active" } else { "sealed" },
                    ledger::segment_filename(number)
                ),
            ))
        } else {
            None
        }
    } else {
        None
    };
    match (events.last(), head.as_ref()) {
        (Some(last), Some(head))
            if head.project_id == project_id
                && head.event_id == last.event_id
                && head.event_sequence == last.event_sequence
                && head.event_sha256 == last.event_sha256
                && expected_head_location
                    .as_ref()
                    .is_some_and(|(number, path)| {
                        head.segment_number == *number && head.segment_file == *path
                    }) => {}
        (None, None) => report.findings.push(finding(
            "CPL_LEDGER_EMPTY",
            "WARNING",
            "ledger",
            "The ledger is initialized but contains no events.",
        )),
        _ => {
            errors += 1;
            report.findings.push(finding(
                "CPL_CHAIN_HEAD_MISMATCH",
                "CRITICAL",
                "chain-head",
                "The chain head does not bind the final readable event.",
            ));
        }
    }

    if let Ok(database) = init_database(root) {
        let indexed: Option<u64> = database
            .query_row("SELECT MAX(event_sequence) FROM cpl_events", [], |row| {
                row.get(0)
            })
            .optional()
            .unwrap_or(None)
            .flatten();
        let authoritative = events.last().map(|event| event.event_sequence);
        if indexed != authoritative {
            warnings += 1;
            report.findings.push(finding(
                "CPL_DERIVED_INDEX_STALE",
                "WARNING",
                "sqlite",
                "The rebuildable SQLite event index is stale.",
            ));
        }
    } else {
        warnings += 1;
        report.findings.push(finding(
            "CPL_DERIVED_INDEX_UNAVAILABLE",
            "WARNING",
            "sqlite",
            "The rebuildable SQLite index could not be inspected.",
        ));
    }

    report.status = if errors > 0 {
        VerificationStatus::Failed
    } else if events.is_empty() {
        VerificationStatus::Incomplete
    } else if warnings > 0 {
        VerificationStatus::VerifiedWithWarnings
    } else {
        VerificationStatus::Verified
    };
    if errors == 0 && !events.is_empty() {
        report.findings.push(finding("CPL_INTEGRITY_VERIFIED", "INFO", "ledger", format!("Verified {} contiguous events and {} immutable record references against the retained chain head.", events.len(), record_count)));
    }
    Ok(report)
}
/// Enforces the release boundary against the authoritative native report.
/// Warnings remain visible but do not make an otherwise verified chain unsafe
/// to release. Every other terminal status blocks before release state changes.
pub fn require_release_verification(report: &VerificationReport) -> CplResult<()> {
    match report.status {
        VerificationStatus::Verified | VerificationStatus::VerifiedWithWarnings => Ok(()),
        VerificationStatus::Incomplete
        | VerificationStatus::Failed
        | VerificationStatus::Unsafe => Err(CplError::new(
            "RELEASE_VERIFICATION_BLOCKED",
            format!(
                "Native CPL verification status {:?} does not permit release finalization.",
                report.status
            ),
            true,
        )),
    }
}

#[cfg(test)]
mod release_gate_tests {
    use super::*;

    fn report(status: VerificationStatus) -> VerificationReport {
        let mut report = VerificationReport::empty(
            "project_test",
            "report_test".into(),
            "2026-07-19T00:00:00.000Z".into(),
        );
        report.status = status;
        report
    }

    #[test]
    fn release_gate_accepts_only_complete_safe_native_verification() {
        for status in [
            VerificationStatus::Verified,
            VerificationStatus::VerifiedWithWarnings,
        ] {
            require_release_verification(&report(status)).unwrap();
        }
        for status in [
            VerificationStatus::Incomplete,
            VerificationStatus::Failed,
            VerificationStatus::Unsafe,
        ] {
            let error = require_release_verification(&report(status)).unwrap_err();
            assert_eq!(error.code, "RELEASE_VERIFICATION_BLOCKED");
        }
    }
}
