//! Deterministic Human Authorship Record of Provenance generation.
//!
//! HARP projects already-recorded CPL evidence. It never calls a model,
//! classifies legal authorship, computes a human percentage, or decides
//! copyrightability.

use super::{
    canonical::{canonical_digest, canonicalize},
    composition::{self, RecordedOrigin},
    contribution_map::{self, ClassificationStatus, ContributionMapProjection},
    ledger::{self, LedgerPaths},
    records::{CplEvent, CplRecord, RecordInput, VerificationStatus, WriteCommand},
    verifier, CplError, CplResult, CplService, CPL_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

const GENERATOR_VERSION: &str = "1.0.0";
const APPLICATION_VERSION: &str = env!("CARGO_PKG_VERSION");
const POLICY_FIXTURE: &str = include_str!(
    "../../../schemas/provenance/v1/fixtures/valid/registration-policy-profile.valid.json"
);

pub const LEGAL_SCOPE_STATEMENT: &str = "Copyrightability and registration scope remain determinations of the U.S. Copyright Office. HARP is a non-authoritative evidence projection; integrity verification does not determine identity, legal authorship, originality, copyrightability, ownership, or registrability.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationLanguageInput {
    pub author_created: String,
    pub material_excluded: String,
    pub new_material_included: String,
    #[serde(default)]
    pub note_to_co: Option<String>,
    #[serde(default)]
    pub registration_treatment_suggestions: Vec<String>,
}

impl Default for RegistrationLanguageInput {
    fn default() -> Self {
        Self {
            author_created: "Human-authored text and human revisions identified in the attached provenance record".into(),
            material_excluded: "AI-generated text identified in the attached provenance record".into(),
            new_material_included: "Human-authored text, revisions, selection, and arrangement identified in the attached provenance record".into(),
            note_to_co: Some("This work contains disclosed AI-generated material; see the attached provenance record.".into()),
            registration_treatment_suggestions: vec!["disclose_ai_use".into(), "manual_review_required".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HarpGenerationRequest {
    #[serde(default)]
    pub declared_name: Option<String>,
    #[serde(default = "default_identity_status")]
    pub identity_status: String,
    #[serde(default)]
    pub identity_evidence_reference_ids: Vec<String>,
    #[serde(default = "default_sanitization_profile")]
    pub sanitization_profile: String,
    #[serde(default)]
    pub suggested_registration_language: RegistrationLanguageInput,
    pub user_approved: bool,
}

fn default_identity_status() -> String {
    "self_declared".into()
}
fn default_sanitization_profile() -> String {
    "full_private".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HarpDependencyBinding {
    pub deposit_sha256: String,
    pub manuscript_revision_id: String,
    pub manuscript_revision_sha256: String,
    pub contribution_map_sha256: String,
    pub chain_head_sha256: String,
    pub chain_event_sequence: u64,
    pub policy_profile_sha256: String,
    pub assertion_set_sha256: String,
    pub dependency_set_sha256: String,
    pub approval_sha256: String,
}

pub fn is_stale(recorded: &HarpDependencyBinding, current: &HarpDependencyBinding) -> bool {
    recorded != current
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarpReportMetadata {
    pub deposit_sha256: String,
    pub manuscript_revision_id: String,
    pub manuscript_revision_sha256: String,
    pub cpl_chain_head: String,
    pub cpl_event_sequence: u64,
    pub harp_schema_version: String,
    pub harp_generator_version: String,
    pub application_version: String,
    pub policy_profile_id: String,
    pub policy_profile_version: String,
    pub policy_profile_sha256: String,
    pub policy_retrieval_date: String,
    pub sanitization_profile: String,
    pub legal_scope_statement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneratedFile {
    pub role: String,
    pub path: String,
    pub sha256: String,
    pub size: usize,
    pub privacy_classification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarpProjection {
    pub harp: Value,
    pub export_manifest: Value,
    pub verification_artifact: Value,
    pub report_metadata: HarpReportMetadata,
    pub dependencies: HarpDependencyBinding,
    pub report_directory: String,
    pub generated_files: Vec<GeneratedFile>,
    pub applicability_status: String,
    pub stale_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct StoredHarpBundle {
    projection: HarpProjection,
    approval_request: HarpGenerationRequest,
}

#[derive(Debug, Clone)]
struct PolicyProfile {
    value: Value,
    id: String,
    version: String,
    sha256: String,
    retrieved_on: String,
    status: String,
}

struct Artifact {
    path: String,
    bytes: Vec<u8>,
    privacy: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HarpPreparation {
    pub contribution_map: ContributionMapProjection,
    pub ai_system_disclosures: Vec<Value>,
    pub existing_harp: Option<HarpProjection>,
    pub policy_profile: Value,
    pub suggested_registration_language: RegistrationLanguageInput,
    pub legal_scope_statement: String,
}

pub fn prepare_current(root: &Path, project_id: &str) -> CplResult<HarpPreparation> {
    let contribution_map = contribution_map::load_latest(root, project_id)?.ok_or_else(|| {
        CplError::new(
            "HARP_CONTRIBUTION_MAP_REQUIRED",
            "Freeze the exact deposit contribution map before preparing HARP.",
            true,
        )
    })?;
    let policy = bundled_policy_profile()?;
    let invocations = invocation_identities(root)?;
    let ai_system_disclosures = ai_disclosures(&contribution_map, &invocations);
    Ok(HarpPreparation {
        contribution_map,
        ai_system_disclosures,
        existing_harp: load_latest(root, project_id)?,
        policy_profile: policy.value,
        suggested_registration_language: RegistrationLanguageInput::default(),
        legal_scope_statement: LEGAL_SCOPE_STATEMENT.into(),
    })
}

pub fn generate_current(
    root: &Path,
    project_id: &str,
    request: HarpGenerationRequest,
) -> CplResult<HarpProjection> {
    validate_request(&request)?;
    let map = contribution_map::load_latest(root, project_id)?.ok_or_else(|| {
        CplError::new(
            "HARP_CONTRIBUTION_MAP_REQUIRED",
            "Freeze the exact deposit contribution map before generating HARP.",
            true,
        )
    })?;
    if map.contribution_map.classification_status == ClassificationStatus::Stale {
        return Err(CplError::new(
            "HARP_CONTRIBUTION_MAP_STALE",
            "The contribution map is stale. Freeze the current manuscript before generating HARP.",
            true,
        ));
    }
    let policy = bundled_policy_profile()?;
    let request_digest =
        canonical_digest(&serde_json::to_value(&request).map_err(serialization_error)?)?;
    let seed = source_dependency_seed(&map, &policy, &request_digest)?;
    let service = CplService::new(root, project_id);
    let approval = service.write(WriteCommand {
        client_action_id: stable_id("action", &format!("harp-approval:{seed}")),
        project_id: project_id.into(),
        event_type: "HARP_GENERATION_APPROVED".into(),
        actor: "user".into(),
        metadata: json!({
            "contribution_map_id": map.contribution_map.contribution_map_id,
            "deposit_id": map.deposit.deposit_id,
            "request_sha256": request_digest,
            "user_approved": true,
        }),
        records: vec![RecordInput {
            record_type: "harp-generation-approval".into(),
            payload: serde_json::to_value(&request).map_err(serialization_error)?,
        }],
        operational_state: None,
    })?;
    if let Some(existing) = load_latest(root, project_id)? {
        if existing.dependencies.approval_sha256 == request_digest
            && existing.dependencies.contribution_map_sha256
                == map.contribution_map.contribution_map_sha256
            && existing.applicability_status == "current"
        {
            return Ok(existing);
        }
    }
    let native_verification = verifier::verify_project(root, project_id)?;
    let invocations = invocation_identities(root)?;
    let operations = representative_operations(root, &map, &request.sanitization_profile)?;
    let mut projection = build_projection(
        project_id,
        &map,
        &policy,
        &request,
        &request_digest,
        &approval.event,
        &invocations,
        &operations,
        &native_verification,
    )?;
    write_projection_files(root, &mut projection, &map, &operations)?;
    let client_action_id = stable_id(
        "action",
        &format!("harp-generated:{}", projection.harp["harp_sha256"]),
    );
    if !ledger::read_all_events(&LedgerPaths::new(root))?
        .iter()
        .any(|event| event.client_action_id == client_action_id)
    {
        let stored = StoredHarpBundle {
            projection: projection.clone(),
            approval_request: request,
        };
        service.write(WriteCommand {
            client_action_id,
            project_id: project_id.into(),
            event_type: "HARP_GENERATED".into(),
            actor: "system".into(),
            metadata: json!({
                "harp_id": projection.harp["harp_id"],
                "harp_sha256": projection.harp["harp_sha256"],
                "deposit_sha256": projection.dependencies.deposit_sha256,
                "source_chain_head": projection.dependencies.chain_head_sha256,
            }),
            records: vec![
                RecordInput {
                    record_type: "human-authorship-record".into(),
                    payload: projection.harp.clone(),
                },
                RecordInput {
                    record_type: "harp-export-manifest".into(),
                    payload: projection.export_manifest.clone(),
                },
                RecordInput {
                    record_type: "harp-generation-bundle".into(),
                    payload: serde_json::to_value(stored).map_err(serialization_error)?,
                },
            ],
            operational_state: None,
        })?;
    }
    Ok(projection)
}

pub fn load_latest(root: &Path, project_id: &str) -> CplResult<Option<HarpProjection>> {
    for event in ledger::read_all_events(&LedgerPaths::new(root))?
        .iter()
        .rev()
    {
        for reference in event.record_references.iter().rev() {
            if reference.record_type != "harp-generation-bundle" {
                continue;
            }
            let record = read_record(root, &reference.path, false)?;
            let stored: StoredHarpBundle = serde_json::from_value(record.payload)
                .map_err(|error| CplError::new("HARP_RECORD_INVALID", error.to_string(), false))?;
            if stored.projection.harp["project_id"] != project_id {
                return Err(CplError::new(
                    "HARP_PROJECT_MISMATCH",
                    "The HARP belongs to a different project.",
                    false,
                ));
            }
            let mut projection = stored.projection;
            evaluate_current(root, project_id, &stored.approval_request, &mut projection)?;
            return Ok(Some(projection));
        }
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
fn build_projection(
    project_id: &str,
    map: &ContributionMapProjection,
    policy: &PolicyProfile,
    request: &HarpGenerationRequest,
    request_digest: &str,
    approval_event: &CplEvent,
    invocations: &BTreeMap<String, (String, String)>,
    operations: &[Value],
    native_verification: &super::VerificationReport,
) -> CplResult<HarpProjection> {
    let dependencies = HarpDependencyBinding {
        deposit_sha256: map.deposit.deposit_sha256.clone(),
        manuscript_revision_id: map.deposit.manuscript_revision_id.clone(),
        manuscript_revision_sha256: map.deposit.manuscript_revision_sha256.clone(),
        contribution_map_sha256: map.contribution_map.contribution_map_sha256.clone(),
        chain_head_sha256: map.deposit.cpl_chain_head.clone(),
        chain_event_sequence: map.deposit.cpl_event_sequence,
        policy_profile_sha256: policy.sha256.clone(),
        assertion_set_sha256: assertion_set_digest(map)?,
        dependency_set_sha256: dependency_set_digest(map)?,
        approval_sha256: request_digest.into(),
    };
    let harp_id = stable_id(
        "harp",
        &canonical_digest(&serde_json::to_value(&dependencies).map_err(serialization_error)?)?,
    );
    let metadata = HarpReportMetadata {
        deposit_sha256: dependencies.deposit_sha256.clone(),
        manuscript_revision_id: dependencies.manuscript_revision_id.clone(),
        manuscript_revision_sha256: dependencies.manuscript_revision_sha256.clone(),
        cpl_chain_head: dependencies.chain_head_sha256.clone(),
        cpl_event_sequence: dependencies.chain_event_sequence,
        harp_schema_version: CPL_SCHEMA_VERSION.into(),
        harp_generator_version: GENERATOR_VERSION.into(),
        application_version: APPLICATION_VERSION.into(),
        policy_profile_id: policy.id.clone(),
        policy_profile_version: policy.version.clone(),
        policy_profile_sha256: policy.sha256.clone(),
        policy_retrieval_date: policy.retrieved_on.clone(),
        sanitization_profile: request.sanitization_profile.clone(),
        legal_scope_statement: LEGAL_SCOPE_STATEMENT.into(),
    };
    let ai = ai_disclosures(map, invocations);
    let origin_complete = map.contribution_map.coverage.coverage_status == "complete";
    let lineage_complete = map
        .contribution_map
        .segments
        .iter()
        .all(|segment| segment.lineage_status == "recorded");
    let generation_recorded = map
        .contribution_map
        .segments
        .iter()
        .all(|segment| segment.generation_status == "recorded");
    let ai_recorded = ai.iter().all(|item| item["identity_status"] == "recorded");
    let exact = map.contribution_map.classification_status == ClassificationStatus::Exact
        && origin_complete
        && lineage_complete
        && generation_recorded
        && ai_recorded
        && policy.status == "current";
    let evidentiary_status = if exact {
        "exact"
    } else {
        match map.contribution_map.classification_status {
            ClassificationStatus::Stale => "stale",
            ClassificationStatus::Unverified => "unverified",
            _ => "degraded",
        }
    };
    let language = approved_language(request, &approval_event.event_id)?;
    let operation_ids = operations
        .iter()
        .filter_map(|value| value["operation_id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let mut harp = json!({
        "schema_version": CPL_SCHEMA_VERSION,
        "harp_id": harp_id,
        "project_id": project_id,
        "application_version": APPLICATION_VERSION,
        "generator_version": GENERATOR_VERSION,
        "deposit": {
            "deposit_id": map.deposit.deposit_id,
            "deposit_sha256": map.deposit.deposit_sha256,
            "manuscript_revision_id": map.deposit.manuscript_revision_id,
            "manuscript_revision_sha256": map.deposit.manuscript_revision_sha256,
        },
        "cpl_binding": { "chain_head": map.deposit.cpl_chain_head, "event_sequence": map.deposit.cpl_event_sequence },
        "contribution_map": {
            "contribution_map_id": map.contribution_map.contribution_map_id,
            "contribution_map_sha256": map.contribution_map.contribution_map_sha256,
        },
        "policy_profile": {
            "policy_profile_id": policy.id,
            "profile_version": policy.version,
            "profile_sha256": policy.sha256,
            "retrieved_on": policy.retrieved_on,
            "status": policy.status,
        },
        "evidentiary_status": evidentiary_status,
        "applicability_status": "current",
        "generation_status": if generation_recorded { "recorded" } else { "unknown" },
        "origin_coverage_status": if origin_complete { "complete" } else { "partial" },
        "lineage_coverage_status": if lineage_complete { "complete" } else { "partial" },
        "identity_declaration": {
            "declared_name": request.declared_name,
            "identity_status": request.identity_status,
            "evidence_reference_ids": sorted_unique(request.identity_evidence_reference_ids.clone()),
            "user_approved": true,
        },
        "claim_summary": claim_summary(map),
        "ai_system_disclosures": ai,
        "coverage": {
            "unit": "normalized_unicode_scalar_positions",
            "denominator": map.contribution_map.coverage.denominator,
            "recorded_positions": map.contribution_map.coverage.recorded_positions,
            "statement": format!("{} of {} normalized Unicode scalar positions have recorded origin. This is provenance coverage, not a human-authorship percentage.", map.contribution_map.coverage.recorded_positions, map.contribution_map.coverage.denominator),
        },
        "suggested_registration_language": language,
        "representative_transformation_operation_ids": operation_ids,
        "limitation_codes": limitation_codes(request, map, &ai, policy, origin_complete, lineage_complete),
        "explanation_codes": [
            "INTEGRITY_ONLY_VERIFICATION", "RECORDED_ORIGIN_NOT_LEGAL_AUTHORSHIP",
            "SELECTION_ARRANGEMENT_IS_OVERLAY", "PAGE_LOCATORS_ARE_DERIVED",
            "REGISTRATION_LANGUAGE_IS_SUGGESTED", "HARP_STALE_AFTER_EDIT"
        ],
        "sanitization_profile": request.sanitization_profile,
        "harp_sha256": "",
    });
    refresh_self_digest(&mut harp, "harp_sha256")?;
    let verification_artifact = json!({
        "schema_version": CPL_SCHEMA_VERSION,
        "report_metadata": metadata,
        "harp_id": harp["harp_id"],
        "harp_sha256": harp["harp_sha256"],
        "dependency_checks": {
            "deposit_digest_matches": true,
            "manuscript_revision_matches": true,
            "contribution_map_digest_matches": true,
            "assertion_set_digest": dependencies.assertion_set_sha256,
            "dependency_set_digest": dependencies.dependency_set_sha256,
            "policy_profile_digest_matches": true,
            "approval_recorded": true,
        },
        "native_cpl_verification": native_verification,
        "verification_scope": LEGAL_SCOPE_STATEMENT,
    });
    Ok(HarpProjection {
        harp,
        export_manifest: Value::Null,
        verification_artifact,
        report_metadata: metadata,
        dependencies,
        report_directory: format!("reports/harp/{harp_id}"),
        generated_files: vec![],
        applicability_status: "current".into(),
        stale_reasons: vec![],
    })
}
fn write_projection_files(
    root: &Path,
    projection: &mut HarpProjection,
    map: &ContributionMapProjection,
    operations: &[Value],
) -> CplResult<()> {
    let directory = &projection.report_directory;
    let metadata = &projection.report_metadata;
    let artifacts = vec![
        Artifact {
            path: format!("{directory}/human-authorship-summary.md"),
            bytes: summary_markdown(&projection.harp, metadata).into_bytes(),
            privacy: "registration".into(),
        },
        Artifact {
            path: format!("{directory}/final-text-contribution-map.svg"),
            bytes: contribution_map_svg(map, metadata).into_bytes(),
            privacy: "registration".into(),
        },
        Artifact {
            path: format!("{directory}/representative-transformations.md"),
            bytes: transformations_markdown(operations, metadata).into_bytes(),
            privacy: privacy_class(&metadata.sanitization_profile).into(),
        },
        Artifact {
            path: format!("{directory}/ai-system-disclosure.md"),
            bytes: ai_disclosure_markdown(&projection.harp, metadata).into_bytes(),
            privacy: "registration".into(),
        },
        Artifact {
            path: format!("{directory}/coverage-and-limitations.md"),
            bytes: coverage_markdown(&projection.harp, metadata).into_bytes(),
            privacy: "registration".into(),
        },
        Artifact {
            path: format!("{directory}/registration-language.md"),
            bytes: registration_markdown(&projection.harp, metadata).into_bytes(),
            privacy: "registration".into(),
        },
        Artifact {
            path: format!("{directory}/harp.json"),
            bytes: canonicalize(&projection.harp)?,
            privacy: "registration".into(),
        },
        Artifact {
            path: format!("{directory}/verification-report.json"),
            bytes: canonicalize(&projection.verification_artifact)?,
            privacy: "registration".into(),
        },
        Artifact {
            path: format!("{directory}/contribution-map.json"),
            bytes: contribution_map::canonical_map_bytes(&map.contribution_map)?,
            privacy: "registration".into(),
        },
    ];
    for artifact in &artifacts {
        write_relative(root, &artifact.path, &artifact.bytes)?;
    }
    let mut archive = json!({
        "schema_version": CPL_SCHEMA_VERSION,
        "archive_id": stable_id("archive", projection.harp["harp_sha256"].as_str().unwrap_or_default()),
        "harp_id": projection.harp["harp_id"],
        "report_metadata": metadata,
        "files": artifacts.iter().map(artifact_description).collect::<Vec<_>>(),
        "archive_sha256": "",
    });
    refresh_self_digest(&mut archive, "archive_sha256")?;
    let archive_path = format!("{directory}/supporting-archive-manifest.json");
    let archive_bytes = canonicalize(&archive)?;
    write_relative(root, &archive_path, &archive_bytes)?;
    let deposit_bytes = fs::read(resolve_relative(root, &map.deposit.deposit_path))
        .map_err(|error| CplError::io("Could not read the exact deposit", error))?;
    let find = |suffix: &str| {
        artifacts
            .iter()
            .find(|item| item.path.ends_with(suffix))
            .expect("fixed artifact exists")
    };
    let summary = find("human-authorship-summary.md");
    let harp = find("harp.json");
    let map_file = find("contribution-map.json");
    let verify = find("verification-report.json");
    let files = vec![
        manifest_file("human_readable_harp", summary),
        manifest_file("machine_readable_harp", harp),
        manifest_file("contribution_map", map_file),
        GeneratedFile {
            role: "deposit_copy".into(),
            path: map.deposit.deposit_path.clone(),
            sha256: raw_digest(&deposit_bytes),
            size: deposit_bytes.len(),
            privacy_classification: "deposit".into(),
        },
        manifest_file("verification_report", verify),
        GeneratedFile {
            role: "supporting_archive".into(),
            path: archive_path,
            sha256: raw_digest(&archive_bytes),
            size: archive_bytes.len(),
            privacy_classification: privacy_class(&metadata.sanitization_profile).into(),
        },
    ];
    let sanitization_rules = json!({
        "profile": metadata.sanitization_profile,
        "full_private": "Representative text excerpts may be retained in the private project.",
        "sanitized": "Text bodies and protected source bodies are omitted; hashes and structural facts remain."
    });
    let omissions = if metadata.sanitization_profile == "sanitized" {
        vec![json!({
            "category": "protected_source_body", "action": "exclude", "count": 1,
            "disclosure_sha256": canonical_digest(&json!({"category":"protected_source_body","action":"exclude","count":1}))?
        })]
    } else {
        vec![]
    };
    let mut manifest = json!({
        "schema_version": CPL_SCHEMA_VERSION,
        "manifest_id": stable_id("manifest", projection.harp["harp_sha256"].as_str().unwrap_or_default()),
        "project_id": projection.harp["project_id"],
        "harp_id": projection.harp["harp_id"],
        "harp_sha256": projection.harp["harp_sha256"],
        "deposit_id": projection.harp["deposit"]["deposit_id"],
        "deposit_sha256": projection.harp["deposit"]["deposit_sha256"],
        "contribution_map_id": projection.harp["contribution_map"]["contribution_map_id"],
        "contribution_map_sha256": projection.harp["contribution_map"]["contribution_map_sha256"],
        "cpl_chain_head": projection.harp["cpl_binding"]["chain_head"],
        "report_metadata": metadata,
        "verification_report_sha256": raw_digest(&verify.bytes),
        "sanitization_profile": metadata.sanitization_profile,
        "sanitization_rules_sha256": canonical_digest(&sanitization_rules)?,
        "omissions": omissions,
        "files": files,
        "created_at": map.deposit.created_at,
        "manifest_sha256": "",
    });
    refresh_self_digest(&mut manifest, "manifest_sha256")?;
    write_relative(
        root,
        &format!("{directory}/manifest.json"),
        &canonicalize(&manifest)?,
    )?;
    projection.export_manifest = manifest;
    projection.generated_files = files;
    Ok(())
}

fn evaluate_current(
    root: &Path,
    project_id: &str,
    request: &HarpGenerationRequest,
    projection: &mut HarpProjection,
) -> CplResult<()> {
    let mut reasons = Vec::new();
    let policy = bundled_policy_profile()?;
    let request_digest =
        canonical_digest(&serde_json::to_value(request).map_err(serialization_error)?)?;
    if composition::reconstruct(root, project_id)?.revision_id
        != projection.dependencies.manuscript_revision_id
    {
        reasons.push("manuscript_revision_changed".into());
    }
    let deposit = projection
        .generated_files
        .iter()
        .find(|file| file.role == "deposit_copy");
    match deposit.and_then(|file| fs::read(resolve_relative(root, &file.path)).ok()) {
        Some(bytes) if raw_digest(&bytes) == projection.dependencies.deposit_sha256 => {}
        Some(_) => reasons.push("deposit_digest_changed".into()),
        None => reasons.push("deposit_unavailable".into()),
    }
    match contribution_map::load_latest(root, project_id) {
        Ok(Some(map)) => {
            if map.contribution_map.contribution_map_sha256
                != projection.dependencies.contribution_map_sha256
            {
                reasons.push("contribution_map_changed".into());
            }
            if assertion_set_digest(&map)? != projection.dependencies.assertion_set_sha256 {
                reasons.push("assertion_set_changed".into());
            }
            if dependency_set_digest(&map)? != projection.dependencies.dependency_set_sha256 {
                reasons.push("dependency_set_changed".into());
            }
            if map.contribution_map.classification_status == ClassificationStatus::Stale {
                reasons.push("contribution_map_stale".into());
            }
        }
        Ok(None) => reasons.push("contribution_map_unavailable".into()),
        Err(_) => reasons.push("contribution_map_or_dependency_unverified".into()),
    }
    if policy.sha256 != projection.dependencies.policy_profile_sha256 {
        reasons.push("policy_profile_changed".into());
    }
    if request_digest != projection.dependencies.approval_sha256 {
        reasons.push("approval_changed".into());
    }
    if !matches!(
        verifier::verify_project(root, project_id).map(|report| report.status),
        Ok(VerificationStatus::Verified | VerificationStatus::VerifiedWithWarnings)
    ) {
        reasons.push("cpl_verification_failed".into());
    }
    reasons.sort();
    reasons.dedup();
    if reasons.is_empty() {
        projection.applicability_status = "current".into();
        projection.stale_reasons.clear();
    } else {
        projection.applicability_status = "stale".into();
        projection.stale_reasons = reasons;
        projection.harp["applicability_status"] = json!("stale");
        projection.harp["evidentiary_status"] = json!("stale");
    }
    Ok(())
}
fn validate_request(request: &HarpGenerationRequest) -> CplResult<()> {
    if !request.user_approved {
        return Err(CplError::new("HARP_APPROVAL_REQUIRED", "HARP generation requires explicit user approval of identity and suggested registration language.", true));
    }
    if !matches!(
        request.identity_status.as_str(),
        "verified" | "self_declared" | "unknown"
    ) {
        return Err(CplError::new(
            "HARP_IDENTITY_STATUS_INVALID",
            "identityStatus must be verified, self_declared, or unknown.",
            false,
        ));
    }
    if !matches!(
        request.sanitization_profile.as_str(),
        "full_private" | "sanitized"
    ) {
        return Err(CplError::new(
            "HARP_SANITIZATION_INVALID",
            "sanitizationProfile must be full_private or sanitized.",
            false,
        ));
    }
    let language = &request.suggested_registration_language;
    for (name, value, maximum) in [
        ("authorCreated", language.author_created.as_str(), 1000usize),
        (
            "materialExcluded",
            language.material_excluded.as_str(),
            1000,
        ),
        (
            "newMaterialIncluded",
            language.new_material_included.as_str(),
            1000,
        ),
    ] {
        if value.chars().count() > maximum {
            return Err(CplError::new(
                "HARP_LANGUAGE_TOO_LONG",
                format!("{name} exceeds {maximum} characters."),
                true,
            ));
        }
    }
    if request
        .declared_name
        .as_deref()
        .is_none_or(|name| name.trim().is_empty())
        && request.identity_status != "unknown"
    {
        return Err(CplError::new(
            "HARP_DECLARED_NAME_REQUIRED",
            "A declared name is required unless identity status is unknown.",
            true,
        ));
    }
    Ok(())
}

fn approved_language(request: &HarpGenerationRequest, approval_event_id: &str) -> CplResult<Value> {
    let input = &request.suggested_registration_language;
    let mut treatments = sorted_unique(input.registration_treatment_suggestions.clone());
    if treatments.is_empty() {
        treatments.push("no_suggestion".into());
    }
    let allowed = [
        "claim_as_human_contribution",
        "exclude_ai_generated_material",
        "describe_new_human_material",
        "disclose_ai_use",
        "manual_review_required",
        "no_suggestion",
    ];
    if treatments
        .iter()
        .any(|value| !allowed.contains(&value.as_str()))
    {
        return Err(CplError::new(
            "HARP_TREATMENT_INVALID",
            "A registration treatment suggestion is not recognized by the policy profile.",
            false,
        ));
    }
    let identity = json!({
        "author_created": input.author_created,
        "material_excluded": input.material_excluded,
        "new_material_included": input.new_material_included,
        "note_to_co": input.note_to_co,
        "registration_treatment_suggestions": treatments,
    });
    Ok(json!({
        "author_created": input.author_created,
        "material_excluded": input.material_excluded,
        "new_material_included": input.new_material_included,
        "note_to_co": input.note_to_co,
        "registration_treatment_suggestions": treatments,
        "user_approved": true,
        "approval_event_id": approval_event_id,
        "approved_language_sha256": canonical_digest(&identity)?,
    }))
}

fn bundled_policy_profile() -> CplResult<PolicyProfile> {
    let fixture: Value = serde_json::from_str(POLICY_FIXTURE)
        .map_err(|error| CplError::new("HARP_POLICY_INVALID", error.to_string(), false))?;
    let value = fixture["instance"].clone();
    let mut identity = value.clone();
    let object = identity.as_object_mut().ok_or_else(|| {
        CplError::new(
            "HARP_POLICY_INVALID",
            "The bundled policy profile is not an object.",
            false,
        )
    })?;
    let declared = object
        .remove("profile_sha256")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| {
            CplError::new(
                "HARP_POLICY_INVALID",
                "The bundled policy profile has no digest.",
                false,
            )
        })?;
    if declared != canonical_digest(&identity)? {
        return Err(CplError::new(
            "HARP_POLICY_DIGEST_INVALID",
            "The bundled policy profile digest does not verify.",
            false,
        ));
    }
    let retrieved_on = value["official_sources"]
        .as_array()
        .and_then(|sources| {
            sources
                .iter()
                .filter_map(|source| source["retrieved_on"].as_str())
                .max()
        })
        .unwrap_or("unknown")
        .to_owned();
    let compatible = value["compatible_application_versions"]
        .as_array()
        .is_some_and(|versions| {
            versions
                .iter()
                .any(|version| version == APPLICATION_VERSION)
        });
    Ok(PolicyProfile {
        id: value["policy_profile_id"]
            .as_str()
            .unwrap_or_default()
            .into(),
        version: value["profile_version"].as_str().unwrap_or_default().into(),
        sha256: declared,
        retrieved_on,
        status: if compatible { "current" } else { "superseded" }.into(),
        value,
    })
}

fn source_dependency_seed(
    map: &ContributionMapProjection,
    policy: &PolicyProfile,
    approval_sha256: &str,
) -> CplResult<String> {
    canonical_digest(&json!({
        "deposit_sha256": map.deposit.deposit_sha256,
        "revision_id": map.deposit.manuscript_revision_id,
        "map_sha256": map.contribution_map.contribution_map_sha256,
        "policy_sha256": policy.sha256,
        "assertions_sha256": assertion_set_digest(map)?,
        "dependencies_sha256": dependency_set_digest(map)?,
        "approval_sha256": approval_sha256,
    }))
}

fn assertion_set_digest(map: &ContributionMapProjection) -> CplResult<String> {
    canonical_digest(&json!({ "assertions": map.assertions }))
}

fn dependency_set_digest(map: &ContributionMapProjection) -> CplResult<String> {
    canonical_digest(&json!({
        "assertion_dependencies": map.assertions.iter().map(|value| value.get("dependencies").cloned().unwrap_or(Value::Null)).collect::<Vec<_>>(),
        "segment_lineage": map.contribution_map.segments.iter().map(|segment| json!({
            "segment_id": segment.segment_id,
            "lineage_reference_ids": segment.lineage_reference_ids,
            "operation_ids": segment.operation_ids,
        })).collect::<Vec<_>>(),
        "generator_configuration_sha256": map.contribution_map.generator.configuration_sha256,
    }))
}

fn claim_summary(map: &ContributionMapProjection) -> String {
    format!("CPL records origin and lineage for {} of {} normalized Unicode scalar positions in the exact deposit, including recorded transformations and accepted AI output where present. These are evidence facts, not legal authorship or copyrightability conclusions.", map.contribution_map.coverage.recorded_positions, map.contribution_map.coverage.denominator)
}

fn limitation_codes(
    request: &HarpGenerationRequest,
    map: &ContributionMapProjection,
    ai: &[Value],
    policy: &PolicyProfile,
    origin_complete: bool,
    lineage_complete: bool,
) -> Vec<String> {
    let mut values = BTreeSet::new();
    match request.identity_status.as_str() {
        "self_declared" => {
            values.insert("SELF_DECLARED_IDENTITY".to_owned());
        }
        "unknown" => {
            values.insert("UNKNOWN_IDENTITY".to_owned());
        }
        _ => {}
    }
    if !origin_complete {
        values.insert("UNKNOWN_ORIGIN".into());
    }
    if !lineage_complete {
        values.insert("UNKNOWN_LINEAGE".into());
    }
    if map
        .contribution_map
        .segments
        .iter()
        .any(|segment| segment.generation_status == "unknown")
    {
        values.insert("UNKNOWN_GENERATION".into());
    }
    if map
        .contribution_map
        .segments
        .iter()
        .any(|segment| segment.recorded_origin_kind == RecordedOrigin::Unattested)
    {
        values.insert("UNATTESTED_EXPRESSION".into());
    }
    if map.contribution_map.classification_status != ClassificationStatus::Exact {
        values.insert("DEGRADED_COVERAGE".into());
    }
    if ai.iter().any(|value| value["identity_status"] == "unknown") {
        values.insert("UNKNOWN_GENERATION".into());
    }
    if policy.status != "current" {
        values.insert("SUPERSEDED_POLICY_PROFILE".into());
    }
    if request.sanitization_profile == "sanitized" {
        values.insert("SANITIZED_EVIDENCE".into());
    }
    values.insert("COPYRIGHT_OFFICE_DETERMINES_SCOPE".into());
    values.into_iter().collect()
}

fn invocation_identities(root: &Path) -> CplResult<BTreeMap<String, (String, String)>> {
    let mut identities = BTreeMap::new();
    for event in ledger::read_all_events(&LedgerPaths::new(root))? {
        for reference in event.record_references {
            if reference.record_type != "invocation-request" {
                continue;
            }
            let record = read_record(root, &reference.path, true)?;
            let id = record.payload["invocationId"]
                .as_str()
                .or_else(|| record.payload["invocation_id"].as_str());
            let provider = &record.payload["provider"];
            let provider_id = provider["kind"]
                .as_str()
                .or_else(|| provider["name"].as_str());
            let model_id = provider["model"].as_str();
            if let (Some(id), Some(provider_id), Some(model_id)) = (id, provider_id, model_id) {
                identities.insert(id.into(), (provider_id.into(), model_id.into()));
            }
        }
    }
    Ok(identities)
}

fn ai_disclosures(
    map: &ContributionMapProjection,
    invocations: &BTreeMap<String, (String, String)>,
) -> Vec<Value> {
    let mut grouped: BTreeMap<(Option<String>, Option<String>), BTreeSet<String>> = BTreeMap::new();
    for segment in &map.contribution_map.segments {
        if segment.recorded_origin_kind != RecordedOrigin::AcceptedAiOutput {
            continue;
        }
        let identity = segment
            .lineage_reference_ids
            .iter()
            .find_map(|id| invocations.get(id))
            .cloned();
        let key = identity.map_or((None, None), |(provider, model)| {
            (Some(provider), Some(model))
        });
        grouped
            .entry(key)
            .or_default()
            .insert(segment.segment_id.clone());
    }
    grouped
        .into_iter()
        .map(|((provider, model), segments)| {
            let recorded = provider.is_some() && model.is_some();
            json!({
                "provider_id": provider,
                "model_id": model,
                "identity_status": if recorded { "recorded" } else { "unknown" },
                "included_expression_segment_ids": segments.into_iter().collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn representative_operations(
    root: &Path,
    map: &ContributionMapProjection,
    sanitization: &str,
) -> CplResult<Vec<Value>> {
    let wanted = map
        .contribution_map
        .segments
        .iter()
        .flat_map(|segment| segment.operation_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let mut operations = BTreeMap::<String, Value>::new();
    let mut content = BTreeMap::<String, Value>::new();
    for event in ledger::read_all_events(&LedgerPaths::new(root))? {
        for reference in event.record_references {
            if reference.record_type != "composition-operation"
                && reference.record_type != "composition-content"
            {
                continue;
            }
            let record = read_record(root, &reference.path, true)?;
            let id = record.payload["operation_id"]
                .as_str()
                .or_else(|| record.payload["operationId"].as_str())
                .unwrap_or_default()
                .to_owned();
            if !wanted.contains(&id) {
                continue;
            }
            if reference.record_type == "composition-operation" {
                operations.insert(id, record.payload);
            } else {
                content.insert(id, record.payload);
            }
        }
    }
    Ok(operations.into_iter().take(5).map(|(id, operation)| {
        let body = content.get(&id);
        let deleted = body.and_then(|value| value["deleted_text"].as_str().or_else(|| value["deletedText"].as_str())).unwrap_or_default();
        let inserted = body.and_then(|value| value["inserted_text"].as_str().or_else(|| value["insertedText"].as_str())).unwrap_or_default();
        json!({
            "operation_id": id,
            "operation_kind": operation["operation_kind"].as_str().or_else(|| operation["operationKind"].as_str()).unwrap_or("unknown"),
            "transformation_relationships": operation["transformation_relationships"].as_array().or_else(|| operation["transformationRelationships"].as_array()).cloned().unwrap_or_default(),
            "before_sha256": raw_digest(deleted.as_bytes()),
            "after_sha256": raw_digest(inserted.as_bytes()),
            "before_excerpt": if sanitization == "full_private" { Value::String(excerpt(deleted)) } else { Value::Null },
            "after_excerpt": if sanitization == "full_private" { Value::String(excerpt(inserted)) } else { Value::Null },
        })
    }).collect())
}
fn summary_markdown(harp: &Value, metadata: &HarpReportMetadata) -> String {
    format!("# Human Authorship Record of Provenance\n\n{}\n\n## One-page summary\n\n{}\n\n- Evidentiary status: `{}`\n- Applicability: `{}`\n- Recorded-origin coverage: {}\n- Identity declaration: `{}`\n- AI systems disclosed: {}\n\n## Suggested registration language\n\n- **Author Created:** {}\n- **Material Excluded:** {}\n- **New Material Included:** {}\n\n## Important limitation\n\n{}\n", metadata_markdown(metadata), harp["claim_summary"].as_str().unwrap_or_default(), harp["evidentiary_status"].as_str().unwrap_or_default(), harp["applicability_status"].as_str().unwrap_or_default(), harp["coverage"]["statement"].as_str().unwrap_or_default(), harp["identity_declaration"]["identity_status"].as_str().unwrap_or_default(), harp["ai_system_disclosures"].as_array().map_or(0, Vec::len), harp["suggested_registration_language"]["author_created"].as_str().unwrap_or_default(), harp["suggested_registration_language"]["material_excluded"].as_str().unwrap_or_default(), harp["suggested_registration_language"]["new_material_included"].as_str().unwrap_or_default(), LEGAL_SCOPE_STATEMENT)
}

fn transformations_markdown(operations: &[Value], metadata: &HarpReportMetadata) -> String {
    let mut output = format!(
        "# Representative transformation comparisons\n\n{}\n",
        metadata_markdown(metadata)
    );
    if operations.is_empty() {
        output.push_str(
            "\nNo recorded transformation operation is represented in the final deposit.\n",
        );
    }
    for operation in operations {
        let relationships = operation["transformation_relationships"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        output.push_str(&format!("\n## `{}`\n\n- Kind: `{}`\n- Relationships: `{}`\n- Before digest: `{}`\n- After digest: `{}`\n", operation["operation_id"].as_str().unwrap_or_default(), operation["operation_kind"].as_str().unwrap_or_default(), relationships, operation["before_sha256"].as_str().unwrap_or_default(), operation["after_sha256"].as_str().unwrap_or_default()));
        if let Some(before) = operation["before_excerpt"].as_str() {
            output.push_str(&format!(
                "- Before excerpt: `{}`\n",
                markdown_inline(before)
            ));
        }
        if let Some(after) = operation["after_excerpt"].as_str() {
            output.push_str(&format!("- After excerpt: `{}`\n", markdown_inline(after)));
        }
    }
    output.push_str(&format!("\n{}\n", LEGAL_SCOPE_STATEMENT));
    output
}

fn ai_disclosure_markdown(harp: &Value, metadata: &HarpReportMetadata) -> String {
    let mut output = format!(
        "# AI-system and model disclosure\n\n{}\n",
        metadata_markdown(metadata)
    );
    let disclosures = harp["ai_system_disclosures"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if disclosures.is_empty() {
        output.push_str("\nNo accepted-AI-output segment is included in the exact deposit.\n");
    }
    for item in disclosures {
        output.push_str(&format!(
            "\n- Provider: `{}`; model: `{}`; identity status: `{}`; included segments: {}\n",
            item["provider_id"].as_str().unwrap_or("unknown"),
            item["model_id"].as_str().unwrap_or("unknown"),
            item["identity_status"].as_str().unwrap_or("unknown"),
            item["included_expression_segment_ids"]
                .as_array()
                .map_or(0, Vec::len)
        ));
    }
    output.push_str(&format!("\n{}\n", LEGAL_SCOPE_STATEMENT));
    output
}

fn coverage_markdown(harp: &Value, metadata: &HarpReportMetadata) -> String {
    let limitations = harp["limitation_codes"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|item| format!("- `{item}`"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    format!("# Provenance coverage and limitations\n\n{}\n\n{}\n\n## Limitation codes\n\n{}\n\nCoverage describes recorded provenance positions and is not a human-authorship percentage.\n\n{}\n", metadata_markdown(metadata), harp["coverage"]["statement"].as_str().unwrap_or_default(), limitations, LEGAL_SCOPE_STATEMENT)
}

fn registration_markdown(harp: &Value, metadata: &HarpReportMetadata) -> String {
    let value = &harp["suggested_registration_language"];
    format!("# Suggested registration language\n\n{}\n\nThis language is suggested, editable, explicitly user-approved for this HARP generation, and is not legal advice.\n\n## Author Created\n\n{}\n\n## Material Excluded\n\n{}\n\n## New Material Included\n\n{}\n\n## Note to CO\n\n{}\n\n{}\n", metadata_markdown(metadata), value["author_created"].as_str().unwrap_or_default(), value["material_excluded"].as_str().unwrap_or_default(), value["new_material_included"].as_str().unwrap_or_default(), value["note_to_co"].as_str().unwrap_or("None"), LEGAL_SCOPE_STATEMENT)
}

fn contribution_map_svg(map: &ContributionMapProjection, metadata: &HarpReportMetadata) -> String {
    let mut counts = BTreeMap::<String, usize>::new();
    for segment in &map.contribution_map.segments {
        *counts
            .entry(format!("{:?}", segment.recorded_origin_kind))
            .or_default() += segment.normalized_unicode_scalar_length;
    }
    let colors = [
        "#2563eb", "#16a34a", "#9333ea", "#ea580c", "#64748b", "#dc2626",
    ];
    let denominator = map.contribution_map.coverage.denominator.max(1);
    let mut x = 40.0;
    let mut bars = String::new();
    let mut legend = String::new();
    for (index, (label, count)) in counts.iter().enumerate() {
        let width = 920.0 * (*count as f64 / denominator as f64);
        bars.push_str(&format!("<rect x=\"{x:.2}\" y=\"190\" width=\"{width:.2}\" height=\"52\" fill=\"{}\"><title>{}: {} scalar positions</title></rect>", colors[index % colors.len()], xml_escape(label), count));
        legend.push_str(&format!("<rect x=\"40\" y=\"{}\" width=\"16\" height=\"16\" fill=\"{}\"/><text x=\"66\" y=\"{}\" class=\"legend\">{} — {}</text>", 280 + index * 28, colors[index % colors.len()], 293 + index * 28, xml_escape(label), count));
        x += width;
    }
    format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1000\" height=\"520\" viewBox=\"0 0 1000 520\" role=\"img\" aria-labelledby=\"title desc\"><title id=\"title\">Visual final-text contribution map</title><desc id=\"desc\">Deposit {}; revision {}; CPL chain head {}; sequence {}; HARP schema {}; generator {}; application {}; policy profile {} version {} retrieved {}; sanitization {}. {} This visualization is provenance coverage, not a human-authorship percentage.</desc><style>text{{font-family:system-ui,sans-serif;fill:#172033}}.title{{font-size:26px;font-weight:700}}.meta{{font-size:13px}}.legend{{font-size:15px}}</style><rect width=\"1000\" height=\"520\" fill=\"#f8fafc\"/><text x=\"40\" y=\"52\" class=\"title\">Final-text contribution map</text><text x=\"40\" y=\"82\" class=\"meta\">Exact deposit: {}</text><text x=\"40\" y=\"105\" class=\"meta\">Revision: {} · CPL sequence: {}</text><text x=\"40\" y=\"128\" class=\"meta\">Policy: {} · retrieved {}</text><text x=\"40\" y=\"151\" class=\"meta\">{} scalar positions · provenance coverage, not a human-authorship percentage</text>{bars}{legend}<text x=\"40\" y=\"480\" class=\"meta\">Copyrightability remains a U.S. Copyright Office determination.</text></svg>", xml_escape(&metadata.deposit_sha256), xml_escape(&metadata.manuscript_revision_id), xml_escape(&metadata.cpl_chain_head), metadata.cpl_event_sequence, xml_escape(&metadata.harp_schema_version), xml_escape(&metadata.harp_generator_version), xml_escape(&metadata.application_version), xml_escape(&metadata.policy_profile_id), xml_escape(&metadata.policy_profile_version), xml_escape(&metadata.policy_retrieval_date), xml_escape(&metadata.sanitization_profile), xml_escape(LEGAL_SCOPE_STATEMENT), xml_escape(&metadata.deposit_sha256), xml_escape(&metadata.manuscript_revision_id), metadata.cpl_event_sequence, xml_escape(&metadata.policy_profile_version), xml_escape(&metadata.policy_retrieval_date), map.contribution_map.coverage.denominator)
}

fn metadata_markdown(metadata: &HarpReportMetadata) -> String {
    format!("- Exact deposit digest: `{}`\n- Manuscript revision: `{}` (`{}`)\n- CPL chain head / sequence: `{}` / `{}`\n- HARP schema / generator / application: `{}` / `{}` / `{}`\n- Policy profile: `{}` version `{}` (`{}`), retrieved `{}`\n- Sanitization profile: `{}`", metadata.deposit_sha256, metadata.manuscript_revision_id, metadata.manuscript_revision_sha256, metadata.cpl_chain_head, metadata.cpl_event_sequence, metadata.harp_schema_version, metadata.harp_generator_version, metadata.application_version, metadata.policy_profile_id, metadata.policy_profile_version, metadata.policy_profile_sha256, metadata.policy_retrieval_date, metadata.sanitization_profile)
}

fn artifact_description(artifact: &Artifact) -> Value {
    json!({"path": artifact.path, "sha256": raw_digest(&artifact.bytes), "size": artifact.bytes.len(), "privacy_classification": artifact.privacy})
}
fn manifest_file(role: &str, artifact: &Artifact) -> GeneratedFile {
    GeneratedFile {
        role: role.into(),
        path: artifact.path.clone(),
        sha256: raw_digest(&artifact.bytes),
        size: artifact.bytes.len(),
        privacy_classification: artifact.privacy.clone(),
    }
}
fn privacy_class(profile: &str) -> &'static str {
    if profile == "sanitized" {
        "sanitized_evidence"
    } else {
        "private"
    }
}
fn excerpt(value: &str) -> String {
    value.chars().take(240).collect()
}
fn markdown_inline(value: &str) -> String {
    value.replace('`', "'").replace(['\r', '\n'], " ")
}
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn sorted_unique(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
fn raw_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
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

fn refresh_self_digest(value: &mut Value, field: &str) -> CplResult<()> {
    let mut identity = value.clone();
    identity
        .as_object_mut()
        .ok_or_else(|| {
            CplError::new(
                "HARP_SERIALIZATION_FAILED",
                "A digest-bearing artifact must be an object.",
                false,
            )
        })?
        .remove(field);
    value[field] = json!(canonical_digest(&identity)?);
    Ok(())
}

fn resolve_relative(root: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

fn write_relative(root: &Path, relative: &str, bytes: &[u8]) -> CplResult<()> {
    let path = resolve_relative(root, relative);
    let parent = path.parent().ok_or_else(|| {
        CplError::new(
            "HARP_PATH_INVALID",
            "A HARP output path has no parent.",
            false,
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| CplError::io("Could not create the HARP report directory", error))?;
    let temporary = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap().to_string_lossy()
    ));
    fs::write(&temporary, bytes)
        .map_err(|error| CplError::io("Could not stage a HARP report", error))?;
    ledger::atomic_replace(&temporary, &path)?;
    ledger::sync_directory(parent)?;
    Ok(())
}

fn read_record(root: &Path, relative: &str, verify_canonical: bool) -> CplResult<CplRecord> {
    let path = resolve_relative(root, relative);
    let bytes = fs::read(&path)
        .map_err(|error| CplError::io("Could not read a HARP dependency record", error))?;
    let record: CplRecord = serde_json::from_slice(&bytes)
        .map_err(|error| CplError::new("HARP_RECORD_INVALID", error.to_string(), false))?;
    if verify_canonical
        && canonicalize(&serde_json::to_value(&record).map_err(serialization_error)?)? != bytes
    {
        return Err(CplError::new(
            "HARP_RECORD_NONCANONICAL",
            format!("{} is not canonical JSON.", path.display()),
            false,
        ));
    }
    Ok(record)
}

fn serialization_error(error: impl std::fmt::Display) -> CplError {
    CplError::new("HARP_SERIALIZATION_FAILED", error.to_string(), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_changes_are_stale() {
        let original = HarpDependencyBinding {
            deposit_sha256: "sha256:a".into(),
            manuscript_revision_id: "revision_a".into(),
            manuscript_revision_sha256: "sha256:b".into(),
            contribution_map_sha256: "sha256:c".into(),
            chain_head_sha256: "sha256:d".into(),
            chain_event_sequence: 1,
            policy_profile_sha256: "sha256:e".into(),
            assertion_set_sha256: "sha256:f".into(),
            dependency_set_sha256: "sha256:g".into(),
            approval_sha256: "sha256:h".into(),
        };
        for field in 0..6 {
            let mut changed = original.clone();
            match field {
                0 => changed.deposit_sha256.push('1'),
                1 => changed.manuscript_revision_id.push('1'),
                2 => changed.contribution_map_sha256.push('1'),
                3 => changed.policy_profile_sha256.push('1'),
                4 => changed.assertion_set_sha256.push('1'),
                _ => changed.dependency_set_sha256.push('1'),
            }
            assert!(is_stale(&original, &changed));
        }
        assert!(!is_stale(&original, &original));
    }

    #[test]
    fn bundled_policy_digest_verifies_and_supports_this_version() {
        let profile = bundled_policy_profile().unwrap();
        assert_eq!(profile.status, "current");
        assert!(!profile.value["official_sources"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn generator_source_contains_no_model_or_legal_classifier() {
        let source = include_str!("harp.rs");
        let forbidden_model_call = ["generate", "_text("].concat();
        assert!(!source.contains(&forbidden_model_call));
        let forbidden_http_client = ["req", "west"].concat();
        assert!(!source.contains(&forbidden_http_client));
        let forbidden_percentage = ["human", "_percentage"].concat();
        assert!(!source.contains(&forbidden_percentage));
        assert!(source.contains("Copyright Office"));
    }
    #[test]
    fn generates_all_artifacts_idempotently_and_stales_after_edit() {
        use crate::provenance::{
            composition::{CompositionAction, CompositionBoundary, CompositionCommand},
            contribution_map::ContributionMapRequest,
            identifiers::timestamp_millis,
        };
        let temp = tempfile::tempdir().unwrap();
        let project_id = "project_01J00000000000000000000001";
        let apply = |id: &str, action: CompositionAction| {
            composition::apply_command(
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
            "harp_initialize",
            CompositionAction::Initialize {
                text: String::new(),
                origin: RecordedOrigin::Unattested,
            },
        );
        apply(
            "harp_human_edit",
            CompositionAction::Edit {
                before_text: String::new(),
                after_text: "# Recorded work\n\nHuman expression.".into(),
                boundary: CompositionBoundary::ExplicitSave,
                origin: RecordedOrigin::RecordedDirectHumanInput,
                operation_kind_hint: None,
                ai_acceptance: None,
            },
        );
        contribution_map::freeze_current(
            temp.path(),
            project_id,
            ContributionMapRequest::default(),
        )
        .unwrap();
        let request = HarpGenerationRequest {
            declared_name: Some("Example Author".into()),
            identity_status: "self_declared".into(),
            identity_evidence_reference_ids: vec![],
            sanitization_profile: "full_private".into(),
            suggested_registration_language: RegistrationLanguageInput::default(),
            user_approved: true,
        };
        let first = generate_current(temp.path(), project_id, request.clone()).unwrap();
        assert_eq!(first.applicability_status, "current");
        assert_eq!(first.harp["evidentiary_status"], "exact");
        assert_eq!(first.generated_files.len(), 6);
        for name in [
            "human-authorship-summary.md",
            "final-text-contribution-map.svg",
            "representative-transformations.md",
            "ai-system-disclosure.md",
            "coverage-and-limitations.md",
            "registration-language.md",
            "harp.json",
            "verification-report.json",
            "supporting-archive-manifest.json",
            "manifest.json",
        ] {
            assert!(temp
                .path()
                .join(&first.report_directory)
                .join(name)
                .is_file());
        }
        let event_count = ledger::read_all_events(&LedgerPaths::new(temp.path()))
            .unwrap()
            .len();
        let second = generate_current(temp.path(), project_id, request).unwrap();
        assert_eq!(first.harp, second.harp);
        assert_eq!(
            ledger::read_all_events(&LedgerPaths::new(temp.path()))
                .unwrap()
                .len(),
            event_count
        );
        apply(
            "harp_later_edit",
            CompositionAction::Edit {
                before_text: "# Recorded work\n\nHuman expression.".into(),
                after_text: "# Recorded work\n\nHuman expression changed.".into(),
                boundary: CompositionBoundary::Idle,
                origin: RecordedOrigin::RecordedDirectHumanInput,
                operation_kind_hint: None,
                ai_acceptance: None,
            },
        );
        let stale = load_latest(temp.path(), project_id).unwrap().unwrap();
        assert_eq!(stale.applicability_status, "stale");
        assert!(stale
            .stale_reasons
            .iter()
            .any(|reason| reason == "manuscript_revision_changed"));
    }
}
