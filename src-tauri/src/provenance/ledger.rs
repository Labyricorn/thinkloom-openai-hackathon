use super::{
    canonical::{canonical_digest, canonicalize, sha256_digest},
    identifiers::timestamp_millis,
    records::{ChainHead, CplEvent, SegmentManifest},
    CplError, CplResult, DurableBoundary, CPL_SCHEMA_VERSION,
};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy)]
pub struct LedgerConfig {
    pub max_events_per_segment: u64,
    pub max_bytes_per_segment: u64,
}

impl Default for LedgerConfig {
    fn default() -> Self {
        Self {
            max_events_per_segment: 10_000,
            max_bytes_per_segment: 10 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LedgerPaths {
    pub root: PathBuf,
    pub active: PathBuf,
    pub sealed: PathBuf,
    pub chain_head: PathBuf,
}

impl LedgerPaths {
    pub fn new(project_root: &Path) -> Self {
        let root = project_root.join("provenance/ledger");
        Self {
            active: root.join("active"),
            sealed: root.join("sealed"),
            chain_head: root.join("chain-head.json"),
            root,
        }
    }

    pub fn initialize(&self) -> CplResult<()> {
        fs::create_dir_all(&self.active)
            .and_then(|_| fs::create_dir_all(&self.sealed))
            .map_err(|error| CplError::io("Could not create CPL ledger directories", error))?;
        if active_segments(self)?.is_empty() {
            let next_number = sealed_segments(self)?
                .iter()
                .filter_map(|path| parse_segment_number(path))
                .max()
                .unwrap_or(0)
                + 1;
            File::create(self.active.join(segment_filename(next_number)))
                .and_then(|file| file.sync_all())
                .map_err(|error| {
                    CplError::io("Could not initialize the active CPL segment", error)
                })?;
            sync_directory(&self.active)?;
        }
        Ok(())
    }
}

pub fn segment_filename(number: u64) -> String {
    format!("segment-{number:06}.jsonl")
}

pub fn segment_manifest_filename(number: u64) -> String {
    format!("segment-{number:06}.manifest.json")
}

fn parse_segment_number(path: &Path) -> Option<u64> {
    path.file_name()?
        .to_str()?
        .strip_prefix("segment-")?
        .strip_suffix(".jsonl")?
        .parse()
        .ok()
}

pub fn active_segments(paths: &LedgerPaths) -> CplResult<Vec<PathBuf>> {
    let mut segments = fs::read_dir(&paths.active)
        .map_err(|error| CplError::io("Could not inspect active CPL segments", error))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| parse_segment_number(path).is_some())
        .collect::<Vec<_>>();
    segments.sort();
    Ok(segments)
}

pub fn sealed_segments(paths: &LedgerPaths) -> CplResult<Vec<PathBuf>> {
    let mut segments = fs::read_dir(&paths.sealed)
        .map_err(|error| CplError::io("Could not inspect sealed CPL segments", error))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| parse_segment_number(path).is_some())
        .collect::<Vec<_>>();
    segments.sort();
    Ok(segments)
}

pub fn current_active_segment(paths: &LedgerPaths) -> CplResult<(u64, PathBuf)> {
    let segments = active_segments(paths)?;
    if segments.len() != 1 {
        return Err(CplError::new(
            "CPL_ACTIVE_SEGMENT_INVALID",
            format!(
                "Expected exactly one active segment, found {}.",
                segments.len()
            ),
            false,
        ));
    }
    let path = segments[0].clone();
    let number = parse_segment_number(&path).ok_or_else(|| {
        CplError::new(
            "CPL_SEGMENT_NAME_INVALID",
            "The active segment name is invalid.",
            false,
        )
    })?;
    Ok((number, path))
}

pub fn read_segment(path: &Path) -> CplResult<Vec<CplEvent>> {
    let bytes =
        fs::read(path).map_err(|error| CplError::io("Could not read CPL segment", error))?;
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err(CplError::new(
            "CPL_SEGMENT_TRUNCATED",
            format!("{} has an incomplete final JSONL line.", path.display()),
            true,
        ));
    }
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .enumerate()
        .map(|(index, line)| {
            let event: CplEvent = serde_json::from_slice(line).map_err(|error| {
                CplError::new(
                    "CPL_EVENT_INVALID",
                    format!("{} line {} is invalid: {error}", path.display(), index + 1),
                    false,
                )
            })?;
            if canonical_event_line(&event)? != line {
                return Err(CplError::new(
                    "CPL_EVENT_NONCANONICAL",
                    format!(
                        "{} line {} is valid JSON but not canonical JSON.",
                        path.display(),
                        index + 1
                    ),
                    false,
                ));
            }
            Ok(event)
        })
        .collect()
}

pub fn read_all_events(paths: &LedgerPaths) -> CplResult<Vec<CplEvent>> {
    let mut events = Vec::new();
    for path in sealed_segments(paths)? {
        events.extend(read_segment(&path)?);
    }
    for path in active_segments(paths)? {
        events.extend(read_segment(&path)?);
    }
    Ok(events)
}

pub fn read_chain_head(paths: &LedgerPaths) -> CplResult<Option<ChainHead>> {
    if !paths.chain_head.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&paths.chain_head)
        .map_err(|error| CplError::io("Could not read the CPL chain head", error))?;
    let head: ChainHead = serde_json::from_slice(&bytes)
        .map_err(|error| CplError::new("CPL_CHAIN_HEAD_INVALID", error.to_string(), false))?;
    if serialize_canonical(&head)? != bytes {
        return Err(CplError::new(
            "CPL_CHAIN_HEAD_NONCANONICAL",
            "The CPL chain head is not canonical JSON.",
            false,
        ));
    }
    Ok(Some(head))
}

fn serialize_canonical<T: Serialize>(value: &T) -> CplResult<Vec<u8>> {
    let value = serde_json::to_value(value)
        .map_err(|error| CplError::new("CPL_SERIALIZATION_FAILED", error.to_string(), false))?;
    canonicalize(&value)
}

pub(crate) fn sync_directory(path: &Path) -> CplResult<()> {
    #[cfg(not(target_os = "windows"))]
    {
        File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| CplError::io("Could not flush a CPL directory", error))?;
    }
    #[cfg(target_os = "windows")]
    let _ = path;
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn atomic_replace(source: &Path, destination: &Path) -> CplResult<()> {
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(CplError::io(
            "Could not atomically replace a CPL file",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn atomic_replace(source: &Path, destination: &Path) -> CplResult<()> {
    fs::rename(source, destination)
        .map_err(|error| CplError::io("Could not atomically replace a CPL file", error))
}

pub(crate) fn write_atomic<T: Serialize>(path: &Path, value: &T) -> CplResult<()> {
    let parent = path.parent().ok_or_else(|| {
        CplError::new(
            "CPL_PATH_INVALID",
            "The CPL file has no parent directory.",
            false,
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| CplError::io("Could not create a CPL parent directory", error))?;
    let temp = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    let bytes = serialize_canonical(value)?;
    let mut file = File::create(&temp)
        .map_err(|error| CplError::io("Could not create a temporary CPL file", error))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| CplError::io("Could not flush a temporary CPL file", error))?;
    drop(file);
    atomic_replace(&temp, path)?;
    sync_directory(parent)
}

fn last_sealed_digest(paths: &LedgerPaths) -> CplResult<Option<String>> {
    sealed_segments(paths)?
        .last()
        .map(|path| {
            fs::read(path)
                .map(|bytes| sha256_digest(&bytes))
                .map_err(|error| CplError::io("Could not hash the prior sealed segment", error))
        })
        .transpose()
}

fn rotate_if_needed(
    paths: &LedgerPaths,
    config: LedgerConfig,
    boundary: &mut dyn FnMut(DurableBoundary) -> CplResult<()>,
) -> CplResult<()> {
    let (number, active) = current_active_segment(paths)?;
    let bytes = fs::read(&active)
        .map_err(|error| CplError::io("Could not inspect the active segment", error))?;
    let events = read_segment(&active)?;
    if events.is_empty()
        || (events.len() as u64) < config.max_events_per_segment
            && (bytes.len() as u64) < config.max_bytes_per_segment
    {
        return Ok(());
    }
    OpenOptions::new()
        .write(true)
        .open(&active)
        .and_then(|file| file.sync_all())
        .map_err(|error| {
            CplError::io("Could not flush the active segment before sealing", error)
        })?;
    boundary(DurableBoundary::SegmentFlushed)?;
    let manifest = SegmentManifest {
        schema_version: CPL_SCHEMA_VERSION.to_owned(),
        segment_number: number,
        previous_segment_file_sha256: last_sealed_digest(paths)?,
        first_event_sha256: events.first().unwrap().event_sha256.clone(),
        final_event_sha256: events.last().unwrap().event_sha256.clone(),
        first_event_sequence: events.first().unwrap().event_sequence,
        final_event_sequence: events.last().unwrap().event_sequence,
        event_count: events.len() as u64,
        byte_length: bytes.len() as u64,
        segment_file_sha256: sha256_digest(&bytes),
        sealed_at: timestamp_millis(),
    };
    let manifest_temp = paths
        .active
        .join(format!(".{}.tmp", segment_manifest_filename(number)));
    let manifest_bytes = serialize_canonical(&manifest)?;
    File::create(&manifest_temp)
        .and_then(|mut file| {
            file.write_all(&manifest_bytes)
                .and_then(|_| file.sync_all())
        })
        .map_err(|error| CplError::io("Could not flush a sealed segment manifest", error))?;
    boundary(DurableBoundary::SegmentManifestFlushed)?;
    let sealed_segment = paths.sealed.join(segment_filename(number));
    let sealed_manifest = paths.sealed.join(segment_manifest_filename(number));
    atomic_replace(&active, &sealed_segment)?;
    boundary(DurableBoundary::SegmentMoved)?;
    atomic_replace(&manifest_temp, &sealed_manifest)?;
    boundary(DurableBoundary::SegmentManifestMoved)?;
    File::create(paths.active.join(segment_filename(number + 1)))
        .and_then(|file| file.sync_all())
        .map_err(|error| CplError::io("Could not create the next active segment", error))?;
    boundary(DurableBoundary::NewActiveSegmentCreated)?;
    sync_directory(&paths.active)?;
    sync_directory(&paths.sealed)
}

pub fn append_event(
    paths: &LedgerPaths,
    config: LedgerConfig,
    mut event: CplEvent,
    boundary: &mut dyn FnMut(DurableBoundary) -> CplResult<()>,
) -> CplResult<(CplEvent, ChainHead)> {
    paths.initialize()?;
    rotate_if_needed(paths, config, boundary)?;
    let events = read_all_events(paths)?;
    let previous = events.last();
    event.event_sequence = previous.map_or(1, |item| item.event_sequence + 1);
    event.previous_event_sha256 = previous.map(|item| item.event_sha256.clone());
    event.event_sha256 = canonical_digest(&event.identity())?;

    let (segment_number, active) = current_active_segment(paths)?;
    let mut line = serialize_canonical(&event)?;
    line.push(b'\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&active)
        .map_err(|error| CplError::io("Could not open the active CPL segment", error))?;
    file.write_all(&line)
        .map_err(|error| CplError::io("Could not append the CPL event", error))?;
    boundary(DurableBoundary::LedgerAppendBeforeFlush)?;
    file.sync_all()
        .map_err(|error| CplError::io("Could not flush the CPL event", error))?;
    boundary(DurableBoundary::LedgerFlushed)?;

    let head = ChainHead {
        schema_version: CPL_SCHEMA_VERSION.to_owned(),
        project_id: event.project_id.clone(),
        segment_number,
        segment_file: format!(
            "provenance/ledger/active/{}",
            segment_filename(segment_number)
        ),
        event_id: event.event_id.clone(),
        event_sequence: event.event_sequence,
        event_sha256: event.event_sha256.clone(),
        updated_at: event.timestamp.clone(),
    };
    let head_temp = paths.root.join(".chain-head.json.tmp");
    let head_bytes = serialize_canonical(&head)?;
    File::create(&head_temp)
        .and_then(|mut file| file.write_all(&head_bytes).and_then(|_| file.sync_all()))
        .map_err(|error| CplError::io("Could not flush the temporary CPL chain head", error))?;
    boundary(DurableBoundary::ChainHeadTemporaryWritten)?;
    atomic_replace(&head_temp, &paths.chain_head)?;
    boundary(DurableBoundary::ChainHeadReplaced)?;
    sync_directory(&paths.root)?;
    boundary(DurableBoundary::ChainHeadDirectorySynced)?;
    Ok((event, head))
}

pub fn advance_chain_head(
    paths: &LedgerPaths,
    project_id: &str,
    event: &CplEvent,
) -> CplResult<ChainHead> {
    let segment_number = locate_event_segment(paths, &event.event_id)?.ok_or_else(|| {
        CplError::new(
            "CPL_EVENT_NOT_FOUND",
            "Cannot advance the chain head to a missing event.",
            false,
        )
    })?;
    let in_active = active_segments(paths)?
        .iter()
        .any(|path| parse_segment_number(path) == Some(segment_number));
    let kind = if in_active { "active" } else { "sealed" };
    let head = ChainHead {
        schema_version: CPL_SCHEMA_VERSION.to_owned(),
        project_id: project_id.to_owned(),
        segment_number,
        segment_file: format!(
            "provenance/ledger/{kind}/{}",
            segment_filename(segment_number)
        ),
        event_id: event.event_id.clone(),
        event_sequence: event.event_sequence,
        event_sha256: event.event_sha256.clone(),
        updated_at: timestamp_millis(),
    };
    write_atomic(&paths.chain_head, &head)?;
    Ok(head)
}

pub fn locate_event_segment(paths: &LedgerPaths, event_id: &str) -> CplResult<Option<u64>> {
    let mut segments = sealed_segments(paths)?;
    segments.extend(active_segments(paths)?);
    for path in segments {
        if read_segment(&path)?
            .iter()
            .any(|event| event.event_id == event_id)
        {
            return Ok(parse_segment_number(&path));
        }
    }
    Ok(None)
}

pub fn canonical_event_line(event: &CplEvent) -> CplResult<Vec<u8>> {
    let value: Value = serde_json::to_value(event)
        .map_err(|error| CplError::new("CPL_SERIALIZATION_FAILED", error.to_string(), false))?;
    canonicalize(&value)
}
