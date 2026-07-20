import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path) => readFile(new URL(path, import.meta.url), "utf8");

test("routes Phase 1 UI state through typed native commands and canonical replay", async () => {
  const [frontend, native, phase1] = await Promise.all([
    source("../src/Thinkloom.tsx"),
    source("../src-tauri/src/lib.rs"),
    source("../src-tauri/src/provenance/phase1.rs"),
  ]);

  assert.match(frontend, /apply_phase1_command/);
  assert.match(frontend, /load_phase1_projection/);
  assert.doesNotMatch(frontend, /persist_state|load_project_state|application-state-snapshot/);
  assert.doesNotMatch(native, /fn persist_state|fn load_project_state|fn record_cpl_action/);

  for (const operation of [
    "SessionCreated", "SessionActivated", "SessionTitleRevised", "PersonaChanged",
    "ChallengeChanged", "GenreChanged", "LoreChanged", "ProviderContextChanged",
    "CloudApprovalChanged", "HumanTurnCreated", "AssistantTurnCreated", "IdeasChanged",
    "DraftingPaperTurnAppended", "DraftingPaperRevised", "ProviderInvocationRequested",
    "ProviderInvocationResponded", "ProviderInvocationFailed", "DistillationDisposed",
    "ExternalContentDeclared",
  ]) assert.match(phase1, new RegExp(operation));

  for (const recordType of [
    "transcript-turn", "conversation-session", "idea-revision", "drafting-paper-revision",
    "invocation-request", "invocation-response", "invocation-failure", "disposition-revision",
    "source-declaration", "voice-transcription",
  ]) assert.match(phase1, new RegExp(recordType));

  assert.match(phase1, /reconstruct_from_events/);
  assert.match(phase1, /record_type == "phase1-operation"/);
  assert.match(phase1, /operational_state: None/);
});

test("records provider intent before I/O and records every outcome afterward", async () => {
  const native = await source("../src-tauri/src/lib.rs");
  const generate = native.slice(native.indexOf("fn generate_text("), native.indexOf("#[cfg_attr(mobile", native.indexOf("fn generate_text(")));
  const request = generate.indexOf("ProviderInvocationRequested");
  const send = generate.indexOf("request.send()");
  const response = generate.indexOf("ProviderInvocationResponded");
  const failure = generate.indexOf("ProviderInvocationFailed");
  assert.ok(request >= 0 && send > request, "request record must precede provider I/O");
  assert.ok(response > send, "response record must follow provider I/O");
  assert.ok(failure > send, "failure record must follow provider I/O");
  assert.match(generate, /No CPL writer lock is held while provider I\/O runs/);
});

test("treats voice transcription as human text without retaining audio", async () => {
  const [frontend, phase1] = await Promise.all([
    source("../src/Thinkloom.tsx"),
    source("../src-tauri/src/provenance/phase1.rs"),
  ]);
  assert.match(frontend, /inputMode.*voice_transcription/s);
  assert.match(phase1, /audio_retained": false/);
  assert.match(phase1, /audio_reference": Value::Null/);
  assert.match(phase1, /voice_transcription_retains_text_but_no_audio_reference_or_digest/);
});
