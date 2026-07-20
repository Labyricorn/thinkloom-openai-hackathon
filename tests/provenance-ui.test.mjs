import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL(path, import.meta.url), "utf8");

test("connects the milestone 9 CPL explorer and HARP preparation views", async () => {
  const [app, workspace, native, modules] = await Promise.all([
    source("../src/Thinkloom.tsx"),
    source("../src/ProvenanceWorkspace.tsx"),
    source("../src-tauri/src/lib.rs"),
    source("../src-tauri/src/provenance/mod.rs"),
  ]);

  assert.match(app, /import ProvenanceWorkspace/);
  assert.match(app, /<ProvenanceWorkspace onNotice=/);
  assert.match(modules, /pub mod explorer;/);
  assert.match(native, /fn load_cpl_explorer/);
  assert.match(native, /fn prepare_harp/);
  assert.match(workspace, /CPL explorer/);
  assert.match(workspace, /Prepare HARP/);
  assert.match(workspace, /load_cpl_explorer/);
  assert.match(workspace, /prepare_harp/);
});

test("renders native evidence, lineage, evaluation, and verification boundaries", async () => {
  const workspace = await source("../src/ProvenanceWorkspace.tsx");

  for (const requirement of [
    "Native CPL verification",
    "Composition timeline",
    "Expression lineage",
    "Assertions and current evaluations",
    "Underlying records",
    "Exact, degraded, stale, and unverified",
    "Statement → assertion → evaluation → record",
  ]) assert.match(workspace, new RegExp(requirement));

  assert.doesNotMatch(workspace, /human_percentage/);
  assert.match(workspace, /not a human-authorship score/);
});

test("keeps evidence categories and legal limits explicit", async () => {
  const workspace = await source("../src/ProvenanceWorkspace.tsx");

  for (const category of [
    "Evidence fact",
    "User declaration",
    "Derived classification",
    "Suggested application language",
    "Legal determination not made",
  ]) assert.match(workspace, new RegExp(category));

  assert.match(workspace, /does not determine legal authorship, originality, copyrightability, ownership, or registrability/);
});

test("requires explicit review and approval before native HARP generation", async () => {
  const workspace = await source("../src/ProvenanceWorkspace.tsx");

  for (const action of [
    "Freeze or select the exact deposit",
    "Confirm the author identity declaration",
    "Review AI systems used",
    "Review final-text classifications",
    "Resolve or accept evidence boundaries",
    "Preview suggested registration language",
    "Choose the archive and explicitly approve",
    "I explicitly approve HARP generation",
    "Approve and generate HARP",
  ]) assert.match(workspace, new RegExp(action));

  assert.match(workspace, /userApproved: true/);
  assert.match(workspace, /generate_harp/);
  assert.match(workspace, /disabled=\{!approved \|\| busy\}/);
});
