//! Deterministic contribution-map projection over a frozen manuscript revision.

use super::{
    canonical::{canonical_digest, canonicalize},
    composition::{self, ExpressionSpan, RecordedOrigin},
    ledger::{self, LedgerPaths},
    records::{CplRecord, RecordInput, WriteCommand},
    verifier, CplError, CplResult, CplService, VerificationStatus, CPL_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path};

const GENERATOR_NAME: &str = "thinkloom-contribution-map";
const GENERATOR_VERSION: &str = "1.0.0";
const APPLICATION_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_PAGE_SCALAR_CAPACITY: usize = 1_800;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContributionMapRequest {
    #[serde(default = "default_page_capacity")]
    pub page_scalar_capacity: usize,
    #[serde(default)]
    pub selected_by_human: bool,
    #[serde(default)]
    pub arranged_by_human: bool,
}

impl Default for ContributionMapRequest {
    fn default() -> Self {
        Self {
            page_scalar_capacity: DEFAULT_PAGE_SCALAR_CAPACITY,
            selected_by_human: false,
            arranged_by_human: false,
        }
    }
}

fn default_page_capacity() -> usize {
    DEFAULT_PAGE_SCALAR_CAPACITY
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ClassificationStatus {
    Exact,
    Degraded,
    Stale,
    Unverified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Coverage {
    pub denominator: usize,
    pub recorded_positions: usize,
    pub coverage_status: String,
    pub denominator_unit: String,
    pub denominator_definition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextFragmentRange {
    pub schema_version: String,
    pub document_revision_id: String,
    pub coordinate_system: String,
    pub start: usize,
    pub end: usize,
    pub preimage_sha256: String,
    pub text_fragment_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MapSegment {
    pub schema_version: String,
    pub segment_id: String,
    pub project_id: String,
    pub revision_id: String,
    pub segment_sequence: usize,
    pub range: TextFragmentRange,
    pub content_sha256: String,
    pub normalized_unicode_scalar_length: usize,
    pub recorded_origin_kind: RecordedOrigin,
    pub actor_identity_status: String,
    pub generation_status: String,
    pub lineage_status: String,
    pub lineage_reference_ids: Vec<String>,
    pub operation_ids: Vec<String>,
    pub transformation_relationships: Vec<String>,
    pub assertion_ids: Vec<String>,
    pub selection_arrangement_assertion_ids: Vec<String>,
    pub included_in_deposit_assertion_id: Option<String>,
    pub classification_status: ClassificationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuralLocator {
    pub segment_id: String,
    pub chapter: Option<usize>,
    pub paragraph: Option<usize>,
    pub page: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LayoutProfile {
    pub layout_profile_id: String,
    pub layout_profile_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratorIdentity {
    pub name: String,
    pub version: String,
    pub configuration_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContributionMap {
    pub schema_version: String,
    pub contribution_map_id: String,
    pub project_id: String,
    pub deposit_id: String,
    pub deposit_sha256: String,
    pub manuscript_revision_id: String,
    pub manuscript_revision_sha256: String,
    pub cpl_chain_head: String,
    pub cpl_event_sequence: u64,
    pub coordinate_system: String,
    pub coverage: Coverage,
    pub layers: Vec<String>,
    pub segments: Vec<MapSegment>,
    pub structural_locators: Vec<StructuralLocator>,
    pub layout_profile: Option<LayoutProfile>,
    pub generator: GeneratorIdentity,
    pub classification_status: ClassificationStatus,
    pub contribution_map_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DepositSnapshot {
    pub schema_version: String,
    pub deposit_id: String,
    pub project_id: String,
    pub application_version: String,
    pub deposit_path: String,
    pub media_type: String,
    pub byte_length: usize,
    pub deposit_sha256: String,
    pub manuscript_revision_id: String,
    pub manuscript_revision_sha256: String,
    pub cpl_chain_head: String,
    pub cpl_event_sequence: u64,
    pub layout_profile: Option<LayoutProfile>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectionBoundary {
    pub boundary_id: String,
    pub status: String,
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContributionMapProjection {
    pub deposit: DepositSnapshot,
    pub contribution_map: ContributionMap,
    pub assertions: Vec<Value>,
    pub assertion_evaluations: Vec<Value>,
    pub boundaries: Vec<ProjectionBoundary>,
}

#[derive(Debug, Clone)]
pub struct ContributionMapInput {
    pub project_id: String,
    pub manuscript: String,
    pub revision_id: String,
    pub spans: Vec<ExpressionSpan>,
    pub deposit_id: String,
    pub deposit_path: String,
    pub cpl_chain_head: String,
    pub cpl_event_sequence: u64,
    pub source_event_id: String,
    pub source_timestamp: String,
    pub source_verified: bool,
    pub request: ContributionMapRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LocatorValue {
    chapter: usize,
    paragraph: usize,
    page: usize,
}

pub fn build(input: ContributionMapInput) -> CplResult<ContributionMapProjection> {
    if input.manuscript.is_empty() {
        return Err(CplError::new(
            "CONTRIBUTION_MAP_EMPTY",
            "A frozen contribution map requires a non-empty manuscript.",
            true,
        ));
    }
    if input.request.page_scalar_capacity == 0 {
        return Err(CplError::new(
            "LAYOUT_PROFILE_INVALID",
            "The page scalar capacity must be greater than zero.",
            false,
        ));
    }

    let manuscript_sha256 = raw_digest(input.manuscript.as_bytes());
    let deposit_sha256 = manuscript_sha256.clone();
    let configuration = json!({
        "algorithm": "unicode-scalar-structural-split-v1",
        "arranged_by_human": input.request.arranged_by_human,
        "page_scalar_capacity": input.request.page_scalar_capacity,
        "selected_by_human": input.request.selected_by_human,
    });
    let configuration_sha256 = canonical_digest(&configuration)?;
    let layout_profile = LayoutProfile {
        layout_profile_id: stable_id("layout", &configuration_sha256),
        layout_profile_sha256: configuration_sha256.clone(),
    };
    let map_id = stable_id(
        "map",
        &format!(
            "{}:{}:{}",
            input.deposit_id, input.revision_id, configuration_sha256
        ),
    );
    let locators = locator_values(&input.manuscript, input.request.page_scalar_capacity);
    let source = input.manuscript.chars().collect::<Vec<_>>();
    let mut source_spans = input.spans.clone();
    source_spans.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.end.cmp(&right.end))
            .then_with(|| left.segment_id.as_bytes().cmp(right.segment_id.as_bytes()))
    });
    validate_source_coverage(&source_spans, source.len())?;
    let source_spans = merge_source_spans(source_spans)?;

    let mut segments = Vec::new();
    let mut structural_locators = Vec::new();
    let mut boundaries = Vec::new();
    for span in source_spans {
        let ancestry_content_sha256 = raw_digest(span.text.as_bytes());
        let mut start = span.start;
        while start < span.end {
            let locator = locators[start];
            let mut end = start + 1;
            while end < span.end && locators[end] == locator {
                end += 1;
            }
            let text = source[start..end].iter().collect::<String>();
            let segment_id = stable_id(
                "segment",
                &format!(
                    "{}:{}:{}:{}:{}",
                    input.deposit_id, span.ancestry_segment_id, start, end, ancestry_content_sha256
                ),
            );
            let included_assertion_id = stable_id(
                "assertion",
                &format!("included:{}:{}", input.deposit_id, segment_id),
            );
            let mut assertion_ids = vec![included_assertion_id.clone()];
            let mut selection_arrangement_assertion_ids = Vec::new();
            if input.request.selected_by_human {
                let id = stable_id(
                    "assertion",
                    &format!("selected:{}:{}", input.deposit_id, segment_id),
                );
                assertion_ids.push(id.clone());
                selection_arrangement_assertion_ids.push(id);
            }
            if input.request.arranged_by_human {
                let id = stable_id(
                    "assertion",
                    &format!("arranged:{}:{}", input.deposit_id, segment_id),
                );
                assertion_ids.push(id.clone());
                selection_arrangement_assertion_ids.push(id);
            }
            let status = segment_status(&span, input.source_verified);
            let relationships = transformation_relationships(&span);
            let content_sha256 = raw_digest(text.as_bytes());
            let segment = MapSegment {
                schema_version: CPL_SCHEMA_VERSION.into(),
                segment_id: segment_id.clone(),
                project_id: input.project_id.clone(),
                revision_id: input.revision_id.clone(),
                segment_sequence: segments.len() + 1,
                range: TextFragmentRange {
                    schema_version: CPL_SCHEMA_VERSION.into(),
                    document_revision_id: input.revision_id.clone(),
                    coordinate_system: "unicode_scalar".into(),
                    start,
                    end,
                    preimage_sha256: manuscript_sha256.clone(),
                    text_fragment_id: stable_id(
                        "fragment",
                        &format!("{}:{start}:{end}:{content_sha256}", input.revision_id),
                    ),
                },
                content_sha256,
                normalized_unicode_scalar_length: end - start,
                recorded_origin_kind: span.origin,
                actor_identity_status: actor_identity(span.origin).into(),
                generation_status: if span.origin == RecordedOrigin::Unattested {
                    "unknown".into()
                } else {
                    "recorded".into()
                },
                lineage_status: if span.origin == RecordedOrigin::Unattested {
                    "unattested".into()
                } else if span.lineage_reference_ids.is_empty() {
                    "unknown".into()
                } else {
                    "recorded".into()
                },
                lineage_reference_ids: sorted_unique(
                    span.lineage_reference_ids
                        .iter()
                        .cloned()
                        .chain(std::iter::once(span.ancestry_segment_id.clone()))
                        .collect(),
                ),
                operation_ids: sorted_unique(span.operation_ids.clone()),
                transformation_relationships: relationships,
                assertion_ids,
                selection_arrangement_assertion_ids,
                included_in_deposit_assertion_id: Some(included_assertion_id),
                classification_status: status,
            };
            if status != ClassificationStatus::Exact {
                boundaries.push(boundary_for_segment(&segment, status));
            }
            structural_locators.push(StructuralLocator {
                segment_id,
                chapter: Some(locator.chapter),
                paragraph: Some(locator.paragraph),
                page: Some(locator.page),
            });
            segments.push(segment);
            start = end;
        }
    }
    validate_map_coverage(&segments, source.len())?;

    let recorded_positions = segments
        .iter()
        .filter(|segment| segment.recorded_origin_kind != RecordedOrigin::Unattested)
        .map(|segment| segment.normalized_unicode_scalar_length)
        .sum::<usize>();
    let classification_status = aggregate_status(&segments);
    let coverage = Coverage {
        denominator: source.len(),
        recorded_positions,
        coverage_status: if recorded_positions == source.len() {
            "complete".into()
        } else {
            "partial".into()
        },
        denominator_unit: "normalized_unicode_scalar_position".into(),
        denominator_definition: "All normalized Unicode scalar positions in the exact frozen deposit manuscript; this is provenance coverage, not a human-authorship percentage.".into(),
    };
    let mut contribution_map = ContributionMap {
        schema_version: CPL_SCHEMA_VERSION.into(),
        contribution_map_id: map_id,
        project_id: input.project_id.clone(),
        deposit_id: input.deposit_id.clone(),
        deposit_sha256: deposit_sha256.clone(),
        manuscript_revision_id: input.revision_id.clone(),
        manuscript_revision_sha256: manuscript_sha256.clone(),
        cpl_chain_head: input.cpl_chain_head.clone(),
        cpl_event_sequence: input.cpl_event_sequence,
        coordinate_system: "unicode_scalar".into(),
        coverage,
        layers: vec![
            "recorded_origin".into(),
            "transformation".into(),
            "selection_arrangement".into(),
            "evidentiary_evaluation".into(),
            "suggested_registration_treatment".into(),
            "structural_locator".into(),
        ],
        segments,
        structural_locators,
        layout_profile: Some(layout_profile.clone()),
        generator: GeneratorIdentity {
            name: GENERATOR_NAME.into(),
            version: GENERATOR_VERSION.into(),
            configuration_sha256,
        },
        classification_status,
        contribution_map_sha256: String::new(),
    };
    refresh_map_digest(&mut contribution_map)?;

    let deposit = DepositSnapshot {
        schema_version: CPL_SCHEMA_VERSION.into(),
        deposit_id: input.deposit_id,
        project_id: input.project_id,
        application_version: APPLICATION_VERSION.into(),
        deposit_path: input.deposit_path,
        media_type: "text/markdown; charset=utf-8".into(),
        byte_length: input.manuscript.len(),
        deposit_sha256,
        manuscript_revision_id: input.revision_id,
        manuscript_revision_sha256: manuscript_sha256,
        cpl_chain_head: input.cpl_chain_head,
        cpl_event_sequence: input.cpl_event_sequence,
        layout_profile: Some(layout_profile),
        created_at: input.source_timestamp.clone(),
    };
    let assertions = build_assertions(
        &contribution_map,
        &deposit,
        &input.source_event_id,
        &input.source_timestamp,
    )?;
    let assertion_evaluations = build_evaluations(
        &assertions,
        &deposit,
        &input.source_event_id,
        &deposit.cpl_chain_head,
        deposit.cpl_event_sequence,
        &input.source_timestamp,
        classification_status,
    )?;
    Ok(ContributionMapProjection {
        deposit,
        contribution_map,
        assertions,
        assertion_evaluations,
        boundaries,
    })
}

pub fn freeze_current(
    root: &Path,
    project_id: &str,
    request: ContributionMapRequest,
) -> CplResult<ContributionMapProjection> {
    if request.page_scalar_capacity == 0 {
        return Err(CplError::new(
            "LAYOUT_PROFILE_INVALID",
            "The page scalar capacity must be greater than zero.",
            false,
        ));
    }
    if let Some(existing) = load_latest(root, project_id)? {
        let current = composition::reconstruct(root, project_id)?;
        let config = configuration_digest(&request)?;
        if existing.contribution_map.manuscript_revision_id == current.revision_id
            && existing.contribution_map.generator.configuration_sha256 == config
        {
            return Ok(existing);
        }
    }

    let projection = composition::reconstruct(root, project_id)?;
    if !projection.initialized || projection.manuscript.is_empty() {
        return Err(CplError::new(
            "CONTRIBUTION_MAP_EMPTY",
            "Initialize and write the manuscript before freezing a contribution map.",
            true,
        ));
    }
    let paths = LedgerPaths::new(root);
    let head = ledger::read_chain_head(&paths)?.ok_or_else(|| {
        CplError::new(
            "CONTRIBUTION_MAP_NO_CHAIN_HEAD",
            "A frozen contribution map requires an existing CPL chain head.",
            true,
        )
    })?;
    let source_verified = matches!(
        verifier::verify_project(root, project_id)?.status,
        VerificationStatus::Verified | VerificationStatus::VerifiedWithWarnings
    );
    let deposit_sha256 = raw_digest(projection.manuscript.as_bytes());
    let config = configuration_digest(&request)?;
    let deposit_id = stable_id(
        "deposit",
        &format!(
            "{project_id}:{}:{deposit_sha256}:{}:{config}",
            projection.revision_id, head.event_sha256
        ),
    );
    let deposit_path = format!("deposits/{deposit_id}.md");
    let input = ContributionMapInput {
        project_id: project_id.into(),
        manuscript: projection.manuscript.clone(),
        revision_id: projection.revision_id.clone(),
        spans: projection.spans,
        deposit_id: deposit_id.clone(),
        deposit_path: deposit_path.clone(),
        cpl_chain_head: head.event_sha256.clone(),
        cpl_event_sequence: head.event_sequence,
        source_event_id: head.event_id.clone(),
        source_timestamp: head.updated_at.clone(),
        source_verified,
        request,
    };
    let bundle = build(input)?;
    write_deposit(root, &deposit_path, projection.manuscript.as_bytes())?;
    let client_action_id = stable_id("action", &format!("contribution-map:{deposit_id}:{config}"));
    let service = CplService::new(root, project_id);
    let bundle_value = serde_json::to_value(&bundle).map_err(serialization_error)?;
    let map_value = serde_json::to_value(&bundle.contribution_map).map_err(serialization_error)?;
    let deposit_value = serde_json::to_value(&bundle.deposit).map_err(serialization_error)?;
    let assertions = bundle.assertions.clone();
    let evaluations = bundle.assertion_evaluations.clone();
    let events = ledger::read_all_events(&paths)?;
    if !events
        .iter()
        .any(|event| event.client_action_id == client_action_id)
    {
        service.write(WriteCommand {
            client_action_id,
            project_id: project_id.into(),
            event_type: "CONTRIBUTION_MAP_FROZEN".into(),
            actor: "system".into(),
            metadata: json!({
                "contribution_map_id": bundle.contribution_map.contribution_map_id,
                "deposit_id": bundle.deposit.deposit_id,
                "manuscript_revision_id": bundle.deposit.manuscript_revision_id,
                "source_chain_head": bundle.deposit.cpl_chain_head,
            }),
            records: std::iter::once(RecordInput {
                record_type: "deposit-snapshot".into(),
                payload: deposit_value,
            })
            .chain(std::iter::once(RecordInput {
                record_type: "contribution-map".into(),
                payload: map_value,
            }))
            .chain(assertions.into_iter().map(|payload| RecordInput {
                record_type: "provenance-assertion".into(),
                payload,
            }))
            .chain(evaluations.into_iter().map(|payload| RecordInput {
                record_type: "assertion-evaluation".into(),
                payload,
            }))
            .chain(std::iter::once(RecordInput {
                record_type: "contribution-map-bundle".into(),
                payload: bundle_value,
            }))
            .collect(),
            operational_state: None,
        })?;
    }
    load_latest(root, project_id)?.ok_or_else(|| {
        CplError::new(
            "CONTRIBUTION_MAP_MISSING",
            "The frozen contribution map could not be reconstructed from canonical records.",
            false,
        )
    })
}

pub fn load_latest(root: &Path, project_id: &str) -> CplResult<Option<ContributionMapProjection>> {
    let events = ledger::read_all_events(&LedgerPaths::new(root))?;
    for event in events.iter().rev() {
        for reference in event.record_references.iter().rev() {
            if reference.record_type != "contribution-map-bundle" {
                continue;
            }
            let record = read_record(root, &reference.path)?;
            let mut projection: ContributionMapProjection = serde_json::from_value(record.payload)
                .map_err(|error| {
                    CplError::new("CONTRIBUTION_MAP_INVALID", error.to_string(), false)
                })?;
            if projection.deposit.project_id != project_id {
                return Err(CplError::new(
                    "CONTRIBUTION_MAP_PROJECT_MISMATCH",
                    "The contribution map belongs to a different project.",
                    false,
                ));
            }
            evaluate_current(root, project_id, &mut projection)?;
            return Ok(Some(projection));
        }
    }
    Ok(None)
}

pub fn canonical_map_bytes(map: &ContributionMap) -> CplResult<Vec<u8>> {
    canonicalize(&serde_json::to_value(map).map_err(serialization_error)?)
}

fn evaluate_current(
    root: &Path,
    project_id: &str,
    projection: &mut ContributionMapProjection,
) -> CplResult<()> {
    let current = composition::reconstruct(root, project_id)?;
    let deposit_path = projection
        .deposit
        .deposit_path
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part));
    let mut global_status = projection.contribution_map.classification_status;
    let mut global_boundary = None;
    if !deposit_path.exists() {
        global_status = ClassificationStatus::Degraded;
        global_boundary = Some((
            "degraded",
            "deposit_missing",
            "The frozen deposit copy is unavailable.",
        ));
    } else {
        let bytes = fs::read(&deposit_path)
            .map_err(|error| CplError::io("Could not read the frozen deposit", error))?;
        if raw_digest(&bytes) != projection.deposit.deposit_sha256 {
            global_status = ClassificationStatus::Stale;
            global_boundary = Some((
                "stale",
                "deposit_changed",
                "The frozen deposit digest no longer matches.",
            ));
        }
    }
    if current.revision_id != projection.deposit.manuscript_revision_id {
        global_status = ClassificationStatus::Stale;
        global_boundary = Some((
            "stale",
            "revision_changed",
            "The active manuscript revision differs from the frozen deposit revision.",
        ));
    }
    let verification = verifier::verify_project(root, project_id)?;
    if !matches!(
        verification.status,
        VerificationStatus::Verified | VerificationStatus::VerifiedWithWarnings
    ) && global_status != ClassificationStatus::Stale
    {
        global_status = ClassificationStatus::Unverified;
        global_boundary = Some((
            "unverified",
            "verification",
            "Native CPL verification is not currently conclusive.",
        ));
    }
    if let Some((status, kind, message)) = global_boundary {
        let denominator = projection.contribution_map.coverage.denominator;
        projection.boundaries.push(ProjectionBoundary {
            boundary_id: stable_id(
                "boundary",
                &format!("{}:{status}:{kind}", projection.deposit.deposit_id),
            ),
            status: status.into(),
            kind: kind.into(),
            start: 0,
            end: denominator,
            message: message.into(),
        });
        for segment in &mut projection.contribution_map.segments {
            segment.classification_status = global_status;
        }
        projection.contribution_map.classification_status = global_status;
    }
    let current_head = ledger::read_chain_head(&LedgerPaths::new(root))?.ok_or_else(|| {
        CplError::new(
            "CONTRIBUTION_MAP_NO_CHAIN_HEAD",
            "The CPL chain head is missing.",
            false,
        )
    })?;
    projection.assertion_evaluations = build_evaluations(
        &projection.assertions,
        &projection.deposit,
        &current_head.event_id,
        &current_head.event_sha256,
        current_head.event_sequence,
        &current_head.updated_at,
        global_status,
    )?;
    refresh_map_digest(&mut projection.contribution_map)?;
    projection.boundaries.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.end.cmp(&right.end))
            .then_with(|| {
                left.boundary_id
                    .as_bytes()
                    .cmp(right.boundary_id.as_bytes())
            })
    });
    projection
        .boundaries
        .dedup_by(|left, right| left.boundary_id == right.boundary_id);
    Ok(())
}

fn build_assertions(
    map: &ContributionMap,
    deposit: &DepositSnapshot,
    source_event_id: &str,
    source_timestamp: &str,
) -> CplResult<Vec<Value>> {
    let mut assertions = Vec::new();
    for segment in &map.segments {
        let predicates = std::iter::once((
            "included_in_deposit",
            segment.included_in_deposit_assertion_id.clone().unwrap(),
        ))
        .chain(
            segment
                .selection_arrangement_assertion_ids
                .iter()
                .map(|id| {
                    let predicate = if id
                        == &stable_id(
                            "assertion",
                            &format!("selected:{}:{}", deposit.deposit_id, segment.segment_id),
                        ) {
                        "selected_by_human"
                    } else {
                        "arranged_by_human"
                    };
                    (predicate, id.clone())
                }),
        );
        for (predicate, assertion_id) in predicates {
            let mut assertion = json!({
                "schema_version": CPL_SCHEMA_VERSION,
                "assertion_id": assertion_id,
                "project_id": map.project_id,
                "subject": {"entity_type": "other", "entity_id": segment.segment_id, "content_sha256": segment.content_sha256},
                "predicate": predicate,
                "object": {"entity_type": "release", "entity_id": deposit.deposit_id, "content_sha256": deposit.deposit_sha256},
                "source_anchor": {"event_id": source_event_id, "event_sequence": deposit.cpl_event_sequence, "event_hash": deposit.cpl_chain_head},
                "source_generation": {"project_epoch": 1, "artifact_revision": null, "transcript_revision": null},
                "lifecycle_phase": "finalized",
                "producer": {"subsystem": "contribution_map_generator", "application_version": APPLICATION_VERSION},
                "provenance": {"basis": if predicate == "included_in_deposit" {"deterministic_derivation"} else {"declared_relationship"}, "evidence": [{"evidence_id": source_event_id, "evidence_type": "provenance_event", "content_sha256": deposit.cpl_chain_head}]},
                "dependencies": [
                    {"dependency_id": source_event_id, "dependency_type": "provenance_event", "expected_sha256": deposit.cpl_chain_head, "expected_generation": {"project_epoch": 1, "artifact_revision": null, "transcript_revision": null}, "evidence_class": "mandatory_retained", "role": "source_anchor", "confidence_dimensions": ["integrity", "chronology", "derivation"]},
                    {"dependency_id": deposit.manuscript_revision_id, "dependency_type": "artifact_revision", "expected_sha256": deposit.manuscript_revision_sha256, "expected_generation": {"project_epoch": 1, "artifact_revision": null, "transcript_revision": null}, "evidence_class": "mandatory_retained", "role": "other", "confidence_dimensions": ["integrity", "derivation", "completeness"]}
                ],
                "reason_code": "DIRECT_HASH_LINKED_DERIVATION",
                "created_at": source_timestamp,
            });
            let digest = canonical_digest(&assertion)?;
            assertion["assertion_sha256"] = Value::String(digest);
            assertions.push(assertion);
        }
    }
    assertions.sort_by(|left, right| {
        left["assertion_id"]
            .as_str()
            .unwrap_or_default()
            .as_bytes()
            .cmp(
                right["assertion_id"]
                    .as_str()
                    .unwrap_or_default()
                    .as_bytes(),
            )
    });
    Ok(assertions)
}

fn build_evaluations(
    assertions: &[Value],
    deposit: &DepositSnapshot,
    evaluated_event_id: &str,
    evaluated_chain_head: &str,
    evaluated_event_sequence: u64,
    evaluated_at: &str,
    map_status: ClassificationStatus,
) -> CplResult<Vec<Value>> {
    let (status, reason, boundary, dependency_status) = match map_status {
        ClassificationStatus::Exact => (
            "exact",
            "DIRECT_HASH_LINKED_DERIVATION",
            Value::Null,
            "valid",
        ),
        ClassificationStatus::Stale => (
            "stale",
            "DEPENDENCY_GENERATION_MISMATCH",
            json!({"kind":"dependency_change","affected_dimensions":["integrity","derivation","completeness"],"dependency_ids":[deposit.manuscript_revision_id],"compatibility":null}),
            "changed",
        ),
        ClassificationStatus::Degraded => (
            "unverified",
            "REQUIRED_EVIDENCE_MISSING",
            json!({"kind":"evidence_access","affected_dimensions":["integrity","derivation","completeness"],"dependency_ids":[deposit.manuscript_revision_id],"compatibility":null}),
            "missing",
        ),
        ClassificationStatus::Unverified => (
            "unverified",
            "REQUIRED_PROVENANCE_UNKNOWN",
            json!({"kind":"missing_provenance","affected_dimensions":["integrity","derivation","completeness"],"dependency_ids":[deposit.manuscript_revision_id],"compatibility":null}),
            "not_evaluated",
        ),
    };
    let mut evaluations = Vec::new();
    for assertion in assertions {
        let assertion_id = assertion["assertion_id"].as_str().unwrap_or_default();
        let assertion_sha256 = assertion["assertion_sha256"].as_str().unwrap_or_default();
        let evaluation_id = stable_id(
            "evaluation",
            &format!("{assertion_id}:{status}:{evaluated_event_id}"),
        );
        evaluations.push(json!({
            "schema_version": CPL_SCHEMA_VERSION,
            "evaluation_id": evaluation_id,
            "assertion_id": assertion_id,
            "assertion_sha256": assertion_sha256,
            "project_id": deposit.project_id,
            "evaluated_against": {"chain_head": evaluated_chain_head, "event_sequence": evaluated_event_sequence, "project_epoch": 1, "schema_catalog_sha256": schema_catalog_digest()},
            "evaluator": {"subsystem": "contribution_map_evaluator", "application_version": APPLICATION_VERSION},
            "status": status,
            "confidence": {"integrity": if status == "exact" {"exact"} else {"unverified"}, "identity":"not_applicable", "chronology": if status == "exact" {"exact"} else {"unverified"}, "derivation": if status == "exact" {"exact"} else {"unverified"}, "authorship":"not_applicable", "completeness": if status == "exact" {"exact"} else {"unverified"}},
            "boundary": boundary,
            "dependency_results": [
                {"dependency_id": assertion["source_anchor"]["event_id"], "evidence_class":"mandatory_retained", "status":"valid", "observed_sha256": deposit.cpl_chain_head, "observed_generation":{"project_epoch":1,"artifact_revision":null,"transcript_revision":null}},
                {"dependency_id": deposit.manuscript_revision_id, "evidence_class":"mandatory_retained", "status": dependency_status, "observed_sha256": if dependency_status == "valid" {Value::String(deposit.manuscript_revision_sha256.clone())} else {Value::Null}, "observed_generation":{"project_epoch":1,"artifact_revision":null,"transcript_revision":null}}
            ],
            "reason_code": reason,
            "supersedes_evaluation_id": null,
            "evaluated_at": evaluated_at,
        }));
    }
    Ok(evaluations)
}

fn locator_values(manuscript: &str, page_capacity: usize) -> Vec<LocatorValue> {
    let chars = manuscript.chars().collect::<Vec<_>>();
    let mut values = vec![
        LocatorValue {
            chapter: 1,
            paragraph: 1,
            page: 1
        };
        chars.len()
    ];
    let mut offset = 0usize;
    let mut chapter = 1usize;
    let mut paragraph = 0usize;
    let mut seen_heading = false;
    let mut previous_blank = true;
    for line in manuscript.split_inclusive('\n') {
        let visible = line.trim_end_matches(['\r', '\n']);
        let blank = visible.trim().is_empty();
        let heading = visible.starts_with("# ") || visible.starts_with("## ");
        if heading {
            if seen_heading {
                chapter += 1;
            }
            seen_heading = true;
        }
        if !blank && previous_blank {
            paragraph += 1;
        }
        let line_length = line.chars().count();
        for index in offset..offset + line_length {
            values[index] = LocatorValue {
                chapter,
                paragraph: paragraph.max(1),
                page: index / page_capacity + 1,
            };
        }
        offset += line_length;
        previous_blank = blank;
    }
    values
}

fn merge_source_spans(spans: Vec<ExpressionSpan>) -> CplResult<Vec<ExpressionSpan>> {
    let mut merged: Vec<ExpressionSpan> = Vec::new();
    for span in spans {
        if let Some(previous) = merged.last_mut() {
            if previous.end == span.start
                && previous.origin == span.origin
                && previous.ancestry_segment_id == span.ancestry_segment_id
                && previous.lineage_reference_ids == span.lineage_reference_ids
                && previous.operation_ids == span.operation_ids
            {
                previous.end = span.end;
                previous.text.push_str(&span.text);
                previous.content_sha256 = raw_digest(previous.text.as_bytes());
                continue;
            }
        }
        merged.push(span);
    }
    Ok(merged)
}
fn validate_source_coverage(spans: &[ExpressionSpan], denominator: usize) -> CplResult<()> {
    let mut cursor = 0usize;
    for span in spans {
        if span.start != cursor || span.end <= span.start || span.end > denominator {
            return Err(CplError::new(
                "CONTRIBUTION_MAP_SOURCE_COVERAGE_INVALID",
                "Composition spans must be ordered, non-overlapping, and cover the frozen manuscript exactly.",
                false,
            ));
        }
        cursor = span.end;
    }
    if cursor != denominator {
        return Err(CplError::new(
            "CONTRIBUTION_MAP_SOURCE_COVERAGE_INVALID",
            "Composition spans do not cover every frozen manuscript Unicode scalar.",
            false,
        ));
    }
    Ok(())
}

fn validate_map_coverage(segments: &[MapSegment], denominator: usize) -> CplResult<()> {
    let mut cursor = 0usize;
    for (index, segment) in segments.iter().enumerate() {
        if segment.segment_sequence != index + 1
            || segment.range.start != cursor
            || segment.range.end <= segment.range.start
            || segment.normalized_unicode_scalar_length != segment.range.end - segment.range.start
        {
            return Err(CplError::new(
                "CONTRIBUTION_MAP_COVERAGE_INVALID",
                "Projected segments must provide complete ordered non-overlapping scalar coverage.",
                false,
            ));
        }
        cursor = segment.range.end;
    }
    if cursor != denominator {
        return Err(CplError::new(
            "CONTRIBUTION_MAP_COVERAGE_INVALID",
            "Projected segment coverage does not equal the declared denominator.",
            false,
        ));
    }
    Ok(())
}

fn segment_status(span: &ExpressionSpan, source_verified: bool) -> ClassificationStatus {
    if !source_verified {
        ClassificationStatus::Unverified
    } else if span.origin == RecordedOrigin::Unattested {
        ClassificationStatus::Unverified
    } else if span.lineage_reference_ids.is_empty() || span.operation_ids.is_empty() {
        ClassificationStatus::Degraded
    } else {
        ClassificationStatus::Exact
    }
}

fn aggregate_status(segments: &[MapSegment]) -> ClassificationStatus {
    if segments
        .iter()
        .any(|segment| segment.classification_status == ClassificationStatus::Stale)
    {
        ClassificationStatus::Stale
    } else if segments
        .iter()
        .any(|segment| segment.classification_status == ClassificationStatus::Degraded)
    {
        ClassificationStatus::Degraded
    } else if segments
        .iter()
        .any(|segment| segment.classification_status == ClassificationStatus::Unverified)
    {
        ClassificationStatus::Unverified
    } else {
        ClassificationStatus::Exact
    }
}

fn boundary_for_segment(segment: &MapSegment, status: ClassificationStatus) -> ProjectionBoundary {
    let (name, kind, message) = if segment.recorded_origin_kind == RecordedOrigin::Unattested {
        (
            "unattested",
            "unattested_expression",
            "No recorded origin resolves this surviving expression range.",
        )
    } else if status == ClassificationStatus::Degraded {
        (
            "degraded",
            "lineage_incomplete",
            "Recorded origin exists, but required lineage or operation evidence is incomplete.",
        )
    } else {
        (
            "unverified",
            "verification",
            "This range could not be verified against the current CPL evidence.",
        )
    };
    ProjectionBoundary {
        boundary_id: stable_id(
            "boundary",
            &format!(
                "{}:{name}:{}:{}",
                segment.segment_id, segment.range.start, segment.range.end
            ),
        ),
        status: name.into(),
        kind: kind.into(),
        start: segment.range.start,
        end: segment.range.end,
        message: message.into(),
    }
}

fn transformation_relationships(span: &ExpressionSpan) -> Vec<String> {
    let mut values = BTreeSet::new();
    match span.origin {
        RecordedOrigin::RecordedDirectHumanInput => {
            values.insert(
                if span
                    .lineage_reference_ids
                    .iter()
                    .any(|id| id.starts_with("operation_"))
                {
                    "modified_by_human"
                } else {
                    "inserted"
                },
            );
        }
        RecordedOrigin::HumanExpressiveInputViaTranscription => {
            values.insert("transcribed_from_human_speech");
        }
        RecordedOrigin::AcceptedAiOutput => {
            values.insert("generated_by_ai");
        }
        RecordedOrigin::ImportedOrPasted => {
            values.insert("pasted_from_external");
        }
        RecordedOrigin::SystemRestoration => {
            values.insert("restored_from_revision");
        }
        RecordedOrigin::Unattested => {
            values.insert("derived_from");
        }
    }
    values.into_iter().map(str::to_owned).collect()
}

fn actor_identity(origin: RecordedOrigin) -> &'static str {
    match origin {
        RecordedOrigin::RecordedDirectHumanInput
        | RecordedOrigin::HumanExpressiveInputViaTranscription => "self_declared",
        RecordedOrigin::AcceptedAiOutput | RecordedOrigin::SystemRestoration => "not_applicable",
        RecordedOrigin::ImportedOrPasted | RecordedOrigin::Unattested => "unknown",
    }
}

fn configuration_digest(request: &ContributionMapRequest) -> CplResult<String> {
    canonical_digest(&json!({
        "algorithm": "unicode-scalar-structural-split-v1",
        "arranged_by_human": request.arranged_by_human,
        "page_scalar_capacity": request.page_scalar_capacity,
        "selected_by_human": request.selected_by_human,
    }))
}

fn refresh_map_digest(map: &mut ContributionMap) -> CplResult<()> {
    let mut identity = serde_json::to_value(&*map).map_err(serialization_error)?;
    identity
        .as_object_mut()
        .expect("ContributionMap always serializes as an object")
        .remove("contribution_map_sha256");
    map.contribution_map_sha256 = canonical_digest(&identity)?;
    Ok(())
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

fn schema_catalog_digest() -> String {
    raw_digest(include_bytes!(
        "../../../schemas/provenance/v1/catalog.json"
    ))
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

fn write_deposit(root: &Path, relative: &str, bytes: &[u8]) -> CplResult<()> {
    let path = relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part));
    let parent = path.parent().ok_or_else(|| {
        CplError::new(
            "DEPOSIT_PATH_INVALID",
            "The deposit path has no parent.",
            false,
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| CplError::io("Could not create the deposits directory", error))?;
    let temporary = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap().to_string_lossy()
    ));
    fs::write(&temporary, bytes)
        .map_err(|error| CplError::io("Could not stage the frozen deposit", error))?;
    ledger::atomic_replace(&temporary, &path)?;
    ledger::sync_directory(parent)?;
    Ok(())
}

fn read_record(root: &Path, relative: &str) -> CplResult<CplRecord> {
    let path = relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part));
    let bytes = fs::read(&path)
        .map_err(|error| CplError::io("Could not read a contribution-map record", error))?;
    let record: CplRecord = serde_json::from_slice(&bytes).map_err(|error| {
        CplError::new("CONTRIBUTION_MAP_RECORD_INVALID", error.to_string(), false)
    })?;
    if canonicalize(&serde_json::to_value(&record).map_err(serialization_error)?)? != bytes {
        return Err(CplError::new(
            "CONTRIBUTION_MAP_RECORD_NONCANONICAL",
            format!("{} is not canonical JSON.", path.display()),
            false,
        ));
    }
    Ok(record)
}

fn serialization_error(error: impl std::fmt::Display) -> CplError {
    CplError::new(
        "CONTRIBUTION_MAP_SERIALIZATION_FAILED",
        error.to_string(),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::{composition::CompositionAction, identifiers::timestamp_millis};
    use tempfile::tempdir;

    fn formal_project_id() -> &'static str {
        "project_01J00000000000000000000001"
    }

    fn command(id: &str, action: CompositionAction) -> composition::CompositionCommand {
        composition::CompositionCommand {
            client_action_id: id.into(),
            actor: "user".into(),
            summary: id.into(),
            occurred_at: timestamp_millis(),
            action,
        }
    }

    fn sample_projection(spans: Vec<ExpressionSpan>) -> ContributionMapProjection {
        sample_projection_with_verification(spans, true)
    }

    fn sample_projection_with_verification(
        spans: Vec<ExpressionSpan>,
        source_verified: bool,
    ) -> ContributionMapProjection {
        build(ContributionMapInput {
            project_id: formal_project_id().into(),
            manuscript: "# One\n\nAlpha beta\n\n## Two\n\nGamma 🧵 delta".into(),
            revision_id: "revision_01J0000000000000000000000B".into(),
            spans,
            deposit_id: "deposit_01J00000000000000000000000".into(),
            deposit_path: "deposits/manuscript.md".into(),
            cpl_chain_head: format!("sha256:{}", "4".repeat(64)),
            cpl_event_sequence: 42,
            source_event_id: "event_01J00000000000000000000004".into(),
            source_timestamp: "2026-07-17T18:42:10.123Z".into(),
            source_verified,
            request: ContributionMapRequest {
                page_scalar_capacity: 12,
                selected_by_human: true,
                arranged_by_human: true,
            },
        })
        .unwrap()
    }

    fn sample_spans() -> Vec<ExpressionSpan> {
        let manuscript = "# One\n\nAlpha beta\n\n## Two\n\nGamma 🧵 delta";
        let split = manuscript.find("## Two").unwrap();
        let scalar_split = manuscript[..split].chars().count();
        let denominator = manuscript.chars().count();
        vec![
            ExpressionSpan {
                segment_id: stable_id("segment", "source-one"),
                ancestry_segment_id: stable_id("segment", "ancestry-one"),
                start: 0,
                end: scalar_split,
                text: manuscript.chars().take(scalar_split).collect(),
                content_sha256: raw_digest(manuscript[..split].as_bytes()),
                origin: RecordedOrigin::RecordedDirectHumanInput,
                lineage_reference_ids: vec![stable_id("operation", "op-one")],
                operation_ids: vec![stable_id("operation", "op-one")],
            },
            ExpressionSpan {
                segment_id: stable_id("segment", "source-two"),
                ancestry_segment_id: stable_id("segment", "ancestry-two"),
                start: scalar_split,
                end: denominator,
                text: manuscript.chars().skip(scalar_split).collect(),
                content_sha256: raw_digest(manuscript[split..].as_bytes()),
                origin: RecordedOrigin::AcceptedAiOutput,
                lineage_reference_ids: vec![stable_id("operation", "op-two")],
                operation_ids: vec![stable_id("operation", "op-two")],
            },
        ]
    }

    #[test]
    fn identical_canonical_input_is_byte_identical_for_any_span_order() {
        let spans = sample_spans();
        let forward = sample_projection(spans.clone());
        let reverse = sample_projection(spans.into_iter().rev().collect());
        assert_eq!(
            canonical_map_bytes(&forward.contribution_map).unwrap(),
            canonical_map_bytes(&reverse.contribution_map).unwrap()
        );
        assert_eq!(forward.contribution_map, reverse.contribution_map);
        let mut identity = serde_json::to_value(&forward.contribution_map).unwrap();
        identity
            .as_object_mut()
            .unwrap()
            .remove("contribution_map_sha256");
        assert_eq!(
            canonical_digest(&identity).unwrap(),
            forward.contribution_map.contribution_map_sha256
        );
    }

    #[test]
    fn equivalent_adjacent_source_splits_merge_to_the_same_map() {
        let original = sample_spans();
        let expected = sample_projection(original.clone());
        let first = &original[0];
        let split = 5usize;
        let left_text = first.text.chars().take(split).collect::<String>();
        let right_text = first.text.chars().skip(split).collect::<String>();
        let mut left = first.clone();
        left.end = split;
        left.text = left_text.clone();
        left.content_sha256 = raw_digest(left_text.as_bytes());
        let mut right = first.clone();
        right.start = split;
        right.text = right_text.clone();
        right.content_sha256 = raw_digest(right_text.as_bytes());
        let actual = sample_projection(vec![right, original[1].clone(), left]);
        assert_eq!(expected.contribution_map, actual.contribution_map);
    }
    #[test]
    fn splits_at_structural_and_page_boundaries_without_losing_ancestry() {
        let projection = sample_projection(sample_spans());
        let denominator = projection.contribution_map.coverage.denominator;
        assert!(projection.contribution_map.segments.len() > 2);
        assert_eq!(
            projection
                .contribution_map
                .segments
                .iter()
                .map(|segment| segment.normalized_unicode_scalar_length)
                .sum::<usize>(),
            denominator
        );
        assert!(projection
            .contribution_map
            .segments
            .iter()
            .all(|segment| !segment.lineage_reference_ids.is_empty()));
        assert!(projection
            .contribution_map
            .structural_locators
            .iter()
            .any(|locator| locator.chapter == Some(2)));
        assert!(projection
            .contribution_map
            .structural_locators
            .iter()
            .any(|locator| locator.page.unwrap() > 1));
    }

    #[test]
    fn reports_unattested_coverage_without_calling_it_non_human() {
        let mut spans = sample_spans();
        spans[0].origin = RecordedOrigin::Unattested;
        spans[0].lineage_reference_ids.clear();
        spans[0].operation_ids.clear();
        let projection = sample_projection(spans);
        assert_eq!(
            projection.contribution_map.classification_status,
            ClassificationStatus::Unverified
        );
        assert_eq!(
            projection.contribution_map.coverage.coverage_status,
            "partial"
        );
        assert!(projection
            .contribution_map
            .coverage
            .denominator_definition
            .contains("not a human-authorship percentage"));
        assert!(projection
            .boundaries
            .iter()
            .any(|boundary| boundary.status == "unattested"));
    }

    #[test]
    fn inconclusive_source_verification_is_visibly_unverified() {
        let projection = sample_projection_with_verification(sample_spans(), false);
        assert_eq!(
            projection.contribution_map.classification_status,
            ClassificationStatus::Unverified
        );
        assert!(projection
            .boundaries
            .iter()
            .all(|boundary| boundary.status == "unverified"));
    }
    #[test]
    fn frozen_map_becomes_stale_after_a_later_composition_revision() {
        let temp = tempdir().unwrap();
        let project_id = formal_project_id();
        composition::apply_command(
            temp.path(),
            project_id,
            command(
                "initialize_map",
                CompositionAction::Initialize {
                    text: "Frozen text".into(),
                    origin: RecordedOrigin::Unattested,
                },
            ),
        )
        .unwrap();
        let frozen =
            freeze_current(temp.path(), project_id, ContributionMapRequest::default()).unwrap();
        assert_eq!(
            frozen.contribution_map.manuscript_revision_id,
            composition::reconstruct(temp.path(), project_id)
                .unwrap()
                .revision_id
        );
        composition::apply_command(
            temp.path(),
            project_id,
            command(
                "later_edit",
                CompositionAction::Edit {
                    before_text: "Frozen text".into(),
                    after_text: "Frozen text changed".into(),
                    boundary: composition::CompositionBoundary::Idle,
                    origin: RecordedOrigin::RecordedDirectHumanInput,
                    operation_kind_hint: None,
                    ai_acceptance: None,
                },
            ),
        )
        .unwrap();
        let evaluated = load_latest(temp.path(), project_id).unwrap().unwrap();
        assert_eq!(
            evaluated.contribution_map.classification_status,
            ClassificationStatus::Stale
        );
        assert!(evaluated
            .boundaries
            .iter()
            .any(|boundary| boundary.status == "stale"));
        assert!(evaluated
            .assertion_evaluations
            .iter()
            .all(|evaluation| evaluation["status"] == "stale"));
    }
    #[test]
    fn verified_frozen_map_is_exact_and_reused_for_identical_input() {
        let temp = tempdir().unwrap();
        let project_id = formal_project_id();
        composition::apply_command(
            temp.path(),
            project_id,
            command(
                "exact_initialize",
                CompositionAction::Initialize {
                    text: String::new(),
                    origin: RecordedOrigin::Unattested,
                },
            ),
        )
        .unwrap();
        composition::apply_command(
            temp.path(),
            project_id,
            command(
                "exact_human_edit",
                CompositionAction::Edit {
                    before_text: String::new(),
                    after_text: "# Chapter\n\nRecorded expression".into(),
                    boundary: composition::CompositionBoundary::ExplicitSave,
                    origin: RecordedOrigin::RecordedDirectHumanInput,
                    operation_kind_hint: None,
                    ai_acceptance: None,
                },
            ),
        )
        .unwrap();
        let request = ContributionMapRequest::default();
        let first = freeze_current(temp.path(), project_id, request.clone()).unwrap();
        assert_eq!(
            first.contribution_map.classification_status,
            ClassificationStatus::Exact
        );
        assert!(first.boundaries.is_empty());
        assert!(first
            .assertion_evaluations
            .iter()
            .all(|evaluation| evaluation["status"] == "exact"));
        let event_count = ledger::read_all_events(&LedgerPaths::new(temp.path()))
            .unwrap()
            .len();
        let second = freeze_current(temp.path(), project_id, request).unwrap();
        assert_eq!(first.contribution_map, second.contribution_map);
        assert_eq!(
            ledger::read_all_events(&LedgerPaths::new(temp.path()))
                .unwrap()
                .len(),
            event_count
        );
    }
    #[test]
    fn missing_frozen_deposit_is_visibly_degraded() {
        let temp = tempdir().unwrap();
        let project_id = formal_project_id();
        composition::apply_command(
            temp.path(),
            project_id,
            command(
                "degraded_initialize",
                CompositionAction::Initialize {
                    text: String::new(),
                    origin: RecordedOrigin::Unattested,
                },
            ),
        )
        .unwrap();
        composition::apply_command(
            temp.path(),
            project_id,
            command(
                "degraded_human_edit",
                CompositionAction::Edit {
                    before_text: String::new(),
                    after_text: "Recorded expression".into(),
                    boundary: composition::CompositionBoundary::Checkpoint,
                    origin: RecordedOrigin::RecordedDirectHumanInput,
                    operation_kind_hint: None,
                    ai_acceptance: None,
                },
            ),
        )
        .unwrap();
        let frozen =
            freeze_current(temp.path(), project_id, ContributionMapRequest::default()).unwrap();
        let deposit_path = frozen
            .deposit
            .deposit_path
            .split('/')
            .fold(temp.path().to_path_buf(), |path, part| path.join(part));
        fs::remove_file(deposit_path).unwrap();
        let evaluated = load_latest(temp.path(), project_id).unwrap().unwrap();
        assert_eq!(
            evaluated.contribution_map.classification_status,
            ClassificationStatus::Degraded
        );
        assert!(evaluated
            .boundaries
            .iter()
            .any(|boundary| boundary.status == "degraded"));
        assert!(evaluated
            .assertion_evaluations
            .iter()
            .all(|evaluation| evaluation["status"] == "unverified"));
    }
    #[test]
    fn complex_unicode_has_complete_contiguous_segment_coverage() {
        let manuscript = "# Café\n\nnaïve e\u{301} 🧑🏽‍💻 🇺🇳 中文\r\nline";
        let denominator = manuscript.chars().count();
        let span = ExpressionSpan {
            segment_id: stable_id("segment", "unicode-source"),
            ancestry_segment_id: stable_id("segment", "unicode-ancestry"),
            start: 0,
            end: denominator,
            text: manuscript.into(),
            content_sha256: raw_digest(manuscript.as_bytes()),
            origin: RecordedOrigin::RecordedDirectHumanInput,
            lineage_reference_ids: vec![stable_id("operation", "unicode-op")],
            operation_ids: vec![stable_id("operation", "unicode-op")],
        };
        let projection = build(ContributionMapInput {
            project_id: formal_project_id().into(),
            manuscript: manuscript.into(),
            revision_id: "revision_01J0000000000000000000000B".into(),
            spans: vec![span],
            deposit_id: "deposit_01J00000000000000000000000".into(),
            deposit_path: "deposits/unicode.md".into(),
            cpl_chain_head: format!("sha256:{}", "7".repeat(64)),
            cpl_event_sequence: 11,
            source_event_id: "event_01J00000000000000000000004".into(),
            source_timestamp: "2026-07-19T12:00:00.000Z".into(),
            source_verified: true,
            request: ContributionMapRequest {
                page_scalar_capacity: 7,
                selected_by_human: true,
                arranged_by_human: true,
            },
        })
        .unwrap();
        let segments = &projection.contribution_map.segments;
        assert_eq!(
            projection.contribution_map.coverage.denominator,
            denominator
        );
        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.normalized_unicode_scalar_length)
                .sum::<usize>(),
            denominator
        );
        assert_eq!(segments.first().unwrap().range.start, 0);
        assert_eq!(segments.last().unwrap().range.end, denominator);
        assert!(segments
            .windows(2)
            .all(|pair| pair[0].range.end == pair[1].range.start));
        assert!(segments.iter().all(|segment| {
            segment.range.end - segment.range.start == segment.normalized_unicode_scalar_length
        }));
    }
}
