import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL(path, import.meta.url), "utf8");

test("freezes contribution maps from native composition revisions and exact deposits", async () => {
  const [native, map, modules] = await Promise.all([
    source("../src-tauri/src/lib.rs"),
    source("../src-tauri/src/provenance/contribution_map.rs"),
    source("../src-tauri/src/provenance/mod.rs"),
  ]);

  assert.match(modules, /pub mod contribution_map;/);
  assert.match(native, /fn freeze_contribution_map/);
  assert.match(native, /fn load_contribution_map/);
  assert.match(native, /contribution_map::freeze_current/);
  assert.match(native, /"deposits"/);
  assert.match(map, /record_type: "deposit-snapshot"/);
  assert.match(map, /record_type: "contribution-map"/);
  assert.match(map, /record_type: "contribution-map-bundle"/);
  assert.match(map, /deposit_sha256/);
  assert.match(map, /manuscript_revision_id/);
});

test("provides complete scalar coverage with deterministic structural locators and ancestry", async () => {
  const map = await source("../src-tauri/src/provenance/contribution_map.rs");

  assert.match(map, /coordinate_system: "unicode_scalar"/);
  assert.match(map, /validate_source_coverage/);
  assert.match(map, /validate_map_coverage/);
  assert.match(map, /merge_source_spans/);
  assert.match(map, /ancestry_segment_id\.clone\(\)/);
  assert.match(map, /chapter: Some/);
  assert.match(map, /paragraph: Some/);
  assert.match(map, /page: Some/);
  assert.doesNotMatch(map, /locale|Collator|toLocaleString/);
  assert.match(map, /canonical_map_bytes/);
  assert.match(map, /identical_canonical_input_is_byte_identical_for_any_span_order/);
  assert.match(map, /equivalent_adjacent_source_splits_merge_to_the_same_map/);
});

test("separates assertions from origin and exposes every evidence boundary", async () => {
  const [map, schema] = await Promise.all([
    source("../src-tauri/src/provenance/contribution_map.rs"),
    source("../schemas/provenance/v1/contribution-map.schema.json"),
  ]);

  for (const predicate of ["included_in_deposit", "selected_by_human", "arranged_by_human"])
    assert.match(map, new RegExp(predicate));
  assert.match(map, /assertion_evaluations/);
  for (const status of ["stale", "degraded", "unverified", "unattested"])
    assert.match(map, new RegExp(`"${status}"`));
  assert.match(map, /not a human-authorship percentage/);
  assert.match(schema, /denominator_unit/);
  assert.match(schema, /denominator_definition/);
  assert.match(map, /frozen_map_becomes_stale_after_a_later_composition_revision/);
  assert.match(map, /verified_frozen_map_is_exact_and_reused_for_identical_input/);
  assert.match(map, /missing_frozen_deposit_is_visibly_degraded/);
  assert.match(map, /inconclusive_source_verification_is_visibly_unverified/);
});
