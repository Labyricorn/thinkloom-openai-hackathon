import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL(path, import.meta.url), "utf8");

test("generates approved deterministic HARP artifacts through the native CPL service", async () => {
  const [native, harp, modules] = await Promise.all([
    source("../src-tauri/src/lib.rs"),
    source("../src-tauri/src/provenance/harp.rs"),
    source("../src-tauri/src/provenance/mod.rs"),
  ]);

  assert.match(modules, /pub mod harp;/);
  assert.match(native, /fn generate_harp/);
  assert.match(native, /fn load_harp/);
  assert.match(native, /harp::generate_current/);
  assert.match(harp, /HARP_GENERATION_APPROVED/);
  assert.match(harp, /HARP_GENERATED/);
  assert.match(harp, /human-authorship-record/);
  assert.match(harp, /harp-export-manifest/);
  assert.match(harp, /harp-generation-bundle/);
});

test("emits every milestone 8 report with common binding metadata", async () => {
  const harp = await source("../src-tauri/src/provenance/harp.rs");

  for (const artifact of [
    "human-authorship-summary.md",
    "final-text-contribution-map.svg",
    "representative-transformations.md",
    "ai-system-disclosure.md",
    "coverage-and-limitations.md",
    "registration-language.md",
    "harp.json",
    "verification-report.json",
    "supporting-archive-manifest.json",
  ]) assert.match(harp, new RegExp(artifact.replaceAll(".", "\\.")));

  for (const binding of [
    "deposit_sha256",
    "manuscript_revision_id",
    "cpl_chain_head",
    "cpl_event_sequence",
    "harp_schema_version",
    "harp_generator_version",
    "application_version",
    "policy_profile_version",
    "policy_retrieval_date",
    "sanitization_profile",
    "legal_scope_statement",
  ]) assert.match(harp, new RegExp(binding));
});

test("does not use an LLM, a legal classifier, or a human percentage", async () => {
  const harp = await source("../src-tauri/src/provenance/harp.rs");

  assert.doesNotMatch(harp, /generate_text\(/);
  assert.doesNotMatch(harp, /reqwest/);
  assert.doesNotMatch(harp, /human_percentage/);
  assert.match(harp, /not a human-authorship percentage/);
  assert.match(harp, /Copyright Office/);
});

test("binds staleness to manuscript, deposit, policy, assertion, and dependency changes", async () => {
  const harp = await source("../src-tauri/src/provenance/harp.rs");

  for (const reason of [
    "manuscript_revision_changed",
    "deposit_digest_changed",
    "policy_profile_changed",
    "assertion_set_changed",
    "dependency_set_changed",
  ]) assert.match(harp, new RegExp(reason));
  assert.match(harp, /fn dependency_changes_are_stale/);
  assert.match(harp, /applicability_status.*stale/s);
});
