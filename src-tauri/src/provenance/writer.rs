use super::{
    canonical::{canonical_digest, canonicalize},
    identifiers::{sortable_id, timestamp_millis},
    ledger::{self, atomic_replace, sync_directory, LedgerConfig, LedgerPaths},
    records::{
        CplEvent, CplRecord, RecordReference, VerificationReport, WriteCommand, WriteResult,
    },
    recovery, verifier, CplError, CplResult, DurableBoundary, CPL_SCHEMA_VERSION,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy)]
pub struct WriterConfig {
    pub ledger: LedgerConfig,
}

impl Default for WriterConfig {
    fn default() -> Self {
        Self {
            ledger: LedgerConfig::default(),
        }
    }
}

pub struct CplService {
    root: PathBuf,
    project_id: String,
    config: WriterConfig,
}

impl CplService {
    pub fn new(root: impl Into<PathBuf>, project_id: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            project_id: project_id.into(),
            config: WriterConfig::default(),
        }
    }

    #[cfg(test)]
    pub fn with_config(
        root: impl Into<PathBuf>,
        project_id: impl Into<String>,
        config: WriterConfig,
    ) -> Self {
        Self {
            root: root.into(),
            project_id: project_id.into(),
            config,
        }
    }

    pub fn initialize(&self) -> CplResult<()> {
        LedgerPaths::new(&self.root).initialize()?;
        fs::create_dir_all(self.root.join("records"))
            .and_then(|_| fs::create_dir_all(self.root.join(".app/recovery/orphans")))
            .and_then(|_| fs::create_dir_all(self.root.join(".app/temp/staging")))
            .map_err(|error| CplError::io("Could not initialize native CPL storage", error))?;
        Ok(())
    }

    pub fn write(&self, command: WriteCommand) -> CplResult<WriteResult> {
        self.write_with_boundary(command, &mut |_| Ok(()))
    }

    pub fn verify(&self) -> CplResult<VerificationReport> {
        verifier::verify_project(&self.root, &self.project_id)
    }

    pub fn recover(&self) -> CplResult<super::RecoveryReport> {
        recovery::recover_project(&self.root, &self.project_id)
    }

    fn write_with_boundary(
        &self,
        command: WriteCommand,
        boundary: &mut dyn FnMut(DurableBoundary) -> CplResult<()>,
    ) -> CplResult<WriteResult> {
        let _lock = ProjectWriterLock::acquire(&self.root)?;
        self.initialize()?;
        recovery::recover_locked(&self.root, &self.project_id)?;
        self.write_locked(command, boundary)
    }

    pub(crate) fn write_prepared(
        &self,
        client_action_id: &str,
        prepare: impl FnOnce() -> CplResult<Option<WriteCommand>>,
    ) -> CplResult<WriteResult> {
        let _lock = ProjectWriterLock::acquire(&self.root)?;
        self.initialize()?;
        recovery::recover_locked(&self.root, &self.project_id)?;
        if let Some(command) = prepare()? {
            return self.write_locked(command, &mut |_| Ok(()));
        }
        let database = init_database(&self.root)?;
        let result_json = database
            .query_row(
                "SELECT result_json FROM cpl_action_receipts WHERE client_action_id=?1",
                params![client_action_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(database_error)?
            .ok_or_else(|| {
                CplError::new(
                    "CPL_RECEIPT_MISSING",
                    "The committed composition action has no recoverable receipt.",
                    false,
                )
            })?;
        let mut result: WriteResult = serde_json::from_str(&result_json)
            .map_err(|error| CplError::new("CPL_RECEIPT_INVALID", error.to_string(), false))?;
        result.idempotent_replay = true;
        Ok(result)
    }

    fn write_locked(
        &self,
        command: WriteCommand,
        boundary: &mut dyn FnMut(DurableBoundary) -> CplResult<()>,
    ) -> CplResult<WriteResult> {
        validate_command(&command, &self.project_id)?;
        let command_sha256 = command.digest()?;
        let mut database = init_database(&self.root)?;

        if let Some((stored_digest, stored_result)) = database
            .query_row(
                "SELECT command_sha256, result_json FROM cpl_action_receipts WHERE client_action_id=?1",
                params![command.client_action_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(database_error)?
        {
            if stored_digest != command_sha256 {
                return Err(CplError::new(
                    "CPL_IDEMPOTENCY_CONFLICT",
                    "The client_action_id was already committed with a different canonical command.",
                    false,
                ));
            }
            let mut result: WriteResult = serde_json::from_str(&stored_result).map_err(|error| {
                CplError::new("CPL_RECEIPT_INVALID", error.to_string(), false)
            })?;
            result.idempotent_replay = true;
            return Ok(result);
        }

        let intent_id = sortable_id("intent")?;
        let event_id = sortable_id("event")?;
        let created_at = timestamp_millis();
        let (records, references) = prepare_records(&command, &intent_id, &created_at)?;
        let paths_json = serde_json::to_string(&references)
            .map_err(|error| CplError::new("CPL_SERIALIZATION_FAILED", error.to_string(), false))?;
        database
            .execute(
                "INSERT INTO write_intents(intent_id,client_action_id,command_sha256,phase,record_paths_json,event_id,updated_at) VALUES(?1,?2,?3,'PREPARED',?4,?5,?6)",
                params![intent_id, command.client_action_id, command_sha256, paths_json, event_id, created_at],
            )
            .map_err(database_error)?;
        boundary(DurableBoundary::IntentPrepared)?;

        persist_records(&self.root, &intent_id, &records, &references, boundary)?;
        set_intent_phase(&database, &intent_id, "RECORDS_DURABLE")?;

        let event = CplEvent {
            schema_version: CPL_SCHEMA_VERSION.to_owned(),
            event_id,
            project_id: command.project_id.clone(),
            event_sequence: 0,
            timestamp: created_at,
            event_type: command.event_type.clone(),
            actor: command.actor.clone(),
            client_action_id: command.client_action_id.clone(),
            command_sha256: command_sha256.clone(),
            record_references: references.clone(),
            metadata: command.metadata.clone(),
            previous_event_sha256: None,
            event_sha256: String::new(),
        };
        let (event, _) = ledger::append_event(
            &LedgerPaths::new(&self.root),
            self.config.ledger,
            event,
            boundary,
        )?;
        set_intent_phase(&database, &intent_id, "LEDGER_APPENDED")?;
        set_intent_phase(&database, &intent_id, "CHAIN_HEAD_ADVANCED")?;

        let result = WriteResult {
            idempotent_replay: false,
            intent_id: intent_id.clone(),
            event: event.clone(),
            records: references.clone(),
        };
        apply_sqlite_state(
            &mut database,
            &command,
            &command_sha256,
            &intent_id,
            &event,
            &result,
        )?;
        boundary(DurableBoundary::SqliteApplied)?;
        set_intent_phase(&database, &intent_id, "COMPLETE")?;
        boundary(DurableBoundary::Complete)?;
        let staging = self.root.join(".app/temp/staging").join(&intent_id);
        if staging.exists() {
            let _ = fs::remove_dir(&staging);
        }
        Ok(result)
    }

    #[cfg(test)]
    pub fn write_with_failure(
        &self,
        command: WriteCommand,
        failure: DurableBoundary,
    ) -> CplResult<WriteResult> {
        let mut fired = false;
        self.write_with_boundary(command, &mut |boundary| {
            if !fired && boundary == failure {
                fired = true;
                Err(super::injected_failure(boundary))
            } else {
                Ok(())
            }
        })
    }
}

fn validate_command(command: &WriteCommand, project_id: &str) -> CplResult<()> {
    if command.project_id != project_id {
        return Err(CplError::new(
            "CPL_PROJECT_MISMATCH",
            "The command targets another project.",
            false,
        ));
    }
    for (label, value) in [
        ("client_action_id", command.client_action_id.as_str()),
        ("event_type", command.event_type.as_str()),
        ("actor", command.actor.as_str()),
    ] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(CplError::new(
                "CPL_COMMAND_INVALID",
                format!("{label} is empty or contains control characters."),
                false,
            ));
        }
    }
    if command.records.is_empty() {
        return Err(CplError::new(
            "CPL_RECORD_REQUIRED",
            "Every provenance mutation requires at least one immutable authoritative record.",
            false,
        ));
    }
    for record in &command.records {
        if !safe_component(&record.record_type) {
            return Err(CplError::new(
                "CPL_RECORD_TYPE_INVALID",
                format!("Unsafe record type '{}'.", record.record_type),
                false,
            ));
        }
    }
    Ok(())
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn prepare_records(
    command: &WriteCommand,
    intent_id: &str,
    created_at: &str,
) -> CplResult<(Vec<CplRecord>, Vec<RecordReference>)> {
    let mut records = Vec::with_capacity(command.records.len());
    let mut references = Vec::with_capacity(command.records.len());
    for input in &command.records {
        let record_id = sortable_id("record")?;
        let mut record = CplRecord {
            schema_version: CPL_SCHEMA_VERSION.to_owned(),
            record_id: record_id.clone(),
            record_type: input.record_type.clone(),
            project_id: command.project_id.clone(),
            intent_id: intent_id.to_owned(),
            client_action_id: command.client_action_id.clone(),
            created_at: created_at.to_owned(),
            payload: input.payload.clone(),
            record_sha256: String::new(),
        };
        record.record_sha256 = canonical_digest(&record.identity())?;
        references.push(RecordReference {
            record_id,
            record_type: input.record_type.clone(),
            path: format!("records/{}/{}.json", input.record_type, record.record_id),
            record_sha256: record.record_sha256.clone(),
        });
        records.push(record);
    }
    Ok((records, references))
}

fn persist_records(
    root: &Path,
    intent_id: &str,
    records: &[CplRecord],
    references: &[RecordReference],
    boundary: &mut dyn FnMut(DurableBoundary) -> CplResult<()>,
) -> CplResult<()> {
    let staging = root.join(".app/temp/staging").join(intent_id);
    fs::create_dir_all(&staging)
        .map_err(|error| CplError::io("Could not create same-filesystem CPL staging", error))?;
    for (index, (record, reference)) in records.iter().zip(references).enumerate() {
        let stage = staging.join(format!("{}.json", record.record_id));
        let bytes = canonicalize(&serde_json::to_value(record).map_err(|error| {
            CplError::new("CPL_SERIALIZATION_FAILED", error.to_string(), false)
        })?)?;
        let mut file = File::create(&stage)
            .map_err(|error| CplError::io("Could not stage an immutable CPL record", error))?;
        file.write_all(&bytes)
            .map_err(|error| CplError::io("Could not write a staged CPL record", error))?;
        if index == 0 {
            boundary(DurableBoundary::FirstRecordStaged)?;
        }
        file.sync_all()
            .map_err(|error| CplError::io("Could not flush a staged CPL record", error))?;
        boundary(DurableBoundary::RecordFlushed)?;
        drop(file);
        let final_path = root.join(reference.path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let parent = final_path.parent().ok_or_else(|| {
            CplError::new("CPL_PATH_INVALID", "Record path has no parent.", false)
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            CplError::io("Could not create the immutable record directory", error)
        })?;
        if final_path.exists() {
            return Err(CplError::new(
                "CPL_RECORD_COLLISION",
                format!("{} already exists.", final_path.display()),
                false,
            ));
        }
        atomic_replace(&stage, &final_path)?;
        boundary(DurableBoundary::RecordMoved)?;
        sync_directory(parent)?;
        boundary(DurableBoundary::RecordDirectorySynced)?;
    }
    Ok(())
}

fn apply_sqlite_state(
    database: &mut Connection,
    command: &WriteCommand,
    command_sha256: &str,
    intent_id: &str,
    event: &CplEvent,
    result: &WriteResult,
) -> CplResult<()> {
    let transaction = database.transaction().map_err(database_error)?;
    if let Some(state) = &command.operational_state {
        let state = serde_json::to_string(state)
            .map_err(|error| CplError::new("CPL_SERIALIZATION_FAILED", error.to_string(), false))?;
        transaction.execute(
            "INSERT INTO project_state(id,json,updated_at) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET json=excluded.json,updated_at=excluded.updated_at",
            params![state, event.timestamp],
        ).map_err(database_error)?;
    }
    index_event(&transaction, event)?;
    let result_json = serde_json::to_string(result)
        .map_err(|error| CplError::new("CPL_SERIALIZATION_FAILED", error.to_string(), false))?;
    transaction.execute(
        "INSERT OR REPLACE INTO cpl_action_receipts(client_action_id,command_sha256,event_id,result_json,committed_at) VALUES(?1,?2,?3,?4,?5)",
        params![command.client_action_id, command_sha256, event.event_id, result_json, event.timestamp],
    ).map_err(database_error)?;
    transaction.execute("UPDATE write_intents SET phase='SQLITE_APPLIED',result_json=?2,updated_at=?3 WHERE intent_id=?1", params![intent_id, result_json, event.timestamp]).map_err(database_error)?;
    transaction.commit().map_err(database_error)
}

pub(crate) fn init_database(root: &Path) -> CplResult<Connection> {
    let state = root.join(".app");
    fs::create_dir_all(&state)
        .map_err(|error| CplError::io("Could not create CPL SQLite storage", error))?;
    let database = Connection::open(state.join("state.sqlite")).map_err(database_error)?;
    database.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=FULL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS project_state (id INTEGER PRIMARY KEY CHECK(id=1), json TEXT NOT NULL, updated_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS write_intents (intent_id TEXT PRIMARY KEY, client_action_id TEXT NOT NULL, command_sha256 TEXT NOT NULL, phase TEXT NOT NULL CHECK(phase IN ('PREPARED','RECORDS_DURABLE','LEDGER_APPENDED','CHAIN_HEAD_ADVANCED','SQLITE_APPLIED','COMPLETE','QUARANTINED','FAILED')), record_paths_json TEXT NOT NULL, event_id TEXT NOT NULL, result_json TEXT, updated_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS cpl_events (event_sequence INTEGER PRIMARY KEY, event_id TEXT NOT NULL UNIQUE, client_action_id TEXT NOT NULL UNIQUE, event_type TEXT NOT NULL, event_sha256 TEXT NOT NULL, previous_event_sha256 TEXT, timestamp TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS cpl_records (record_id TEXT PRIMARY KEY, record_type TEXT NOT NULL, path TEXT NOT NULL UNIQUE, record_sha256 TEXT NOT NULL, event_id TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS cpl_action_receipts (client_action_id TEXT PRIMARY KEY, command_sha256 TEXT NOT NULL, event_id TEXT NOT NULL, result_json TEXT NOT NULL, committed_at TEXT NOT NULL);",
    ).map_err(database_error)?;
    Ok(database)
}

pub(crate) fn index_event(database: &Connection, event: &CplEvent) -> CplResult<()> {
    database.execute(
        "INSERT OR REPLACE INTO cpl_events(event_sequence,event_id,client_action_id,event_type,event_sha256,previous_event_sha256,timestamp) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![event.event_sequence, event.event_id, event.client_action_id, event.event_type, event.event_sha256, event.previous_event_sha256, event.timestamp],
    ).map_err(database_error)?;
    for record in &event.record_references {
        database.execute(
            "INSERT OR REPLACE INTO cpl_records(record_id,record_type,path,record_sha256,event_id) VALUES(?1,?2,?3,?4,?5)",
            params![record.record_id, record.record_type, record.path, record.record_sha256, event.event_id],
        ).map_err(database_error)?;
    }
    Ok(())
}

pub(crate) fn rebuild_indexes(root: &Path, events: &[CplEvent]) -> CplResult<()> {
    let phase1_project_id = events.iter().find_map(|event| {
        event
            .record_references
            .iter()
            .any(|record| record.record_type == "phase1-operation")
            .then(|| event.project_id.clone())
    });
    let composition_project_id = events.iter().find_map(|event| {
        event
            .record_references
            .iter()
            .any(|record| record.record_type == "composition-command")
            .then(|| event.project_id.clone())
    });
    let mut database = init_database(root)?;
    let transaction = database.transaction().map_err(database_error)?;
    transaction
        .execute("DELETE FROM cpl_records", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM cpl_events", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM cpl_action_receipts", [])
        .map_err(database_error)?;
    let mut latest_operational_state = None;
    for event in events {
        index_event(&transaction, event)?;
        for reference in &event.record_references {
            if reference.record_type == "application-state-snapshot" {
                let path = root.join(reference.path.replace('/', std::path::MAIN_SEPARATOR_STR));
                let record: CplRecord =
                    serde_json::from_slice(&fs::read(&path).map_err(|error| {
                        CplError::io(
                            "Could not rebuild operational state from its canonical record",
                            error,
                        )
                    })?)
                    .map_err(|error| {
                        CplError::new("CPL_RECORD_INVALID", error.to_string(), false)
                    })?;
                latest_operational_state = Some((record.payload, event.timestamp.clone()));
            }
        }
        let first_record = event.record_references.first().ok_or_else(|| {
            CplError::new(
                "CPL_RECORD_REQUIRED",
                "Committed event has no authoritative record reference.",
                false,
            )
        })?;
        let first_path = root.join(
            first_record
                .path
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        let first_record: CplRecord =
            serde_json::from_slice(&fs::read(&first_path).map_err(|error| {
                CplError::io("Could not reconstruct the committed intent identity", error)
            })?)
            .map_err(|error| CplError::new("CPL_RECORD_INVALID", error.to_string(), false))?;
        let result = WriteResult {
            idempotent_replay: false,
            intent_id: first_record.intent_id,
            event: event.clone(),
            records: event.record_references.clone(),
        };
        let result_json = serde_json::to_string(&result)
            .map_err(|error| CplError::new("CPL_SERIALIZATION_FAILED", error.to_string(), false))?;
        transaction.execute(
            "INSERT INTO cpl_action_receipts(client_action_id,command_sha256,event_id,result_json,committed_at) VALUES(?1,?2,?3,?4,?5)",
            params![event.client_action_id, event.command_sha256, event.event_id, result_json, event.timestamp],
        ).map_err(database_error)?;
    }
    if let Some((state, updated_at)) = latest_operational_state {
        let state = serde_json::to_string(&state)
            .map_err(|error| CplError::new("CPL_SERIALIZATION_FAILED", error.to_string(), false))?;
        transaction.execute(
            "INSERT INTO project_state(id,json,updated_at) VALUES(1,?1,?2) ON CONFLICT(id) DO UPDATE SET json=excluded.json,updated_at=excluded.updated_at",
            params![state, updated_at],
        ).map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)?;
    if let Some(project_id) = phase1_project_id {
        super::phase1::rebuild_projection_cache(root, &project_id, events)?;
    }
    if let Some(project_id) = composition_project_id {
        super::composition::rebuild_projection_cache(root, &project_id, events)?;
    }
    Ok(())
}

pub(crate) fn set_intent_phase(
    database: &Connection,
    intent_id: &str,
    phase: &str,
) -> CplResult<()> {
    database
        .execute(
            "UPDATE write_intents SET phase=?2,updated_at=?3 WHERE intent_id=?1",
            params![intent_id, phase, timestamp_millis()],
        )
        .map_err(database_error)?;
    Ok(())
}

pub(crate) fn database_error(error: rusqlite::Error) -> CplError {
    CplError::new("CPL_DATABASE_ERROR", error.to_string(), true)
}

pub(crate) struct ProjectWriterLock {
    file: File,
}

impl ProjectWriterLock {
    pub(crate) fn acquire(root: &Path) -> CplResult<Self> {
        let app = root.join(".app/locks");
        fs::create_dir_all(&app)
            .map_err(|error| CplError::io("Could not create the CPL lock directory", error))?;
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(app.join("cpl.writer.lock"))
            .map_err(|error| CplError::io("Could not open the CPL writer lock", error))?;
        lock_file(&file)?;
        Ok(Self { file })
    }
}

impl Drop for ProjectWriterLock {
    fn drop(&mut self) {
        let _ = unlock_file(&self.file);
    }
}

#[cfg(target_os = "windows")]
fn lock_file(file: &File) -> CplResult<()> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: *mut std::ffi::c_void,
    }
    #[link(name = "Kernel32")]
    extern "system" {
        fn LockFileEx(
            file: *mut std::ffi::c_void,
            flags: u32,
            reserved: u32,
            low: u32,
            high: u32,
            overlapped: *mut Overlapped,
        ) -> i32;
    }
    let mut overlapped = Overlapped {
        internal: 0,
        internal_high: 0,
        offset: 0,
        offset_high: 0,
        event: std::ptr::null_mut(),
    };
    let result = unsafe {
        LockFileEx(
            file.as_raw_handle(),
            0x2,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        )
    };
    if result == 0 {
        return Err(CplError::io(
            "Could not acquire the exclusive OS CPL writer lock",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn unlock_file(file: &File) -> CplResult<()> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: *mut std::ffi::c_void,
    }
    #[link(name = "Kernel32")]
    extern "system" {
        fn UnlockFileEx(
            file: *mut std::ffi::c_void,
            reserved: u32,
            low: u32,
            high: u32,
            overlapped: *mut Overlapped,
        ) -> i32;
    }
    let mut overlapped = Overlapped {
        internal: 0,
        internal_high: 0,
        offset: 0,
        offset_high: 0,
        event: std::ptr::null_mut(),
    };
    let result =
        unsafe { UnlockFileEx(file.as_raw_handle(), 0, u32::MAX, u32::MAX, &mut overlapped) };
    if result == 0 {
        return Err(CplError::io(
            "Could not release the CPL writer lock",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn lock_file(file: &File) -> CplResult<()> {
    use std::os::fd::AsRawFd;
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    if unsafe { flock(file.as_raw_fd(), 2) } != 0 {
        return Err(CplError::io(
            "Could not acquire the exclusive OS CPL writer lock",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn unlock_file(file: &File) -> CplResult<()> {
    use std::os::fd::AsRawFd;
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    if unsafe { flock(file.as_raw_fd(), 8) } != 0 {
        return Err(CplError::io(
            "Could not release the CPL writer lock",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}
