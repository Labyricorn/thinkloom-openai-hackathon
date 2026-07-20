import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

test("establishes the exact CPL 1.0 project marker and conforming layout", async () => {
  const [format, native, writer] = await Promise.all([
    readFile(new URL("../src-tauri/src/project_format.rs", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/provenance/writer.rs", import.meta.url), "utf8"),
  ]);

  for (const exact of [
    'PROJECT_FORMAT: &str = "thinkloom-cpl"',
    'PROJECT_FORMAT_VERSION: &str = "1.0"',
    'PROVENANCE_CONFORMANCE: &str = "cpl-1.0"',
  ]) assert.match(format, new RegExp(exact.replaceAll(/[.*+?^${}()|[\]\\]/g, "\\$&")));

  for (const path of ["records", "provenance/ledger/active", "provenance/ledger/sealed", "reports", ".app/locks", ".app/temp", ".app/recovery"]) {
    assert.match(format, new RegExp(path.replaceAll(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
  assert.match(native, /project_format: project_format::PROJECT_FORMAT/);
  assert.match(native, /project_format_version: project_format::PROJECT_FORMAT_VERSION/);
  assert.match(native, /provenance_conformance: project_format::PROVENANCE_CONFORMANCE/);
  assert.match(writer, /\.app\/locks/);
  assert.match(writer, /\.app\/temp\/staging/);
  assert.doesNotMatch(`${native}\n${writer}`, /\.thinkloom/);
});

test("classifies legacy projects before recovery and exposes preservation-only controls", async () => {
  const [format, native, frontend] = await Promise.all([
    readFile(new URL("../src-tauri/src/project_format.rs", import.meta.url), "utf8"),
    readFile(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8"),
    readFile(new URL("../src/Thinkloom.tsx", import.meta.url), "utf8"),
  ]);

  assert.match(format, /LegacyPreviewReadOnly/);
  assert.match(format, /schema_version_without_marker_is_legacy_and_inspection_is_read_only/);
  assert.match(format, /preservation_archive_retains_source_bytes_without_changing_source/);
  assert.match(format, /Not verified, converted, or CPL-conforming/);
  assert.match(format, /LEGACY_ARCHIVE_INSIDE_PROJECT/);

  const inspect = native.indexOf("project_format::inspect_project(&root)");
  const recover = native.indexOf("CplService::new(&root, &manifest.project_id).recover()", inspect);
  assert.ok(inspect >= 0 && recover > inspect, "project inspection must precede CPL recovery");
  assert.match(native, /inspection\.classification != project_format::ProjectClassification::CplConforming/);
  assert.match(native, /set_read_only_project/);
  assert.match(native, /show_project_folder/);
  assert.match(native, /create_legacy_preservation_archive/);
  assert.match(native, /LEGACY_BACKUP_REFUSED/);

  assert.match(frontend, /Legacy preview project/);
  assert.match(frontend, /Show project folder/);
  assert.match(frontend, /Create preservation archive/);
  assert.match(frontend, /No migration was attempted/);
});
