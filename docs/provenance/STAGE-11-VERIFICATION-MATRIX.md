# Stage 11 — Verification matrix

Status: **Complete for Thinkloom 0.5.11**

Milestone/version rule: **Milestone 11 → 0.5.11**

## Release rule

Release finalization is a native security boundary. Immediately before changing `project.json`, appending `RELEASE_FINALIZED`, committing, or tagging, Thinkloom runs the authoritative native CPL verifier. Only `VERIFIED` and `VERIFIED_WITH_WARNINGS` may proceed. `INCOMPLETE`, `FAILED`, and `UNSAFE` return `RELEASE_VERIFICATION_BLOCKED` without creating release state. The frontend uses the same five native status values and does not optimistically mark a release finalized.

## Executable matrix

| Required scenario | Executable coverage |
| --- | --- |
| Schema fixtures and deterministic regeneration | `provenance-schema.test.mjs`; generated fixture and vector diff gate |
| Canonical JSON, Unicode, timestamps, paths, and JSONL | Schema vector suite plus native canonical and identifier tests |
| Duplicate action and concurrent writer behavior | Native idempotency, conflict, OS-lock, and concurrent-preimage tests |
| Failure after every durable write phase | Native crash injection for record, ledger, chain-head, SQLite, and segment-rotation boundaries |
| Segment rotation and cross-segment verification | Native sealed/active segment linkage and recovery tests |
| Manual typing and deletion | Typed Phase 1 replay and native composition deletion test |
| Paste/import behavior | Instrumented paste signal and imported-origin replay test |
| Human revision of AI-origin material | Native mixed-origin transformation/revision lineage test |
| AI transformation of human material | Native AI-acceptance replacement with source lineage test |
| Partial AI acceptance | Exact accepted/rejected Unicode scalar disposition test |
| Selection and arrangement without changing origin | Origin-preserving native move implementation and mismatch refusal test |
| Voice transcription with no audio persistence | Typed transcription record and no-audio path/digest/file tests |
| Restore and checkpoint lineage | Restoration replay, checkpoint flush, and immutable record tests |
| Unknown and unattested spans | Unverified coverage and boundary tests without negative authorship inference |
| Deposit/HARP staleness | Frozen-map and HARP dependency/revision staleness tests |
| Complete segment coverage across complex Unicode | Combining mark, modifier, ZWJ, flag, CJK, and CRLF scalar coverage test |
| Native verifier/frontend consistency | Shared status-set test plus native and frontend release gates |
| Sanitized archive disclosure | Omission-category, retained-file, binding, and selective-scope tests |
| Prohibited score/legal wording | Production-source scan for numeric human scores and affirmative legal conclusions |
| Packaged Windows end-to-end | Release executable PE check, MSI and NSIS signature/version checks, and hidden launch smoke test |

`tests/verification-matrix.test.mjs` is the executable index for this table. It fails if a required native or frontend case disappears. `tests/windows-package.test.mjs` is intentionally a post-build Windows gate and is run by `npm run verify:windows-package`.

## Origin-preserving arrangement

A `move` operation may only rearrange the exact existing Unicode scalar multiset. Thinkloom moves the existing lineage units, retaining recorded origin, ancestry, lineage references, and prior operation references, while appending the move operation identity. A move that inserts, deletes, or substitutes content is rejected before any CPL event is committed.

## Interpretation boundary

The matrix verifies integrity, chronology, replay, origin records, transformation records, coverage, disclosure, and release safety. It does not produce a human-authorship score or determine legal authorship, originality, copyrightability, ownership, or registrability.
