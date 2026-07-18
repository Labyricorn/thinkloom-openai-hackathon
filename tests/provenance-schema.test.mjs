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
  "backup-manifest", "chain-head", "content-reference", "conversation-session", "derived-index-manifest",
  "disposition-revision", "edit-transaction", "encrypted-key-envelope", "idea", "idea-revision",
  "invocation-failure", "invocation-request", "invocation-response", "invocation-state", "invocation-stream-state",
  "invocation-stream-summary", "ledger-segment-manifest", "manuscript-revision", "model-capability-snapshot",
  "model-configuration-snapshot", "project-key-manifest", "project-manifest", "prompt-template",
  "prompt-template-reference", "provenance-event", "provenance-policy", "purge-manifest", "record-envelope",
  "recovery-key-envelope", "release-manifest", "release-state", "sanitized-export-manifest", "text-fragment-reference",
  "transcript-correction", "transcript-normalization", "transcript-turn", "verification-report", "write-intent",
];

test("catalogs every approved Stage 2 schema under Draft 2020-12", () => {
  assert.equal(catalog.catalog_version, "1.0");
  assert.equal(catalog.provenance_schema_version, "1.0");
  assert.equal(catalog.application_version, "0.3.0");
  assert.deepEqual(catalog.compatible_application_versions, ["0.3.0"]);
  assert.equal(catalog.native_writer_conformance, false);
  assert.equal(catalog.dialect, "https://json-schema.org/draft/2020-12/schema");
  assert.deepEqual(catalog.schemas.map(({ name }) => name), requiredSchemas);
  assert.equal(new Set(catalog.schemas.map(({ id }) => id)).size, requiredSchemas.length);
  for (const { schema } of schemaEntries) {
    assert.equal(schema.additionalProperties, false, `${schema.$id} must be closed at its top level`);
  }
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