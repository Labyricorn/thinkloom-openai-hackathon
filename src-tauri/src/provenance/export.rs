//! Non-mutating HARP registration exports and privacy-preserving archives.
//!
//! Sanitized archives are deliberately selective. They bind retained structural
//! evidence to the source CPL and disclose every omitted category without
//! claiming to contain the complete private project.

use super::{
    canonical::{canonical_digest, canonicalize, sha256_digest},
    contribution_map,
    harp::{self, HarpProjection, LEGAL_SCOPE_STATEMENT},
    ledger::{self, LedgerPaths},
    records::CplRecord,
    CplError, CplResult, CPL_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

const SELECTIVE_CLAIM: &str = "selective_disclosed_subset";
const VERIFICATION_SCOPE: &str = "Verified against the disclosed retained evidence and source bindings. The sanitized archive is intentionally incomplete and is not evidence of complete project history, identity, or legal authorship.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportFileBinding {
    pub path: String,
    pub sha256: String,
    pub byte_length: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBinding {
    pub source_chain_head_sha256: String,
    pub sanitization_profile: String,
    pub omissions: Vec<String>,
    pub files: Vec<ExportFileBinding>,
    pub content_sha256: String,
}

impl ExportBinding {
    pub fn verify(&self) -> CplResult<bool> {
        let identity = serde_json::to_value((
            &self.source_chain_head_sha256,
            &self.sanitization_profile,
            &self.omissions,
            &self.files,
        ))
        .map_err(serialization_error)?;
        Ok(canonical_digest(&identity)? == self.content_sha256)
    }
}

pub fn disclosure_for_omission(category: &str, retained_binding: &Value) -> CplResult<Value> {
    Ok(json!({
        "category": category,
        "retained_binding_sha256": canonical_digest(retained_binding)?,
    }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarpExportRequest {
    #[serde(default)]
    pub redact_personal_identifiers: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HarpExportArtifact {
    pub role: String,
    pub path: String,
    pub sha256: String,
    pub byte_length: u64,
    pub privacy_classification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SanitizedArchiveVerification {
    pub status: String,
    pub archive_sha256: String,
    pub source_chain_head: String,
    pub harp_sha256: String,
    pub deposit_sha256: String,
    pub retained_file_count: usize,
    pub omission_disclosure_count: usize,
    pub completeness_claim: String,
    pub verification_scope_statement: String,
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarpExportResult {
    pub export_directory: String,
    pub artifacts: Vec<HarpExportArtifact>,
    pub sanitized_manifest: Value,
    pub sanitized_verification: SanitizedArchiveVerification,
}

#[derive(Clone)]
struct ArchiveEntry {
    path: String,
    bytes: Vec<u8>,
}

pub fn create_harp_exports(
    root: &Path,
    project_id: &str,
    request: HarpExportRequest,
) -> CplResult<HarpExportResult> {
    let harp = harp::load_latest(root, project_id)?.ok_or_else(|| {
        CplError::new(
            "HARP_EXPORT_REQUIRED",
            "Generate and approve a HARP before creating registration exports.",
            true,
        )
    })?;
    if harp.applicability_status != "current" {
        return Err(CplError::new(
            "HARP_EXPORT_STALE",
            "The current HARP is stale. Freeze the current deposit and regenerate HARP before export.",
            true,
        ));
    }
    let map = contribution_map::load_latest(root, project_id)?.ok_or_else(|| {
        CplError::new(
            "HARP_EXPORT_MAP_REQUIRED",
            "The HARP contribution map is unavailable.",
            true,
        )
    })?;
    let harp_id = required_string(&harp.harp, "harp_id")?;
    let harp_sha256 = required_string(&harp.harp, "harp_sha256")?;
    let source_chain_head = required_string(&harp.harp["cpl_binding"], "chain_head")?;
    let deposit_sha256 = required_string(&harp.harp["deposit"], "deposit_sha256")?;
    let created_at = required_string(&harp.export_manifest, "created_at")?;
    let export_directory = format!("exports/harp/{harp_id}");

    let deposit_path = resolve_relative(root, &map.deposit.deposit_path);
    let deposit = fs::read(&deposit_path)
        .map_err(|error| CplError::io("Could not read the exact HARP deposit", error))?;
    if sha256_digest(&deposit) != deposit_sha256 {
        return Err(CplError::new(
            "HARP_EXPORT_DEPOSIT_MISMATCH",
            "The deposit copy no longer matches the HARP binding.",
            false,
        ));
    }

    let declared_name = harp.harp["identity_declaration"]["declared_name"]
        .as_str()
        .unwrap_or_default();
    let worksheet = registration_worksheet(&harp, declared_name);
    let readable = human_readable_harp(&harp, declared_name);
    let machine = canonicalize(&harp.harp)?;
    let sanitized_name = if request.redact_personal_identifiers && !declared_name.is_empty() {
        "[REDACTED BY USER]"
    } else {
        declared_name
    };
    let sanitized_worksheet = registration_worksheet(&harp, sanitized_name);
    let sanitized_readable = human_readable_harp(&harp, sanitized_name);

    let separate = vec![
        (
            "registration_worksheet",
            "registration-worksheet.md",
            worksheet.into_bytes(),
            "registration",
        ),
        (
            "human_readable_harp",
            "human-readable-harp.md",
            readable.into_bytes(),
            "registration",
        ),
        (
            "machine_readable_harp",
            "machine-readable-harp.json",
            machine,
            "registration",
        ),
        (
            "deposit_copy",
            "deposit-copy.md",
            deposit.clone(),
            "deposit",
        ),
    ];
    let mut artifacts = Vec::new();
    for (role, name, bytes, privacy) in &separate {
        let relative = format!("{export_directory}/{name}");
        write_relative(root, &relative, bytes)?;
        artifacts.push(artifact(role, &relative, bytes, privacy));
    }

    let binding = json!({
        "schema_version": CPL_SCHEMA_VERSION,
        "harp_id": harp_id,
        "harp_sha256": harp_sha256,
        "deposit_sha256": deposit_sha256,
        "contribution_map_id": harp.harp["contribution_map"]["contribution_map_id"],
        "contribution_map_sha256": harp.harp["contribution_map"]["contribution_map_sha256"],
        "source_chain_head": source_chain_head,
        "source_event_sequence": harp.harp["cpl_binding"]["event_sequence"],
        "applicability_status": harp.applicability_status,
        "verification_scope_statement": VERIFICATION_SCOPE,
        "legal_scope_statement": LEGAL_SCOPE_STATEMENT,
    });
    let verification_binding = sanitized_verification_binding(&harp);
    let omission_counts = omission_counts(root, &harp, request.redact_personal_identifiers)?;
    let omissions = omission_disclosures(
        &omission_counts,
        &source_chain_head,
        &harp_sha256,
        &deposit_sha256,
    )?;

    let mut sanitized_entries = vec![
        ArchiveEntry {
            path: "registration-worksheet.md".into(),
            bytes: sanitized_worksheet.into_bytes(),
        },
        ArchiveEntry {
            path: "human-readable-harp.md".into(),
            bytes: sanitized_readable.into_bytes(),
        },
        ArchiveEntry {
            path: "evidence/harp-binding.json".into(),
            bytes: canonicalize(&binding)?,
        },
        ArchiveEntry {
            path: "evidence/contribution-map.json".into(),
            bytes: contribution_map::canonical_map_bytes(&map.contribution_map)?,
        },
        ArchiveEntry {
            path: "evidence/verification-binding.json".into(),
            bytes: canonicalize(&verification_binding)?,
        },
    ];
    sanitized_entries.sort_by(|left, right| left.path.cmp(&right.path));
    let sanitized_inventory = inventory(&sanitized_entries);
    let rules_sha256 = canonical_digest(&Value::Array(omissions.clone()))?;
    let export_id = harp_id.replacen("harp_", "record_", 1);
    let sanitized_manifest = json!({
        "schema_version": CPL_SCHEMA_VERSION,
        "export_id": export_id,
        "project_id": project_id,
        "harp_id": harp_id,
        "harp_sha256": harp_sha256,
        "deposit_sha256": deposit_sha256,
        "source_chain_head": source_chain_head,
        "profile": "sanitized",
        "completeness_claim": SELECTIVE_CLAIM,
        "verification_scope_statement": VERIFICATION_SCOPE,
        "omission_rules": omissions,
        "rules_sha256": rules_sha256,
        "files": sanitized_inventory,
        "created_at": created_at,
    });
    let manifest_bytes = canonicalize(&sanitized_manifest)?;
    let sanitized_path = format!("{export_directory}/sanitized-supporting-archive.zip");
    write_archive(
        root,
        &sanitized_path,
        &sanitized_entries,
        "omission-manifest.json",
        &manifest_bytes,
    )?;
    let sanitized_archive = fs::read(resolve_relative(root, &sanitized_path))
        .map_err(|error| CplError::io("Could not read the sanitized archive", error))?;
    artifacts.push(artifact(
        "sanitized_supporting_archive",
        &sanitized_path,
        &sanitized_archive,
        "sanitized_evidence",
    ));

    let full_entries = full_private_entries(root, &separate, &harp, &map.deposit.deposit_path)?;
    let full_manifest = json!({
        "schema_version": CPL_SCHEMA_VERSION,
        "project_id": project_id,
        "harp_id": harp_id,
        "harp_sha256": harp_sha256,
        "deposit_sha256": deposit_sha256,
        "source_chain_head": source_chain_head,
        "profile": "full_private",
        "privacy_warning": "Contains private CPL records, ledger metadata, source bodies, and the exact deposit. Keep private unless each file has been reviewed.",
        "files": inventory(&full_entries),
        "created_at": created_at,
    });
    let full_path = format!("{export_directory}/full-private-archive.zip");
    write_archive(
        root,
        &full_path,
        &full_entries,
        "full-private-manifest.json",
        &canonicalize(&full_manifest)?,
    )?;
    let full_archive = fs::read(resolve_relative(root, &full_path))
        .map_err(|error| CplError::io("Could not read the full private archive", error))?;
    artifacts.push(artifact(
        "full_private_archive",
        &full_path,
        &full_archive,
        "private",
    ));

    let verification = verify_sanitized_archive(
        &resolve_relative(root, &sanitized_path),
        Some((&source_chain_head, &harp_sha256, &deposit_sha256)),
    )?;
    if verification.status != "verified_selective" {
        return Err(CplError::new(
            "HARP_SANITIZED_EXPORT_FAILED",
            "The sanitized archive did not pass retained-evidence verification.",
            false,
        ));
    }
    Ok(HarpExportResult {
        export_directory,
        artifacts,
        sanitized_manifest,
        sanitized_verification: verification,
    })
}

pub fn verify_sanitized_archive(
    path: &Path,
    expected: Option<(&str, &str, &str)>,
) -> CplResult<SanitizedArchiveVerification> {
    let archive_bytes = fs::read(path)
        .map_err(|error| CplError::io("Could not read the sanitized archive", error))?;
    let file = fs::File::open(path)
        .map_err(|error| CplError::io("Could not open the sanitized archive", error))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| CplError::new("HARP_ARCHIVE_INVALID", error.to_string(), false))?;
    let manifest: Value = {
        let mut item = archive.by_name("omission-manifest.json").map_err(|_| {
            CplError::new(
                "HARP_ARCHIVE_MANIFEST_MISSING",
                "The sanitized archive has no omission manifest.",
                false,
            )
        })?;
        let mut bytes = Vec::new();
        item.read_to_end(&mut bytes)
            .map_err(|error| CplError::io("Could not read the omission manifest", error))?;
        serde_json::from_slice(&bytes).map_err(|error| {
            CplError::new("HARP_ARCHIVE_MANIFEST_INVALID", error.to_string(), false)
        })?
    };
    let source_chain_head = required_string(&manifest, "source_chain_head")?;
    let harp_sha256 = required_string(&manifest, "harp_sha256")?;
    let deposit_sha256 = required_string(&manifest, "deposit_sha256")?;
    let mut findings = Vec::new();
    if manifest["completeness_claim"] != SELECTIVE_CLAIM {
        findings.push("The manifest does not declare a selective disclosed subset.".into());
    }
    if manifest["verification_scope_statement"] != VERIFICATION_SCOPE {
        findings.push("The verification scope statement is missing or changed.".into());
    }
    let omissions = manifest["omission_rules"].as_array().ok_or_else(|| {
        CplError::new(
            "HARP_ARCHIVE_OMISSIONS_INVALID",
            "The omission disclosure list is invalid.",
            false,
        )
    })?;
    if canonical_digest(&Value::Array(omissions.clone()))? != manifest["rules_sha256"] {
        findings.push("The omission-rule digest does not match.".into());
    }
    let categories = omissions
        .iter()
        .filter_map(|rule| rule["category"].as_str())
        .collect::<BTreeSet<_>>();
    for category in omission_categories() {
        if !categories.contains(category) {
            findings.push(format!("Omission category {category} is not disclosed."));
        }
    }
    for rule in omissions {
        let retained_identity = json!({
            "category": rule["category"],
            "source_chain_head": source_chain_head,
            "harp_sha256": harp_sha256,
            "deposit_sha256": deposit_sha256,
        });
        if canonical_digest(&retained_identity)? != rule["retained_binding_sha256"] {
            findings.push(format!(
                "Omission disclosure {} is not bound to the disclosed retained evidence.",
                rule["category"].as_str().unwrap_or("unknown")
            ));
        }
        let identity = json!({
            "category": rule["category"],
            "action": rule["action"],
            "count": rule["count"],
            "retained_binding_sha256": rule["retained_binding_sha256"],
        });
        if canonical_digest(&identity)? != rule["disclosure_sha256"] {
            findings.push(format!(
                "Omission disclosure {} has an invalid digest.",
                rule["category"].as_str().unwrap_or("unknown")
            ));
        }
    }
    let files = manifest["files"].as_array().ok_or_else(|| {
        CplError::new(
            "HARP_ARCHIVE_FILES_INVALID",
            "The retained-file inventory is invalid.",
            false,
        )
    })?;
    let mut retained_files = BTreeMap::<String, Vec<u8>>::new();
    for binding in files {
        let name = required_string(binding, "path")?;
        if name.contains('\\') || name.starts_with('/') || name.contains("../") {
            findings.push(format!("Unsafe retained path {name}."));
            continue;
        }
        match archive.by_name(&name) {
            Ok(mut item) => {
                let mut bytes = Vec::new();
                item.read_to_end(&mut bytes).map_err(|error| {
                    CplError::io("Could not read retained sanitized evidence", error)
                })?;
                if sha256_digest(&bytes) != binding["sha256"]
                    || bytes.len() as u64 != binding["size"].as_u64().unwrap_or(u64::MAX)
                {
                    findings.push(format!("Retained file {name} does not match its binding."));
                }
                retained_files.insert(name.clone(), bytes);
            }
            Err(_) => findings.push(format!("Retained file {name} is missing.")),
        }
    }
    match retained_files.get("evidence/harp-binding.json") {
        Some(bytes) => {
            let binding: Value = serde_json::from_slice(bytes).map_err(|error| {
                CplError::new("HARP_ARCHIVE_BINDING_INVALID", error.to_string(), false)
            })?;
            if binding["source_chain_head"] != source_chain_head
                || binding["harp_sha256"] != harp_sha256
                || binding["deposit_sha256"] != deposit_sha256
            {
                findings
                    .push("The retained HARP binding does not match the omission manifest.".into());
            }
            if let Some(map_bytes) = retained_files.get("evidence/contribution-map.json") {
                let mut map: Value = serde_json::from_slice(map_bytes).map_err(|error| {
                    CplError::new("HARP_ARCHIVE_MAP_INVALID", error.to_string(), false)
                })?;
                let recorded = map["contribution_map_sha256"].clone();
                map.as_object_mut()
                    .ok_or_else(|| {
                        CplError::new(
                            "HARP_ARCHIVE_MAP_INVALID",
                            "The retained contribution map is not an object.",
                            false,
                        )
                    })?
                    .remove("contribution_map_sha256");
                if canonical_digest(&map)? != recorded
                    || binding["contribution_map_sha256"] != recorded
                {
                    findings.push("The retained contribution map self-digest does not match the HARP binding.".into());
                }
            } else {
                findings.push("The retained contribution map is missing.".into());
            }
        }
        None => findings.push("The retained HARP binding is missing.".into()),
    }
    if let Some((chain, harp, deposit)) = expected {
        if source_chain_head != chain || harp_sha256 != harp || deposit_sha256 != deposit {
            findings.push(
                "The archive does not match the expected CPL, HARP, or deposit binding.".into(),
            );
        }
    }
    Ok(SanitizedArchiveVerification {
        status: if findings.is_empty() {
            "verified_selective".into()
        } else {
            "failed".into()
        },
        archive_sha256: sha256_digest(&archive_bytes),
        source_chain_head,
        harp_sha256,
        deposit_sha256,
        retained_file_count: files.len(),
        omission_disclosure_count: omissions.len(),
        completeness_claim: SELECTIVE_CLAIM.into(),
        verification_scope_statement: VERIFICATION_SCOPE.into(),
        findings,
    })
}

fn registration_worksheet(harp: &HarpProjection, declared_name: &str) -> String {
    let language = &harp.harp["suggested_registration_language"];
    format!(
        "# Registration worksheet\n\n- HARP: `{}`\n- Exact deposit: `{}`\n- CPL chain: `{}` at sequence `{}`\n- Applicant/author declaration: {}\n- Identity status: `{}`\n- HARP applicability: `{}`\n\n## Author Created\n\n{}\n\n## Material Excluded\n\n{}\n\n## New Material Included\n\n{}\n\n## Note to CO\n\n{}\n\nThis is suggested application language, not legal advice.\n\n{}\n",
        harp.harp["harp_id"].as_str().unwrap_or("unknown"),
        harp.harp["deposit"]["deposit_sha256"].as_str().unwrap_or("unknown"),
        harp.harp["cpl_binding"]["chain_head"].as_str().unwrap_or("unknown"),
        harp.harp["cpl_binding"]["event_sequence"],
        if declared_name.is_empty() { "Not declared" } else { declared_name },
        harp.harp["identity_declaration"]["identity_status"].as_str().unwrap_or("unknown"),
        harp.applicability_status,
        language["author_created"].as_str().unwrap_or_default(),
        language["material_excluded"].as_str().unwrap_or_default(),
        language["new_material_included"].as_str().unwrap_or_default(),
        language["note_to_co"].as_str().unwrap_or("None"),
        LEGAL_SCOPE_STATEMENT,
    )
}

fn human_readable_harp(harp: &HarpProjection, declared_name: &str) -> String {
    format!(
        "# Human Authorship Record of Provenance\n\n- HARP ID: `{}`\n- Exact deposit digest: `{}`\n- Manuscript revision: `{}`\n- CPL chain head / sequence: `{}` / `{}`\n- Evidentiary status: `{}`\n- Applicability: `{}`\n- Declared author: {}\n- Identity status: `{}`\n\n## Evidence claim\n\n{}\n\n## Coverage\n\n{}\n\nThis record verifies provenance integrity and retained evidence relationships. It does not report a human-authorship percentage and does not claim complete private history in a sanitized archive.\n\n{}\n",
        harp.harp["harp_id"].as_str().unwrap_or("unknown"),
        harp.harp["deposit"]["deposit_sha256"].as_str().unwrap_or("unknown"),
        harp.harp["deposit"]["manuscript_revision_id"].as_str().unwrap_or("unknown"),
        harp.harp["cpl_binding"]["chain_head"].as_str().unwrap_or("unknown"),
        harp.harp["cpl_binding"]["event_sequence"],
        harp.harp["evidentiary_status"].as_str().unwrap_or("unknown"),
        harp.applicability_status,
        if declared_name.is_empty() { "Not declared" } else { declared_name },
        harp.harp["identity_declaration"]["identity_status"].as_str().unwrap_or("unknown"),
        harp.harp["claim_summary"].as_str().unwrap_or_default(),
        harp.harp["coverage"]["statement"].as_str().unwrap_or_default(),
        LEGAL_SCOPE_STATEMENT,
    )
}

fn sanitized_verification_binding(harp: &HarpProjection) -> Value {
    json!({
        "schema_version": CPL_SCHEMA_VERSION,
        "harp_id": harp.harp["harp_id"],
        "harp_sha256": harp.harp["harp_sha256"],
        "native_cpl_status": harp.verification_artifact["native_cpl_verification"]["status"],
        "event_count": harp.verification_artifact["native_cpl_verification"]["event_count"],
        "record_count": harp.verification_artifact["native_cpl_verification"]["record_count"],
        "dependency_checks": harp.verification_artifact["dependency_checks"],
        "verification_scope_statement": VERIFICATION_SCOPE,
    })
}

fn omission_counts(
    root: &Path,
    harp: &HarpProjection,
    redact_personal_identifiers: bool,
) -> CplResult<BTreeMap<String, usize>> {
    let mut counts = omission_categories()
        .iter()
        .map(|category| ((*category).to_owned(), 0usize))
        .collect::<BTreeMap<_, _>>();
    for event in ledger::read_all_events(&LedgerPaths::new(root))? {
        for reference in event.record_references {
            let bytes = fs::read(resolve_relative(root, &reference.path))
                .map_err(|error| CplError::io("Could not inspect a private CPL record", error))?;
            let record: CplRecord = serde_json::from_slice(&bytes).map_err(|error| {
                CplError::new("HARP_EXPORT_RECORD_INVALID", error.to_string(), false)
            })?;
            let kind = record.payload["kind"].as_str().unwrap_or_default();
            if matches!(kind, "HUMAN_TURN_CREATED" | "ASSISTANT_TURN_CREATED") {
                counts
                    .entry("private_conversation".into())
                    .and_modify(|value| *value += 1);
            }
            if kind == "CLOUD_APPROVAL_CHANGED" {
                counts
                    .entry("credential_authorization_material".into())
                    .and_modify(|value| *value += 1);
            }
            if matches!(
                kind,
                "PROVIDER_CONTEXT_CHANGED" | "PROVIDER_INVOCATION_REQUESTED"
            ) {
                counts
                    .entry("provider_metadata_not_required".into())
                    .and_modify(|value| *value += 1);
            }
            if contains_rejected_output(&record.payload) {
                counts
                    .entry("rejected_model_output".into())
                    .and_modify(|value| *value += 1);
            }
            if matches!(
                reference.record_type.as_str(),
                "composition-content" | "composition-initialization" | "phase1-operation"
            ) {
                counts
                    .entry("protected_source_body".into())
                    .and_modify(|value| *value += 1);
            }
            counts
                .entry("internal_path".into())
                .and_modify(|value| *value += 1);
        }
    }
    counts
        .entry("protected_source_body".into())
        .and_modify(|value| *value += 1);
    if redact_personal_identifiers
        && harp.harp["identity_declaration"]["declared_name"]
            .as_str()
            .is_some_and(|name| !name.is_empty())
    {
        counts.insert("personal_identifier".into(), 1);
    }
    Ok(counts)
}

fn contains_rejected_output(value: &Value) -> bool {
    match value {
        Value::Object(values) => {
            values.get("disposition").and_then(Value::as_str) == Some("rejected")
                || values
                    .get("rejectedRanges")
                    .or_else(|| values.get("rejected_ranges"))
                    .and_then(Value::as_array)
                    .is_some_and(|ranges| !ranges.is_empty())
                || values.values().any(contains_rejected_output)
        }
        Value::Array(values) => values.iter().any(contains_rejected_output),
        _ => false,
    }
}

fn omission_disclosures(
    counts: &BTreeMap<String, usize>,
    source_chain_head: &str,
    harp_sha256: &str,
    deposit_sha256: &str,
) -> CplResult<Vec<Value>> {
    omission_categories()
        .iter()
        .map(|category| {
            let action = if *category == "personal_identifier" {
                "redact"
            } else {
                "exclude"
            };
            let retained = json!({
                "category": category,
                "source_chain_head": source_chain_head,
                "harp_sha256": harp_sha256,
                "deposit_sha256": deposit_sha256,
            });
            let identity = json!({
                "category": category,
                "action": action,
                "count": counts.get(*category).copied().unwrap_or_default(),
                "retained_binding_sha256": canonical_digest(&retained)?,
            });
            let mut disclosure = identity.clone();
            disclosure["disclosure_sha256"] = Value::String(canonical_digest(&identity)?);
            Ok(disclosure)
        })
        .collect()
}

fn omission_categories() -> &'static [&'static str] {
    &[
        "private_conversation",
        "rejected_model_output",
        "credential_authorization_material",
        "personal_identifier",
        "internal_path",
        "provider_metadata_not_required",
        "protected_source_body",
    ]
}

fn full_private_entries(
    root: &Path,
    separate: &[(&str, &str, Vec<u8>, &str)],
    harp: &HarpProjection,
    deposit_path: &str,
) -> CplResult<Vec<ArchiveEntry>> {
    let mut entries = separate
        .iter()
        .map(|(_, name, bytes, _)| ArchiveEntry {
            path: (*name).into(),
            bytes: bytes.clone(),
        })
        .collect::<Vec<_>>();
    let roots = ["records", "provenance/ledger", &harp.report_directory];
    for relative_root in roots {
        let absolute_root = resolve_relative(root, relative_root);
        if !absolute_root.exists() {
            continue;
        }
        for entry in WalkDir::new(&absolute_root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|error| {
                    CplError::new("HARP_EXPORT_PATH_INVALID", error.to_string(), false)
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = fs::read(entry.path())
                .map_err(|error| CplError::io("Could not read private HARP evidence", error))?;
            entries.push(ArchiveEntry {
                path: format!("project/{relative}"),
                bytes,
            });
        }
    }
    if !entries.iter().any(|entry| entry.path == "deposit-copy.md") {
        entries.push(ArchiveEntry {
            path: "deposit-copy.md".into(),
            bytes: fs::read(resolve_relative(root, deposit_path))
                .map_err(|error| CplError::io("Could not read the private deposit", error))?,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    entries.dedup_by(|left, right| left.path == right.path);
    Ok(entries)
}

fn inventory(entries: &[ArchiveEntry]) -> Vec<Value> {
    entries
        .iter()
        .map(|entry| {
            json!({
                "path": entry.path,
                "sha256": sha256_digest(&entry.bytes),
                "size": entry.bytes.len(),
            })
        })
        .collect()
}

fn write_archive(
    root: &Path,
    relative: &str,
    entries: &[ArchiveEntry],
    manifest_name: &str,
    manifest: &[u8],
) -> CplResult<()> {
    let destination = resolve_relative(root, relative);
    let parent = destination.parent().ok_or_else(|| {
        CplError::new(
            "HARP_EXPORT_PATH_INVALID",
            "The archive path has no parent.",
            false,
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| CplError::io("Could not create the HARP export directory", error))?;
    let temporary = destination.with_extension("zip.tmp");
    let file = fs::File::create(&temporary)
        .map_err(|error| CplError::io("Could not stage the HARP archive", error))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for entry in entries {
        zip.start_file(&entry.path, options)
            .map_err(|error| CplError::new("HARP_ARCHIVE_WRITE_FAILED", error.to_string(), true))?;
        zip.write_all(&entry.bytes)
            .map_err(|error| CplError::io("Could not write a HARP archive entry", error))?;
    }
    zip.start_file(manifest_name, options)
        .map_err(|error| CplError::new("HARP_ARCHIVE_WRITE_FAILED", error.to_string(), true))?;
    zip.write_all(manifest)
        .map_err(|error| CplError::io("Could not write the HARP archive manifest", error))?;
    let mut output = zip
        .finish()
        .map_err(|error| CplError::new("HARP_ARCHIVE_WRITE_FAILED", error.to_string(), true))?;
    output
        .flush()
        .map_err(|error| CplError::io("Could not flush the HARP archive", error))?;
    output
        .sync_all()
        .map_err(|error| CplError::io("Could not sync the HARP archive", error))?;
    ledger::atomic_replace(&temporary, &destination)?;
    ledger::sync_directory(parent)?;
    Ok(())
}

fn write_relative(root: &Path, relative: &str, bytes: &[u8]) -> CplResult<()> {
    let destination = resolve_relative(root, relative);
    let parent = destination.parent().ok_or_else(|| {
        CplError::new(
            "HARP_EXPORT_PATH_INVALID",
            "The export path has no parent.",
            false,
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| CplError::io("Could not create the HARP export directory", error))?;
    let temporary = parent.join(format!(
        ".{}.tmp",
        destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    ));
    fs::write(&temporary, bytes)
        .map_err(|error| CplError::io("Could not stage a HARP export artifact", error))?;
    ledger::atomic_replace(&temporary, &destination)?;
    ledger::sync_directory(parent)?;
    Ok(())
}

fn artifact(role: &str, path: &str, bytes: &[u8], privacy: &str) -> HarpExportArtifact {
    HarpExportArtifact {
        role: role.into(),
        path: path.into(),
        sha256: sha256_digest(bytes),
        byte_length: bytes.len() as u64,
        privacy_classification: privacy.into(),
    }
}

fn required_string(value: &Value, field: &str) -> CplResult<String> {
    value[field].as_str().map(str::to_owned).ok_or_else(|| {
        CplError::new(
            "HARP_EXPORT_BINDING_INVALID",
            format!("The HARP export is missing {field}."),
            false,
        )
    })
}

fn resolve_relative(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

fn serialization_error(error: impl std::fmt::Display) -> CplError {
    CplError::new("CPL_EXPORT_INVALID", error.to_string(), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_binding_detects_changes() {
        let mut binding = ExportBinding {
            source_chain_head_sha256: "sha256:a".into(),
            sanitization_profile: "sanitized".into(),
            omissions: vec!["private_conversation".into()],
            files: vec![],
            content_sha256: String::new(),
        };
        let identity = serde_json::to_value((
            &binding.source_chain_head_sha256,
            &binding.sanitization_profile,
            &binding.omissions,
            &binding.files,
        ))
        .unwrap();
        binding.content_sha256 = canonical_digest(&identity).unwrap();
        assert!(binding.verify().unwrap());
        binding.omissions.push("protected_source_body".into());
        assert!(!binding.verify().unwrap());
    }

    #[test]
    fn rejection_detection_is_structural() {
        assert!(contains_rejected_output(&json!({
            "disposition": "rejected"
        })));
        assert!(contains_rejected_output(&json!({
            "nested": { "rejectedRanges": [{"start": 0, "end": 2}] }
        })));
        assert!(!contains_rejected_output(&json!({
            "disposition": "accepted"
        })));
    }

    #[test]
    fn creates_six_separate_artifacts_and_verifies_selective_archive_without_mutation() {
        use crate::provenance::{
            composition::{
                CompositionAction, CompositionBoundary, CompositionCommand, RecordedOrigin,
            },
            contribution_map::ContributionMapRequest,
            harp::{HarpGenerationRequest, RegistrationLanguageInput},
            identifiers::timestamp_millis,
        };

        let temp = tempfile::tempdir().unwrap();
        let project_id = "project_01J00000000000000000000010";
        let apply = |id: &str, action: CompositionAction| {
            crate::provenance::composition::apply_command(
                temp.path(),
                project_id,
                CompositionCommand {
                    client_action_id: id.into(),
                    actor: "user".into(),
                    summary: id.into(),
                    occurred_at: timestamp_millis(),
                    action,
                },
            )
            .unwrap();
        };
        apply(
            "export_initialize",
            CompositionAction::Initialize {
                text: String::new(),
                origin: RecordedOrigin::Unattested,
            },
        );
        apply(
            "export_human_edit",
            CompositionAction::Edit {
                before_text: String::new(),
                after_text: "Exact registered expression.".into(),
                boundary: CompositionBoundary::ExplicitSave,
                origin: RecordedOrigin::RecordedDirectHumanInput,
                operation_kind_hint: None,
                ai_acceptance: None,
            },
        );
        crate::provenance::contribution_map::freeze_current(
            temp.path(),
            project_id,
            ContributionMapRequest::default(),
        )
        .unwrap();
        crate::provenance::harp::generate_current(
            temp.path(),
            project_id,
            HarpGenerationRequest {
                declared_name: Some("Example Author".into()),
                identity_status: "self_declared".into(),
                identity_evidence_reference_ids: vec![],
                sanitization_profile: "full_private".into(),
                suggested_registration_language: RegistrationLanguageInput::default(),
                user_approved: true,
            },
        )
        .unwrap();
        let head_before = ledger::read_chain_head(&LedgerPaths::new(temp.path()))
            .unwrap()
            .unwrap();

        let result = create_harp_exports(
            temp.path(),
            project_id,
            HarpExportRequest {
                redact_personal_identifiers: true,
            },
        )
        .unwrap();
        let roles = result
            .artifacts
            .iter()
            .map(|artifact| artifact.role.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            roles,
            BTreeSet::from([
                "registration_worksheet",
                "human_readable_harp",
                "machine_readable_harp",
                "deposit_copy",
                "sanitized_supporting_archive",
                "full_private_archive",
            ])
        );
        assert_eq!(result.sanitized_verification.status, "verified_selective");
        assert_eq!(result.sanitized_verification.omission_disclosure_count, 7);
        assert_eq!(
            result.sanitized_manifest["completeness_claim"],
            SELECTIVE_CLAIM
        );
        let head_after = ledger::read_chain_head(&LedgerPaths::new(temp.path()))
            .unwrap()
            .unwrap();
        assert_eq!(
            head_before, head_after,
            "export must not mutate the CPL chain"
        );

        let sanitized = result
            .artifacts
            .iter()
            .find(|artifact| artifact.role == "sanitized_supporting_archive")
            .unwrap();
        let file = fs::File::open(resolve_relative(temp.path(), &sanitized.path)).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let names = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_owned())
            .collect::<Vec<_>>();
        assert!(!names.iter().any(|name| name.contains("records/")
            || name.contains("ledger/")
            || name == "deposit-copy.md"
            || name == "machine-readable-harp.json"));
        let mut readable = String::new();
        archive
            .by_name("human-readable-harp.md")
            .unwrap()
            .read_to_string(&mut readable)
            .unwrap();
        assert!(!readable.contains("Example Author"));
        assert!(readable.contains("[REDACTED BY USER]"));

        let private = result
            .artifacts
            .iter()
            .find(|artifact| artifact.role == "full_private_archive")
            .unwrap();
        let file = fs::File::open(resolve_relative(temp.path(), &private.path)).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let names = (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_owned())
            .collect::<Vec<_>>();
        assert!(names
            .iter()
            .any(|name| name.starts_with("project/records/")));
        assert!(names
            .iter()
            .any(|name| name.starts_with("project/provenance/ledger/")));
        assert!(names.iter().any(|name| name == "deposit-copy.md"));
    }
}
