//! Project-format classification and legacy preservation boundaries.
//!
//! Inspection is deliberately read-only. Only an exact supported marker can
//! proceed to CPL recovery; every other recognized project remains read-only.

use crate::provenance::{canonical::sha256_digest, CplError, CplResult};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};
use walkdir::WalkDir;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

pub const PROJECT_FORMAT: &str = "thinkloom-cpl";
pub const PROJECT_FORMAT_VERSION: &str = "1.0";
pub const PROVENANCE_CONFORMANCE: &str = "cpl-1.0";

pub fn manifest_has_supported_marker(bytes: &[u8]) -> bool {
    serde_json::from_slice::<Value>(bytes).is_ok_and(|value| {
        value.get("project_format").and_then(Value::as_str) == Some(PROJECT_FORMAT)
            && value.get("project_format_version").and_then(Value::as_str)
                == Some(PROJECT_FORMAT_VERSION)
            && value.get("provenance_conformance").and_then(Value::as_str)
                == Some(PROVENANCE_CONFORMANCE)
    })
}

pub const REQUIRED_DIRECTORIES: &[&str] = &[
    "records",
    "provenance/ledger/active",
    "provenance/ledger/sealed",
    "reports",
    ".app",
    ".app/locks",
    ".app/temp",
    ".app/recovery",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProjectClassification {
    CplConforming,
    LegacyPreviewReadOnly,
    UnsupportedReadOnly,
    CplBlocked,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectInspection {
    pub classification: ProjectClassification,
    pub message: String,
}

pub fn initialize_conforming_layout(root: &Path) -> CplResult<()> {
    for directory in [
        "manuscript/sections",
        "ideas",
        "conversations/transcripts",
        "records/conversations",
        "records/invocations",
        "records/prompt-templates",
        "records/sources",
        "records/transformations",
        "records/composition",
        "provenance/schema",
        "provenance/ledger/active",
        "provenance/ledger/sealed",
        "provenance/indexes",
        "provenance/integrity",
        "provenance/report-config",
        "releases",
        "deposits",
        "reports/harp",
        "assets",
        ".app/locks",
        ".app/temp",
        ".app/temp/staging",
        ".app/recovery/orphans",
        ".app/snapshots",
    ] {
        fs::create_dir_all(root.join(directory)).map_err(|error| {
            CplError::io("Could not create the conforming project layout", error)
        })?;
    }
    Ok(())
}

pub fn inspect_project(root: &Path) -> CplResult<ProjectInspection> {
    let manifest_path = root.join("project.json");
    if !manifest_path.is_file() {
        return Err(CplError::new(
            "PROJECT_INVALID",
            "The selected folder does not contain project.json.",
            false,
        ));
    }
    let bytes = fs::read(&manifest_path)
        .map_err(|error| CplError::io("Could not read the project marker", error))?;
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => {
            return Ok(ProjectInspection {
                classification: ProjectClassification::UnsupportedReadOnly,
                message: "The project manifest is not valid JSON. It was not modified, recovered, or verified.".to_owned(),
            })
        }
    };

    let format = value.get("project_format").and_then(Value::as_str);
    let version = value.get("project_format_version").and_then(Value::as_str);
    let conformance = value.get("provenance_conformance").and_then(Value::as_str);
    let marker_absent = [
        "project_format",
        "project_format_version",
        "provenance_conformance",
    ]
    .iter()
    .all(|key| value.get(key).is_none());
    if marker_absent {
        return Ok(ProjectInspection {
            classification: ProjectClassification::LegacyPreviewReadOnly,
            message: "Legacy preview project detected. Migration is deferred until after Thinkloom 1.0.0; only folder access and a byte-preserving preservation archive are available.".to_owned(),
        });
    }
    if format != Some(PROJECT_FORMAT)
        || version != Some(PROJECT_FORMAT_VERSION)
        || conformance != Some(PROVENANCE_CONFORMANCE)
    {
        return Ok(ProjectInspection {
            classification: ProjectClassification::UnsupportedReadOnly,
            message: "The project has an incomplete or unsupported CPL marker. It was not modified, recovered, or verified.".to_owned(),
        });
    }

    let missing = REQUIRED_DIRECTORIES
        .iter()
        .filter(|relative| {
            let path = root.join(relative);
            !path.is_dir()
                || fs::symlink_metadata(&path)
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
        })
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Ok(ProjectInspection {
            classification: ProjectClassification::CplBlocked,
            message: format!(
                "The CPL marker is present, but the required project structure is incomplete: {}. No recovery was attempted.",
                missing.join(", ")
            ),
        });
    }

    Ok(ProjectInspection {
        classification: ProjectClassification::CplConforming,
        message: "The exact supported CPL 1.0 marker and required project structure are present."
            .to_owned(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InventoryEntry {
    path: String,
    kind: &'static str,
    digest_or_target: String,
}

fn source_inventory(source: &Path) -> CplResult<Vec<InventoryEntry>> {
    let mut entries = Vec::new();
    for entry in WalkDir::new(source).follow_links(false).into_iter() {
        let entry = entry.map_err(|error| {
            CplError::new(
                "LEGACY_ARCHIVE_READ_FAILED",
                format!("Could not inspect the legacy project: {error}"),
                true,
            )
        })?;
        if entry.path() == source {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| CplError::new("LEGACY_ARCHIVE_PATH_FAILED", error.to_string(), false))?
            .to_string_lossy()
            .replace('\\', "/");
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| CplError::io("Could not inspect a legacy project entry", error))?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(entry.path())
                .map_err(|error| CplError::io("Could not read a legacy project link", error))?;
            entries.push(InventoryEntry {
                path: relative,
                kind: "symlink",
                digest_or_target: target.to_string_lossy().into_owned(),
            });
        } else if metadata.is_dir() {
            entries.push(InventoryEntry {
                path: relative,
                kind: "directory",
                digest_or_target: String::new(),
            });
        } else if metadata.is_file() {
            entries.push(InventoryEntry {
                path: relative,
                kind: "file",
                digest_or_target: sha256_digest(&fs::read(entry.path()).map_err(|error| {
                    CplError::io("Could not read a legacy project file", error)
                })?),
            });
        } else {
            return Err(CplError::new(
                "LEGACY_ARCHIVE_UNSUPPORTED_ENTRY",
                format!("The legacy project contains an unsupported entry: {relative}"),
                false,
            ));
        }
    }
    entries.sort_by(|left, right| left.path.as_bytes().cmp(right.path.as_bytes()));
    Ok(entries)
}

pub fn create_legacy_preservation_archive(source: &Path, destination: &Path) -> CplResult<()> {
    let inspection = inspect_project(source)?;
    if inspection.classification != ProjectClassification::LegacyPreviewReadOnly {
        return Err(CplError::new(
            "LEGACY_ARCHIVE_NOT_PERMITTED",
            "Preservation archives are available only for unmarked legacy preview projects.",
            false,
        ));
    }
    let source = fs::canonicalize(source)
        .map_err(|error| CplError::io("Could not resolve the legacy project folder", error))?;
    let parent = destination.parent().ok_or_else(|| {
        CplError::new(
            "LEGACY_ARCHIVE_PATH_INVALID",
            "The preservation archive destination has no parent folder.",
            false,
        )
    })?;
    let parent = fs::canonicalize(parent)
        .map_err(|error| CplError::io("Could not resolve the archive destination", error))?;
    let destination = parent.join(destination.file_name().ok_or_else(|| {
        CplError::new(
            "LEGACY_ARCHIVE_PATH_INVALID",
            "Choose a complete ZIP destination.",
            false,
        )
    })?);
    if destination.starts_with(&source) {
        return Err(CplError::new(
            "LEGACY_ARCHIVE_INSIDE_PROJECT",
            "The preservation archive must be saved outside the legacy project so the source remains untouched.",
            false,
        ));
    }

    let before = source_inventory(&source)?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("legacy-preservation.zip"),
        uuid::Uuid::new_v4()
    ));
    let result = (|| -> CplResult<()> {
        let file = File::create(&temporary)
            .map_err(|error| CplError::io("Could not create the preservation archive", error))?;
        let mut zip = ZipWriter::new(file);
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("PRESERVATION-NOTICE.txt", stored)
            .map_err(zip_error)?;
        zip.write_all(b"Legacy preview-project preservation archive\nNot verified, converted, or CPL-conforming\nMigration is deferred until after Thinkloom 1.0.0.\n")
            .map_err(|error| CplError::io("Could not write the preservation label", error))?;

        for item in &before {
            let archive_path = format!("legacy-project/{}", item.path);
            let source_path = item
                .path
                .split('/')
                .fold(source.clone(), |path, component| path.join(component));
            match item.kind {
                "directory" => zip
                    .add_directory(format!("{archive_path}/"), stored)
                    .map_err(zip_error)?,
                "symlink" => {
                    let link_options = stored.unix_permissions(0o120777);
                    zip.start_file(archive_path, link_options)
                        .map_err(zip_error)?;
                    zip.write_all(item.digest_or_target.as_bytes())
                        .map_err(|error| {
                            CplError::io("Could not preserve a legacy project link", error)
                        })?;
                }
                "file" => {
                    zip.start_file(archive_path, stored).map_err(zip_error)?;
                    let mut input = File::open(&source_path).map_err(|error| {
                        CplError::io("Could not read a legacy project file", error)
                    })?;
                    let mut bytes = Vec::new();
                    input.read_to_end(&mut bytes).map_err(|error| {
                        CplError::io("Could not read a legacy project file", error)
                    })?;
                    zip.write_all(&bytes).map_err(|error| {
                        CplError::io("Could not preserve a legacy project file", error)
                    })?;
                }
                _ => unreachable!(),
            }
        }
        let file = zip.finish().map_err(zip_error)?;
        file.sync_all()
            .map_err(|error| CplError::io("Could not flush the preservation archive", error))?;
        crate::provenance::ledger::atomic_replace(&temporary, &destination)?;
        crate::provenance::ledger::sync_directory(&parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return result;
    }
    let after = source_inventory(&source)?;
    if before != after {
        let _ = fs::remove_file(&destination);
        return Err(CplError::new(
            "LEGACY_PROJECT_CHANGED_DURING_ARCHIVE",
            "The legacy project changed while it was being archived. The incomplete archive was removed; retry after writes stop.",
            true,
        ));
    }
    Ok(())
}

fn zip_error(error: zip::result::ZipError) -> CplError {
    CplError::new("LEGACY_ARCHIVE_FAILED", error.to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::tempdir;
    use zip::ZipArchive;

    fn write_manifest(root: &Path, value: Value) {
        fs::write(
            root.join("project.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn schema_version_without_marker_is_legacy_and_inspection_is_read_only() {
        let temp = tempdir().unwrap();
        write_manifest(
            temp.path(),
            serde_json::json!({"schemaVersion":"1.0","applicationVersion":"0.5.3"}),
        );
        fs::write(temp.path().join("original.bin"), [0, 1, 2, 255]).unwrap();
        let before = source_inventory(temp.path()).unwrap();
        let result = inspect_project(temp.path()).unwrap();
        assert_eq!(
            result.classification,
            ProjectClassification::LegacyPreviewReadOnly
        );
        assert_eq!(before, source_inventory(temp.path()).unwrap());
    }

    #[test]
    fn exact_marker_requires_the_complete_layout() {
        let temp = tempdir().unwrap();
        write_manifest(
            temp.path(),
            serde_json::json!({
                "project_format": PROJECT_FORMAT,
                "project_format_version": PROJECT_FORMAT_VERSION,
                "provenance_conformance": PROVENANCE_CONFORMANCE
            }),
        );
        assert_eq!(
            inspect_project(temp.path()).unwrap().classification,
            ProjectClassification::CplBlocked
        );
        initialize_conforming_layout(temp.path()).unwrap();
        assert_eq!(
            inspect_project(temp.path()).unwrap().classification,
            ProjectClassification::CplConforming
        );
    }

    #[test]
    fn unsupported_or_partial_marker_never_enters_cpl_recovery() {
        let temp = tempdir().unwrap();
        write_manifest(
            temp.path(),
            serde_json::json!({
                "project_format": PROJECT_FORMAT,
                "project_format_version": "2.0",
                "provenance_conformance": PROVENANCE_CONFORMANCE
            }),
        );
        assert_eq!(
            inspect_project(temp.path()).unwrap().classification,
            ProjectClassification::UnsupportedReadOnly
        );
    }

    #[test]
    fn preservation_archive_retains_source_bytes_without_changing_source() {
        let source_parent = tempdir().unwrap();
        let source = source_parent.path().join("preview");
        fs::create_dir_all(source.join("nested")).unwrap();
        write_manifest(
            &source,
            serde_json::json!({"schemaVersion":"1.0","applicationVersion":"0.5.3"}),
        );
        let original = [0, 13, 10, 255, 128, 42];
        fs::write(source.join("nested/data.bin"), original).unwrap();
        let before = source_inventory(&source).unwrap();
        let destination_parent = tempdir().unwrap();
        let destination = destination_parent.path().join("preview-preservation.zip");
        create_legacy_preservation_archive(&source, &destination).unwrap();
        assert_eq!(before, source_inventory(&source).unwrap());

        let mut archive = ZipArchive::new(File::open(destination).unwrap()).unwrap();
        let mut restored = Vec::new();
        archive
            .by_name("legacy-project/nested/data.bin")
            .unwrap()
            .read_to_end(&mut restored)
            .unwrap();
        assert_eq!(restored, original);
        let mut notice = String::new();
        archive
            .by_name("PRESERVATION-NOTICE.txt")
            .unwrap()
            .read_to_string(&mut notice)
            .unwrap();
        assert!(notice.contains("Not verified, converted, or CPL-conforming"));
    }
    #[test]
    fn preservation_archive_cannot_write_inside_the_legacy_project() {
        let source = tempdir().unwrap();
        write_manifest(
            source.path(),
            serde_json::json!({"schemaVersion":"1.0","applicationVersion":"0.5.3"}),
        );
        fs::write(source.path().join("original.txt"), b"unchanged").unwrap();
        let before = source_inventory(source.path()).unwrap();
        let destination = source.path().join("preservation.zip");
        let error = create_legacy_preservation_archive(source.path(), &destination)
            .expect_err("an archive inside the source must be refused");
        assert_eq!(error.code, "LEGACY_ARCHIVE_INSIDE_PROJECT");
        assert_eq!(before, source_inventory(source.path()).unwrap());
        assert!(!destination.exists());
    }
}
