import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";
import { canonicalize, hashEvent, releaseMerkleRoot, sha256 } from "../scripts/provenance-stage2-lib.mjs";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const schemaRoot = path.join(repositoryRoot, "schemas", "provenance", "v1");
const readJson = async (...segments) => JSON.parse(await readFile(path.join(schemaRoot, ...segments), "utf8"));
const catalog = await readJson("catalog.json");
const schemaEntries = await Promise.all(catalog.schemas.map(async (entry) => ({ ...entry, schema: await readJson(entry.file) })));

function validator() {
  const ajv = new Ajv2020({ allErrors: true, strict: true, validateFormats: true });
  addFormats(ajv);
  for (const entry of schemaEntries) {
    assert.equal(ajv.validateSchema(entry.schema), true, `${entry.name} is not a valid Draft 2020-12 schema: ${ajv.errorsText(ajv.errors)}`);
    ajv.addSchema(entry.schema);
  }
  return ajv;
}

const requiredSchemas = [
  "assertion-evaluation", "backup-manifest", "chain-head", "composition-operation", "content-reference", "contribution-map",
  "conversation-session", "deposit-snapshot", "derived-index-manifest", "disposition-revision", "edit-transaction",
  "encrypted-key-envelope", "expression-segment", "harp-export-manifest", "human-authorship-record", "idea", "idea-revision",
  "invocation-failure", "invocation-request", "invocation-response", "invocation-state", "invocation-stream-state",
  "invocation-stream-summary", "ledger-segment-manifest", "manuscript-revision", "model-capability-snapshot",
  "model-configuration-snapshot", "project-key-manifest", "project-manifest", "prompt-template",
  "prompt-template-reference", "provenance-assertion", "provenance-event", "provenance-policy", "purge-manifest", "record-envelope",
  "recovery-key-envelope", "registration-policy-profile", "release-manifest", "release-state", "sanitized-export-manifest",
  "text-fragment-reference", "transcript-correction", "transcript-normalization", "transcript-turn", "verification-report", "write-intent",
];

test("catalogs every approved Stage 2 schema under Draft 2020-12", () => {
  assert.equal(catalog.catalog_version, "1.1");
  assert.equal(catalog.package_version, "0.5.2");
  assert.equal(catalog.provenance_schema_version, "1.0");
  assert.equal(catalog.application_version, "0.5.2");
  assert.deepEqual(catalog.compatible_application_versions, ["0.4.0", "0.5.0", "0.5.1", "0.5.2", "0.5.3", "0.5.4", "0.5.5", "0.5.6", "0.5.7", "0.5.8", "0.5.9", "0.5.10", "0.5.11", "0.6.0"]);
  assert.equal(catalog.cpl_runtime_target, "0.6.0");
  assert.equal(catalog.native_writer_conformance, false);
  assert.match(catalog.assertion_semantics_compatibility, /v0\.4.+remain valid/i);
  assert.equal(catalog.dialect, "https://json-schema.org/draft/2020-12/schema");
  assert.deepEqual(catalog.schemas.map(({ name }) => name), requiredSchemas);
  assert.equal(new Set(catalog.schemas.map(({ id }) => id)).size, requiredSchemas.length);
  for (const entry of catalog.schemas) {
    const isCompositionExtension = ["composition-operation", "expression-segment", "contribution-map", "deposit-snapshot", "registration-policy-profile", "human-authorship-record", "harp-export-manifest"].includes(entry.name);
    assert.equal(entry.introduced_in_application_version, isCompositionExtension ? "0.5.2" : "0.4.0");
    assert.ok(entry.compatible_application_versions.includes("0.5.3"));
    assert.ok(entry.compatible_application_versions.includes("0.5.10"));
    assert.ok(entry.compatible_application_versions.includes("0.5.11"));
    assert.ok(entry.compatible_application_versions.includes("0.6.0"));
    if (!isCompositionExtension) assert.ok(entry.compatible_application_versions.includes("0.4.0"));
  }
  for (const { schema } of schemaEntries) {
    assert.equal(schema.additionalProperties, false, `${schema.$id} must be closed at its top level`);
  }
});

test("publishes complete versioned assertion and composition registries", async () => {
  const legacyExpected = ["assertion-boundary-kinds", "assertion-confidence-dimensions", "assertion-evaluation-statuses", "assertion-evidence-classes", "assertion-lifecycle-phases", "assertion-reason-codes"];
  const compositionExpected = ["composition-assertion-predicates", "composition-operation-kinds", "contribution-map-layers", "harp-explanation-codes", "harp-limitation-codes", "recorded-origin-kinds", "registration-treatment-suggestions", "transformation-relationships"];
  assert.deepEqual(catalog.registries.map(({ name }) => name).sort(), [...legacyExpected, ...compositionExpected].sort());
  for (const entry of catalog.registries) {
    const registry = await readJson(entry.file);
    const isCompositionExtension = compositionExpected.includes(entry.name);
    assert.equal(registry.registry_version, isCompositionExtension ? "1.1" : "1.0");
    assert.equal(registry.provenance_schema_version, "1.0");
    assert.equal(registry.application_version, isCompositionExtension ? "0.5.2" : "0.4.0");
    assert.equal(registry.introduced_in_application_version, isCompositionExtension ? "0.5.2" : "0.4.0");
    assert.ok(registry.compatible_application_versions.includes("0.5.3"));
    assert.ok(registry.compatible_application_versions.includes("0.5.10"));
    assert.ok(registry.compatible_application_versions.includes("0.5.11"));
    assert.ok(registry.compatible_application_versions.includes("0.6.0"));
    assert.ok(registry.entries.length > 0);
    assert.ok(registry.entries.every(({ meaning }) => typeof meaning === "string" && meaning.length > 0));
    assert.equal(new Set(registry.entries.map(({ code }) => code)).size, registry.entries.length);
  }
});

test("keeps composition dimensions independent and preserves v0.4 assertion semantics", async () => {
  const registries = Object.fromEntries(await Promise.all(catalog.registries.map(async (entry) => [entry.name, await readJson(entry.file)])));
  assert.deepEqual(registries["composition-operation-kinds"].entries.map(({ code }) => code), ["insert", "delete", "replace", "move", "paste", "transcription", "ai_acceptance", "restoration"]);
  assert.deepEqual(registries["recorded-origin-kinds"].entries.map(({ code }) => code), ["recorded_direct_human_input", "human_expressive_input_via_transcription", "accepted_ai_output", "imported_or_pasted", "system_restoration", "unattested"]);
  assert.deepEqual(registries["composition-assertion-predicates"].entries.map(({ code }) => code), ["derived_from", "generated_by", "modified_by_human", "selected_by_human", "arranged_by_human", "included_in_deposit"]);
  assert.ok(registries["contribution-map-layers"].entries.some(({ code }) => code === "selection_arrangement"));
  assert.ok(registries["registration-treatment-suggestions"].entries.some(({ code }) => code === "manual_review_required"));
  assert.deepEqual(registries["assertion-confidence-dimensions"].entries.map(({ code }) => code), ["integrity", "identity", "chronology", "derivation", "authorship", "completeness"]);
  assert.equal(registries["assertion-confidence-dimensions"].application_version, "0.4.0");
});
test("accepts every valid fixture", async () => {
  const ajv = validator();
  for (const entry of schemaEntries) {
    const fixture = await readJson(entry.valid_fixture);
    const validate = ajv.getSchema(entry.id);
    assert.equal(validate(fixture.instance), true, `${entry.name}: ${ajv.errorsText(validate.errors)}`);
  }
});

test("rejects every invalid fixture case", async () => {
  const ajv = validator();
  for (const entry of schemaEntries) {
    const fixture = await readJson(entry.invalid_fixture);
    assert.ok(fixture.cases.length >= 2, `${entry.name} needs multiple invalid cases`);
    for (const requiredProperty of entry.schema.required ?? []) {
      assert.ok(fixture.cases.some(({ description }) => description === `${entry.name}.${requiredProperty}: reject missing required property`), `${entry.name} lacks a missing-${requiredProperty} fixture`);
    }
    assert.ok(fixture.cases.some(({ description }) => description === `${entry.name}: reject unexpected property`), `${entry.name} lacks an additional-property fixture`);
    const validate = ajv.getSchema(entry.id);
    for (const invalidCase of fixture.cases) {
      assert.equal(validate(invalidCase.instance), false, `${entry.name} unexpectedly accepted: ${invalidCase.description}`);
    }
  }
});

test("reproduces canonical JSON and rejects prohibited values", async () => {
  const vectors = await readJson("vectors", "canonicalization.json");
  for (const vector of vectors.cases) {
    assert.equal(canonicalize(vector.input), vector.canonical_json, vector.id);
    assert.equal(sha256(vector.canonical_json), vector.utf8_sha256, vector.id);
  }
  assert.throws(() => canonicalize({ value: Number.NaN }), /non-finite/);
  assert.throws(() => canonicalize({ value: undefined }), /undefined/);
  assert.throws(() => canonicalize({ "é": 1, "e\u0301": 2 }), /key collision/);
  assert.throws(() => canonicalize({ value: 1n }), /bigint/);
  const sparse = [];
  sparse.length = 2;
  sparse[1] = "value";
  assert.throws(() => canonicalize(sparse), /sparse arrays/);
  assert.throws(() => canonicalize(new Date()), /plain objects/);
  assert.throws(() => canonicalize({ value: "\ud800" }), /unpaired Unicode surrogate/);
  assert.equal(canonicalize(JSON.parse('{"__proto__":{"safe":true}}')), '{"__proto__":{"safe":true}}');
});

test("reproduces contiguous event hashes and canonical LF-only JSONL", async () => {
  const vector = await readJson("vectors", "event-chain.json");
  for (const [index, event] of vector.events.entries()) {
    assert.equal(event.event_sequence, index + 1);
    assert.equal(event.previous_event_hash, index ? vector.events[index - 1].event_hash : null);
    assert.equal(hashEvent(event), event.event_hash);
  }
  assert.equal(vector.canonical_jsonl, `${vector.events.map(canonicalize).join("\n")}\n`);
  assert.equal(sha256(vector.canonical_jsonl), vector.canonical_jsonl_sha256);
  assert.equal(vector.canonical_jsonl.includes("\r"), false);
  assert.equal(vector.canonical_jsonl.charCodeAt(0) === 0xfeff, false);
});

test("preserves protected-record identity across key rotation", async () => {
  const vector = await readJson("vectors", "protected-record-and-key-rotation.json");
  assert.equal(canonicalize(vector.plaintext_identity), vector.canonical_plaintext_identity);
  assert.equal(sha256(vector.plaintext_identity), vector.plaintext_sha256);
  assert.equal(vector.initial_envelope.record_id, vector.rotated_envelope.record_id);
  assert.equal(vector.initial_envelope.plaintext_sha256, vector.rotated_envelope.plaintext_sha256);
  assert.notEqual(vector.initial_envelope.encryption.key_id, vector.rotated_envelope.encryption.key_id);
  assert.notEqual(vector.initial_envelope.encryption.ciphertext_sha256, vector.rotated_envelope.encryption.ciphertext_sha256);
});

test("reproduces the release-files Merkle root without self-reference", async () => {
  const vector = await readJson("vectors", "release-merkle.json");
  assert.equal(vector.algorithm_id, "thinkloom-release-merkle-v1");
  assert.equal(releaseMerkleRoot(vector.entries), vector.root);
  assert.equal(vector.entries.some(({ path: entryPath }) => vector.excluded_paths.includes(entryPath)), false);
  assert.match(vector.self_reference_rule, /MUST NOT/);
});

test("validates both retention policy vectors and keeps minimal as default", async () => {
  const ajv = validator();
  const vector = await readJson("vectors", "retention-policy-modes.json");
  assert.equal(vector.default_mode, "minimal");
  for (const mode of [vector.minimal, vector.full_private]) {
    for (const [schemaName, field] of [["provenance-policy", "policy"], ["invocation-request", "invocation_request"], ["invocation-response", "invocation_response"]]) {
      const validate = ajv.getSchema(`https://thinkloom.app/schemas/provenance/1.0/${schemaName}.schema.json`);
      assert.equal(validate(mode[field]), true, `${schemaName}: ${ajv.errorsText(validate.errors)}`);
    }
  }
  assert.deepEqual(vector.minimal.invocation_request.messages, []);
  assert.equal(vector.minimal.invocation_response.provider_metadata, null);
  assert.equal(vector.full_private.policy.encryption_mode, "protected");
  assert.ok(vector.full_private.invocation_request.messages.length > 0);
});
test("enforces canonical timestamps and repository-relative paths", async () => {
  const ajv = validator();
  const vector = await readJson("vectors", "timestamp-and-path.json");
  const timestampSchema = schemaEntries.find(({ name }) => name === "project-manifest").schema.properties.created_at;
  const pathSchema = schemaEntries.find(({ name }) => name === "prompt-template-reference").schema.properties.path;
  const validateTimestamp = ajv.compile(timestampSchema);
  const validatePath = ajv.compile(pathSchema);
  for (const value of vector.timestamps.accepted) assert.equal(validateTimestamp(value), true, value);
  for (const value of vector.timestamps.rejected) assert.equal(validateTimestamp(value), false, value);
  for (const value of vector.repository_paths.accepted) assert.equal(validatePath(value), true, value);
  for (const value of vector.repository_paths.rejected) assert.equal(validatePath(value), false, value);
  assert.equal(vector.repository_paths.accepted.at(-1), vector.repository_paths.accepted.at(-1).normalize("NFC"));
});

test("verifies sealed segments and event linkage across a segment boundary", async () => {
  const ajv = validator();
  const vector = await readJson("vectors", "cross-segment-chain.json");
  const segmentValidator = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/ledger-segment-manifest.schema.json");
  const chainHeadValidator = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/chain-head.schema.json");
  const allEvents = [];
  for (const [index, segment] of vector.segments.entries()) {
    assert.equal(segmentValidator(segment.manifest), true, ajv.errorsText(segmentValidator.errors));
    assert.equal(Buffer.byteLength(segment.jsonl), segment.manifest.byte_length);
    assert.equal(sha256(segment.jsonl), segment.manifest.segment_file_hash);
    assert.equal(segment.manifest.previous_segment_file_hash, index ? vector.segments[index - 1].manifest.segment_file_hash : null);
    const events = segment.jsonl.trimEnd().split("\n").map(JSON.parse);
    assert.equal(events.length, segment.manifest.event_count);
    assert.equal(events[0].event_hash, segment.manifest.first_event_hash);
    assert.equal(events.at(-1).event_hash, segment.manifest.final_event_hash);
    allEvents.push(...events);
  }
  for (const [index, event] of allEvents.entries()) {
    assert.equal(event.event_sequence, index + 1);
    assert.equal(event.previous_event_hash, index ? allEvents[index - 1].event_hash : null);
  }
  assert.equal(chainHeadValidator(vector.chain_head), true, ajv.errorsText(chainHeadValidator.errors));
  assert.equal(vector.chain_head.event_hash, allEvents.at(-1).event_hash);
});

test("reproduces every defined self-digest identity", async () => {
  const ajv = validator();
  const vector = await readJson("vectors", "self-digest-identities.json");
  assert.equal(sha256(vector.prompt_template.identity), vector.prompt_template.digest);
  const promptValidator = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/prompt-template.schema.json");
  assert.equal(promptValidator(vector.prompt_template.complete_record), true, ajv.errorsText(promptValidator.errors));
  assert.equal(hashEvent({ ...vector.provenance_event.identity, event_hash: vector.provenance_event.digest }), vector.provenance_event.digest);
  assert.equal(sha256(vector.protected_record.identity), vector.protected_record.digest);
  assert.equal(releaseMerkleRoot(vector.release_manifest.merkle_identity), vector.release_manifest.digest);
  assert.ok(vector.release_manifest.excluded_paths.includes("release-manifest.json"));
  for (const [key, schemaName] of [["contribution_map", "contribution-map"], ["registration_policy_profile", "registration-policy-profile"], ["human_authorship_record", "human-authorship-record"], ["harp_export_manifest", "harp-export-manifest"]]) {
    assert.equal(sha256(vector[key].identity), vector[key].digest, key);
    const validate = ajv.getSchema(`https://thinkloom.app/schemas/provenance/1.0/${schemaName}.schema.json`);
    assert.equal(validate(vector[key].complete_record), true, `${key}: ${ajv.errorsText(validate.errors)}`);
  }
});

test("validates key recovery materials and sanitized non-mutating export disclosure", async () => {
  const ajv = validator();
  const protectedVector = await readJson("vectors", "protected-record-and-key-rotation.json");
  for (const [schemaName, field] of [["encrypted-key-envelope", "device_key_envelope"], ["recovery-key-envelope", "recovery_key_envelope"], ["project-key-manifest", "project_key_manifest"], ["record-envelope", "initial_envelope"], ["record-envelope", "rotated_envelope"]]) {
    const validate = ajv.getSchema(`https://thinkloom.app/schemas/provenance/1.0/${schemaName}.schema.json`);
    assert.equal(validate(protectedVector[field]), true, `${schemaName}: ${ajv.errorsText(validate.errors)}`);
  }
  assert.equal(protectedVector.recovery_key_envelope.key_derivation.algorithm, "argon2id");

  const sanitizedVector = await readJson("vectors", "sanitized-export.json");
  const validateSanitized = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/sanitized-export-manifest.schema.json");
  assert.equal(validateSanitized(sanitizedVector.manifest), true, ajv.errorsText(validateSanitized.errors));
  assert.equal(sha256(sanitizedVector.manifest.omission_rules), sanitizedVector.manifest.rules_sha256);
  assert.equal(sanitizedVector.manifest.completeness_claim, "selective_disclosed_subset");
  assert.deepEqual(sanitizedVector.manifest.omission_rules.map((rule) => rule.category), [
    "private_conversation",
    "rejected_model_output",
    "credential_authorization_material",
    "personal_identifier",
    "internal_path",
    "provider_metadata_not_required",
    "protected_source_body",
  ]);
  for (const rule of sanitizedVector.manifest.omission_rules) {
    const { disclosure_sha256, ...identity } = rule;
    assert.equal(sha256(identity), disclosure_sha256);
  }
  assert.equal(sanitizedVector.source_chain_head_before, sanitizedVector.source_chain_head_after);
  assert.ok(sanitizedVector.exported_record_count < sanitizedVector.source_record_count);
});

test("rebuilds deterministic derived-index evidence independent of input order", async () => {
  const ajv = validator();
  const vector = await readJson("vectors", "deterministic-derived-index.json");
  const sortContent = (items) => [...items].sort((left, right) => {
    const leftKey = canonicalize(left);
    const rightKey = canonicalize(right);
    return leftKey < rightKey ? -1 : leftKey > rightKey ? 1 : 0;
  });
  for (const order of vector.input_orders) assert.deepEqual(sortContent(order), vector.deterministic_content);
  assert.equal(sha256(vector.deterministic_content), vector.manifest.content_sha256);
  assert.equal(sha256(vector.configuration), vector.manifest.generator.configuration_sha256);
  const validate = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/derived-index-manifest.schema.json");
  assert.equal(validate(vector.manifest), true, ajv.errorsText(validate.errors));
});

test("covers every verification status and finding severity", async () => {
  const ajv = validator();
  const vector = await readJson("vectors", "verification-statuses.json");
  const validate = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/verification-report.schema.json");
  for (const report of vector.reports) assert.equal(validate(report), true, ajv.errorsText(validate.errors));
  assert.deepEqual([...new Set(vector.reports.map(({ status }) => status))].sort(), [...vector.required_statuses].sort());
  assert.deepEqual([...new Set(vector.reports.flatMap(({ findings }) => findings.map(({ severity }) => severity)))].sort(), [...vector.required_severities].sort());
});

test("validates backup and release manifests bound to one chain head", async () => {
  const ajv = validator();
  const vector = await readJson("vectors", "backup-and-release-manifests.json");
  const validateBackup = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/backup-manifest.schema.json");
  const validateRelease = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/release-manifest.schema.json");
  assert.equal(validateBackup(vector.backup_manifest), true, ajv.errorsText(validateBackup.errors));
  assert.equal(validateRelease(vector.release_manifest), true, ajv.errorsText(validateRelease.errors));
  assert.equal(vector.backup_manifest.source_chain_head, vector.release_manifest.source_chain_head);
  assert.equal(releaseMerkleRoot(vector.release_manifest.files), vector.release_manifest.release_files_merkle_root);
});
test("keeps canonical assertions immutable while evaluations change over time", async () => {
  const ajv = validator();
  const vector = await readJson("vectors", "assertion-envelope-and-invalidation.json");
  const validateAssertion = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/provenance-assertion.schema.json");
  const validateEvaluation = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/assertion-evaluation.schema.json");

  assert.equal(validateAssertion(vector.assertion), true, ajv.errorsText(validateAssertion.errors));
  assert.equal(sha256(vector.assertion_identity), vector.assertion.assertion_sha256);
  const identity = { ...vector.assertion };
  delete identity.assertion_sha256;
  assert.deepEqual(identity, vector.assertion_identity);
  const validateEvent = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/provenance-event.schema.json");
  assert.equal(validateEvent(vector.assertion_recording_event), true, ajv.errorsText(validateEvent.errors));
  assert.equal(hashEvent(vector.assertion_recording_event), vector.assertion_recording_event.event_hash);
  assert.notEqual(vector.assertion.source_anchor.event_id, vector.assertion_recording_event.event_id);
  assert.ok(vector.assertion_recording_event.event_sequence > vector.assertion.source_anchor.event_sequence);
  assert.equal(vector.assertion_recording_event.outputs[0].record_id, vector.assertion.assertion_id);
  assert.equal(vector.assertion_recording_event.outputs[0].sha256, vector.assertion.assertion_sha256);

  const expectedStatuses = ["exact", "degraded", "refused", "stale", "unverified"];
  assert.deepEqual([...new Set(vector.evaluations.map(({ status }) => status))].sort(), expectedStatuses.sort());
  for (const [index, evaluation] of vector.evaluations.entries()) {
    assert.equal(validateEvaluation(evaluation), true, `${evaluation.status}: ${ajv.errorsText(validateEvaluation.errors)}`);
    assert.equal(evaluation.assertion_id, vector.assertion.assertion_id);
    assert.equal(evaluation.assertion_sha256, vector.assertion.assertion_sha256);
    assert.deepEqual(evaluation.dependency_results.map(({ dependency_id }) => dependency_id).sort(), vector.assertion.dependencies.map(({ dependency_id }) => dependency_id).sort());
    for (const result of evaluation.dependency_results) {
      assert.equal(result.evidence_class, vector.assertion.dependencies.find(({ dependency_id }) => dependency_id === result.dependency_id).evidence_class);
    }
    assert.deepEqual(Object.keys(evaluation.confidence).sort(), ["authorship", "chronology", "completeness", "derivation", "identity", "integrity"]);
    assert.equal(Object.hasOwn(evaluation.confidence, "score"), false);
    if (index) assert.equal(evaluation.supersedes_evaluation_id, vector.evaluations[index - 1].evaluation_id);
  }

  const exact = vector.evaluations.find(({ status }) => status === "exact");
  assert.equal(exact.boundary, null);
  assert.equal(Object.values(exact.confidence).some((value) => ["degraded", "unverified"].includes(value)), false);
  assert.equal(exact.dependency_results.filter(({ evidence_class }) => evidence_class !== "shadow").every(({ status }) => status === "valid"), true);
  assert.ok(exact.dependency_results.some(({ evidence_class, status }) => evidence_class === "shadow" && status === "not_evaluated"));
  assert.ok(vector.evaluations.find(({ status }) => status === "stale").dependency_results.some(({ status }) => status === "changed"));

  for (const [name, invalid] of Object.entries(vector.forbidden_exact_cases)) {
    const validate = ["unknown_provenance", "unknown_artifact_generation"].includes(name) ? validateAssertion : validateEvaluation;
    assert.equal(validate(invalid), false, `${name} must not validate`);
  }
  assert.doesNotMatch(JSON.stringify(vector), /human.{0,20}percentage|authorship.{0,20}score/i);
});

test("keeps assertion status, reason, confidence, and evidence semantics registry-driven", async () => {
  const vector = await readJson("vectors", "assertion-envelope-and-invalidation.json");
  const registries = Object.fromEntries(await Promise.all(catalog.registries.map(async (entry) => [entry.name, await readJson(entry.file)])));
  const statusRegistry = registries["assertion-evaluation-statuses"];
  const reasonRegistry = registries["assertion-reason-codes"];
  const evidenceRegistry = registries["assertion-evidence-classes"];
  const lifecycleRegistry = registries["assertion-lifecycle-phases"];

  assert.ok(lifecycleRegistry.entries.some(({ code }) => code === vector.assertion.lifecycle_phase));
  assert.deepEqual(vector.consumer_decisions, statusRegistry.entries.map(({ code: status, consumer_action: action }) => ({ status, action })));
  for (const evaluation of vector.evaluations) {
    const reason = reasonRegistry.entries.find(({ code }) => code === evaluation.reason_code);
    assert.ok(reason, evaluation.reason_code);
    assert.ok(reason.permitted_statuses.includes(evaluation.status), `${evaluation.reason_code} cannot explain ${evaluation.status}`);
  }
  assert.deepEqual(evidenceRegistry.entries.map(({ code }) => code), ["mandatory_live", "mandatory_retained", "advisory", "shadow"]);
  assert.deepEqual(evidenceRegistry.entries.map(({ exact_effect }) => exact_effect), ["required", "required", "may_degrade", "no_authority"]);
});
test("enforces composition-operation origin rules and refuses unknown exact classifications", async () => {
  const ajv = validator();
  const vector = await readJson("vectors", "composition-and-harp-classification.json");
  const validateOperation = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/composition-operation.schema.json");
  const validateSegment = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/expression-segment.schema.json");
  const validateHarp = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/human-authorship-record.schema.json");

  for (const [name, operation] of Object.entries(vector.operations)) {
    assert.equal(validateOperation(operation), true, `${name}: ${ajv.errorsText(validateOperation.errors)}`);
  }
  assert.equal(vector.operations.paste.recorded_origin_kind, "imported_or_pasted");
  assert.notEqual(vector.operations.paste.recorded_origin_kind, "recorded_direct_human_input");
  assert.equal(vector.operations.ai_acceptance.recorded_origin_kind, "accepted_ai_output");
  assert.ok(vector.operations.ai_acceptance.invocation_id);
  assert.ok(vector.operations.ai_acceptance.disposition_id);

  assert.equal(validateSegment(vector.exact_expression_segment), true, ajv.errorsText(validateSegment.errors));
  assert.equal(validateHarp(vector.exact_harp), true, ajv.errorsText(validateHarp.errors));
  for (const [name, instance] of Object.entries(vector.forbidden_exact_segments)) {
    assert.equal(validateSegment(instance), false, `segment ${name} must not validate as exact`);
  }
  for (const [name, instance] of Object.entries(vector.forbidden_exact_harps)) {
    assert.equal(validateHarp(instance), false, `HARP ${name} must not validate as exact`);
  }
  assert.deepEqual(vector.independent_dimensions, ["recorded_origin", "transformation", "selection_arrangement", "evidentiary_evaluation", "suggested_registration_treatment"]);
  for (const field of vector.prohibited_claim_fields) assert.equal(Object.hasOwn(vector.exact_harp, field), false, field);
});

test("reproduces complete ordered contribution maps independently of input order", async () => {
  const ajv = validator();
  const vector = await readJson("vectors", "contribution-map-determinism.json");
  const validate = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/contribution-map.schema.json");
  assert.equal(validate(vector.deterministic_map), true, ajv.errorsText(validate.errors));

  const sortSegments = (segments) => [...segments].sort((left, right) => left.segment_sequence - right.segment_sequence || (left.segment_id < right.segment_id ? -1 : left.segment_id > right.segment_id ? 1 : 0));
  const canonicalOrders = vector.input_orders.map((segments) => canonicalize(sortSegments(segments)));
  assert.equal(canonicalOrders[0], canonicalOrders[1]);
  assert.equal(sha256(canonicalize(vector.deterministic_map)), vector.canonical_map_sha256);
  const { deterministic_map: map } = vector;
  const mapIdentity = { ...map };
  delete mapIdentity.contribution_map_sha256;
  assert.equal(sha256(mapIdentity), map.contribution_map_sha256);
  assert.equal(map.coverage.coverage_status, "complete");
  assert.equal(map.coverage.recorded_positions, map.coverage.denominator);

  let cursor = 0;
  for (const segment of map.segments) {
    assert.equal(segment.range.coordinate_system, "unicode_scalar");
    assert.equal(segment.range.start, cursor, `${segment.segment_id} must begin at the prior end`);
    assert.ok(segment.range.end > segment.range.start);
    assert.equal(segment.range.end - segment.range.start, segment.normalized_unicode_scalar_length);
    cursor = segment.range.end;
  }
  assert.equal(cursor, map.coverage.denominator);
  assert.ok(map.layers.includes("selection_arrangement"));
  assert.ok(map.layers.includes("recorded_origin"));
});

test("binds HARP to one exact deposit and makes later revisions stale without rewriting history", async () => {
  const ajv = validator();
  const vector = await readJson("vectors", "harp-deposit-staleness.json");
  const validateDeposit = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/deposit-snapshot.schema.json");
  const validateHarp = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/human-authorship-record.schema.json");
  const validateManifest = ajv.getSchema("https://thinkloom.app/schemas/provenance/1.0/harp-export-manifest.schema.json");
  assert.equal(validateDeposit(vector.deposit_snapshot), true, ajv.errorsText(validateDeposit.errors));
  assert.equal(validateHarp(vector.current_harp), true, ajv.errorsText(validateHarp.errors));
  assert.equal(validateHarp(vector.stale_after_edit), true, ajv.errorsText(validateHarp.errors));
  assert.equal(validateManifest(vector.export_manifest), true, ajv.errorsText(validateManifest.errors));

  assert.equal(vector.current_harp.deposit.deposit_sha256, vector.deposit_snapshot.deposit_sha256);
  assert.equal(vector.current_harp.deposit.manuscript_revision_id, vector.deposit_snapshot.manuscript_revision_id);
  assert.equal(vector.current_harp.cpl_binding.chain_head, vector.deposit_snapshot.cpl_chain_head);
  assert.equal(vector.current_harp.applicability_status, "current");
  assert.equal(vector.stale_after_edit.applicability_status, "stale");
  assert.notEqual(vector.stale_after_edit.deposit.manuscript_revision_sha256, vector.current_harp.deposit.manuscript_revision_sha256);
  assert.equal(vector.current_harp.suggested_registration_language.user_approved, true);

  for (const [record, digestField] of [[vector.current_harp, "harp_sha256"], [vector.stale_after_edit, "harp_sha256"], [vector.export_manifest, "manifest_sha256"]]) {
    const identity = { ...record };
    delete identity[digestField];
    assert.equal(sha256(identity), record[digestField]);
  }
  const fileByRole = Object.fromEntries(vector.export_manifest.files.map((file) => [file.role, file]));
  assert.equal(fileByRole.machine_readable_harp.sha256, vector.current_harp.harp_sha256);
  assert.equal(fileByRole.deposit_copy.sha256, vector.deposit_snapshot.deposit_sha256);
  assert.equal(vector.export_manifest.harp_sha256, vector.current_harp.harp_sha256);
  assert.equal(vector.export_manifest.deposit_sha256, vector.deposit_snapshot.deposit_sha256);
  assert.doesNotMatch(JSON.stringify({ claim_summary: vector.current_harp.claim_summary, coverage: vector.current_harp.coverage, language: vector.current_harp.suggested_registration_language }), /human\s*(?:percentage|%)|ai\s*(?:percentage|%)|copyright verified|originality proven|authorship certified/i);
});