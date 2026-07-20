use super::{
    ledger::{self, LedgerConfig, LedgerPaths},
    writer::{init_database, CplService, WriterConfig},
    DurableBoundary, RecordInput, RecoveryClassification, VerificationStatus, WriteCommand,
};
use serde_json::json;
use std::sync::Arc;

fn command(project_id: &str, action: &str, value: i64) -> WriteCommand {
    WriteCommand {
        client_action_id: action.to_owned(),
        project_id: project_id.to_owned(),
        event_type: "TEST_ACTION".to_owned(),
        actor: "system".to_owned(),
        metadata: json!({"value": value}),
        records: vec![RecordInput {
            record_type: "test-record".to_owned(),
            payload: json!({"value": value}),
        }],
        operational_state: Some(json!({"value": value})),
    }
}

#[test]
fn retries_are_idempotent_and_conflicts_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let service = CplService::new(temp.path(), "project_test");
    let first = service
        .write(command("project_test", "client_action_1", 1))
        .unwrap();
    let replay = service
        .write(command("project_test", "client_action_1", 1))
        .unwrap();
    assert_eq!(first.event.event_id, replay.event.event_id);
    assert!(replay.idempotent_replay);
    assert_eq!(
        ledger::read_all_events(&LedgerPaths::new(temp.path()))
            .unwrap()
            .len(),
        1
    );
    let conflict = service
        .write(command("project_test", "client_action_1", 2))
        .unwrap_err();
    assert_eq!(conflict.code, "CPL_IDEMPOTENCY_CONFLICT");
}

#[test]
fn os_writer_lock_serializes_concurrent_actions() {
    let temp = tempfile::tempdir().unwrap();
    let root = Arc::new(temp.path().to_path_buf());
    std::thread::scope(|scope| {
        for index in 0..8 {
            let root = Arc::clone(&root);
            scope.spawn(move || {
                CplService::new(root.as_path(), "project_test")
                    .write(command(
                        "project_test",
                        &format!("client_action_{index}"),
                        index,
                    ))
                    .unwrap();
            });
        }
    });
    let events = ledger::read_all_events(&LedgerPaths::new(temp.path())).unwrap();
    assert_eq!(events.len(), 8);
    assert_eq!(
        events
            .iter()
            .map(|event| event.event_sequence)
            .collect::<Vec<_>>(),
        (1..=8).collect::<Vec<_>>()
    );
    assert_eq!(
        CplService::new(temp.path(), "project_test")
            .verify()
            .unwrap()
            .status,
        VerificationStatus::Verified
    );
}

#[test]
fn rotates_and_verifies_cross_segment_linkage() {
    let temp = tempfile::tempdir().unwrap();
    let service = CplService::with_config(
        temp.path(),
        "project_test",
        WriterConfig {
            ledger: LedgerConfig {
                max_events_per_segment: 2,
                max_bytes_per_segment: u64::MAX,
            },
        },
    );
    for index in 0..5 {
        service
            .write(command(
                "project_test",
                &format!("rotation_action_{index}"),
                index,
            ))
            .unwrap();
    }
    let paths = LedgerPaths::new(temp.path());
    assert_eq!(ledger::sealed_segments(&paths).unwrap().len(), 2);
    assert_eq!(ledger::active_segments(&paths).unwrap().len(), 1);
    assert_eq!(
        service.verify().unwrap().status,
        VerificationStatus::Verified
    );
}

#[test]
fn every_durable_boundary_recovers_and_retries_safely() {
    let boundaries = [
        DurableBoundary::IntentPrepared,
        DurableBoundary::FirstRecordStaged,
        DurableBoundary::RecordFlushed,
        DurableBoundary::RecordMoved,
        DurableBoundary::RecordDirectorySynced,
        DurableBoundary::LedgerAppendBeforeFlush,
        DurableBoundary::LedgerFlushed,
        DurableBoundary::ChainHeadTemporaryWritten,
        DurableBoundary::ChainHeadReplaced,
        DurableBoundary::ChainHeadDirectorySynced,
        DurableBoundary::SqliteApplied,
        DurableBoundary::Complete,
    ];
    for boundary in boundaries {
        let temp = tempfile::tempdir().unwrap();
        let service = CplService::new(temp.path(), "project_test");
        let action = format!("fault_{boundary:?}");
        let failure = service
            .write_with_failure(command("project_test", &action, 7), boundary)
            .unwrap_err();
        assert_eq!(failure.code, "CPL_INJECTED_FAILURE", "{boundary:?}");
        let recovery = service.recover().unwrap();
        assert!(
            !matches!(
                recovery.classification,
                RecoveryClassification::IntegrityFailure
            ),
            "{boundary:?}: {:?}",
            recovery.actions
        );
        let result = service.write(command("project_test", &action, 7)).unwrap();
        assert_eq!(result.event.event_sequence, 1, "{boundary:?}");
        let report = service.verify().unwrap();
        assert_eq!(
            report.status,
            VerificationStatus::Verified,
            "{boundary:?}: {:?}",
            report.findings
        );
        assert_eq!(report.event_count, 1, "{boundary:?}");
    }
}

#[test]
fn recovery_rebuilds_sqlite_from_authoritative_events() {
    let temp = tempfile::tempdir().unwrap();
    let service = CplService::new(temp.path(), "project_test");
    service
        .write(command("project_test", "rebuild_action", 4))
        .unwrap();
    let database = init_database(temp.path()).unwrap();
    database.execute("DELETE FROM cpl_events", []).unwrap();
    database.execute("DELETE FROM cpl_records", []).unwrap();
    database
        .execute("DELETE FROM cpl_action_receipts", [])
        .unwrap();
    drop(database);
    let recovery = service.recover().unwrap();
    assert!(!matches!(
        recovery.classification,
        RecoveryClassification::IntegrityFailure
    ));
    assert_eq!(
        service
            .write(command("project_test", "rebuild_action", 4))
            .unwrap()
            .event
            .event_sequence,
        1
    );
    assert_eq!(
        service.verify().unwrap().status,
        VerificationStatus::Verified
    );
}

#[test]
fn every_segment_rotation_boundary_recovers_without_sequence_loss() {
    let boundaries = [
        DurableBoundary::SegmentFlushed,
        DurableBoundary::SegmentManifestFlushed,
        DurableBoundary::SegmentMoved,
        DurableBoundary::SegmentManifestMoved,
        DurableBoundary::NewActiveSegmentCreated,
    ];
    for boundary in boundaries {
        let temp = tempfile::tempdir().unwrap();
        let service = CplService::with_config(
            temp.path(),
            "project_test",
            WriterConfig {
                ledger: LedgerConfig {
                    max_events_per_segment: 1,
                    max_bytes_per_segment: u64::MAX,
                },
            },
        );
        service
            .write(command("project_test", "rotation_seed", 1))
            .unwrap();
        let failure = service
            .write_with_failure(command("project_test", "rotation_retry", 2), boundary)
            .unwrap_err();
        assert_eq!(failure.code, "CPL_INJECTED_FAILURE", "{boundary:?}");
        let recovery = service.recover().unwrap();
        assert!(
            !matches!(
                recovery.classification,
                RecoveryClassification::IntegrityFailure
            ),
            "{boundary:?}: {:?}",
            recovery.actions
        );
        service
            .write(command("project_test", "rotation_retry", 2))
            .unwrap();
        let events = ledger::read_all_events(&LedgerPaths::new(temp.path())).unwrap();
        assert_eq!(
            events
                .iter()
                .map(|event| event.event_sequence)
                .collect::<Vec<_>>(),
            vec![1, 2],
            "{boundary:?}"
        );
        assert_eq!(
            service.verify().unwrap().status,
            VerificationStatus::Verified,
            "{boundary:?}"
        );
    }
}

#[test]
fn corrupt_sqlite_is_quarantined_and_rebuilt_from_canonical_records() {
    let temp = tempfile::tempdir().unwrap();
    let service = CplService::new(temp.path(), "project_test");
    let mut rebuild_command = command("project_test", "sqlite_rebuild", 9);
    rebuild_command.records[0].record_type = "application-state-snapshot".to_owned();
    service.write(rebuild_command).unwrap();
    std::fs::write(
        temp.path().join(".app/state.sqlite"),
        b"not a sqlite database",
    )
    .unwrap();
    let recovery = service.recover().unwrap();
    assert_eq!(
        recovery.classification,
        RecoveryClassification::RecoverableAutomatically
    );
    assert!(recovery
        .actions
        .iter()
        .any(|action| action.contains("Quarantined unreadable SQLite")));
    assert!(!recovery.quarantined_paths.is_empty());
    let mut replay_command = command("project_test", "sqlite_rebuild", 9);
    replay_command.records[0].record_type = "application-state-snapshot".to_owned();
    let replay = service.write(replay_command).unwrap();
    assert!(replay.idempotent_replay);
    assert!(!replay.intent_id.is_empty());
    let database = init_database(temp.path()).unwrap();
    let state: String = database
        .query_row("SELECT json FROM project_state WHERE id=1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&state).unwrap(),
        json!({"value": 9})
    );
    assert_eq!(
        service.verify().unwrap().status,
        VerificationStatus::Verified
    );
}

#[test]
fn stale_lock_file_artifact_does_not_block_the_os_managed_lock() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".app")).unwrap();
    std::fs::write(
        temp.path().join(".app/cpl.writer.lock"),
        b"stale process metadata",
    )
    .unwrap();
    let service = CplService::new(temp.path(), "project_test");
    service
        .write(command("project_test", "stale_lock_action", 1))
        .unwrap();
    assert_eq!(
        service.verify().unwrap().status,
        VerificationStatus::Verified
    );
}
