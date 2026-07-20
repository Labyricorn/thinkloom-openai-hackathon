import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL(path, import.meta.url), "utf8");

test("routes both manuscript surfaces through instrumented TipTap transactions", async () => {
  const frontend = await source("../src/Thinkloom.tsx");

  assert.match(frontend, /useEditor\(\{[\s\S]*onUpdate:/);
  assert.match(frontend, /apply_composition_command/);
  assert.match(frontend, /label="Final manuscript editor"/);
  assert.doesNotMatch(frontend, /finalEditor|<textarea[^>]+aria-label="Final manuscript"/);
  assert.match(frontend, /onPasteCapture[\s\S]*origin: "imported_or_pasted"/);
  assert.doesNotMatch(frontend, /onPasteCapture[\s\S]{0,180}recorded_direct_human_input/);
});

test("captures every required coalescing boundary without originality heuristics", async () => {
  const [frontend, composition] = await Promise.all([
    source("../src/Thinkloom.tsx"),
    source("../src-tauri/src/provenance/composition.rs"),
  ]);

  for (const boundary of [
    "idle", "focus_loss", "section_change", "ai_operation", "checkpoint",
    "phase_change", "explicit_save", "document_close",
  ]) assert.match(`${frontend}\n${composition}`, new RegExp(`\\b${boundary}\\b`));

  for (const origin of [
    "recorded_direct_human_input", "human_expressive_input_via_transcription",
    "accepted_ai_output", "imported_or_pasted", "system_restoration", "unattested",
  ]) assert.match(`${frontend}\n${composition}`, new RegExp(origin));

  assert.doesNotMatch(composition, /edit[_ -]?count|elapsed[_ -]?time|word[_ -]?count|retained[_ -]?word[_ -]?ratio/i);
});

test("binds partial AI dispositions and deterministically replays surviving-span lineage", async () => {
  const [frontend, composition, native, writer] = await Promise.all([
    source("../src/Thinkloom.tsx"),
    source("../src-tauri/src/provenance/composition.rs"),
    source("../src-tauri/src/lib.rs"),
    source("../src-tauri/src/provenance/writer.rs"),
  ]);

  assert.match(frontend, /invocationId: project\.generation\.id/);
  assert.match(frontend, /acceptedRanges: \[\{ start: 0, end: acceptedEnd \}\]/);
  assert.match(frontend, /rejectedRanges: acceptedEnd < responseEnd/);
  assert.match(composition, /ai-acceptance-disposition/);
  assert.match(composition, /result_revision_id/);
  assert.match(composition, /record_type == "composition-command"/);
  assert.match(composition, /lineage_reference_ids/);
  assert.match(composition, /RecordedOrigin::Unattested/);
  assert.match(writer, /rebuild_projection_cache/);
  assert.match(native, /manuscript\/manuscript\.md/);
  assert.match(composition, /replays_manual_paste_ai_revision_and_restoration_with_lineage/);
  assert.match(composition, /unicode_scalar_diff_and_partial_ai_ranges_are_exact/);
  assert.match(composition, /refuses_a_stale_preimage_without_committing_an_event/);
  assert.match(composition, /concurrent_edits_against_one_preimage_commit_exactly_once/);
  assert.match(composition, /retries_are_idempotent_and_action_id_reuse_conflicts/);
});
