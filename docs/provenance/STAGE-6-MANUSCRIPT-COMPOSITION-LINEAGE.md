# Stage 6 — Manuscript composition and lineage

Status: **Complete for Thinkloom 0.5.6**

Milestone/version rule: **Milestone 6 → 0.5.6**

## Delivered boundary

Thinkloom 0.5.6 makes typed native composition commands the authoritative manuscript history. TipTap/ProseMirror `onUpdate` transactions are coalesced at idle, focus-loss, section-change, AI-operation, checkpoint, phase-change, explicit-save, and document-close boundaries. Paste, drop/move, and restoration have explicit boundary signals as well. The drafting and finalization views use the same structured editor; no finalization textarea can bypass CPL capture.

Every command supplies the exact prior and resulting manuscript. Native replay rejects stale preimages and no-op commands before an event is committed. A Unicode-scalar prefix/suffix diff derives the changed range without using edit counts, elapsed time, word counts, retained-word ratios, or any other originality heuristic.

## Recorded origins and lineage

Inserted expression is classified as one of:

- recorded direct human input;
- human expressive input via transcription;
- accepted AI output;
- imported or pasted material;
- system restoration; or
- unattested expression.

Paste is explicitly mapped to `imported_or_pasted`, never automatically to human authorship. New or legacy manuscript baselines that lack composition evidence are initialized as `unattested`.

The native projection tracks lineage per normalized Unicode scalar and emits complete, non-overlapping expression spans. Each span carries stable ancestry, lineage references, operation references, origin, exact text, and a content digest. When a writer revises AI-origin text, replacement units reference the deleted AI ancestry and operation chain, preserving both the AI preimage and the later human operation.

## AI acceptance

AI acceptance records bind the provider invocation, retained response, accepted scalar ranges, rejected scalar ranges, partial/full disposition, operation, and result manuscript revision. Native validation requires non-empty in-range spans, rejects overlaps, requires complete scalar coverage, and verifies that the partial flag agrees with the rejected disposition.

## Canonical replay and derived files

The immutable `composition-command` record is the authoritative replay input. Each edit also emits `composition-operation`, `composition-content`, `manuscript-revision`, and `expression-segment` records; AI acceptance adds `ai-acceptance-disposition`. SQLite's `composition_projection` is disposable and rebuilt from ledger events and canonical records. `manuscript/manuscript.md` is written only from the resulting native projection.

Composition preparation and its CPL write execute under the same exclusive project writer lock, so concurrent clients cannot commit two edits against the same preimage. The UI additionally serializes composition commands in submission order.

## Acceptance evidence

The Rust suite verifies mixed manual, paste, AI, human-revision, and restoration replay; Unicode-scalar partial-AI ranges; rejection of stale preimages without ledger mutation; exactly-one concurrent preimage commits; and idempotent retries with action-ID conflict detection. Static application tests verify the shared editor path, required boundaries and origins, paste classification, AI range bindings, canonical replay, and cache rebuild integration.

Milestone 6 did not itself produce the contribution-map projection or HARP. The contribution-map projection is delivered by Milestone 7 / 0.5.7; HARP remains assigned to Milestone 8.
