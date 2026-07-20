//! Read-only CPL explorer and HARP traceability projection.

use super::{
    composition::{self, CompositionProjection},
    contribution_map::{self, ContributionMapProjection},
    harp::{self, HarpProjection},
    ledger::{self, LedgerPaths},
    records::{CplRecord, VerificationReport},
    verifier, CplResult,
};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};

#[derive(Debug, Clone, Serialize)]
pub struct ExplorerRecord {
    pub record_id: String,
    pub record_type: String,
    pub path: String,
    pub record_sha256: String,
    pub subject_ids: Vec<String>,
    pub accessible: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExplorerEvent {
    pub event_id: String,
    pub event_sequence: u64,
    pub timestamp: String,
    pub event_type: String,
    pub actor: String,
    pub event_sha256: String,
    pub previous_event_sha256: Option<String>,
    pub records: Vec<ExplorerRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarpStatementTrace {
    pub statement_id: String,
    pub harp_path: String,
    pub statement: String,
    pub category: String,
    pub segment_ids: Vec<String>,
    pub assertion_ids: Vec<String>,
    pub evaluation_ids: Vec<String>,
    pub record_ids: Vec<String>,
    pub trace_note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CplExplorerProjection {
    pub verification: VerificationReport,
    pub events: Vec<ExplorerEvent>,
    pub composition: CompositionProjection,
    pub contribution_map: Option<ContributionMapProjection>,
    pub harp: Option<HarpProjection>,
    pub harp_statement_traces: Vec<HarpStatementTrace>,
}

pub fn load(root: &Path, project_id: &str) -> CplResult<CplExplorerProjection> {
    let verification = verifier::verify_project(root, project_id)?;
    let events = explorer_events(root)?;
    let composition = composition::reconstruct(root, project_id)?;
    let contribution_map = contribution_map::load_latest(root, project_id)?;
    let harp = harp::load_latest(root, project_id)?;
    let harp_statement_traces = match (&harp, &contribution_map) {
        (Some(harp), Some(map)) => statement_traces(harp, map, &events),
        _ => Vec::new(),
    };
    Ok(CplExplorerProjection {
        verification,
        events,
        composition,
        contribution_map,
        harp,
        harp_statement_traces,
    })
}

fn explorer_events(root: &Path) -> CplResult<Vec<ExplorerEvent>> {
    ledger::read_all_events(&LedgerPaths::new(root))?
        .into_iter()
        .map(|event| {
            let records = event
                .record_references
                .iter()
                .map(|reference| {
                    let path = resolve_relative(root, &reference.path);
                    let record = fs::read(&path)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<CplRecord>(&bytes).ok());
                    let subject_ids = record
                        .as_ref()
                        .map(|record| identifiers_in(&record.payload))
                        .unwrap_or_default();
                    ExplorerRecord {
                        record_id: reference.record_id.clone(),
                        record_type: reference.record_type.clone(),
                        path: reference.path.clone(),
                        record_sha256: reference.record_sha256.clone(),
                        subject_ids,
                        accessible: record.is_some(),
                    }
                })
                .collect();
            Ok(ExplorerEvent {
                event_id: event.event_id,
                event_sequence: event.event_sequence,
                timestamp: event.timestamp,
                event_type: event.event_type,
                actor: event.actor,
                event_sha256: event.event_sha256,
                previous_event_sha256: event.previous_event_sha256,
                records,
            })
        })
        .collect()
}

fn statement_traces(
    harp: &HarpProjection,
    map: &ContributionMapProjection,
    events: &[ExplorerEvent],
) -> Vec<HarpStatementTrace> {
    let all_segments = map
        .contribution_map
        .segments
        .iter()
        .map(|segment| segment.segment_id.clone())
        .collect::<Vec<_>>();
    let all_assertions = map
        .assertions
        .iter()
        .filter_map(|value| value["assertion_id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let all_evaluations = map
        .assertion_evaluations
        .iter()
        .filter_map(|value| value["evaluation_id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    let all_records = matching_records(
        events,
        &all_segments,
        &all_assertions,
        &all_evaluations,
        &["human-authorship-record", "harp-export-manifest"],
    );
    let approval_records = matching_records(
        events,
        &[],
        &[],
        &[],
        &["harp-generation-approval", "human-authorship-record"],
    );
    let mut traces = vec![
        trace(
            "deposit-binding",
            "deposit",
            format!("Exact deposit {} is bound to manuscript revision {}.", harp.harp["deposit"]["deposit_sha256"].as_str().unwrap_or("unknown"), harp.harp["deposit"]["manuscript_revision_id"].as_str().unwrap_or("unknown")),
            "evidence_fact",
            &all_segments,
            &all_assertions,
            &all_evaluations,
            &all_records,
            "The deposit binding is supported by inclusion assertions, their current evaluations, the frozen map, and deposit records.",
        ),
        trace(
            "cpl-binding",
            "cpl_binding",
            format!("HARP uses CPL chain sequence {} at {}.", harp.harp["cpl_binding"]["event_sequence"], harp.harp["cpl_binding"]["chain_head"].as_str().unwrap_or("unknown")),
            "evidence_fact",
            &all_segments,
            &all_assertions,
            &all_evaluations,
            &all_records,
            "The chain binding is checked only by the native verifier.",
        ),
        trace(
            "contribution-map-binding",
            "contribution_map",
            format!("Contribution map {} has digest {}.", harp.harp["contribution_map"]["contribution_map_id"].as_str().unwrap_or("unknown"), harp.harp["contribution_map"]["contribution_map_sha256"].as_str().unwrap_or("unknown")),
            "derived_classification",
            &all_segments,
            &all_assertions,
            &all_evaluations,
            &all_records,
            "The map is a deterministic projection of recorded origin, lineage, assertions, and evaluations.",
        ),
        trace(
            "evidentiary-status",
            "evidentiary_status",
            format!("Evidentiary status is {} and applicability is {}.", harp.harp["evidentiary_status"].as_str().unwrap_or("unknown"), harp.applicability_status),
            "derived_classification",
            &all_segments,
            &all_assertions,
            &all_evaluations,
            &all_records,
            "This status describes recorded evidence integrity and coverage; it is not a frontend validity or legal-authorship declaration.",
        ),
        trace(
            "claim-summary",
            "claim_summary",
            harp.harp["claim_summary"].as_str().unwrap_or("No claim summary recorded.").to_owned(),
            "derived_classification",
            &all_segments,
            &all_assertions,
            &all_evaluations,
            &all_records,
            "Every included segment is connected to its inclusion assertion, current evaluation, and underlying composition records.",
        ),
        trace(
            "coverage",
            "coverage.statement",
            harp.harp["coverage"]["statement"].as_str().unwrap_or("No coverage statement recorded.").to_owned(),
            "derived_classification",
            &all_segments,
            &all_assertions,
            &all_evaluations,
            &all_records,
            "Coverage counts normalized Unicode scalar positions with recorded provenance; it is never a human-authorship percentage.",
        ),
        trace(
            "identity-declaration",
            "identity_declaration",
            format!("Identity was {} as {}.", harp.harp["identity_declaration"]["identity_status"].as_str().unwrap_or("unknown"), harp.harp["identity_declaration"]["declared_name"].as_str().unwrap_or("unnamed")),
            "user_declaration",
            &[],
            &[],
            &[],
            &approval_records,
            "Identity is a user declaration tied to the approval record; Thinkloom does not infer or verify it unless separate evidence is recorded.",
        ),
        trace(
            "policy-profile",
            "policy_profile",
            format!("Registration suggestions use policy profile {} version {}, retrieved {}.", harp.harp["policy_profile"]["policy_profile_id"].as_str().unwrap_or("unknown"), harp.harp["policy_profile"]["profile_version"].as_str().unwrap_or("unknown"), harp.harp["policy_profile"]["retrieved_on"].as_str().unwrap_or("unknown")),
            "evidence_fact",
            &[],
            &[],
            &[],
            &all_records,
            "The immutable policy-profile identity and digest are embedded in the HARP record.",
        ),
        trace(
            "registration-language",
            "suggested_registration_language",
            "Author Created, Material Excluded, New Material Included, and Note to CO language was explicitly approved.".into(),
            "suggested_application_language",
            &all_segments,
            &all_assertions,
            &all_evaluations,
            &approval_records,
            "The wording is an approved suggestion linked to the evidence set and approval event; it is editable and is not legal advice.",
        ),
        trace(
            "limitations",
            "limitation_codes",
            format!("HARP records {} limitation codes.", harp.harp["limitation_codes"].as_array().map_or(0, Vec::len)),
            "derived_classification",
            &all_segments,
            &all_assertions,
            &all_evaluations,
            &all_records,
            "Limitations expose missing, degraded, sanitized, self-declared, or legally undetermined dimensions.",
        ),
        trace(
            "legal-scope",
            "explanation_codes",
            harp.report_metadata.legal_scope_statement.clone(),
            "legal_determination_not_made",
            &[],
            &[],
            &[],
            &all_records,
            "This is an explicit boundary: Thinkloom does not make the listed legal determinations.",
        ),
    ];
    for (index, disclosure) in harp.harp["ai_system_disclosures"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let segment_ids = disclosure["included_expression_segment_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let assertion_ids = assertion_ids_for_segments(map, &segment_ids);
        let evaluation_ids = evaluation_ids_for_assertions(map, &assertion_ids);
        let records = matching_records(events, &segment_ids, &assertion_ids, &evaluation_ids, &[]);
        traces.push(trace(
            &format!("ai-disclosure-{index}"),
            &format!("ai_system_disclosures[{index}]"),
            format!("Provider {} model {} is recorded for {} included segments.", disclosure["provider_id"].as_str().unwrap_or("unknown"), disclosure["model_id"].as_str().unwrap_or("unknown"), segment_ids.len()),
            "evidence_fact",
            &segment_ids,
            &assertion_ids,
            &evaluation_ids,
            &records,
            "AI-system identity is read from recorded invocation requests and joined to accepted-output segment lineage.",
        ));
    }
    traces
}

fn trace(
    statement_id: &str,
    harp_path: &str,
    statement: String,
    category: &str,
    segment_ids: &[String],
    assertion_ids: &[String],
    evaluation_ids: &[String],
    record_ids: &[String],
    trace_note: &str,
) -> HarpStatementTrace {
    HarpStatementTrace {
        statement_id: statement_id.into(),
        harp_path: harp_path.into(),
        statement,
        category: category.into(),
        segment_ids: segment_ids.to_vec(),
        assertion_ids: assertion_ids.to_vec(),
        evaluation_ids: evaluation_ids.to_vec(),
        record_ids: record_ids.to_vec(),
        trace_note: trace_note.into(),
    }
}

fn assertion_ids_for_segments(map: &ContributionMapProjection, segments: &[String]) -> Vec<String> {
    map.contribution_map
        .segments
        .iter()
        .filter(|segment| segments.contains(&segment.segment_id))
        .flat_map(|segment| segment.assertion_ids.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn evaluation_ids_for_assertions(
    map: &ContributionMapProjection,
    assertions: &[String],
) -> Vec<String> {
    map.assertion_evaluations
        .iter()
        .filter(|value| {
            value["assertion_id"]
                .as_str()
                .is_some_and(|id| assertions.iter().any(|assertion| assertion == id))
        })
        .filter_map(|value| value["evaluation_id"].as_str().map(str::to_owned))
        .collect()
}

fn matching_records(
    events: &[ExplorerEvent],
    segments: &[String],
    assertions: &[String],
    evaluations: &[String],
    record_types: &[&str],
) -> Vec<String> {
    let wanted = segments
        .iter()
        .chain(assertions)
        .chain(evaluations)
        .collect::<BTreeSet<_>>();
    events
        .iter()
        .flat_map(|event| &event.records)
        .filter(|record| {
            record_types.contains(&record.record_type.as_str())
                || record.subject_ids.iter().any(|id| wanted.contains(id))
        })
        .map(|record| record.record_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn identifiers_in(value: &Value) -> Vec<String> {
    let mut values = BTreeSet::new();
    collect_identifiers(value, &mut values);
    values.into_iter().collect()
}

fn collect_identifiers(value: &Value, output: &mut BTreeSet<String>) {
    match value {
        Value::String(value) if is_identifier(value) => {
            output.insert(value.clone());
        }
        Value::Array(values) => {
            for value in values {
                collect_identifiers(value, output);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                collect_identifiers(value, output);
            }
        }
        _ => {}
    }
}

fn is_identifier(value: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "assertion_",
        "deposit_",
        "disposition_",
        "evaluation_",
        "event_",
        "fragment_",
        "harp_",
        "invocation_",
        "map_",
        "operation_",
        "record_",
        "revision_",
        "segment_",
    ];
    PREFIXES.iter().any(|prefix| value.starts_with(prefix))
}

fn resolve_relative(root: &Path, relative: &str) -> std::path::PathBuf {
    relative
        .split('/')
        .fold(root.to_path_buf(), |path, part| path.join(part))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursively_finds_traceable_identifiers_without_hashes_or_text() {
        let value = serde_json::json!({
            "subject": { "segment_id": "segment_01J0000000000000000000000Y" },
            "dependencies": ["assertion_01J0000000000000000000000V"],
            "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "text": "ordinary prose",
        });
        assert_eq!(
            identifiers_in(&value),
            vec![
                "assertion_01J0000000000000000000000V".to_owned(),
                "segment_01J0000000000000000000000Y".to_owned(),
            ]
        );
    }

    #[test]
    fn every_evidentiary_harp_statement_links_to_native_cpl_evidence() {
        use crate::provenance::{
            composition::{
                CompositionAction, CompositionBoundary, CompositionCommand, RecordedOrigin,
            },
            contribution_map::ContributionMapRequest,
            harp::{HarpGenerationRequest, RegistrationLanguageInput},
            identifiers::timestamp_millis,
            records::VerificationStatus,
        };

        let temp = tempfile::tempdir().unwrap();
        let project_id = "project_01J00000000000000000000009";
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
            "explorer_initialize",
            CompositionAction::Initialize {
                text: String::new(),
                origin: RecordedOrigin::Unattested,
            },
        );
        apply(
            "explorer_human_edit",
            CompositionAction::Edit {
                before_text: String::new(),
                after_text: "Traceable human expression.".into(),
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
        harp::generate_current(
            temp.path(),
            project_id,
            HarpGenerationRequest {
                declared_name: Some("Example Author".into()),
                identity_status: "self_declared".into(),
                identity_evidence_reference_ids: vec![],
                sanitization_profile: "sanitized".into(),
                suggested_registration_language: RegistrationLanguageInput::default(),
                user_approved: true,
            },
        )
        .unwrap();

        let projection = load(temp.path(), project_id).unwrap();
        assert_eq!(projection.verification.status, VerificationStatus::Verified);
        assert!(!projection.events.is_empty());
        assert!(projection.harp.is_some());
        assert!(!projection.harp_statement_traces.is_empty());

        for trace in &projection.harp_statement_traces {
            assert!(!trace.statement.is_empty());
            assert!(!trace.harp_path.is_empty());
            assert!(!trace.record_ids.is_empty());
            if matches!(
                trace.category.as_str(),
                "derived_classification" | "suggested_application_language"
            ) {
                assert!(!trace.assertion_ids.is_empty());
                assert!(!trace.evaluation_ids.is_empty());
            }
        }
        let claim = projection
            .harp_statement_traces
            .iter()
            .find(|trace| trace.statement_id == "claim-summary")
            .unwrap();
        assert!(!claim.segment_ids.is_empty());
        assert!(!claim.assertion_ids.is_empty());
        assert!(!claim.evaluation_ids.is_empty());
        assert!(!claim.record_ids.is_empty());

        let identity = projection
            .harp_statement_traces
            .iter()
            .find(|trace| trace.statement_id == "identity-declaration")
            .unwrap();
        assert_eq!(identity.category, "user_declaration");
        assert!(identity.assertion_ids.is_empty());
        assert!(identity.evaluation_ids.is_empty());
        assert!(!identity.record_ids.is_empty());
    }
}
