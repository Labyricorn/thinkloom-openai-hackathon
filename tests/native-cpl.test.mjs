import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const rustModules = [
  "canonical",
  "identifiers",
  "records",
  "writer",
  "ledger",
  "recovery",
  "verifier",
  "composition",
  "assertions",
  "projections",
  "harp",
  "export",
];

test("places all authoritative provenance operations behind the native CPL service", async () => {
  const [frontend, lib, moduleSources] = await Promise.all([
    readFile(new URL("../src/Thinkloom.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8"),
    Promise.all(rustModules.map((name) => readFile(new URL(`../src-tauri/src/provenance/${name}.rs`, import.meta.url), "utf8"))),
  ]);
  const native = moduleSources.join("\n");

  assert.match(lib, /pub mod provenance/);
  assert.match(frontend, /apply_phase1_command/);
  assert.doesNotMatch(frontend, /persistNativeState|persist_state/);
  assert.match(frontend, /verify_provenance/);
  assert.match(frontend, /Native integrity verification/);
  assert.doesNotMatch(frontend, /previousHash|manuscriptHash|const hash\s*=|event\.hash/);
  assert.doesNotMatch(frontend, /History verified:.*linked events/);
  assert.doesNotMatch(frontend, /provenanceChainHead:\s*project/);

  for (const phase of ["PREPARED", "RECORDS_DURABLE", "LEDGER_APPENDED", "CHAIN_HEAD_ADVANCED", "SQLITE_APPLIED", "COMPLETE", "QUARANTINED", "FAILED"]) {
    assert.match(native, new RegExp(phase));
  }
  for (const capability of ["client_action_id", "LockFileEx", "event_sequence", "SegmentManifest", "VerificationReport", "canonicalize", "normalize_nfc", "RecoveryClassification"]) {
    assert.match(native, new RegExp(capability));
  }
});

test("keeps milestone-three crash and concurrency acceptance tests executable", async () => {
  const tests = await readFile(new URL("../src-tauri/src/provenance/tests.rs", import.meta.url), "utf8");
  for (const boundary of [
    "IntentPrepared",
    "FirstRecordStaged",
    "RecordFlushed",
    "RecordMoved",
    "RecordDirectorySynced",
    "LedgerAppendBeforeFlush",
    "LedgerFlushed",
    "ChainHeadTemporaryWritten",
    "ChainHeadReplaced",
    "SegmentManifestFlushed",
    "SegmentMoved",
    "NewActiveSegmentCreated",
  ]) {
    assert.match(tests, new RegExp(boundary));
  }
  assert.match(tests, /os_writer_lock_serializes_concurrent_actions/);
  assert.match(tests, /retries_are_idempotent_and_conflicts_are_rejected/);
  assert.match(tests, /recovery_rebuilds_sqlite_from_authoritative_events/);
});
