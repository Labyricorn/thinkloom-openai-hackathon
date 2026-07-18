# Thinkloom Stage 1 Completion Checklist

Status: **Normative traceability and handoff gate**

Stage 1 is complete only when every item below is represented without contradiction in the normative specification and state-machine companion. This checklist also defines the minimum Stage 2 and later implementation test handoff.

## 1. Normative specification gate

| Requirement | Normative location | Status |
|---|---|---|
| Correct tamper-evident claim and trust limitation | Specification §§3, 26 | Complete |
| Authority hierarchy | Specification §4 | Complete |
| Self-contained repository boundaries | Specification §5 | Complete |
| Stable IDs and contiguous event sequences | Specification §6 | Complete |
| Canonical repository paths | Specification §7.1 | Complete |
| UTC timestamp representation | Specification §7.2 | Complete |
| RFC 8785 plus Thinkloom canonicalization rules | Specification §8 | Complete |
| Exact self-hash exclusion requirement | Specification §8 | Complete |
| Immutable records and append-only revisions | Specification §9 | Complete |
| Text range and revision identity | Specification §10 | Complete |
| Meaningful edit transaction boundary | Specification §10 | Complete |
| Minimal/full retention semantics | Specification §11 | Complete |
| Sanitized export separation | Specification §11.3 | Complete |
| Prospective policy changes | Specification §11.4 | Complete |
| Encryption identity | Specification §11.5 | Complete |
| Protected-project portability and recovery | Specification §11.5 | Complete |
| Sensitive SQLite restriction | Specification §11.5 | Complete |
| Single native writer boundary | Specification §12 | Complete |
| Cross-store write-intent phases | Specification §12; State Machines §1 | Complete |
| Ledger authority over SQLite | Specification §§4, 12 | Complete |
| Idempotent `client_action_id` behavior | State Machines §1.2 | Complete |
| Segmented ledger and sealing rules | Specification §13; State Machines §5 | Complete |
| Provider I/O outside the writer lock | Specification §14; State Machines §3 | Complete |
| Stale-context handling | Specification §14; State Machines §3 | Complete |
| Native verification authority | Specification §15 | Complete |
| Verification findings/statuses and gates | Specification §15 | Complete |
| Historical index generator warning behavior | Specification §§15–16 | Complete |
| Deterministic derived-index rules | Specification §16 | Complete |
| Two-stage Git checkpoint without circularity | Specification §17; State Machines §6 | Complete |
| Frozen Git tree/index requirement | Specification §17; State Machines §6 | Complete |
| SQLite online backup | Specification §18 | Complete |
| Staged verified backup import | Specification §18; State Machines §8 | Complete |
| Non-self-referential release state machine | Specification §19; State Machines §7 | Complete |
| Bounded encrypted temporary output | Specification §20; State Machines §4 | Complete |
| Pre-durable-write secret filtering | Specification §21 | Complete |
| No audio retention | Specification §21 | Complete |
| Sanitization versus emergency purge | Specification §22; State Machines §10 | Complete |
| Explicit legacy preview-project policy | Specification §23 | Complete |
| Stage 2 formal schema inventory | Specification §24 | Complete |
| Migration deferred until after 1.0.0 | Specification §§23–24, 26 | Complete |

## 2. Stage 2 schema and fixture handoff

Stage 2 MUST NOT implement application behavior. It produces formal contracts and evidence that those contracts are deterministic.

Required outputs:

- JSON Schema Draft 2020-12 files listed in Specification §24.
- A schema catalog mapping schema IDs, versions, filenames, and compatible application versions.
- Valid fixture for every schema.
- Invalid fixtures covering each required property, enum, pattern, bound, path restriction, and additional-property policy.
- Canonical JSON vectors, including RFC 8785 and Thinkloom NFC preprocessing.
- Timestamp and repository-path vectors.
- Event hash and contiguous-sequence vectors.
- Cross-segment chain and sealed-manifest vectors.
- Self-digest identity vectors for templates, encrypted records, manifests, and other self-hashing objects.
- Minimal and full-private invocation fixtures.
- Sanitized-export omission fixtures.
- Encrypted-envelope, key-envelope, key-rotation, and recovery fixtures.
- Deterministic derived-index fixtures.
- Backup/release manifest and Merkle-root fixtures.
- Verification-report fixtures for every status and severity.

Stage 2 must preserve the distinction between the provenance schema version and the Thinkloom application version.

## 3. Durable-boundary fault-injection matrix

The native implementation and test harness MUST support deterministic termination or injected failure after:

- Write-intent creation
- First staged record write
- Each record flush/fsync
- Each authoritative atomic move
- Parent-directory durability step
- Ledger append before flush
- Ledger flush/fsync
- Chain-head temporary write
- Chain-head replacement
- SQLite operational domain update
- SQLite idempotency-result update
- Segment-manifest write
- Segment sealing move
- New active-segment creation
- Segment-opening event append
- Git source-tree capture
- Source checkpoint commit
- Checkpoint acknowledgment event
- Audit checkpoint commit
- Release source freeze
- Release manifest generation
- Release-file verification
- Release commit
- Release tag creation
- Backup snapshot completion
- Backup archive finalization
- Import extraction and each verification gate

For each boundary, tests MUST prove one of:

1. The operation is absent and safely retryable.
2. The operation is committed and idempotently discoverable.
3. Recovery deterministically completes it.
4. Recovery quarantines it without presenting false success.
5. Authoritative contradiction is reported and editing remains blocked.

## 4. Concurrency and idempotency tests

- Hundreds of concurrent frontend commands serialize without event loss or sequence gaps.
- Duplicate retries with the same canonical command return the original result.
- Reuse of `client_action_id` with a different command is rejected.
- Concurrent provider completions create distinct contiguous events.
- Provider calls do not hold the writer lock.
- Segment rotation cannot race an append.
- Checkpoint and release operations use frozen trees despite continuing edits.
- Clock rollback does not change event ordering.
- Stale OS lock artifacts do not block a project after process death.

## 5. Recovery and integrity tests

- Truncated active JSONL line
- Complete ledger event ahead of chain head
- Chain head ahead of ledger
- SQLite behind the ledger
- SQLite ahead of the ledger
- Missing or corrupt write-intent database
- Durable records without an event
- Event referencing a missing record
- Duplicate event sequence
- Event-sequence gap across a segment boundary
- Duplicate segment number
- Corrupted sealed segment or manifest
- Orphaned staging directory
- Stale or nondeterministic derived index
- Historical generator unavailable
- Unsupported authoritative schema
- Git unavailable or damaged while ledger remains valid
- Disk full and permission loss at every write phase
- Antivirus/synchronization lock during replacement

## 6. Invocation and lineage tests

- Successful local and cloud invocation
- Failed provider invocation
- Cancellation before and during streaming
- Partially streamed response
- Output spool limit exceeded
- Concurrent invocation spools and aggregate limit
- Crash with recoverable encrypted spool
- Spool key missing or corrupt
- Stale spool cleanup after the configured period
- No provider-resume claim for an unsupported provider
- Response completion after manuscript context changes
- Revalidation, rebase, explicit stale acceptance, and rejection paths
- Full, partial, and rejected disposition revisions
- Manual edit transactions after generated-text acceptance
- Transcript correction after downstream use
- Range verification across UTF-8, Unicode scalar, UTF-16, and editor coordinates

## 7. Privacy, secret, and encryption tests

- Minimal retention omits every prohibited raw field.
- Full private retention contains allowed records but no credentials.
- Minimal-to-full and full-to-minimal changes are prospective.
- Sanitized export does not mutate project storage.
- Sanitized export discloses every omission class.
- Secret detection occurs before filesystem, SQLite, spool, Git, log, and archive writes.
- Low-entropy redacted values are not exposed by guessable digests.
- No audio byte, path, filename, or content digest exists anywhere persistent.
- Protected records verify while locked at the ciphertext level.
- Authorized verification detects plaintext modification.
- Incorrect key and authenticated-metadata failures are detected.
- Wrapping-key rotation leaves record ciphertext and references unchanged.
- Recovery key re-entry and test unwrap are required before protection activation.
- Protected backup restores on a different device using only approved recovery material.
- Loss of both device and recovery access produces the documented unrecoverable state.
- Sensitive operational SQLite payloads are not left unprotected.
- Temporary cleanup is described and tested as cryptographic/logical deletion.

## 8. Backup, import, and release tests

- SQLite online backup during active editing
- SQLite snapshot integrity and manifest digest
- Backup file digest corruption
- Missing and extra manifest entries
- ZIP traversal, absolute/device path, symlink, case collision, duplicate entry, and decompression-bomb attempts
- Interrupted import at every staging/verification/activation phase
- No unverified file reaches an active destination
- Destination identity and conflict handling
- `VERIFIED_WITH_WARNINGS` security/non-security distinction
- `INCOMPLETE` import quarantine
- `FAILED` import block
- `UNSAFE` package rejection
- Release failure at every state transition
- Source commit/tree/chain-head consistency
- Release manifest self-reference exclusion
- Merkle ordering and path-normalization vectors
- Release commit and tag binding
- Missing optional binary produces warning without falsifying authoritative integrity

## 9. Native verifier and UI tests

- UI Verify History invokes the native verifier.
- Frontend cannot manufacture `VERIFIED` status.
- Finding severity maps correctly to overall status.
- Incremental verification never skips an altered authoritative record.
- Full verification is mandatory for release and import.
- Stored indexes are not trusted as verification inputs.
- A stale index is repairable without changing provenance.
- Locked encrypted evidence yields `INCOMPLETE`, not `FAILED`.
- Git-only damage yields warning when authoritative evidence remains valid.
- Missing authoritative records yield `FAILED`.
- Unsafe archive structure yields `UNSAFE`.

## 10. Legacy 1.0 behavior tests

- Known preview markers are detected without modifying the project.
- Normal opening and editing are refused.
- Show Project Folder remains available.
- Raw archival ZIP preserves the selected legacy tree without conversion.
- The archive is labeled as unverified and unconverted.
- No 1.0 provenance verification or evidence report is offered.
- Legacy Git history remains unchanged.
- No migration schema or implied migration success appears in 1.0.

## 11. Stage 1 disposition

Stage 1 is **complete** when:

- The normative documents have passed editorial review.
- No older active project document is allowed to silently override their provenance mechanics.
- The Stage 2 schema work uses this checklist as its acceptance boundary.
- Any future architectural change is recorded as a versioned normative amendment rather than an informal implementation choice.

Completion of Stage 1 does not claim implementation conformance and does not change the current application version.
