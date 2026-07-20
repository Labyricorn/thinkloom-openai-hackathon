# Stage 3 Native CPL Service

Status: **Complete for Thinkloom 0.5.3**

Implementation milestone: **3**

Application release: **Thinkloom 0.5.3**

Schema family: **Provenance 1.0 with the additive 0.5.2 composition/HARP package**

Project-format conformance target: **Thinkloom 0.6.0 / CPL 1.0**

## Outcome

Thinkloom 0.5.3 replaces frontend and monolithic Rust provenance authority with one modular native Composition Provenance Ledger service. The service creates immutable records, assigns contiguous event sequences, writes canonical ledger events, advances the chain head, maintains rebuildable SQLite indexes, performs recovery, and returns structured native verification reports.

The implementation is organized under `src-tauri/src/provenance/`:

```text
canonical.rs
identifiers.rs
records.rs
writer.rs
ledger.rs
recovery.rs
verifier.rs
composition.rs
assertions.rs
projections.rs
harp.rs
export.rs
```

## Canonical identity

The native canonicalization implementation:

- Normalizes JSON keys and string values to Unicode NFC.
- Rejects NFC key collisions.
- Orders object keys using RFC 8785 UTF-16 ordering.
- Emits UTF-8 without a BOM and JSONL with LF delimiters.
- Applies deterministic JSON number rendering, including normalized exponent signs and negative zero.
- Produces lowercase `sha256:` digest strings.
- Defines explicit identity objects for records and events, excluding only their respective self-digest fields.

Native tests reproduce the Stage 2 key-order, NFC, and number vectors. Canonical timestamps use RFC 3339 UTC with exactly millisecond precision. Native identifiers use type prefixes plus a sortable millisecond and per-millisecond sequence component.

## Single-writer transaction

Each mutation receives a stable `client_action_id` and executes while holding an exclusive operating-system file lock for that project. The lock is held only for native provenance mutation and recovery; provider calls and Git work remain outside it.

The writer follows the normative phase sequence:

```text
PREPARED
RECORDS_DURABLE
LEDGER_APPENDED
CHAIN_HEAD_ADVANCED
SQLITE_APPLIED
COMPLETE
```

Unsafe or abandoned operations finish as `QUARANTINED` or `FAILED`. Immutable records are canonicalized and flushed in same-filesystem staging before atomic movement into `records/`. The event sequence is derived from the verified ledger immediately before append and is never reserved by SQLite.

Committed action receipts make identical retries return the original event and record references. Reusing the same action ID with a different canonical command digest is an integrity error.

## Segmented ledger and recovery

The ledger maintains one active JSONL segment and immutable sealed segments. Default rotation thresholds are 10,000 events or 10 MiB. Each sealed manifest binds its previous segment digest, event range, event count, byte length, file digest, and seal timestamp.

Recovery runs under the writer lock and can:

- Remove an unreferenced incomplete active JSONL suffix.
- Complete an interrupted segment-manifest move after validating its digest.
- Advance a chain head that is behind a complete durable event.
- Reconstruct a missing head from the verified final event.
- Quarantine staged or final immutable records that have no committed event.
- Replay committed actions and rebuild SQLite event, record, and idempotency indexes.
- Refuse automatic repair when the head is ahead of the readable ledger or an authoritative contradiction exists.

## Native verification

The verifier returns the normative `VERIFIED`, `VERIFIED_WITH_WARNINGS`, `INCOMPLETE`, `FAILED`, or `UNSAFE` status with scoped findings. It checks:

- Project identity, exact timestamps, contiguous sequences, previous-event links, and event digests.
- Active and sealed segment readability and sealed-manifest bindings.
- Chain-head agreement with the final readable event.
- Safe record paths, canonical record bytes, record identity, and referenced digests.
- Registered assertion and evaluation structure when those record types are present.
- Rebuildable SQLite index agreement, reported only as a warning when stale or unavailable.

React no longer creates provenance hashes, chain heads, authoritative events, evidence-package hashes, or a local “valid” result. It supplies a stable action identifier and renders only the native verification report. Browser fallback explicitly refuses to synthesize an evidence package.

## Verification matrix

The Rust suite injects deterministic termination after every native writer durability boundary:

- Write-intent creation.
- First staged record write.
- Record flush, atomic move, and directory durability step.
- Active-segment and sealed-manifest flushes.
- Segment and manifest moves and next-active-segment creation.
- Ledger append before flush and after flush.
- Chain-head temporary write, atomic replacement, and directory durability step.
- SQLite application and final completion.

For every point, recovery proves that the action is absent and retryable, committed and idempotently discoverable, deterministically completed, or quarantined without false success. Separate tests cover concurrent writers, conflicting retries, cross-segment verification, tamper detection, and SQLite reconstruction.

## Release boundary

Thinkloom 0.5.3 completes Milestone 3 and provides the native CPL service, but remains a v0.5 preview release. It does not add the unambiguous conforming project marker or convert existing preview projects. Milestone 4, released as 0.5.4 under the milestone/version convention, subsequently established that project boundary. The limitation in this Stage 3 record continues to apply to every unmarked project.
