# Stage 5 — Phase 1 CPL routing

Status: **Complete for Thinkloom 0.5.5**

Milestone/version rule: **Milestone 5 → 0.5.5**

## Outcome

Thinkloom 0.5.5 removes the generic `persist_state` provenance route. Phase 1 now submits typed native commands to the CPL service, and the ideation workspace is reconstructed by replaying canonical `phase1-operation` records in ledger order. SQLite's `phase1_projection` table is a disposable cache and can be rebuilt from the ledger and immutable records.

The React state object is no longer written as an authoritative record or SQLite recovery source. Canonical Phase 1 replay covers:

- human typed turns and voice-mediated human turns;
- retained assistant responses and their provider invocation identity;
- session creation, activation, and title revision;
- persona, challenge level, genre, lore, provider context, and cloud-approval changes;
- idea creation, merging, selection/status changes, and every idea revision;
- human or assistant transcript turns appended to the drafting paper;
- manual drafting-paper edits and clearing;
- distillation request, response, and accepted disposition;
- pasted/imported/external material declarations; and
- provider invocation requests, responses, and bounded failures.

## Record model

Every Phase 1 event binds exactly one authoritative `phase1-operation` record containing a validated `Phase1Command`. The same event also binds operation-specific evidence records such as `transcript-turn`, `idea-revision`, `drafting-paper-revision`, `invocation-request`, `invocation-response`, `invocation-failure`, `disposition-revision`, and `source-declaration`.

Typed commands use stable `client_action_id` values and the native writer's existing idempotency, canonicalization, writer lock, staging, ledger, chain-head, and recovery rules. They set `operational_state` to `None`; the former `application-state-snapshot` route is not used by the application.

## Provider lifecycle boundary

Before a model request or provider connectivity request performs network I/O, the runtime commits a `ProviderInvocationRequested` command containing:

- invocation and session identifiers;
- purpose and provider context;
- canonical digests for prompt template, input, and supplied context; and
- the exact request time.

The CPL write completes and releases the exclusive writer lock before provider I/O begins. A successful call is followed by `ProviderInvocationResponded`; an error or unsuccessful connectivity result is followed by `ProviderInvocationFailed` with a bounded summary. A retained assistant turn separately binds the response to the visible conversation.

## Voice and external material

Voice recognition yields a human `transcript-turn` with `input_mode: voice_transcription`. Its companion record fixes `audio_retained` to `false` and `audio_reference` to null. No audio body, path, identifier, or digest is recorded.

Paste handlers prevent an undeclared state-only insertion and instead commit `ExternalContentDeclared` with the retained text, target, resulting text, and the writer-facing declaration. Paste is recorded as external material; it is not automatically classified as human-authored.

## Reconstruction and recovery

`load_phase1_projection` reads canonical ledger events and records. It does not load a serialized React object. Recovery index rebuilding also reconstructs the disposable Phase 1 cache from those canonical inputs. Derived `ideas/ideas.json` and `conversations/sessions.json` files are convenience mirrors, not replay authority.

Focused native tests prove:

- typed records reconstruct Phase 1 after deleting the projection cache;
- reconstruction does not require `application-state-snapshot`;
- a provider request is durable before simulated I/O and another writer can proceed between request and outcome; and
- voice transcription retains text without an audio reference or audio digest.

Static acceptance tests additionally ensure the frontend uses the typed command/projection routes, all required command and evidence families exist, and response/failure recording follows provider I/O.

## Boundary retained

This milestone instruments Phase 1 only. Manuscript transaction capture, surviving-expression lineage, partial AI acceptance ranges, contribution maps, HARP generation, and dedicated CPL/HARP interfaces remain assigned to later milestones. Thinkloom 0.5.5 does not generate HARP, infer copyrightability, or convert unmarked preview projects.
