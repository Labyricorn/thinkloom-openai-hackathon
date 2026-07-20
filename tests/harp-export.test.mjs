import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL(path, import.meta.url), "utf8");

test("creates the six milestone 10 artifacts through the native HARP export service", async () => {
  const [native, exporter, modules, ui] = await Promise.all([
    source("../src-tauri/src/lib.rs"),
    source("../src-tauri/src/provenance/export.rs"),
    source("../src-tauri/src/provenance/mod.rs"),
    source("../src/HarpExportPanel.tsx"),
  ]);

  assert.match(modules, /pub mod export;/);
  assert.match(native, /fn export_harp_artifacts/);
  assert.match(native, /fn verify_harp_sanitized_archive/);
  assert.match(ui, /export_harp_artifacts/);
  assert.match(ui, /verify_harp_sanitized_archive/);

  for (const role of [
    "registration_worksheet",
    "human_readable_harp",
    "machine_readable_harp",
    "deposit_copy",
    "sanitized_supporting_archive",
    "full_private_archive",
  ]) assert.match(exporter, new RegExp(`"${role}"`));
});

test("discloses and hash-binds every required sanitized omission category", async () => {
  const [exporter, generator] = await Promise.all([
    source("../src-tauri/src/provenance/export.rs"),
    source("../scripts/generate-provenance-stage2.mjs"),
  ]);

  for (const category of [
    "private_conversation",
    "rejected_model_output",
    "credential_authorization_material",
    "personal_identifier",
    "internal_path",
    "provider_metadata_not_required",
    "protected_source_body",
  ]) {
    assert.match(exporter, new RegExp(`"${category}"`));
    assert.match(generator, new RegExp(`"${category}"`));
  }
  assert.match(exporter, /retained_binding_sha256/);
  assert.match(exporter, /disclosure_sha256/);
  assert.match(exporter, /rules_sha256/);
  assert.match(exporter, /canonical_digest\(&identity\)/);
});

test("verifies retained files without claiming sanitized completeness", async () => {
  const [exporter, ui] = await Promise.all([
    source("../src-tauri/src/provenance/export.rs"),
    source("../src/HarpExportPanel.tsx"),
  ]);

  assert.match(exporter, /selective_disclosed_subset/);
  assert.match(exporter, /verified_selective/);
  assert.match(exporter, /intentionally incomplete/);
  assert.match(exporter, /sha256_digest\(&bytes\) != binding\["sha256"\]/);
  assert.match(exporter, /export must not mutate the CPL chain/);
  assert.doesNotMatch(exporter, /CplService::new/);
  assert.match(ui, /selective evidence subset/i);
  assert.match(ui, /does not claim the omitted private history is present/i);
  assert.match(ui, /Full private archive/);
  assert.match(ui, /Redact the declared author name/);
});
