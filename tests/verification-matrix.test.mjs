import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const source = (relative) => readFile(path.join(root, relative), "utf8");

test("maps every Milestone 11 requirement to executable verification", async () => {
  const files = [
    "tests/provenance-schema.test.mjs",
    "tests/native-cpl.test.mjs",
    "tests/phase1-provenance.test.mjs",
    "tests/composition-provenance.test.mjs",
    "tests/contribution-map.test.mjs",
    "tests/harp-generation.test.mjs",
    "tests/provenance-ui.test.mjs",
    "tests/harp-export.test.mjs",
    "src-tauri/src/provenance/tests.rs",
    "src-tauri/src/provenance/composition.rs",
    "src-tauri/src/provenance/contribution_map.rs",
    "src-tauri/src/provenance/harp.rs",
    "src-tauri/src/provenance/export.rs",
    "src-tauri/src/provenance/phase1.rs",
    "src-tauri/src/provenance/verifier.rs",
    "src/Thinkloom.tsx",
  ];
  const corpus = (await Promise.all(files.map(source))).join("\n");
  const requirements = new Map([
    ["schema fixtures and deterministic regeneration", /(?=[\s\S]*accepts every valid fixture)(?=[\s\S]*rejects every invalid fixture)(?=[\s\S]*deterministic derived-index)/i],
    ["canonical JSON, Unicode, timestamp, path, and JSONL vectors", /(?=[\s\S]*canonical JSON)(?=[\s\S]*canonical timestamps)(?=[\s\S]*repository-relative paths)(?=[\s\S]*JSONL)/i],
    ["duplicate actions and concurrent writers", /(?=[\s\S]*retries_are_idempotent_and_conflicts_are_rejected)(?=[\s\S]*os_writer_lock_serializes_concurrent_actions)/],
    ["failure after every durable write phase", /(?=[\s\S]*every_durable_boundary_recovers_and_retries_safely)(?=[\s\S]*every_segment_rotation_boundary_recovers_without_sequence_loss)/],
    ["segment rotation and cross-segment verification", /rotates_and_verifies_cross_segment_linkage/],
    ["manual typing and deletion", /(?=[\s\S]*typed_records_reconstruct_phase1)(?=[\s\S]*transform_delete)/],
    ["paste and import", /(?=[\s\S]*ImportedOrPasted)(?=[\s\S]*onPasteCapture)/],
    ["human revision of AI-origin material", /transform_revise/],
    ["AI transformation of human material", /transform_ai/],
    ["partial AI acceptance", /unicode_scalar_diff_and_partial_ai_ranges_are_exact/],
    ["selection and arrangement without origin changes", /selection_and_arrangement_preserve_each_source_origin/],
    ["voice transcription without audio persistence", /voice_transcription_retains_text_but_no_audio_reference_or_digest/],
    ["restore and checkpoint lineage", /(?=[\s\S]*replays_manual_paste_ai_revision_and_restoration_with_lineage)(?=[\s\S]*create_checkpoint)/],
    ["unknown and unattested spans", /reports_unattested_coverage_without_calling_it_non_human/],
    ["deposit and HARP staleness", /(?=[\s\S]*binds HARP to one exact deposit)(?=[\s\S]*frozen_map_becomes_stale)/],
    ["complete segment coverage across complex Unicode", /complex_unicode_has_complete_contiguous_segment_coverage/],
    ["native verifier and frontend consistency", /(?=[\s\S]*require_release_verification)(?=[\s\S]*verify_provenance)(?=[\s\S]*VERIFIED_WITH_WARNINGS)/],
    ["sanitized archive disclosure", /discloses and hash-binds every required sanitized omission category/],
    ["release blocking", /release_gate_accepts_only_complete_safe_native_verification/],
  ]);
  for (const [requirement, pattern] of requirements) {
    assert.match(corpus, pattern, `Missing executable coverage: ${requirement}`);
  }
});

test("prohibits scores and affirmative legal conclusions in shipped source", async () => {
  const roots = ["src", "src-tauri/src"];
  const productionFiles = [];
  for (const directory of roots) {
    const entries = await readdir(path.join(root, directory), { recursive: true, withFileTypes: true });
    for (const entry of entries) {
      if (!entry.isFile() || !/\.(?:rs|tsx|ts|css)$/.test(entry.name)) continue;
      productionFiles.push(path.join(entry.parentPath, entry.name));
    }
  }
  const corpus = (await Promise.all(productionFiles.map((file) => readFile(file, "utf8")))).join("\n");
  for (const prohibited of [
    /\b\d+(?:\.\d+)?%\s+human\b/i,
    /human[- ]authorship\s+(?:score|percentage)\s*[:=]\s*\d/i,
    /Thinkloom\s+(?:determines|certifies|proves)\s+(?:legal authorship|originality|copyrightability|ownership|registrability)/i,
    /(?:legal authorship|copyrightability)\s*[:=]\s*["']?(?:yes|verified|valid)/i,
  ]) assert.doesNotMatch(corpus, prohibited);
});

test("uses the same native verification statuses in the frontend release gate", async () => {
  const [records, frontend, verifier, native] = await Promise.all([
    source("src-tauri/src/provenance/records.rs"),
    source("src/Thinkloom.tsx"),
    source("src-tauri/src/provenance/verifier.rs"),
    source("src-tauri/src/lib.rs"),
  ]);
  for (const status of ["VERIFIED", "VERIFIED_WITH_WARNINGS", "INCOMPLETE", "FAILED", "UNSAFE"]) {
    assert.match(frontend, new RegExp(`\\b${status}\\b`));
  }
  assert.match(records, /VerifiedWithWarnings[\s\S]*Incomplete[\s\S]*Failed[\s\S]*Unsafe/);
  assert.match(verifier, /Verified \| VerificationStatus::VerifiedWithWarnings => Ok/);
  assert.match(native, /verify_project\(&root, &manifest\.project_id\)[\s\S]*require_release_verification/);
  assert.doesNotMatch(frontend, /status\s*===\s*["']VALID["']/);
});
