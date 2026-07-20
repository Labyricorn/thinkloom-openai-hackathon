use super::{
    canonical::sha256_digest,
    ledger::{self, atomic_replace, LedgerPaths},
    records::{
        CplEvent, RecoveryClassification, RecoveryReport, SegmentManifest, VerificationStatus,
    },
    verifier,
    writer::{init_database, rebuild_indexes, set_intent_phase, ProjectWriterLock},
    CplError, CplResult,
};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};

pub fn recover_project(root: &Path, project_id: &str) -> CplResult<RecoveryReport> {
    let paths = LedgerPaths::new(root);
    paths.initialize()?;
    let _lock = ProjectWriterLock::acquire(root)?;
    recover_locked(root, project_id)
}

pub(crate) fn recover_locked(root: &Path, project_id: &str) -> CplResult<RecoveryReport> {
    let paths = LedgerPaths::new(root);
    paths.initialize()?;
    let mut actions = Vec::new();
    let mut quarantined_paths = Vec::new();
    let database = recover_database(root, &mut actions, &mut quarantined_paths)?;
    let mut repaired = !actions.is_empty();

    for entry in fs::read_dir(&paths.active)
        .map_err(|error| CplError::io("Could not inspect interrupted segment rotation", error))?
        .filter_map(Result::ok)
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(".segment-") || !name.ends_with(".manifest.json.tmp") {
            continue;
        }
        let manifest: SegmentManifest =
            serde_json::from_slice(&fs::read(entry.path()).map_err(|error| {
                CplError::io("Could not read an interrupted segment manifest", error)
            })?)
            .map_err(|error| {
                CplError::new("CPL_SEGMENT_MANIFEST_INVALID", error.to_string(), false)
            })?;
        let sealed_segment = paths
            .sealed
            .join(ledger::segment_filename(manifest.segment_number));
        let sealed_manifest = paths
            .sealed
            .join(ledger::segment_manifest_filename(manifest.segment_number));
        if sealed_segment.exists() && !sealed_manifest.exists() {
            let bytes = fs::read(&sealed_segment).map_err(|error| {
                CplError::io("Could not verify an interrupted sealed segment", error)
            })?;
            if sha256_digest(&bytes) != manifest.segment_file_sha256 {
                return Err(CplError::new(
                    "CPL_SEGMENT_MANIFEST_MISMATCH",
                    "Interrupted segment rotation has a manifest/segment digest contradiction.",
                    false,
                ));
            }
            atomic_replace(&entry.path(), &sealed_manifest)?;
            actions.push(format!(
                "Completed the durable manifest move for sealed segment {}.",
                manifest.segment_number
            ));
            repaired = true;
        }
    }

    let head_before = ledger::read_chain_head(&paths)?;
    let (_, active) = ledger::current_active_segment(&paths)?;
    let active_bytes = fs::read(&active).map_err(|error| {
        CplError::io(
            "Could not inspect the active segment during recovery",
            error,
        )
    })?;
    if !active_bytes.is_empty() && !active_bytes.ends_with(b"\n") {
        let complete_end = active_bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        let complete_events = parse_complete_prefix(&active_bytes[..complete_end])?;
        let readable_ids = ledger::sealed_segments(&paths)?
            .into_iter()
            .map(|path| ledger::read_segment(&path))
            .collect::<CplResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .chain(complete_events.iter().cloned())
            .map(|event| event.event_id)
            .collect::<Vec<_>>();
        if head_before
            .as_ref()
            .is_some_and(|head| !readable_ids.iter().any(|id| id == &head.event_id))
        {
            let mut verification = verifier::verify_project(root, project_id)?;
            verification.status = VerificationStatus::Failed;
            return Ok(RecoveryReport {
                classification: RecoveryClassification::IntegrityFailure,
                actions: vec![
                    "Refused to truncate an active suffix referenced by the chain head.".to_owned(),
                ],
                quarantined_paths,
                verification,
            });
        }
        let file = OpenOptions::new()
            .write(true)
            .open(&active)
            .map_err(|error| CplError::io("Could not open the truncated active segment", error))?;
        file.set_len(complete_end as u64)
            .and_then(|_| file.sync_all())
            .map_err(|error| {
                CplError::io(
                    "Could not remove an uncommitted partial JSONL suffix",
                    error,
                )
            })?;
        actions.push("Removed an incomplete active-segment suffix that was not referenced by the chain head.".to_owned());
        repaired = true;
    }

    let events = ledger::read_all_events(&paths)?;
    if let Err(message) = validate_event_chain(&events, project_id) {
        let mut verification = verifier::verify_project(root, project_id)?;
        verification.status = VerificationStatus::Failed;
        verification.findings.push(super::VerificationFinding {
            code: "CPL_RECOVERY_CHAIN_CONTRADICTION".to_owned(),
            severity: "CRITICAL".to_owned(),
            scope: "recovery".to_owned(),
            message,
        });
        return Ok(RecoveryReport {
            classification: RecoveryClassification::IntegrityFailure,
            actions,
            quarantined_paths,
            verification,
        });
    }

    match (head_before.as_ref(), events.last()) {
        (Some(head), Some(last))
            if head.event_id == last.event_id && head.event_sha256 == last.event_sha256 =>
        {
            let number =
                ledger::locate_event_segment(&paths, &last.event_id)?.ok_or_else(|| {
                    CplError::new(
                        "CPL_EVENT_NOT_FOUND",
                        "The head event has no segment.",
                        false,
                    )
                })?;
            let active = ledger::active_segments(&paths)?.iter().any(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == ledger::segment_filename(number))
            });
            let expected = format!(
                "provenance/ledger/{}/{}",
                if active { "active" } else { "sealed" },
                ledger::segment_filename(number)
            );
            if head.segment_number != number || head.segment_file != expected {
                ledger::advance_chain_head(&paths, project_id, last)?;
                actions.push(
                    "Rebound the chain head to the recovered segment location after rotation."
                        .to_owned(),
                );
                repaired = true;
            }
        }
        (Some(head), Some(last))
            if events.iter().any(|event| {
                event.event_id == head.event_id && event.event_sha256 == head.event_sha256
            }) =>
        {
            ledger::advance_chain_head(&paths, project_id, last)?;
            actions.push(format!(
                "Advanced the chain head from sequence {} to durable sequence {}.",
                head.event_sequence, last.event_sequence
            ));
            repaired = true;
        }
        (None, Some(last)) => {
            ledger::advance_chain_head(&paths, project_id, last)?;
            actions.push(format!(
                "Reconstructed the missing chain head at sequence {}.",
                last.event_sequence
            ));
            repaired = true;
        }
        (Some(_), None) => {
            let mut verification = verifier::verify_project(root, project_id)?;
            verification.status = VerificationStatus::Failed;
            return Ok(RecoveryReport {
                classification: RecoveryClassification::IntegrityFailure,
                actions: vec![
                    "The chain head is ahead of the readable ledger; no event was invented."
                        .to_owned(),
                ],
                quarantined_paths,
                verification,
            });
        }
        (Some(_), Some(_)) => {
            let mut verification = verifier::verify_project(root, project_id)?;
            verification.status = VerificationStatus::Failed;
            return Ok(RecoveryReport { classification: RecoveryClassification::IntegrityFailure, actions: vec!["The chain head does not occur in the readable ledger; automatic recovery was refused.".to_owned()], quarantined_paths, verification });
        }
        (None, None) => {}
    }

    let mut statement = database.prepare("SELECT intent_id,client_action_id,phase,record_paths_json FROM write_intents WHERE phase NOT IN ('COMPLETE','FAILED','QUARANTINED') ORDER BY intent_id")
        .map_err(super::writer::database_error)?;
    let intents = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(super::writer::database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(super::writer::database_error)?;
    drop(statement);

    for (intent_id, client_action_id, phase, references_json) in intents {
        if events
            .iter()
            .any(|event| event.client_action_id == client_action_id)
        {
            set_intent_phase(&database, &intent_id, "COMPLETE")?;
            actions.push(format!(
                "Replayed committed action {client_action_id} into SQLite."
            ));
            repaired = true;
            continue;
        }
        let references: Vec<super::records::RecordReference> =
            serde_json::from_str(&references_json)
                .map_err(|error| CplError::new("CPL_INTENT_INVALID", error.to_string(), false))?;
        let quarantine = root.join(".app/recovery/orphans").join(&intent_id);
        fs::create_dir_all(&quarantine)
            .map_err(|error| CplError::io("Could not create the recovery quarantine", error))?;
        let mut moved = false;
        for reference in references {
            let source = root.join(reference.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if source.exists() {
                let destination =
                    unique_destination(&quarantine, source.file_name().unwrap_or_default());
                fs::rename(&source, &destination).map_err(|error| {
                    CplError::io(
                        "Could not quarantine an uncommitted immutable record",
                        error,
                    )
                })?;
                quarantined_paths.push(relative_display(root, &destination));
                moved = true;
            }
        }
        let staging = root.join(".app/temp/staging").join(&intent_id);
        if staging.exists() {
            for entry in fs::read_dir(&staging)
                .map_err(|error| CplError::io("Could not inspect abandoned CPL staging", error))?
                .filter_map(Result::ok)
            {
                let destination = unique_destination(&quarantine, &entry.file_name());
                fs::rename(entry.path(), &destination).map_err(|error| {
                    CplError::io("Could not quarantine abandoned CPL staging", error)
                })?;
                quarantined_paths.push(relative_display(root, &destination));
                moved = true;
            }
            let _ = fs::remove_dir(&staging);
        }
        if moved {
            set_intent_phase(&database, &intent_id, "QUARANTINED")?;
            actions.push(format!(
                "Quarantined uncommitted files for {client_action_id} from phase {phase}."
            ));
        } else {
            set_intent_phase(&database, &intent_id, "FAILED")?;
            actions.push(format!(
                "Closed abandoned intent {intent_id} with no durable authoritative files."
            ));
        }
        repaired = true;
    }

    rebuild_indexes(root, &events)?;
    let verification = verifier::verify_project(root, project_id)?;
    let classification = if matches!(
        verification.status,
        VerificationStatus::Failed | VerificationStatus::Unsafe
    ) {
        RecoveryClassification::IntegrityFailure
    } else if repaired {
        RecoveryClassification::RecoverableAutomatically
    } else {
        RecoveryClassification::Clean
    };
    Ok(RecoveryReport {
        classification,
        actions,
        quarantined_paths,
        verification,
    })
}

fn recover_database(
    root: &Path,
    actions: &mut Vec<String>,
    quarantined_paths: &mut Vec<String>,
) -> CplResult<rusqlite::Connection> {
    match init_database(root) {
        Ok(database) => Ok(database),
        Err(original) => {
            let database_path = root.join(".app/state.sqlite");
            if !database_path.exists() {
                return Err(original);
            }
            let quarantine = root.join(".app/recovery/orphans").join(format!(
                "sqlite-{}",
                super::identifiers::sortable_id("recovery")?
            ));
            fs::create_dir_all(&quarantine).map_err(|error| {
                CplError::io("Could not create corrupt-SQLite quarantine", error)
            })?;
            for suffix in ["", "-wal", "-shm"] {
                let source = PathBuf::from(format!("{}{}", database_path.display(), suffix));
                if source.exists() {
                    let name = source.file_name().unwrap_or_default();
                    let destination = quarantine.join(name);
                    fs::rename(&source, &destination).map_err(|error| {
                        CplError::io("Could not quarantine corrupt SQLite state", error)
                    })?;
                    quarantined_paths.push(relative_display(root, &destination));
                }
            }
            actions.push(format!("Quarantined unreadable SQLite state after {} and prepared a rebuild from the authoritative ledger.", original.message));
            init_database(root)
        }
    }
}

fn parse_complete_prefix(bytes: &[u8]) -> CplResult<Vec<CplEvent>> {
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            serde_json::from_slice(line)
                .map_err(|error| CplError::new("CPL_EVENT_INVALID", error.to_string(), false))
        })
        .collect()
}

fn validate_event_chain(events: &[CplEvent], project_id: &str) -> Result<(), String> {
    let mut previous = None;
    for (index, event) in events.iter().enumerate() {
        if event.project_id != project_id
            || event.event_sequence != index as u64 + 1
            || event.previous_event_sha256 != previous
            || !event.verify_digest().map_err(|error| error.message)?
        {
            return Err(format!(
                "Authoritative event {} contradicts the expected contiguous chain.",
                event.event_id
            ));
        }
        previous = Some(event.event_sha256.clone());
    }
    Ok(())
}

fn unique_destination(directory: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let direct = directory.join(name);
    if !direct.exists() {
        return direct;
    }
    let stem = Path::new(name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("orphan");
    let extension = Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("bin");
    (1..)
        .map(|index| directory.join(format!("{stem}-{index}.{extension}")))
        .find(|path| !path.exists())
        .unwrap()
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
