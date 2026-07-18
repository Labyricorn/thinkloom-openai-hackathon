import { mkdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { canonicalize, hashEvent, releaseMerkleRoot, sha256 } from "./provenance-stage2-lib.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.join(root, "schemas", "provenance", "v1");
const validDir = path.join(output, "fixtures", "valid");
const invalidDir = path.join(output, "fixtures", "invalid");
const vectorDir = path.join(output, "vectors");
const registryDir = path.join(output, "registries");
const baseId = "https://thinkloom.app/schemas/provenance/1.0";
const draft = "https://json-schema.org/draft/2020-12/schema";
const timestampValue = "2026-07-17T18:42:10.123Z";
const laterTimestamp = "2026-07-17T18:43:11.456Z";
const digest = (character = "a") => `sha256:${character.repeat(64)}`;
const ulid = (suffix = "0") => `01J${"0".repeat(22)}${suffix}`;
const typedId = (type, suffix = "0") => `${type}_${ulid(suffix)}`;
const ids = {
  project: typedId("project", "1"), intent: typedId("intent", "2"), event: typedId("event", "3"), event2: typedId("event", "4"),
  record: typedId("record", "5"), session: typedId("session", "6"), turn: typedId("turn", "7"), invocation: typedId("invocation", "8"),
  idea: typedId("idea", "9"), revision: typedId("revision", "A"), revision2: typedId("revision", "B"), fragment: typedId("fragment", "C"),
  checkpoint: typedId("checkpoint", "D"), release: typedId("release", "E"), template: typedId("template", "F"), key: typedId("key", "G"),
  policy: typedId("policy", "H"), correction: typedId("correction", "J"), normalization: typedId("normalization", "K"), disposition: typedId("disposition", "M"),
  transaction: typedId("transaction", "N"), failure: typedId("failure", "P"), stream: typedId("stream", "Q"), purge: typedId("purge", "R"),
  assertion: typedId("assertion", "V"), evaluation: typedId("evaluation", "W"),
};

if (!Object.values(ids).every((value) => /^[a-z]+_[0-9A-HJKMNP-TV-Z]{26}$/.test(value))) throw new Error("Generated fixture identifier is invalid.");

const schemaVersion = { const: "1.0" };
const timestamp = { type: "string", format: "date-time", pattern: "^\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}\\.\\d{3}Z$" };
const sha = { type: "string", pattern: "^sha256:[a-f0-9]{64}$" };
const ulidPattern = "[0-9A-HJKMNP-TV-Z]{26}";
const identifier = (prefix) => ({ type: "string", pattern: `^${prefix}_${ulidPattern}$` });
const typedIdentifier = { type: "string", pattern: `^[a-z][a-z0-9_]*_${ulidPattern}$` };
const repositoryPath = { type: "string", minLength: 1, pattern: "^(?!/)(?![A-Za-z]:)(?!.*\\\\)(?!.*(?:^|/)\\.\\.?(?:/|$))[^\\u0000-\\u001f]+$" };
const nonEmptyString = { type: "string", minLength: 1 };
const jsonValue = {};
const nullable = (schema) => ({ anyOf: [schema, { type: "null" }] });
const ref = (name) => ({ $ref: `${baseId}/${name}.schema.json` });
const arrayOf = (items, options = {}) => ({ type: "array", items, ...options });
const mapOf = (additionalProperties) => ({ type: "object", additionalProperties });
const closed = (properties, required = Object.keys(properties), extra = {}) => ({ type: "object", properties, required, additionalProperties: false, ...extra });

const assertionLifecyclePhases = ["proposed", "generated", "staged_preview", "accepted_into_work", "revised", "finalized", "published", "superseded", "purged"];
const assertionEvaluationStatuses = ["exact", "degraded", "refused", "stale", "unverified"];
const confidenceDimensions = ["integrity", "identity", "chronology", "derivation", "authorship", "completeness"];
const confidenceValues = ["exact", "degraded", "unverified", "not_applicable"];
const evidenceClasses = ["mandatory_live", "mandatory_retained", "advisory", "shadow"];
const assertionBoundaryKinds = ["missing_provenance", "unknown_generation", "evidence_access", "compatibility", "dependency_change", "policy", "interpretation", "coverage"];
const dependencyResultStatuses = ["valid", "missing", "changed", "inaccessible", "incompatible", "not_evaluated"];
const assertionReasonCodes = [
  "DIRECT_HASH_LINKED_DERIVATION",
  "VERIFIED_TRANSITIVE_DERIVATION",
  "REQUIRED_PROVENANCE_UNKNOWN",
  "SOURCE_GENERATION_UNKNOWN",
  "REQUIRED_EVIDENCE_MISSING",
  "DEPENDENCY_DIGEST_MISMATCH",
  "DEPENDENCY_GENERATION_MISMATCH",
  "SOURCE_ANCHOR_STALE",
  "SCHEMA_INCOMPATIBLE",
  "POLICY_REFUSED",
  "AUTHORIZED_EVIDENCE_UNAVAILABLE",
  "ADVISORY_EVIDENCE_UNAVAILABLE",
  "ASSERTION_NOT_EVALUATED",
  "AUTHORSHIP_UNCERTAIN",
  "CHRONOLOGY_INCOMPLETE",
  "COMPLETENESS_INCOMPLETE",
];
const assertionBoundaryMeanings = {
  missing_provenance: "Required provenance basis or evidence identity is absent.",
  unknown_generation: "A required source or dependency generation is unknown.",
  evidence_access: "Evidence exists or is expected but cannot currently be accessed.",
  compatibility: "The observed schema or producer contract is incompatible.",
  dependency_change: "A dependency digest, generation, or source anchor changed.",
  policy: "A retention, authorization, or disclosure policy prohibits evaluation.",
  interpretation: "Evidence integrity is known but its asserted interpretation is limited.",
  coverage: "The evaluation does not cover every element required by the assertion scope.",
};
const assertionReasonMeanings = {
  DIRECT_HASH_LINKED_DERIVATION: "The asserted relationship is supported by direct retained records and matching digests.",
  VERIFIED_TRANSITIVE_DERIVATION: "A deterministic verified chain of assertions supports the relationship.",
  REQUIRED_PROVENANCE_UNKNOWN: "Required provenance basis or evidence identity is unknown.",
  SOURCE_GENERATION_UNKNOWN: "A required project, artifact, or transcript generation is unknown.",
  REQUIRED_EVIDENCE_MISSING: "Evidence required for evaluation is absent.",
  DEPENDENCY_DIGEST_MISMATCH: "A dependency digest differs from the asserted expectation.",
  DEPENDENCY_GENERATION_MISMATCH: "A dependency generation differs from the asserted expectation.",
  SOURCE_ANCHOR_STALE: "The source anchor no longer matches the evaluation target.",
  SCHEMA_INCOMPATIBLE: "The available schema cannot be evaluated under the required contract.",
  POLICY_REFUSED: "Policy explicitly prohibits producing the requested conclusion.",
  AUTHORIZED_EVIDENCE_UNAVAILABLE: "Required protected evidence cannot be evaluated with current authorization.",
  ADVISORY_EVIDENCE_UNAVAILABLE: "Advisory evidence is unavailable and the named confidence dimensions are degraded.",
  ASSERTION_NOT_EVALUATED: "No authoritative evaluation has completed for this assertion and target.",
  AUTHORSHIP_UNCERTAIN: "Evidence does not support an exact authorship interpretation.",
  CHRONOLOGY_INCOMPLETE: "Ordering or generation evidence is incomplete.",
  COMPLETENESS_INCOMPLETE: "The evaluation does not cover all evidence required by its asserted scope.",
};

const schemas = new Map();
function add(name, title, body, example, description) {
  const schema = { $schema: draft, $id: `${baseId}/${name}.schema.json`, title, description, ...body, examples: [example] };
  schemas.set(name, schema);
  return schema;
}

const actor = closed({ type: { enum: ["user", "assistant", "system"] }, actor_id: nullable(nonEmptyString) });
const provider = closed({ provider_id: nonEmptyString, model_id: nonEmptyString, processing_mode: { enum: ["local", "cloud"] } });
const fileEntry = closed({ path: repositoryPath, sha256: sha, size: { type: "integer", minimum: 0 } });
const finding = closed({ severity: { enum: ["INFO", "WARNING", "ERROR", "CRITICAL"] }, code: { type: "string", pattern: "^[A-Z][A-Z0-9_]+$" }, message: nonEmptyString, authoritative_evidence_affected: { type: "boolean" }, path: nullable(repositoryPath), event_id: nullable(identifier("event")) });

const textRangeExample = {
  schema_version: "1.0",
  document_revision_id: ids.revision,
  coordinate_system: "utf8_byte",
  start: 0,
  end: 12,
  preimage_sha256: digest("1"),
  text_fragment_id: ids.fragment,
};

add("text-fragment-reference", "Text fragment reference", closed({
  schema_version: schemaVersion,
  document_revision_id: identifier("revision"),
  coordinate_system: { enum: ["utf8_byte", "unicode_scalar", "utf16_code_unit", "editor_position"] },
  start: { type: "integer", minimum: 0 },
  end: { type: "integer", minimum: 0 },
  preimage_sha256: sha,
  text_fragment_id: nullable(identifier("fragment")),
}), textRangeExample, "Stable revision-bound text range with an explicit coordinate system.");

const contentReferenceExample = {
  schema_version: "1.0",
  record_id: ids.record,
  record_type: "manuscript_revision",
  sha256: digest("2"),
  path: "manuscript/revisions/revision.json",
  revision_id: ids.revision,
  range: textRangeExample,
};
add("content-reference", "Content reference", closed({
  schema_version: schemaVersion,
  record_id: typedIdentifier,
  record_type: { enum: ["conversation_turn", "invocation_request", "invocation_response", "invocation_failure", "idea_revision", "manuscript_revision", "edit_transaction", "prompt_template", "model_configuration", "provenance_assertion", "assertion_evaluation", "release_file", "source", "other"] },
  sha256: sha,
  path: nullable(repositoryPath),
  revision_id: nullable(identifier("revision")),
  range: nullable(ref("text-fragment-reference")),
}, ["schema_version", "record_id", "record_type", "sha256", "path", "revision_id", "range"]), contentReferenceExample, "Content-addressed link to an authoritative record or revision.");

const policyExample = {
  schema_version: "1.0", policy_id: ids.policy, project_id: ids.project, retention_mode: "minimal", encryption_mode: "none", default_export_profile: "sanitized", effective_at: timestampValue,
};
add("provenance-policy", "Provenance policy", closed({
  schema_version: schemaVersion, policy_id: identifier("policy"), project_id: identifier("project"), retention_mode: { enum: ["minimal", "full_private"] }, encryption_mode: { enum: ["none", "protected"] }, default_export_profile: { enum: ["full", "sanitized"] }, effective_at: timestamp,
}), policyExample, "Prospective project retention, protection, and export defaults.");

const projectExample = {
  schema_version: "1.0", application_version: "0.4.0", project_id: ids.project, title: "The Attention Commons", created_at: timestampValue, updated_at: laterTimestamp, current_phase: "ideation", publication_status: "working", provenance_policy_id: ids.policy, audio_retained: false,
};
add("project-manifest", "Project manifest", closed({
  schema_version: schemaVersion, application_version: { type: "string", pattern: "^\\d+\\.\\d+\\.\\d+$" }, project_id: identifier("project"), title: nonEmptyString, description: { type: "string" }, created_at: timestamp, updated_at: timestamp, current_phase: { enum: ["ideation", "drafting", "finalization"] }, publication_status: { enum: ["working", "finalized", "published"] }, provenance_policy_id: identifier("policy"), audio_retained: { const: false },
}, ["schema_version", "application_version", "project_id", "title", "created_at", "updated_at", "current_phase", "publication_status", "provenance_policy_id", "audio_retained"]), projectExample, "Canonical identity and lifecycle metadata for a 1.0 project.");

const writeIntentExample = {
  schema_version: "1.0", intent_id: ids.intent, client_action_id: "client-action-0001", project_id: ids.project, operation_type: "IDEA_ACCEPTED", phase: "PREPARED", event_id: null, event_sequence: null, staged_path: ".app/temp/intents/intent.json", command_sha256: digest("3"), result: null, created_at: timestampValue, updated_at: timestampValue,
};
add("write-intent", "Cross-store write intent", closed({
  schema_version: schemaVersion, intent_id: identifier("intent"), client_action_id: { type: "string", minLength: 8, maxLength: 200 }, project_id: identifier("project"), operation_type: { type: "string", pattern: "^[A-Z][A-Z0-9_]+$" }, phase: { enum: ["PREPARED", "RECORDS_DURABLE", "LEDGER_APPENDED", "CHAIN_HEAD_ADVANCED", "SQLITE_APPLIED", "COMPLETE", "QUARANTINED", "FAILED"] }, event_id: nullable(identifier("event")), event_sequence: nullable({ type: "integer", minimum: 1 }), staged_path: nullable(repositoryPath), command_sha256: sha, result: nullable(jsonValue), created_at: timestamp, updated_at: timestamp,
}), writeIntentExample, "Rebuildable SQLite coordination record for a cross-store mutation.");

const eventExample = {
  schema_version: "1.0", event_id: ids.event, event_sequence: 1, client_action_id: "client-action-0001", project_id: ids.project, timestamp: timestampValue, event_type: "PROJECT_CREATED", actor: { type: "user", actor_id: null }, provider: null, inputs: [], outputs: [contentReferenceExample], relationships: { parent_event_ids: [], invocation_id: null }, metadata: { summary: "Created project" }, previous_event_hash: null, event_hash: digest("4"),
};
add("provenance-event", "Provenance event", closed({
  schema_version: schemaVersion, event_id: identifier("event"), event_sequence: { type: "integer", minimum: 1 }, client_action_id: { type: "string", minLength: 8, maxLength: 200 }, project_id: identifier("project"), timestamp, event_type: { type: "string", pattern: "^[A-Z][A-Z0-9_]+$" }, actor, provider: nullable(provider), inputs: arrayOf(ref("content-reference")), outputs: arrayOf(ref("content-reference")), relationships: closed({ parent_event_ids: arrayOf(identifier("event"), { uniqueItems: true }), invocation_id: nullable(identifier("invocation")) }), metadata: { type: "object" }, previous_event_hash: nullable(sha), event_hash: sha,
}), eventExample, "Canonical chronological action with content references and SHA-256 chain linkage.");

const chainHeadExample = { schema_version: "1.0", project_id: ids.project, active_segment_number: 1, event_id: ids.event, event_sequence: 1, event_hash: digest("4"), updated_at: timestampValue };
add("chain-head", "Chain head", closed({ schema_version: schemaVersion, project_id: identifier("project"), active_segment_number: { type: "integer", minimum: 1 }, event_id: identifier("event"), event_sequence: { type: "integer", minimum: 1 }, event_hash: sha, updated_at: timestamp }), chainHeadExample, "Durable pointer to the last committed provenance event.");

const segmentExample = { schema_version: "1.0", project_id: ids.project, segment_number: 1, previous_segment_file_hash: null, first_event_hash: digest("4"), final_event_hash: digest("5"), first_event_sequence: 1, final_event_sequence: 2, event_count: 2, byte_length: 1024, segment_file_hash: digest("6"), sealed_at: timestampValue };
add("ledger-segment-manifest", "Ledger segment manifest", closed({ schema_version: schemaVersion, project_id: identifier("project"), segment_number: { type: "integer", minimum: 1 }, previous_segment_file_hash: nullable(sha), first_event_hash: sha, final_event_hash: sha, first_event_sequence: { type: "integer", minimum: 1 }, final_event_sequence: { type: "integer", minimum: 1 }, event_count: { type: "integer", minimum: 1 }, byte_length: { type: "integer", minimum: 1 }, segment_file_hash: sha, sealed_at: timestamp }), segmentExample, "Immutable seal for one ledger segment.");

const promptExample = { schema_version: "1.0", template_id: ids.template, version: 1, purpose: "draft_from_selected_ideas", canonical_template_body: "Use {{context}} to draft a passage.", variables: { context: { description: "Selected idea context", required: true } }, creation_application_version: "0.4.0", retention_classification: "project_record", template_sha256: digest("7"), created_at: timestampValue };
add("prompt-template", "Prompt template", closed({ schema_version: schemaVersion, template_id: identifier("template"), version: { type: "integer", minimum: 1 }, purpose: { type: "string", pattern: "^[a-z][a-z0-9_]+$" }, canonical_template_body: nonEmptyString, variables: mapOf(closed({ description: nonEmptyString, required: { type: "boolean" } })), creation_application_version: { type: "string", pattern: "^\\d+\\.\\d+\\.\\d+$" }, retention_classification: { enum: ["application_default", "project_record", "protected_project_record"] }, template_sha256: sha, created_at: timestamp }), promptExample, "Immutable versioned prompt template. Its digest identity excludes template_sha256.");

const promptRefExample = { schema_version: "1.0", template_id: ids.template, version: 1, path: "records/prompt-templates/drafting/template.json", template_sha256: digest("7") };
add("prompt-template-reference", "Prompt template reference", closed({ schema_version: schemaVersion, template_id: identifier("template"), version: { type: "integer", minimum: 1 }, path: repositoryPath, template_sha256: sha }), promptRefExample, "Reference to an immutable prompt-template version.");

const capabilityExample = { schema_version: "1.0", snapshot_id: ids.record, provider_id: "ollama", model_id: "llama3.2", capabilities: { streaming: true, structured_output: false, tools: false, vision: false }, observed_at: timestampValue };
add("model-capability-snapshot", "Model capability snapshot", closed({ schema_version: schemaVersion, snapshot_id: identifier("record"), provider_id: nonEmptyString, model_id: nonEmptyString, capabilities: closed({ streaming: { type: "boolean" }, structured_output: { type: "boolean" }, tools: { type: "boolean" }, vision: { type: "boolean" } }), observed_at: timestamp }), capabilityExample, "Immutable observed capability set for an invoked model.");

const modelConfigExample = { schema_version: "1.0", snapshot_id: ids.record, provider_id: "ollama", model_id: "llama3.2", processing_mode: "local", endpoint_classification: "loopback", temperature_milli: 700, context_limit: 8192, maximum_output: 800, structured_output_mode: "none", tool_configuration: [], capability_snapshot_sha256: digest("8"), created_at: timestampValue };
add("model-configuration-snapshot", "Model configuration snapshot", closed({ schema_version: schemaVersion, snapshot_id: identifier("record"), provider_id: nonEmptyString, model_id: nonEmptyString, processing_mode: { enum: ["local", "cloud"] }, endpoint_classification: { enum: ["loopback", "private_network", "public_cloud", "custom"] }, temperature_milli: nullable({ type: "integer", minimum: 0, maximum: 2000 }), context_limit: nullable({ type: "integer", minimum: 1 }), maximum_output: nullable({ type: "integer", minimum: 1 }), structured_output_mode: { enum: ["none", "json", "json_schema", "provider_native"] }, tool_configuration: arrayOf(nonEmptyString, { uniqueItems: true }), capability_snapshot_sha256: sha, created_at: timestamp }), modelConfigExample, "Effective credential-free model settings used by one invocation.");

const invocationStateExample = { schema_version: "1.0", invocation_id: ids.invocation, state: "REQUEST_RECORDED", state_sequence: 2, updated_at: timestampValue, failure_id: null };
add("invocation-state", "Invocation state", closed({ schema_version: schemaVersion, invocation_id: identifier("invocation"), state: { enum: ["REQUEST_PREPARED", "REQUEST_RECORDED", "MODEL_RUNNING", "RESPONSE_RECEIVED", "RESPONSE_RECORDED", "STAGED_PREVIEW", "FAILED", "CANCELLED", "ABANDONED"] }, state_sequence: { type: "integer", minimum: 1 }, updated_at: timestamp, failure_id: nullable(identifier("failure")) }), invocationStateExample, "Operational invocation lifecycle projection.");

const streamStateExample = { schema_version: "1.0", stream_id: ids.stream, invocation_id: ids.invocation, state: "STREAMING", bytes_received: 512, started_at: timestampValue, updated_at: laterTimestamp };
add("invocation-stream-state", "Invocation stream state", closed({ schema_version: schemaVersion, stream_id: identifier("stream"), invocation_id: identifier("invocation"), state: { enum: ["CREATED", "CONNECTING", "STREAMING", "COMPLETED", "FAILED", "CANCELLED", "ABANDONED", "RECOVERING"] }, bytes_received: { type: "integer", minimum: 0, maximum: 67108864 }, started_at: timestamp, updated_at: timestamp }), streamStateExample, "Non-authoritative operational streaming state.");

const streamSummaryExample = { schema_version: "1.0", stream_id: ids.stream, invocation_id: ids.invocation, terminal_state: "COMPLETED", bytes_received: 512, chunks_received: 4, duration_milliseconds: 2400, partial_output_preserved: false, completed_at: laterTimestamp };
add("invocation-stream-summary", "Invocation stream summary", closed({ schema_version: schemaVersion, stream_id: identifier("stream"), invocation_id: identifier("invocation"), terminal_state: { enum: ["COMPLETED", "FAILED", "CANCELLED", "ABANDONED"] }, bytes_received: { type: "integer", minimum: 0, maximum: 67108864 }, chunks_received: { type: "integer", minimum: 0 }, duration_milliseconds: { type: "integer", minimum: 0, maximum: 1800000 }, partial_output_preserved: { type: "boolean" }, completed_at: timestamp }), streamSummaryExample, "Bounded immutable summary of a completed or interrupted stream.");

const requestExample = { schema_version: "1.0", invocation_id: ids.invocation, project_id: ids.project, purpose: "draft_from_selected_ideas", initiated_by_event_id: ids.event, retention_mode: "full_private", provider: { provider_id: "ollama", model_id: "llama3.2", processing_mode: "local" }, model_configuration_sha256: digest("8"), prompt_template: promptRefExample, messages: [{ role: "user", content: "Use the selected ideas to draft a passage." }], context_references: [contentReferenceExample], manuscript_revision_id: ids.revision, idea_revision_ids: [ids.revision2], conversation_head: ids.turn, submitted_at: timestampValue, redactions: [{ category: "personal_identifier", replacement: "[redacted]" }] };
add("invocation-request", "Invocation request", closed({ schema_version: schemaVersion, invocation_id: identifier("invocation"), project_id: identifier("project"), purpose: { type: "string", pattern: "^[a-z][a-z0-9_]+$" }, initiated_by_event_id: identifier("event"), retention_mode: { enum: ["minimal", "full_private"] }, provider, model_configuration_sha256: sha, prompt_template: ref("prompt-template-reference"), messages: arrayOf(closed({ role: { enum: ["system", "user", "assistant", "tool"] }, content: nonEmptyString })), context_references: arrayOf(ref("content-reference")), manuscript_revision_id: nullable(identifier("revision")), idea_revision_ids: arrayOf(identifier("revision"), { uniqueItems: true }), conversation_head: nullable(identifier("turn")), submitted_at: timestamp, redactions: arrayOf(closed({ category: { enum: ["credential", "personal_identifier", "signed_url", "policy_omission", "other"] }, replacement: nonEmptyString })) }), requestExample, "Credential-free immutable provider request evidence; minimal mode may omit messages.");

const responseExample = { schema_version: "1.0", record_id: ids.record, invocation_id: ids.invocation, retention_mode: "full_private", content_type: "draft_preview", retained_text: "Attention is personal in experience and public in consequence.", finish_reason: "stop", usage: { input_tokens: 120, output_tokens: 14 }, provider_metadata: { request_id: "local-0001", load_duration_milliseconds: 120 }, validation: { schema_valid: true, repair_attempted: false }, content_sha256: digest("9"), created_at: laterTimestamp };
add("invocation-response", "Invocation response", closed({ schema_version: schemaVersion, record_id: identifier("record"), invocation_id: identifier("invocation"), retention_mode: { enum: ["minimal", "full_private"] }, content_type: { enum: ["conversation_reply", "idea_suggestion", "draft_preview", "editorial_preview", "structured_result", "other"] }, retained_text: nullable({ type: "string" }), finish_reason: nullable(nonEmptyString), usage: nullable(closed({ input_tokens: nullable({ type: "integer", minimum: 0 }), output_tokens: nullable({ type: "integer", minimum: 0 }) })), provider_metadata: nullable({ type: "object" }), validation: closed({ schema_valid: { type: "boolean" }, repair_attempted: { type: "boolean" } }), content_sha256: sha, created_at: timestamp }), responseExample, "Immutable response evidence filtered according to retention policy.");

const failureExample = { schema_version: "1.0", failure_id: ids.failure, invocation_id: ids.invocation, code: "PROVIDER_TIMEOUT", category: "provider", recoverable: true, bounded_summary: "The provider did not respond within the configured timeout.", partial_output_preserved: false, occurred_at: laterTimestamp };
add("invocation-failure", "Invocation failure", closed({ schema_version: schemaVersion, failure_id: identifier("failure"), invocation_id: identifier("invocation"), code: { type: "string", pattern: "^[A-Z][A-Z0-9_]+$" }, category: { enum: ["provider", "validation", "cancellation", "output_limit", "application", "privacy"] }, recoverable: { type: "boolean" }, bounded_summary: { type: "string", minLength: 1, maxLength: 2000 }, partial_output_preserved: { type: "boolean" }, occurred_at: timestamp }), failureExample, "Immutable bounded record of an invocation failure.");

const sessionExample = { schema_version: "1.0", session_id: ids.session, project_id: ids.project, title: "Opening discussion", mode: "typed", started_at: timestampValue, ended_at: laterTimestamp, turn_count: 2, audio_retained: false, retention_mode: "minimal" };
add("conversation-session", "Conversation session", closed({ schema_version: schemaVersion, session_id: identifier("session"), project_id: identifier("project"), title: { type: "string" }, mode: { enum: ["typed", "voice", "mixed"] }, started_at: timestamp, ended_at: nullable(timestamp), turn_count: { type: "integer", minimum: 0 }, audio_retained: { const: false }, retention_mode: { enum: ["minimal", "full_private"] } }), sessionExample, "Canonical conversation-session metadata without audio retention.");

const turnExample = { schema_version: "1.0", turn_id: ids.turn, session_id: ids.session, speaker: "user", input_mode: "typed", retained_text: "Attention shapes public life.", raw_transcript: null, started_at: timestampValue, ended_at: laterTimestamp, audio_retained: false, invocation_id: null, event_id: ids.event };
add("transcript-turn", "Transcript turn", closed({ schema_version: schemaVersion, turn_id: identifier("turn"), session_id: identifier("session"), speaker: { enum: ["user", "assistant"] }, input_mode: { enum: ["typed", "voice"] }, retained_text: nonEmptyString, raw_transcript: nullable({ type: "string" }), started_at: timestamp, ended_at: timestamp, audio_retained: { const: false }, invocation_id: nullable(identifier("invocation")), event_id: identifier("event") }), turnExample, "Immutable retained conversation turn permitted by project policy.");

const correctionExample = { schema_version: "1.0", correction_id: ids.correction, turn_id: ids.turn, revision_number: 1, corrected_text: "Attention shapes our public life.", reason: "user_correction", created_at: laterTimestamp, event_id: ids.event2 };
add("transcript-correction", "Transcript correction", closed({ schema_version: schemaVersion, correction_id: identifier("correction"), turn_id: identifier("turn"), revision_number: { type: "integer", minimum: 1 }, corrected_text: nonEmptyString, reason: { enum: ["user_correction", "transcription_error", "clarification", "other"] }, created_at: timestamp, event_id: identifier("event") }), correctionExample, "Immutable ordered correction to a retained transcript turn.");

const normalizationExample = { schema_version: "1.0", normalization_id: ids.normalization, turn_id: ids.turn, revision_number: 1, normalized_text: "Attention shapes public life.", method: "application", source_correction_id: ids.correction, created_at: laterTimestamp, event_id: ids.event2 };
add("transcript-normalization", "Transcript normalization", closed({ schema_version: schemaVersion, normalization_id: identifier("normalization"), turn_id: identifier("turn"), revision_number: { type: "integer", minimum: 1 }, normalized_text: nonEmptyString, method: { enum: ["application", "user_approved", "provider"] }, source_correction_id: nullable(identifier("correction")), created_at: timestamp, event_id: identifier("event") }), normalizationExample, "Immutable normalized representation derived from a transcript revision.");

const keyEnvelopeExample = { schema_version: "1.0", envelope_id: ids.record, key_id: ids.key, envelope_type: "device", algorithm: "xchacha20-poly1305", key_derivation: null, nonce: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", wrapped_key: "d3JhcHBlZC1wcm9qZWN0LWtleQ==", ciphertext_sha256: digest("a"), created_at: timestampValue };
add("encrypted-key-envelope", "Encrypted key envelope", closed({ schema_version: schemaVersion, envelope_id: identifier("record"), key_id: identifier("key"), envelope_type: { enum: ["device", "recovery"] }, algorithm: { enum: ["xchacha20-poly1305"] }, key_derivation: nullable(closed({ algorithm: { const: "argon2id" }, memory_kib: { type: "integer", minimum: 65536 }, iterations: { type: "integer", minimum: 1 }, parallelism: { type: "integer", minimum: 1 }, salt: { type: "string", minLength: 16 } })), nonce: { type: "string", minLength: 32 }, wrapped_key: { type: "string", minLength: 16 }, ciphertext_sha256: sha, created_at: timestamp }), keyEnvelopeExample, "Authenticated envelope wrapping a project data-encryption key.");

const recoveryEnvelopeExample = { ...keyEnvelopeExample, envelope_id: typedId("record", "S"), envelope_type: "recovery", key_derivation: { algorithm: "argon2id", memory_kib: 65536, iterations: 3, parallelism: 1, salt: "c2FsdC1mb3ItdGhpbmtsb29t" } };
add("recovery-key-envelope", "Recovery key envelope", closed({ ...schemas.get("encrypted-key-envelope").properties, envelope_type: { const: "recovery" }, key_derivation: closed({ algorithm: { const: "argon2id" }, memory_kib: { type: "integer", minimum: 65536 }, iterations: { type: "integer", minimum: 1 }, parallelism: { type: "integer", minimum: 1 }, salt: { type: "string", minLength: 16 } }) }), recoveryEnvelopeExample, "Portable recovery envelope for a protected project key.");

const keyManifestExample = { schema_version: "1.0", project_id: ids.project, key_id: ids.key, algorithm: "xchacha20-poly1305", device_envelope_path: "provenance/keys/device-envelope.json", recovery_envelope_path: "provenance/keys/recovery-envelope.json", recovery_verified_at: timestampValue, created_at: timestampValue };
add("project-key-manifest", "Project key manifest", closed({ schema_version: schemaVersion, project_id: identifier("project"), key_id: identifier("key"), algorithm: { const: "xchacha20-poly1305" }, device_envelope_path: repositoryPath, recovery_envelope_path: repositoryPath, recovery_verified_at: timestamp, created_at: timestamp }), keyManifestExample, "Stable key hierarchy references for protected project portability.");

const plaintextIdentity = { schema_version: "1.0", record_id: ids.record, record_type: "invocation_request", payload: { purpose: "draft_from_selected_ideas" } };
const recordEnvelopeExample = { schema_version: "1.0", project_id: ids.project, record_id: ids.record, record_type: "invocation_request", protection: "protected", plaintext_sha256: sha256(plaintextIdentity), payload: null, encryption: { algorithm: "xchacha20-poly1305", key_id: ids.key, nonce: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", ciphertext: "ZW5jcnlwdGVkLXJlY29yZA==", ciphertext_sha256: digest("b") } };
const recordEnvelopeProperties = {
  schema_version: schemaVersion,
  project_id: identifier("project"),
  record_id: identifier("record"),
  record_type: { type: "string", pattern: "^[a-z][a-z0-9_]+$" },
  protection: { enum: ["none", "protected"] },
  plaintext_sha256: sha,
  payload: nullable(jsonValue),
  encryption: nullable(closed({
    algorithm: { const: "xchacha20-poly1305" },
    key_id: identifier("key"),
    nonce: { type: "string", minLength: 32 },
    ciphertext: { type: "string", minLength: 16 },
    ciphertext_sha256: sha,
  })),
};
const recordEnvelopeRules = {
  allOf: [
    {
      if: { properties: { protection: { const: "none" } }, required: ["protection"] },
      then: {
        properties: {
          payload: { not: { type: "null" } },
          encryption: { type: "null" },
        },
      },
    },
    {
      if: { properties: { protection: { const: "protected" } }, required: ["protection"] },
      then: {
        properties: {
          payload: { type: "null" },
          encryption: { not: { type: "null" } },
        },
      },
    },
  ],
};
add("record-envelope", "Authoritative record envelope", closed(recordEnvelopeProperties, undefined, recordEnvelopeRules), recordEnvelopeExample, "Stable logical record identity with plaintext or authenticated protected payload.");

const ideaExample = { schema_version: "1.0", idea_id: ids.idea, project_id: ids.project, current_revision_id: ids.revision, status: "accepted", created_at: timestampValue, updated_at: laterTimestamp };
add("idea", "Idea identity", closed({ schema_version: schemaVersion, idea_id: identifier("idea"), project_id: identifier("project"), current_revision_id: identifier("revision"), status: { enum: ["suggested", "accepted", "rejected", "archived"] }, created_at: timestamp, updated_at: timestamp }), ideaExample, "Stable idea identity and current derived status.");

const ideaRevisionExample = { schema_version: "1.0", revision_id: ids.revision, idea_id: ids.idea, revision_number: 1, title: "Attention as civic infrastructure", summary: "Institutions influence shared attention.", detail: "Treat attention as a public condition.", tags: ["thesis"], source_turn_ids: [ids.turn], parent_idea_ids: [], created_by: "mixed", created_at: timestampValue, content_sha256: digest("c") };
add("idea-revision", "Idea revision", closed({ schema_version: schemaVersion, revision_id: identifier("revision"), idea_id: identifier("idea"), revision_number: { type: "integer", minimum: 1 }, title: nonEmptyString, summary: nonEmptyString, detail: { type: "string" }, tags: arrayOf(nonEmptyString, { uniqueItems: true }), source_turn_ids: arrayOf(identifier("turn"), { uniqueItems: true }), parent_idea_ids: arrayOf(identifier("idea"), { uniqueItems: true }), created_by: { enum: ["user", "assistant", "mixed"] }, created_at: timestamp, content_sha256: sha }), ideaRevisionExample, "Immutable content revision for one idea.");

const manuscriptRevisionExample = { schema_version: "1.0", revision_id: ids.revision, project_id: ids.project, parent_revision_id: null, canonical_markdown_path: "manuscript/revisions/revision.md", canonical_markdown_sha256: digest("d"), word_count: 42, created_by: "user", created_at: timestampValue };
add("manuscript-revision", "Manuscript revision", closed({ schema_version: schemaVersion, revision_id: identifier("revision"), project_id: identifier("project"), parent_revision_id: nullable(identifier("revision")), canonical_markdown_path: repositoryPath, canonical_markdown_sha256: sha, word_count: { type: "integer", minimum: 0 }, created_by: { enum: ["user", "assistant_acceptance", "system_restore"] }, created_at: timestamp }), manuscriptRevisionExample, "Immutable canonical manuscript revision.");

const operation = closed({ operation_type: { enum: ["insert", "delete", "replace", "move", "format"] }, source_range: nullable(ref("text-fragment-reference")), destination_position: nullable({ type: "integer", minimum: 0 }), inserted_text_sha256: nullable(sha), result_fragment_id: nullable(identifier("fragment")) });
const editExample = { schema_version: "1.0", transaction_id: ids.transaction, project_id: ids.project, base_revision_id: ids.revision, result_revision_id: ids.revision2, started_at: timestampValue, ended_at: laterTimestamp, operations: [{ operation_type: "insert", source_range: null, destination_position: 0, inserted_text_sha256: digest("e"), result_fragment_id: ids.fragment }], preimage_sha256: digest("d"), result_sha256: digest("e") };
add("edit-transaction", "Manuscript edit transaction", closed({ schema_version: schemaVersion, transaction_id: identifier("transaction"), project_id: identifier("project"), base_revision_id: identifier("revision"), result_revision_id: identifier("revision"), started_at: timestamp, ended_at: timestamp, operations: arrayOf(operation, { minItems: 1 }), preimage_sha256: sha, result_sha256: sha }), editExample, "Meaningful coalesced manual or acceptance-driven edit transaction.");
const dispositionExample = { schema_version: "1.0", disposition_id: ids.disposition, invocation_id: ids.invocation, revision_number: 1, status: "partially_accepted", accepted_ranges: [textRangeExample], rejected_ranges: [], destination: { revision_id: ids.revision2, path: "manuscript/revisions/revision.md", operation: "insert", position: 0 }, stale_context_resolution: "revalidated", decided_at: laterTimestamp, event_id: ids.event2 };
add("disposition-revision", "Invocation disposition revision", closed({ schema_version: schemaVersion, disposition_id: identifier("disposition"), invocation_id: identifier("invocation"), revision_number: { type: "integer", minimum: 1 }, status: { enum: ["staged", "accepted", "partially_accepted", "rejected", "discarded"] }, accepted_ranges: arrayOf(ref("text-fragment-reference")), rejected_ranges: arrayOf(ref("text-fragment-reference")), destination: nullable(closed({ revision_id: identifier("revision"), path: repositoryPath, operation: { enum: ["insert", "replace", "append", "new_section"] }, position: nullable({ type: "integer", minimum: 0 }) })), stale_context_resolution: { enum: ["not_stale", "revalidated", "rebased", "explicitly_confirmed", "rejected"] }, decided_at: timestamp, event_id: identifier("event") }), dispositionExample, "Immutable ordered record of user action on one model result.");

const indexManifestExample = { schema_version: "1.0", index_type: "contribution_graph", source_chain_head: digest("f"), source_event_count: 42, generator: { name: "thinkloom-indexer", version: "1.0.0", configuration_sha256: digest("0") }, content_sha256: digest("1"), generated_at: timestampValue };
add("derived-index-manifest", "Derived index manifest", closed({ schema_version: schemaVersion, index_type: { enum: ["content_index", "contribution_graph", "manuscript_lineage", "checkpoint_index"] }, source_chain_head: sha, source_event_count: { type: "integer", minimum: 0 }, generator: closed({ name: nonEmptyString, version: { type: "string", pattern: "^\\d+\\.\\d+\\.\\d+$" }, configuration_sha256: sha }), content_sha256: sha, generated_at: timestamp }), indexManifestExample, "Non-authoritative reproducibility metadata for a derived index.");

const generationCoordinates = closed({
  project_epoch: { type: "integer", minimum: 1 },
  artifact_revision: nullable({ type: "integer", minimum: 1 }),
  transcript_revision: nullable({ type: "integer", minimum: 1 }),
});
const assertionEntityReference = closed({
  entity_type: { enum: ["project", "artifact_revision", "manuscript_revision", "idea_revision", "transcript_revision", "invocation_request", "invocation_output", "prompt_template", "model_configuration", "release", "assertion", "other"] },
  entity_id: typedIdentifier,
  content_sha256: sha,
});
const assertionProducer = closed({
  subsystem: { type: "string", pattern: "^[a-z][a-z0-9_]+$" },
  application_version: { type: "string", pattern: "^\\d+\\.\\d+\\.\\d+$" },
});
const assertionEvidence = closed({
  evidence_id: typedIdentifier,
  evidence_type: { type: "string", pattern: "^[a-z][a-z0-9_]+$" },
  content_sha256: sha,
});
const assertionDependency = closed({
  dependency_id: typedIdentifier,
  dependency_type: { type: "string", pattern: "^[a-z][a-z0-9_]+$" },
  expected_sha256: sha,
  expected_generation: generationCoordinates,
  evidence_class: { enum: evidenceClasses },
  role: { enum: ["source_anchor", "content_basis", "model_configuration", "prompt_template", "authorship_basis", "chronology_basis", "compatibility_basis", "other"] },
  confidence_dimensions: arrayOf({ enum: confidenceDimensions }, { minItems: 1, uniqueItems: true }),
});
const assertionIdentity = {
  schema_version: "1.0",
  assertion_id: ids.assertion,
  project_id: ids.project,
  subject: { entity_type: "artifact_revision", entity_id: ids.revision2, content_sha256: digest("e") },
  predicate: "derived_from",
  object: { entity_type: "invocation_output", entity_id: ids.record, content_sha256: digest("9") },
  source_anchor: { event_id: ids.event2, event_sequence: 2, event_hash: digest("5") },
  source_generation: { project_epoch: 1, artifact_revision: 12, transcript_revision: 4 },
  lifecycle_phase: "accepted_into_work",
  producer: { subsystem: "provenance_writer", application_version: "0.4.0" },
  provenance: { basis: "direct_record", evidence: [{ evidence_id: ids.event2, evidence_type: "provenance_event", content_sha256: digest("5") }, { evidence_id: ids.record, evidence_type: "invocation_response", content_sha256: digest("9") }] },
  dependencies: [
    { dependency_id: ids.event2, dependency_type: "provenance_event", expected_sha256: digest("5"), expected_generation: { project_epoch: 1, artifact_revision: 12, transcript_revision: 4 }, evidence_class: "mandatory_retained", role: "source_anchor", confidence_dimensions: ["integrity", "chronology", "derivation"] },
    { dependency_id: ids.record, dependency_type: "model_configuration", expected_sha256: digest("8"), expected_generation: { project_epoch: 1, artifact_revision: 12, transcript_revision: 4 }, evidence_class: "mandatory_live", role: "model_configuration", confidence_dimensions: ["identity", "derivation"] },
    { dependency_id: ids.turn, dependency_type: "transcript_turn", expected_sha256: digest("7"), expected_generation: { project_epoch: 1, artifact_revision: 12, transcript_revision: 4 }, evidence_class: "advisory", role: "authorship_basis", confidence_dimensions: ["authorship", "completeness"] },
    { dependency_id: ids.idea, dependency_type: "shadow_comparison", expected_sha256: digest("2"), expected_generation: { project_epoch: 1, artifact_revision: 12, transcript_revision: 4 }, evidence_class: "shadow", role: "other", confidence_dimensions: ["completeness"] },
  ],
  reason_code: "DIRECT_HASH_LINKED_DERIVATION",
  created_at: laterTimestamp,
};
const assertionExample = { ...assertionIdentity, assertion_sha256: sha256(assertionIdentity) };
const assertionRules = {
  allOf: [
    {
      if: { properties: { subject: { type: "object", properties: { entity_type: { const: "artifact_revision" } }, required: ["entity_type"] } }, required: ["subject"] },
      then: { properties: { source_generation: { type: "object", properties: { artifact_revision: { type: "integer", minimum: 1 } }, required: ["artifact_revision"] } } },
    },
    {
      if: { properties: { subject: { type: "object", properties: { entity_type: { const: "transcript_revision" } }, required: ["entity_type"] } }, required: ["subject"] },
      then: { properties: { source_generation: { type: "object", properties: { transcript_revision: { type: "integer", minimum: 1 } }, required: ["transcript_revision"] } } },
    },
  ],
};
add("provenance-assertion", "Canonical provenance assertion", closed({
  schema_version: schemaVersion,
  assertion_id: identifier("assertion"),
  project_id: identifier("project"),
  subject: assertionEntityReference,
  predicate: { type: "string", pattern: "^[a-z][a-z0-9_]+$" },
  object: assertionEntityReference,
  source_anchor: closed({ event_id: identifier("event"), event_sequence: { type: "integer", minimum: 1 }, event_hash: sha }),
  source_generation: generationCoordinates,
  lifecycle_phase: { enum: assertionLifecyclePhases },
  producer: assertionProducer,
  provenance: closed({ basis: { enum: ["direct_record", "deterministic_derivation", "declared_relationship", "verifier_observation"] }, evidence: arrayOf(assertionEvidence, { minItems: 1 }) }),
  dependencies: arrayOf(assertionDependency, { minItems: 1 }),
  reason_code: { enum: ["DIRECT_HASH_LINKED_DERIVATION", "VERIFIED_TRANSITIVE_DERIVATION"] },
  created_at: timestamp,
  assertion_sha256: sha,
}, undefined, assertionRules), assertionExample, "Immutable machine-evaluable subject-predicate-object assertion anchored to prior authoritative evidence. Its digest identity excludes only assertion_sha256.");

const confidenceAssessment = closed(Object.fromEntries(confidenceDimensions.map((dimension) => [dimension, { enum: confidenceValues }])));
const assertionBoundary = closed({
  kind: { enum: assertionBoundaryKinds },
  affected_dimensions: arrayOf({ enum: confidenceDimensions }, { minItems: 1, uniqueItems: true }),
  dependency_ids: arrayOf(typedIdentifier, { uniqueItems: true }),
  compatibility: nullable(closed({ required_schema: nonEmptyString, observed_schema: nullable(nonEmptyString) })),
});
const dependencyResult = closed({
  dependency_id: typedIdentifier,
  evidence_class: { enum: evidenceClasses },
  status: { enum: dependencyResultStatuses },
  observed_sha256: nullable(sha),
  observed_generation: nullable(generationCoordinates),
});
const evaluationExample = {
  schema_version: "1.0",
  evaluation_id: ids.evaluation,
  assertion_id: ids.assertion,
  assertion_sha256: assertionExample.assertion_sha256,
  project_id: ids.project,
  evaluated_against: { chain_head: digest("6"), event_sequence: 3, project_epoch: 1, schema_catalog_sha256: digest("7") },
  evaluator: { subsystem: "native_verifier", application_version: "0.4.0" },
  status: "exact",
  confidence: { integrity: "exact", identity: "exact", chronology: "exact", derivation: "exact", authorship: "not_applicable", completeness: "exact" },
  boundary: null,
  dependency_results: [
    { dependency_id: ids.event2, evidence_class: "mandatory_retained", status: "valid", observed_sha256: digest("5"), observed_generation: { project_epoch: 1, artifact_revision: 12, transcript_revision: 4 } },
    { dependency_id: ids.record, evidence_class: "mandatory_live", status: "valid", observed_sha256: digest("8"), observed_generation: { project_epoch: 1, artifact_revision: 12, transcript_revision: 4 } },
    { dependency_id: ids.turn, evidence_class: "advisory", status: "valid", observed_sha256: digest("7"), observed_generation: { project_epoch: 1, artifact_revision: 12, transcript_revision: 4 } },
    { dependency_id: ids.idea, evidence_class: "shadow", status: "not_evaluated", observed_sha256: null, observed_generation: null },
  ],
  reason_code: "DIRECT_HASH_LINKED_DERIVATION",
  supersedes_evaluation_id: null,
  evaluated_at: "2026-07-17T18:44:12.789Z",
};
const exactConfidenceProperties = Object.fromEntries(confidenceDimensions.map((dimension) => [dimension, { enum: ["exact", "not_applicable"] }]));
const evaluationRules = {
  allOf: [
    {
      if: { properties: { status: { const: "exact" } }, required: ["status"] },
      then: { properties: {
        boundary: { type: "null" },
        reason_code: { enum: ["DIRECT_HASH_LINKED_DERIVATION", "VERIFIED_TRANSITIVE_DERIVATION"] },
        confidence: { type: "object", properties: exactConfidenceProperties, required: confidenceDimensions },
        dependency_results: { type: "array", items: { type: "object", if: { properties: { evidence_class: { const: "shadow" } }, required: ["evidence_class"] }, else: { properties: { status: { const: "valid" } }, required: ["status"] } } },
      } },
    },
    {
      if: { properties: { status: { not: { const: "exact" } } }, required: ["status"] },
      then: { properties: { boundary: { not: { type: "null" } } } },
    },
    {
      if: { properties: { status: { const: "degraded" } }, required: ["status"] },
      then: { properties: {
        reason_code: { enum: ["ADVISORY_EVIDENCE_UNAVAILABLE", "AUTHORSHIP_UNCERTAIN", "CHRONOLOGY_INCOMPLETE", "COMPLETENESS_INCOMPLETE"] },
        confidence: { type: "object", anyOf: confidenceDimensions.map((dimension) => ({ properties: { [dimension]: { const: "degraded" } }, required: [dimension] })) },
        dependency_results: { type: "array", not: { contains: { type: "object", properties: { evidence_class: { enum: ["mandatory_live", "mandatory_retained"] }, status: { not: { const: "valid" } } }, required: ["evidence_class", "status"] } } },
      } },
    },
    {
      if: { properties: { status: { const: "refused" } }, required: ["status"] },
      then: { properties: { reason_code: { enum: ["SCHEMA_INCOMPATIBLE", "POLICY_REFUSED", "AUTHORIZED_EVIDENCE_UNAVAILABLE"] } } },
    },
    {
      if: { properties: { status: { const: "stale" } }, required: ["status"] },
      then: { properties: {
        reason_code: { enum: ["DEPENDENCY_DIGEST_MISMATCH", "DEPENDENCY_GENERATION_MISMATCH", "SOURCE_ANCHOR_STALE"] },
        dependency_results: { type: "array", contains: { type: "object", properties: { status: { const: "changed" } }, required: ["status"] }, minContains: 1 },
      } },
    },
    {
      if: { properties: { status: { const: "unverified" } }, required: ["status"] },
      then: { properties: { reason_code: { enum: ["REQUIRED_PROVENANCE_UNKNOWN", "SOURCE_GENERATION_UNKNOWN", "REQUIRED_EVIDENCE_MISSING", "AUTHORIZED_EVIDENCE_UNAVAILABLE", "ASSERTION_NOT_EVALUATED", "CHRONOLOGY_INCOMPLETE", "COMPLETENESS_INCOMPLETE"] } } },
    },
  ],
};
add("assertion-evaluation", "Point-in-time assertion evaluation", closed({
  schema_version: schemaVersion,
  evaluation_id: identifier("evaluation"),
  assertion_id: identifier("assertion"),
  assertion_sha256: sha,
  project_id: identifier("project"),
  evaluated_against: closed({ chain_head: sha, event_sequence: { type: "integer", minimum: 1 }, project_epoch: { type: "integer", minimum: 1 }, schema_catalog_sha256: sha }),
  evaluator: assertionProducer,
  status: { enum: assertionEvaluationStatuses },
  confidence: confidenceAssessment,
  boundary: nullable(assertionBoundary),
  dependency_results: arrayOf(dependencyResult, { minItems: 1 }),
  reason_code: { enum: assertionReasonCodes },
  supersedes_evaluation_id: nullable(identifier("evaluation")),
  evaluated_at: timestamp,
}, undefined, evaluationRules), evaluationExample, "Immutable verifier conclusion about one assertion at an explicit chain head and project epoch.");
const verificationExample = { schema_version: "1.0", verification_id: ids.record, project_id: ids.project, status: "VERIFIED_WITH_WARNINGS", verified_through_event_id: ids.event, verified_chain_head: digest("4"), checked_segments: 1, checked_events: 1, checked_records: 4, findings: [{ severity: "WARNING", code: "INDEX_STALE", message: "Contribution index is stale.", authoritative_evidence_affected: false, path: "provenance/indexes/contribution-graph.json", event_id: null }], verifier_version: "1.0.0", completed_at: laterTimestamp };
add("verification-report", "Verification report", closed({ schema_version: schemaVersion, verification_id: identifier("record"), project_id: identifier("project"), status: { enum: ["VERIFIED", "VERIFIED_WITH_WARNINGS", "INCOMPLETE", "FAILED", "UNSAFE"] }, verified_through_event_id: nullable(identifier("event")), verified_chain_head: nullable(sha), checked_segments: { type: "integer", minimum: 0 }, checked_events: { type: "integer", minimum: 0 }, checked_records: { type: "integer", minimum: 0 }, findings: arrayOf(finding), verifier_version: { type: "string", pattern: "^\\d+\\.\\d+\\.\\d+$" }, completed_at: timestamp }), verificationExample, "Authoritative native verification result rendered by the UI.");

const backupExample = { schema_version: "1.0", application_version: "0.4.0", project_id: ids.project, created_at: timestampValue, source_chain_head: digest("4"), sqlite_snapshot: { path: "database-snapshot/state.sqlite", sha256: digest("2"), size: 4096, sqlite_version: "3.46.0", database_schema_version: 1, completed_at: timestampValue }, files: [{ path: "project.json", sha256: digest("3"), size: 256 }], protected: false, recovery_key_envelope_path: null };
add("backup-manifest", "Project backup manifest", closed({ schema_version: schemaVersion, application_version: { type: "string", pattern: "^\\d+\\.\\d+\\.\\d+$" }, project_id: identifier("project"), created_at: timestamp, source_chain_head: sha, sqlite_snapshot: closed({ path: repositoryPath, sha256: sha, size: { type: "integer", minimum: 1 }, sqlite_version: nonEmptyString, database_schema_version: { type: "integer", minimum: 1 }, completed_at: timestamp }), files: arrayOf(fileEntry, { minItems: 1 }), protected: { type: "boolean" }, recovery_key_envelope_path: nullable(repositoryPath) }), backupExample, "Complete staged-backup inventory including a verified online SQLite snapshot.");

const releaseStateExample = { schema_version: "1.0", release_id: ids.release, project_id: ids.project, state: "RELEASE_STAGED", state_sequence: 5, source_commit: "0123456789abcdef0123456789abcdef01234567", source_chain_head: digest("4"), updated_at: laterTimestamp, failure: null };
add("release-state", "Release state", closed({ schema_version: schemaVersion, release_id: identifier("release"), project_id: identifier("project"), state: { enum: ["WORKING", "FREEZING_SOURCE", "SOURCE_FROZEN", "GENERATING_RELEASE", "RELEASE_STAGED", "RELEASE_VERIFIED", "RELEASE_COMMITTED", "RELEASE_TAGGED", "COMPLETE", "RECOVERABLE", "FAILED"] }, state_sequence: { type: "integer", minimum: 1 }, source_commit: nullable({ type: "string", pattern: "^[a-f0-9]{40,64}$" }), source_chain_head: nullable(sha), updated_at: timestamp, failure: nullable(nonEmptyString) }), releaseStateExample, "Durable idempotent release-finalization state.");

const releaseFiles = [{ path: "manuscript/final-manuscript.md", sha256: digest("4"), size: 1024 }, { path: "evidence/creative-process-report.md", sha256: digest("5"), size: 512 }];
const releaseExample = { schema_version: "1.0", application_version: "0.4.0", release_id: ids.release, project_id: ids.project, version: "1.0.0", created_at: laterTimestamp, source_commit: "0123456789abcdef0123456789abcdef01234567", source_chain_head: digest("4"), source_manuscript: { path: "manuscript/final-manuscript.md", sha256: digest("4") }, sanitized: false, files: releaseFiles, release_files_merkle_root: releaseMerkleRoot(releaseFiles), merkle_algorithm: "thinkloom-release-merkle-v1" };
add("release-manifest", "Release manifest", closed({ schema_version: schemaVersion, application_version: { type: "string", pattern: "^\\d+\\.\\d+\\.\\d+$" }, release_id: identifier("release"), project_id: identifier("project"), version: { type: "string", pattern: "^\\d+\\.\\d+\\.\\d+$" }, created_at: timestamp, source_commit: { type: "string", pattern: "^[a-f0-9]{40,64}$" }, source_chain_head: sha, source_manuscript: closed({ path: repositoryPath, sha256: sha }), sanitized: { type: "boolean" }, files: arrayOf(fileEntry, { minItems: 1 }), release_files_merkle_root: sha, merkle_algorithm: { const: "thinkloom-release-merkle-v1" } }), releaseExample, "Non-self-referential binding of a frozen source to verified release files.");

const sanitizedExample = { schema_version: "1.0", export_id: ids.record, project_id: ids.project, source_chain_head: digest("4"), profile: "sanitized", omission_rules: [{ category: "private_conversation", action: "exclude", count: 2 }], rules_sha256: digest("6"), files: [{ path: "final-manuscript.md", sha256: digest("4"), size: 1024 }], created_at: laterTimestamp };
add("sanitized-export-manifest", "Sanitized export manifest", closed({ schema_version: schemaVersion, export_id: identifier("record"), project_id: identifier("project"), source_chain_head: sha, profile: { const: "sanitized" }, omission_rules: arrayOf(closed({ category: { enum: ["private_conversation", "rejected_output", "provider_detail", "personal_identifier", "internal_path", "source_body", "other"] }, action: { enum: ["exclude", "redact", "summarize"] }, count: { type: "integer", minimum: 0 } })), rules_sha256: sha, files: arrayOf(fileEntry, { minItems: 1 }), created_at: timestamp }), sanitizedExample, "Disclosure and file inventory for a non-mutating sanitized evidence export.");

const purgeExample = { schema_version: "1.0", purge_id: ids.purge, project_id: ids.project, requested_at: timestampValue, completed_at: laterTimestamp, reason: "Remove accidentally retained credential", superseded_chain_head: digest("4"), new_chain_root: digest("5"), affected_record_ids: [ids.record], affected_paths: ["records/invocations/request.json"], git_history_rewritten: true, prior_copies_revocable: false, confirmation_phrase_sha256: digest("6") };
add("purge-manifest", "Emergency purge manifest", closed({ schema_version: schemaVersion, purge_id: identifier("purge"), project_id: identifier("project"), requested_at: timestamp, completed_at: timestamp, reason: nonEmptyString, superseded_chain_head: sha, new_chain_root: sha, affected_record_ids: arrayOf(identifier("record"), { minItems: 1, uniqueItems: true }), affected_paths: arrayOf(repositoryPath, { uniqueItems: true }), git_history_rewritten: { const: true }, prior_copies_revocable: { const: false }, confirmation_phrase_sha256: sha }), purgeExample, "Explicit disclosure of a destructive provenance and Git reconstitution.");
const schemaNames = [...schemas.keys()].sort();

function clone(value) {
  return structuredClone(value);
}

function valueAt(instance, valuePath) {
  return valuePath.reduce((value, segment) => value?.[segment], instance);
}

function invalidPatternValue(pattern) {
  if (pattern.includes("sha256")) return "sha256:INVALID";
  if (pattern.includes("\\d{4}")) return "not-a-timestamp";
  if (pattern.includes("A-HJKMNP")) return "invalid-id";
  if (pattern.includes("A-Za-z]:")) return "../outside-project";
  return " invalid!";
}

function resolvedNode(node) {
  if (!node?.$ref) return node;
  const match = /\/([^/]+)\.schema\.json$/.exec(node.$ref);
  return match ? schemas.get(match[1]) ?? node : node;
}

function invalidCases(name, schema, example) {
  const cases = [];
  const labels = new Set();
  const push = (description, mutation) => {
    if (labels.has(description)) return;
    const instance = clone(example);
    mutation(instance);
    labels.add(description);
    cases.push({ description, instance });
  };

  const walk = (rawNode, instancePath, labelPath, seenRefs = new Set()) => {
    let node = rawNode;
    if (node?.$ref) {
      if (seenRefs.has(node.$ref)) return;
      seenRefs = new Set(seenRefs).add(node.$ref);
      node = resolvedNode(node);
    }
    const sample = valueAt(example, instancePath);
    if (!node || sample === undefined) return;

    if (Array.isArray(node.anyOf)) {
      const candidate = node.anyOf.find((entry) => sample === null ? entry.type === "null" : entry.type !== "null");
      if (candidate) walk(candidate, instancePath, labelPath, seenRefs);
      return;
    }

    if (Array.isArray(node.enum) && node.enum.length) {
      push(`${labelPath}: reject value outside enum`, (instance) => {
        const parent = valueAt(instance, instancePath.slice(0, -1));
        parent[instancePath.at(-1)] = "__INVALID_ENUM__";
      });
    }
    if (Object.hasOwn(node, "const")) {
      push(`${labelPath}: reject value different from const`, (instance) => {
        const parent = valueAt(instance, instancePath.slice(0, -1));
        parent[instancePath.at(-1)] = typeof node.const === "boolean" ? !node.const : "__INVALID_CONST__";
      });
    }
    if (node.pattern && typeof sample === "string") {
      push(`${labelPath}: reject pattern mismatch`, (instance) => {
        const parent = valueAt(instance, instancePath.slice(0, -1));
        parent[instancePath.at(-1)] = invalidPatternValue(node.pattern);
      });
    }
    if (Number.isFinite(node.minimum) && typeof sample === "number") {
      push(`${labelPath}: reject value below minimum`, (instance) => {
        const parent = valueAt(instance, instancePath.slice(0, -1));
        parent[instancePath.at(-1)] = node.minimum - 1;
      });
    }
    if (Number.isFinite(node.maximum) && typeof sample === "number") {
      push(`${labelPath}: reject value above maximum`, (instance) => {
        const parent = valueAt(instance, instancePath.slice(0, -1));
        parent[instancePath.at(-1)] = node.maximum + 1;
      });
    }
    if (Number.isFinite(node.minLength) && node.minLength > 0 && typeof sample === "string") {
      push(`${labelPath}: reject string below minLength`, (instance) => {
        const parent = valueAt(instance, instancePath.slice(0, -1));
        parent[instancePath.at(-1)] = "";
      });
    }
    if (Number.isFinite(node.maxLength) && typeof sample === "string") {
      push(`${labelPath}: reject string above maxLength`, (instance) => {
        const parent = valueAt(instance, instancePath.slice(0, -1));
        parent[instancePath.at(-1)] = "x".repeat(node.maxLength + 1);
      });
    }
    if (Number.isFinite(node.minItems) && node.minItems > 0 && Array.isArray(sample)) {
      push(`${labelPath}: reject array below minItems`, (instance) => {
        const parent = valueAt(instance, instancePath.slice(0, -1));
        parent[instancePath.at(-1)] = [];
      });
    }

    if (node.type === "object" && sample && typeof sample === "object" && !Array.isArray(sample)) {
      for (const requiredName of node.required ?? []) {
        if (!Object.hasOwn(sample, requiredName)) continue;
        push(`${labelPath}.${requiredName}: reject missing required property`, (instance) => {
          delete valueAt(instance, instancePath)[requiredName];
        });
      }
      if (node.additionalProperties === false) {
        push(`${labelPath}: reject unexpected property`, (instance) => {
          valueAt(instance, instancePath).__unexpected = true;
        });
      }
      for (const [propertyName, propertySchema] of Object.entries(node.properties ?? {})) {
        if (Object.hasOwn(sample, propertyName)) walk(propertySchema, [...instancePath, propertyName], `${labelPath}.${propertyName}`, seenRefs);
      }
    }

    if (node.type === "array" && Array.isArray(sample) && sample.length && node.items) {
      walk(node.items, [...instancePath, 0], `${labelPath}[0]`, seenRefs);
    }
  };

  walk(schema, [], name);
  if (name === "record-envelope") {
    push("record-envelope: protected payload must not remain plaintext", (instance) => {
      instance.payload = { leaked: true };
    });
    push("record-envelope: unprotected payload requires null encryption", (instance) => {
      instance.protection = "none";
      instance.payload = { retained: true };
    });
  }
  if (name === "recovery-key-envelope") {
    push("recovery-key-envelope: recovery derivation is mandatory", (instance) => {
      instance.key_derivation = null;
    });
  }
  if (name === "provenance-assertion") {
    push("provenance-assertion: artifact generation must be explicit", (instance) => {
      instance.source_generation.artifact_revision = null;
    });

  }
  if (name === "assertion-evaluation") {
    push("assertion-evaluation: unknown confidence cannot be exact", (instance) => {
      instance.confidence.derivation = "unverified";
    });
    push("assertion-evaluation: exact cannot carry an uncertainty boundary", (instance) => {
      instance.boundary = { kind: "coverage", affected_dimensions: ["completeness"], dependency_ids: [], compatibility: null };
    });
    push("assertion-evaluation: exact requires every dependency to validate", (instance) => {
      instance.dependency_results[0].status = "missing";
      instance.dependency_results[0].observed_sha256 = null;
    });
    push("assertion-evaluation: stale requires a changed dependency", (instance) => {
      instance.status = "stale";
      instance.boundary = { kind: "dependency_change", affected_dimensions: ["integrity"], dependency_ids: [instance.dependency_results[0].dependency_id], compatibility: null };
      instance.reason_code = "DEPENDENCY_DIGEST_MISMATCH";
    });
  }
  if (!cases.length) throw new Error(`No invalid fixture cases generated for ${name}.`);
  return cases;
}

async function writeJson(filePath, value) {
  await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
}

await rm(output, { recursive: true, force: true });
await Promise.all([validDir, invalidDir, vectorDir, registryDir].map((directory) => mkdir(directory, { recursive: true })));

for (const name of schemaNames) {
  const schema = schemas.get(name);
  await writeJson(path.join(output, `${name}.schema.json`), schema);
  await writeJson(path.join(validDir, `${name}.valid.json`), {
    schema: schema.$id,
    description: `Valid ${schema.title} example.`,
    instance: schema.examples[0],
  });
  await writeJson(path.join(invalidDir, `${name}.invalid.json`), {
    schema: schema.$id,
    cases: invalidCases(name, schema, schema.examples[0]),
  });
}

const registries = [
  {
    name: "assertion-lifecycle-phases",
    description: "Immutable assertion lifecycle phase observed when the relationship was asserted.",
    entries: [
      { code: "proposed", meaning: "A relationship has been proposed but not adopted." },
      { code: "generated", meaning: "A relationship was produced by a deterministic or model-assisted operation." },
      { code: "staged_preview", meaning: "The relationship is visible for review but not accepted into canonical work." },
      { code: "accepted_into_work", meaning: "The relationship was explicitly accepted into canonical project work." },
      { code: "revised", meaning: "The relationship describes a later immutable revision." },
      { code: "finalized", meaning: "The relationship belongs to finalized project content." },
      { code: "published", meaning: "The relationship is bound into a published release." },
      { code: "superseded", meaning: "A later assertion replaces this relationship for current interpretation without erasing it." },
      { code: "purged", meaning: "The relationship is affected by an explicitly disclosed emergency reconstitution." },
    ],
  },
  {
    name: "assertion-evaluation-statuses",
    description: "Consumer-facing point-in-time assertion conclusions.",
    entries: [
      { code: "exact", consumer_action: "promote", meaning: "All mandatory evidence and relevant confidence dimensions support the asserted scope." },
      { code: "degraded", consumer_action: "warn", meaning: "Use is possible only within the recorded uncertainty boundary." },
      { code: "refused", consumer_action: "refuse", meaning: "Policy, authorization, or compatibility prohibits a conclusion." },
      { code: "stale", consumer_action: "reevaluate", meaning: "A source anchor, digest, or generation dependency changed." },
      { code: "unverified", consumer_action: "withhold_exact", meaning: "Mandatory evaluation or evidence is incomplete without a demonstrated contradiction." },
    ],
  },
  {
    name: "assertion-confidence-dimensions",
    description: "Independent non-numeric confidence dimensions.",
    values: confidenceValues,
    entries: [
      { code: "integrity", meaning: "Whether canonical bytes, digests, and chain bindings validate." },
      { code: "identity", meaning: "Whether referenced subjects, objects, producers, and evidence identities resolve." },
      { code: "chronology", meaning: "Whether ordering and generation coordinates are complete and consistent." },
      { code: "derivation", meaning: "Whether the asserted transformation or relationship is supported by evidence." },
      { code: "authorship", meaning: "Whether the claimed actor relationship is supported without percentage attribution." },
      { code: "completeness", meaning: "Whether all evidence required for the asserted scope was evaluated." },
    ],
  },
  {
    name: "assertion-evidence-classes",
    description: "Evidence availability and authority requirements.",
    entries: [
      { code: "mandatory_live", exact_effect: "required", meaning: "Current accessible evidence is required for exact evaluation." },
      { code: "mandatory_retained", exact_effect: "required", meaning: "Retained authoritative evidence and its digest are required for exact evaluation." },
      { code: "advisory", exact_effect: "may_degrade", meaning: "Absence may degrade a named dimension but does not alter authoritative evidence." },
      { code: "shadow", exact_effect: "no_authority", meaning: "Comparison-only evidence that never becomes authoritative by observation." },
    ],
  },
  {
    name: "assertion-boundary-kinds",
    description: "Machine-readable boundaries preventing an unqualified exact conclusion.",
    entries: assertionBoundaryKinds.map((code) => ({ code, meaning: assertionBoundaryMeanings[code] })),
  },
  {
    name: "assertion-reason-codes",
    description: "Stable explanations for assertion creation and evaluation outcomes.",
    entries: [
      { code: "DIRECT_HASH_LINKED_DERIVATION", permitted_statuses: ["exact"] },
      { code: "VERIFIED_TRANSITIVE_DERIVATION", permitted_statuses: ["exact"] },
      { code: "REQUIRED_PROVENANCE_UNKNOWN", permitted_statuses: ["unverified"] },
      { code: "SOURCE_GENERATION_UNKNOWN", permitted_statuses: ["unverified"] },
      { code: "REQUIRED_EVIDENCE_MISSING", permitted_statuses: ["unverified"] },
      { code: "DEPENDENCY_DIGEST_MISMATCH", permitted_statuses: ["stale"] },
      { code: "DEPENDENCY_GENERATION_MISMATCH", permitted_statuses: ["stale"] },
      { code: "SOURCE_ANCHOR_STALE", permitted_statuses: ["stale"] },
      { code: "SCHEMA_INCOMPATIBLE", permitted_statuses: ["refused"] },
      { code: "POLICY_REFUSED", permitted_statuses: ["refused"] },
      { code: "AUTHORIZED_EVIDENCE_UNAVAILABLE", permitted_statuses: ["refused", "unverified"] },
      { code: "ADVISORY_EVIDENCE_UNAVAILABLE", permitted_statuses: ["degraded"] },
      { code: "ASSERTION_NOT_EVALUATED", permitted_statuses: ["unverified"] },
      { code: "AUTHORSHIP_UNCERTAIN", permitted_statuses: ["degraded"] },
      { code: "CHRONOLOGY_INCOMPLETE", permitted_statuses: ["degraded", "unverified"] },
      { code: "COMPLETENESS_INCOMPLETE", permitted_statuses: ["degraded", "unverified"] },
    ],
  },
].map((registry) => ({
  registry_version: "1.0",
  provenance_schema_version: "1.0",
  application_version: "0.4.0",
  ...registry,
  entries: registry.entries.map((entry) => registry.name === "assertion-reason-codes" ? { ...entry, meaning: assertionReasonMeanings[entry.code] } : entry),
}));
for (const registry of registries) await writeJson(path.join(registryDir, `${registry.name}.json`), registry);
const catalog = {
  catalog_version: "1.0",
  dialect: draft,
  provenance_schema_version: "1.0",
  application_version: "0.4.0",
  compatible_application_versions: ["0.4.0"],
  native_writer_conformance: false,
  registries: registries.map(({ name, description }) => ({
    name,
    description,
    id: `${baseId}/registries/${name}.json`,
    file: `registries/${name}.json`,
  })),
  normative_specification: "../../../docs/provenance/STAGE-1-NORMATIVE-SPECIFICATION.md",
  generated_by: "scripts/generate-provenance-stage2.mjs",
  schemas: schemaNames.map((name) => ({
    name,
    id: schemas.get(name).$id,
    file: `${name}.schema.json`,
    valid_fixture: `fixtures/valid/${name}.valid.json`,
    invalid_fixture: `fixtures/invalid/${name}.invalid.json`,
  })),
};
await writeJson(path.join(output, "catalog.json"), catalog);

const canonicalInputs = [
  { id: "rfc8785-key-order", input: { z: 1, a: 2, nested: { beta: true, alpha: false } } },
  { id: "thinkloom-nfc", input: { text: "Cafe\u0301", "e\u0301": "normalize keys and values" } },
  { id: "rfc8785-numbers", input: { numbers: [333333333.3333333, 1e30, 4.5, 0.002, 1e-27] } },
  { id: "thinkloom-timestamp-and-path", input: { timestamp: timestampValue, path: "records/invocations/request.json" } },
];
const canonicalizationVector = {
  vector_version: "1.0",
  algorithm: "Unicode NFC followed by RFC 8785 JSON Canonicalization Scheme and SHA-256 over UTF-8 bytes",
  cases: canonicalInputs.map(({ id, input }) => {
    const canonical_json = canonicalize(input);
    return { id, input, canonical_json, utf8_sha256: sha256(canonical_json) };
  }),
  rejected_inputs: [
    { id: "non-finite-number", description: "NaN and infinity are not JSON and MUST be rejected before canonicalization." },
    { id: "undefined-value", description: "Undefined values MUST be rejected rather than silently omitted." },
    { id: "nfc-key-collision", description: "Distinct source keys that normalize to the same NFC key MUST be rejected." },
    { id: "unpaired-surrogate", description: "I-JSON prohibits unpaired Unicode surrogate code units." },
    { id: "non-json-runtime-types", description: "BigInt, symbols, functions, sparse arrays, and non-plain objects MUST be rejected." },
  ],
};
await writeJson(path.join(vectorDir, "canonicalization.json"), canonicalizationVector);

const baseEvents = [
  { ...eventExample, event_id: ids.event, event_sequence: 1, event_type: "PROJECT_CREATED", previous_event_hash: null, event_hash: "" },
  { ...eventExample, event_id: ids.event2, event_sequence: 2, event_type: "IDEA_ACCEPTED", previous_event_hash: "", event_hash: "", timestamp: laterTimestamp },
  { ...eventExample, event_id: typedId("event", "T"), event_sequence: 3, event_type: "MANUSCRIPT_EDITED", previous_event_hash: "", event_hash: "", timestamp: "2026-07-17T18:44:12.789Z" },
];
for (let index = 0; index < baseEvents.length; index += 1) {
  baseEvents[index].previous_event_hash = index ? baseEvents[index - 1].event_hash : null;
  baseEvents[index].event_hash = hashEvent(baseEvents[index]);
}
const canonicalEventLines = baseEvents.map((event) => canonicalize(event));
await writeJson(path.join(vectorDir, "event-chain.json"), {
  vector_version: "1.0",
  event_hash_identity: "Canonical event object excluding only event_hash; previous_event_hash remains included.",
  sequence_rule: "Committed event_sequence values begin at 1 and are contiguous with no reuse.",
  jsonl_rule: "Each canonical event is UTF-8 without BOM followed by exactly one LF byte (0x0A).",
  events: baseEvents,
  canonical_jsonl: `${canonicalEventLines.join("\n")}\n`,
  canonical_jsonl_sha256: sha256(`${canonicalEventLines.join("\n")}\n`),
  rejected_boundary_variants: ["CRLF line endings", "missing final LF", "blank records", "UTF-8 BOM"],
});

const protectedPlaintextIdentity = {
  schema_version: "1.0",
  record_id: ids.record,
  record_type: "invocation_request",
  payload: { purpose: "draft_from_selected_ideas" },
};
await writeJson(path.join(vectorDir, "protected-record-and-key-rotation.json"), {
  vector_version: "1.0",
  plaintext_identity_fields: ["schema_version", "record_id", "record_type", "payload"],
  plaintext_identity: protectedPlaintextIdentity,
  canonical_plaintext_identity: canonicalize(protectedPlaintextIdentity),
  plaintext_sha256: sha256(protectedPlaintextIdentity),
  device_key_envelope: keyEnvelopeExample,
  recovery_key_envelope: recoveryEnvelopeExample,
  project_key_manifest: keyManifestExample,
  initial_envelope: recordEnvelopeExample,
  rotated_envelope: {
    ...recordEnvelopeExample,
    encryption: { ...recordEnvelopeExample.encryption, key_id: typedId("key", "V"), nonce: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB", ciphertext: "cm90YXRlZC1jaXBoZXJ0ZXh0", ciphertext_sha256: digest("c") },
  },
  invariant: "Key rotation changes encryption metadata and ciphertext but preserves record_id and plaintext_sha256.",
  excluded_from_plaintext_identity: ["project_id", "protection", "plaintext_sha256", "encryption", "path", "nonce", "ciphertext"],
});

await writeJson(path.join(vectorDir, "release-merkle.json"), {
  vector_version: "1.0",
  algorithm_id: "thinkloom-release-merkle-v1",
  ordering: "Sort entries by raw UTF-8 path bytes in ascending order.",
  leaf: "SHA-256(UTF8('thinkloom-release-leaf-v1\\0') || UTF8(path) || 0x00 || UTF8(decimal size) || 0x00 || raw 32-byte file SHA-256 digest)",
  node: "SHA-256(UTF8('thinkloom-release-node-v1\\0') || raw left digest || raw right digest); duplicate the final node when a level has odd cardinality.",
  empty: "SHA-256(UTF8('thinkloom-release-empty-v1\\0'))",
  entries: releaseFiles,
  root: releaseMerkleRoot(releaseFiles),
  excluded_paths: ["release-manifest.json", "release-files.sha256"],
  self_reference_rule: "The manifest and any flat digest file that contains the root MUST NOT be Merkle leaves.",
});

const minimalRequest = {
  ...requestExample,
  retention_mode: "minimal",
  messages: [],
  redactions: [],
};
const minimalResponse = {
  ...responseExample,
  retention_mode: "minimal",
  retained_text: null,
  provider_metadata: null,
};
await writeJson(path.join(vectorDir, "retention-policy-modes.json"), {
  vector_version: "1.0",
  default_mode: "minimal",
  minimal: {
    policy: policyExample,
    invocation_request: minimalRequest,
    invocation_response: minimalResponse,
    prohibited_durable_fields: ["complete provider-facing prompt", "complete supplied context", "unaccepted raw response", "provider transport metadata", "audio bytes or references"],
  },
  full_private: {
    policy: { ...policyExample, retention_mode: "full_private", encryption_mode: "protected" },
    invocation_request: requestExample,
    invocation_response: responseExample,
    requirements: ["protected records", "verified recovery envelope", "authorized verification for plaintext evidence"],
  },
  policy_change_rule: "Changes are prospective. Reducing retention does not delete prior records; deletion requires emergency purge.",
});

await writeJson(path.join(vectorDir, "timestamp-and-path.json"), {
  vector_version: "1.0",
  timestamps: {
    accepted: [timestampValue, laterTimestamp, "2000-01-01T00:00:00.000Z"],
    rejected: ["2026-07-17T18:42:10Z", "2026-07-17T18:42:10.12Z", "2026-07-17T18:42:10.1234Z", "2026-07-17T11:42:10.123-07:00", "not-a-timestamp"],
  },
  repository_paths: {
    accepted: ["project.json", "records/invocations/request.json", "manuscript/Café.md"],
    rejected: ["", "/absolute/path", "C:/absolute/path", "records\\request.json", ".", "..", "records/../outside.json", "records/./alias.json"],
  },
  normalization_rule: "Paths use NFC, forward slashes, repository-relative spelling, and no dot segments.",
});

const segmentOneJsonl = `${canonicalEventLines.slice(0, 2).join("\n")}\n`;
const segmentTwoJsonl = `${canonicalEventLines.slice(2).join("\n")}\n`;
const segmentOneHash = sha256(segmentOneJsonl);
const segmentTwoHash = sha256(segmentTwoJsonl);
const segmentOneManifest = {
  ...segmentExample,
  previous_segment_file_hash: null,
  first_event_hash: baseEvents[0].event_hash,
  final_event_hash: baseEvents[1].event_hash,
  first_event_sequence: 1,
  final_event_sequence: 2,
  event_count: 2,
  byte_length: Buffer.byteLength(segmentOneJsonl),
  segment_file_hash: segmentOneHash,
};
const segmentTwoManifest = {
  ...segmentExample,
  segment_number: 2,
  previous_segment_file_hash: segmentOneHash,
  first_event_hash: baseEvents[2].event_hash,
  final_event_hash: baseEvents[2].event_hash,
  first_event_sequence: 3,
  final_event_sequence: 3,
  event_count: 1,
  byte_length: Buffer.byteLength(segmentTwoJsonl),
  segment_file_hash: segmentTwoHash,
  sealed_at: "2026-07-17T18:45:13.012Z",
};
await writeJson(path.join(vectorDir, "cross-segment-chain.json"), {
  vector_version: "1.0",
  segments: [
    { jsonl: segmentOneJsonl, manifest: segmentOneManifest },
    { jsonl: segmentTwoJsonl, manifest: segmentTwoManifest },
  ],
  chain_head: {
    ...chainHeadExample,
    active_segment_number: 2,
    event_id: baseEvents[2].event_id,
    event_sequence: 3,
    event_hash: baseEvents[2].event_hash,
    updated_at: segmentTwoManifest.sealed_at,
  },
  cross_segment_rule: "A sealed segment names the prior segment file digest, and event hash linkage remains contiguous across the boundary.",
});

const promptIdentity = { ...promptExample };
delete promptIdentity.template_sha256;
const promptWithDigest = { ...promptIdentity, template_sha256: sha256(promptIdentity) };
await writeJson(path.join(vectorDir, "self-digest-identities.json"), {
  vector_version: "1.0",
  prompt_template: {
    identity: promptIdentity,
    digest: promptWithDigest.template_sha256,
    complete_record: promptWithDigest,
    excluded_fields: ["template_sha256"],
  },
  provenance_event: {
    identity: { ...baseEvents[0], event_hash: undefined },
    digest: baseEvents[0].event_hash,
    excluded_fields: ["event_hash"],
  },
  protected_record: {
    identity: protectedPlaintextIdentity,
    digest: sha256(protectedPlaintextIdentity),
    excluded_fields: ["project_id", "protection", "plaintext_sha256", "encryption", "path", "nonce", "ciphertext"],
  },
  release_manifest: {
    merkle_identity: releaseFiles,
    digest: releaseMerkleRoot(releaseFiles),
    excluded_paths: ["release-manifest.json", "release-files.sha256"],
  },
});

const sanitizationRules = [
  { category: "private_conversation", action: "exclude", count: 2 },
  { category: "personal_identifier", action: "redact", count: 1 },
  { category: "provider_detail", action: "summarize", count: 1 },
];
const sanitizedManifestVector = {
  ...sanitizedExample,
  omission_rules: sanitizationRules,
  rules_sha256: sha256(sanitizationRules),
};
await writeJson(path.join(vectorDir, "sanitized-export.json"), {
  vector_version: "1.0",
  source_chain_head_before: sanitizedManifestVector.source_chain_head,
  source_chain_head_after: sanitizedManifestVector.source_chain_head,
  source_record_count: 7,
  exported_record_count: 3,
  manifest: sanitizedManifestVector,
  non_mutation_rule: "Sanitization creates a disclosure-bearing export and never rewrites the project or source chain.",
});

const derivedInputsA = [
  { from: ids.idea, to: ids.revision2, relation: "influenced" },
  { from: ids.turn, to: ids.idea, relation: "suggested" },
];
const derivedInputsB = [...derivedInputsA].reverse();
const deterministicIndexContent = [...derivedInputsA].sort((left, right) => { const leftKey = canonicalize(left); const rightKey = canonicalize(right); return leftKey < rightKey ? -1 : leftKey > rightKey ? 1 : 0; });
const derivedConfiguration = { sort: "canonical-json-code-unit", graph_version: 1 };
const derivedManifestVector = {
  ...indexManifestExample,
  source_chain_head: baseEvents.at(-1).event_hash,
  source_event_count: baseEvents.length,
  generator: { ...indexManifestExample.generator, configuration_sha256: sha256(derivedConfiguration) },
  content_sha256: sha256(deterministicIndexContent),
};
await writeJson(path.join(vectorDir, "deterministic-derived-index.json"), {
  vector_version: "1.0",
  input_orders: [derivedInputsA, derivedInputsB],
  deterministic_content: deterministicIndexContent,
  configuration: derivedConfiguration,
  manifest: derivedManifestVector,
  determinism_rule: "Input order does not affect canonical derived content; volatile generation metadata is outside content_sha256.",
});

const verificationReports = [
  { ...verificationExample, verification_id: typedId("record", "W"), status: "VERIFIED", findings: [] },
  { ...verificationExample, verification_id: typedId("record", "X"), status: "VERIFIED_WITH_WARNINGS", findings: [{ ...verificationExample.findings[0], severity: "WARNING" }] },
  { ...verificationExample, verification_id: typedId("record", "Y"), status: "INCOMPLETE", findings: [{ ...verificationExample.findings[0], severity: "INFO", code: "OPTIONAL_BINARY_MISSING", message: "An optional release binary is unavailable." }] },
  { ...verificationExample, verification_id: typedId("record", "Z"), status: "FAILED", findings: [{ ...verificationExample.findings[0], severity: "ERROR", code: "EVENT_HASH_MISMATCH", message: "An event digest does not match canonical content.", authoritative_evidence_affected: true }] },
  { ...verificationExample, verification_id: typedId("record", "0"), status: "UNSAFE", findings: [{ ...verificationExample.findings[0], severity: "CRITICAL", code: "CHAIN_RECONSTITUTED", message: "The chain cannot establish ordinary continuity.", authoritative_evidence_affected: true }] },
];
await writeJson(path.join(vectorDir, "verification-statuses.json"), {
  vector_version: "1.0",
  reports: verificationReports,
  required_statuses: ["VERIFIED", "VERIFIED_WITH_WARNINGS", "INCOMPLETE", "FAILED", "UNSAFE"],
  required_severities: ["INFO", "WARNING", "ERROR", "CRITICAL"],
});

await writeJson(path.join(vectorDir, "backup-and-release-manifests.json"), {
  vector_version: "1.0",
  backup_manifest: { ...backupExample, source_chain_head: baseEvents.at(-1).event_hash },
  release_manifest: { ...releaseExample, source_chain_head: baseEvents.at(-1).event_hash },
  binding_rule: "Backup and release inventories bind the same frozen authoritative chain head while remaining distinct artifact classes.",
});
const vectorAssertionIdentity = {
  ...assertionIdentity,
  source_anchor: { event_id: baseEvents[1].event_id, event_sequence: baseEvents[1].event_sequence, event_hash: baseEvents[1].event_hash },
  dependencies: assertionIdentity.dependencies.map((dependency, index) => index === 0 ? { ...dependency, expected_sha256: baseEvents[1].event_hash } : dependency),
};
const vectorAssertion = { ...vectorAssertionIdentity, assertion_sha256: sha256(vectorAssertionIdentity) };
const assertionRecordingEvent = {
  ...eventExample,
  event_id: typedId("event", "Y"),
  event_sequence: 4,
  timestamp: "2026-07-17T18:45:13.012Z",
  event_type: "ASSERTION_RECORDED",
  inputs: [{ ...contentReferenceExample, record_id: baseEvents[1].event_id, record_type: "other", sha256: baseEvents[1].event_hash, path: "provenance/ledger/active.jsonl", revision_id: null, range: null }],
  outputs: [{ ...contentReferenceExample, record_id: vectorAssertion.assertion_id, record_type: "provenance_assertion", sha256: vectorAssertion.assertion_sha256, path: "records/assertions/assertion.json", revision_id: null, range: null }],
  relationships: { parent_event_ids: [baseEvents[1].event_id], invocation_id: null },
  metadata: { summary: "Recorded canonical provenance assertion", assertion_id: vectorAssertion.assertion_id },
  previous_event_hash: baseEvents.at(-1).event_hash,
  event_hash: "",
};
assertionRecordingEvent.event_hash = hashEvent(assertionRecordingEvent);
const exactAssertionEvaluation = {
  ...evaluationExample,
  assertion_sha256: vectorAssertion.assertion_sha256,
  evaluated_against: { ...evaluationExample.evaluated_against, chain_head: assertionRecordingEvent.event_hash, event_sequence: assertionRecordingEvent.event_sequence },
  dependency_results: evaluationExample.dependency_results.map((result, index) => index === 0 ? { ...result, observed_sha256: baseEvents[1].event_hash } : result),
};
const degradedAssertionEvaluation = {
  ...exactAssertionEvaluation,
  evaluation_id: typedId("evaluation", "X"),
  status: "degraded",
  confidence: { ...exactAssertionEvaluation.confidence, authorship: "degraded", completeness: "degraded" },
  boundary: { kind: "evidence_access", affected_dimensions: ["authorship", "completeness"], dependency_ids: [ids.turn], compatibility: null },
  dependency_results: exactAssertionEvaluation.dependency_results.map((result) => result.dependency_id === ids.turn ? { ...result, status: "missing", observed_sha256: null, observed_generation: null } : result),
  reason_code: "ADVISORY_EVIDENCE_UNAVAILABLE",
  supersedes_evaluation_id: exactAssertionEvaluation.evaluation_id,
  evaluated_at: "2026-07-17T18:45:13.012Z",
};
const staleAssertionEvaluation = {
  ...degradedAssertionEvaluation,
  evaluation_id: typedId("evaluation", "Y"),
  evaluated_against: { ...degradedAssertionEvaluation.evaluated_against, chain_head: digest("a"), event_sequence: 5 },
  status: "stale",
  confidence: { integrity: "degraded", identity: "exact", chronology: "degraded", derivation: "degraded", authorship: "unverified", completeness: "degraded" },
  boundary: { kind: "dependency_change", affected_dimensions: ["integrity", "chronology", "derivation"], dependency_ids: [ids.event2], compatibility: null },
  dependency_results: exactAssertionEvaluation.dependency_results.map((result) => result.dependency_id === ids.event2 ? { ...result, status: "changed", observed_sha256: digest("0"), observed_generation: { project_epoch: 1, artifact_revision: 13, transcript_revision: 4 } } : result),
  reason_code: "DEPENDENCY_GENERATION_MISMATCH",
  supersedes_evaluation_id: degradedAssertionEvaluation.evaluation_id,
  evaluated_at: "2026-07-17T18:46:14.345Z",
};
const refusedAssertionEvaluation = {
  ...staleAssertionEvaluation,
  evaluation_id: typedId("evaluation", "Z"),
  status: "refused",
  confidence: { integrity: "exact", identity: "unverified", chronology: "exact", derivation: "unverified", authorship: "not_applicable", completeness: "unverified" },
  boundary: { kind: "compatibility", affected_dimensions: ["identity", "derivation", "completeness"], dependency_ids: [ids.record], compatibility: { required_schema: "1.0", observed_schema: "2.0" } },
  dependency_results: exactAssertionEvaluation.dependency_results.map((result) => result.dependency_id === ids.record ? { ...result, status: "incompatible", observed_sha256: null, observed_generation: null } : result),
  reason_code: "SCHEMA_INCOMPATIBLE",
  supersedes_evaluation_id: staleAssertionEvaluation.evaluation_id,
  evaluated_at: "2026-07-17T18:47:15.678Z",
};
const unverifiedAssertionEvaluation = {
  ...refusedAssertionEvaluation,
  evaluation_id: typedId("evaluation", "0"),
  status: "unverified",
  confidence: { integrity: "exact", identity: "unverified", chronology: "exact", derivation: "unverified", authorship: "not_applicable", completeness: "unverified" },
  boundary: { kind: "evidence_access", affected_dimensions: ["identity", "derivation", "completeness"], dependency_ids: [ids.record], compatibility: null },
  dependency_results: exactAssertionEvaluation.dependency_results.map((result) => result.dependency_id === ids.record ? { ...result, status: "inaccessible", observed_sha256: null, observed_generation: null } : result),
  reason_code: "AUTHORIZED_EVIDENCE_UNAVAILABLE",
  supersedes_evaluation_id: refusedAssertionEvaluation.evaluation_id,
  evaluated_at: "2026-07-17T18:48:16.901Z",
};
await writeJson(path.join(vectorDir, "assertion-envelope-and-invalidation.json"), {
  vector_version: "1.0",
  assertion_recording_event: assertionRecordingEvent,
  assertion: vectorAssertion,
  assertion_identity: vectorAssertionIdentity,
  evaluations: [exactAssertionEvaluation, degradedAssertionEvaluation, staleAssertionEvaluation, refusedAssertionEvaluation, unverifiedAssertionEvaluation],
  consumer_decisions: [
    { status: "exact", action: "promote" },
    { status: "degraded", action: "warn" },
    { status: "refused", action: "refuse" },
    { status: "stale", action: "reevaluate" },
    { status: "unverified", action: "withhold_exact" },
  ],
  forbidden_exact_cases: {
    unknown_provenance: { ...vectorAssertion, provenance: undefined },
    unknown_confidence: { ...exactAssertionEvaluation, confidence: { ...exactAssertionEvaluation.confidence, derivation: "unverified" } },
    incompatible_dependency: { ...exactAssertionEvaluation, dependency_results: exactAssertionEvaluation.dependency_results.map((result, index) => index === 0 ? { ...result, status: "incompatible" } : result) },
    uncertainty_boundary: { ...exactAssertionEvaluation, boundary: { kind: "coverage", affected_dimensions: ["completeness"], dependency_ids: [], compatibility: null } },
    unknown_artifact_generation: { ...vectorAssertion, source_generation: { ...vectorAssertion.source_generation, artifact_revision: null } },
  },
  invariants: [
    "The source anchor identifies prior basis evidence and differs from the later assertion-recording event.",
    "The assertion digest excludes only assertion_sha256.",
    "Evaluations are immutable point-in-time conclusions; later records supersede current use without mutation.",
    "Unknown provenance, generation, compatibility, or confidence never produces exact.",
    "Consumers decide from canonical assertions, evaluations, and registries without producer-specific internal state.",
  ],
});
const readme = `# Thinkloom provenance schemas 1.0\n\nThis generated package is the formal Stage 2 contract for the approved provenance architecture. It targets JSON Schema Draft 2020-12 and ships with ${schemaNames.length} schemas, valid fixtures, invalid fixture suites, and deterministic verification vectors.\n\n- \`catalog.json\` is the machine-readable inventory.\n- \`*.schema.json\` are the formal contracts.\n- \`registries\` defines assertion lifecycle, evaluation status, confidence, evidence, boundary, and reason semantics.\n- \`fixtures/valid\` contains one canonical valid instance per schema.\n- \`fixtures/invalid\` covers required fields, closed-object policy, and populated enum, pattern, and numeric/string/array bounds.\n- \`vectors\` fixes canonicalization, timestamps, paths, JSONL, event and segment chains, self-digests, protected records, key rotation and recovery, retention policy, sanitized export, deterministic indexes, verification reports, canonical assertions and evaluations, dependency invalidation, backups, and release-Merkle behavior.\n\nRegenerate with \`npm run provenance:schema:generate\`. Verify with \`npm run provenance:schema:test\`. Generated files must be committed together with generator and vector changes. Runtime adoption is a later implementation stage; presence of this package does not imply the current native writer already conforms.\n`;
await writeFile(path.join(output, "README.md"), readme, "utf8");

console.log(`Generated ${schemaNames.length} provenance schemas and fixtures in ${path.relative(root, output)}.`);