# Thinkloom Stage 1 Normative Provenance Specification

Status: **Approved architecture baseline for formal schema work**  
Target: **Thinkloom 1.0.0**  
Provenance schema family: **1.0**  
Migration support: **Deferred until after Thinkloom 1.0.0**

## 1. Purpose and precedence

This specification defines the authority, persistence, integrity, privacy, recovery, verification, backup, and release contracts for Thinkloom provenance.

It supersedes the provenance-specific transaction order, single-ledger layout, mutable-record assumptions, live-database snapshot method, and release-binding sequence in the earlier MVP architecture and implementation plans. It does not supersede their product requirements, native Tauri boundary, preview-first generation model, user-control requirements, accessibility requirements, or prohibition on retained audio.

Thinkloom 0.3.0 includes the formal Stage 2 schemas and verification vectors, but its native writer is not represented as conforming to this specification. Full conformance begins only after the native implementation and required fault-injection tests are complete.

## 2. Normative language

The words **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, **SHOULD NOT**, and **MAY** are normative.

- **MUST/MUST NOT** identify a conformance requirement.
- **SHOULD/SHOULD NOT** identify a strong recommendation that requires a documented reason when not followed.
- **MAY** identifies permitted optional behavior.

## 3. Correct product claim

Thinkloom provenance is:

> A local, transactionally coordinated, tamper-evident creative-process record with configurable retention, native verification, recoverable storage, and reproducible release manifests.

Thinkloom MUST NOT claim that local provenance is tamper-proof, an independently trusted timestamp, conclusive legal proof, or a quantitative measure of human versus AI authorship.

The strongest valid claim without an external anchor is:

> The system can detect changes relative to a previously retained chain head, signed release, or external anchor.

## 4. Authority hierarchy

The following hierarchy is binding:

1. **Immutable filesystem records and the provenance ledger** are authoritative evidence.
2. **Canonical publication files and manuscript revisions** are authoritative publication content when bound by ledger references.
3. **Release manifests** are authoritative bindings for a completed release.
4. **SQLite** stores operational state, UI state, write intents, idempotency indexes, and rebuildable query indexes.
5. **Git** stores meaningful milestone history and release state but is not the provenance authority.
6. **Derived indexes and generated reports** are disposable, reproducible caches or projections.

SQLite MUST NOT be the only location of an evidentiary fact. A valid ledger MUST take precedence over contradictory SQLite state. Git failure MUST NOT invalidate an otherwise valid provenance ledger.

## 5. Repository boundaries

A conforming 1.0 project SHOULD organize authoritative and operational data under these boundaries:

```text
publication-project/
├── project.json
├── manuscript/
├── ideas/
├── records/
│   ├── conversations/
│   ├── invocations/
│   ├── prompt-templates/
│   ├── sources/
│   └── transformations/
├── provenance/
│   ├── schema/
│   ├── ledger/active/
│   ├── ledger/sealed/
│   ├── indexes/
│   ├── integrity/
│   └── report-config/
├── releases/
├── reports/
├── assets/
├── .app/
│   ├── state.sqlite
│   ├── locks/
│   ├── temp/
│   └── recovery/
└── .git/
```

Operational `.app/` data, routine generated reports, temporary files, live database files, spools, and non-release exports MUST NOT be tracked in Git. Authoritative records, ledger segments, schemas, canonical publication content, release manifests, and retained release hashes MUST be tracked at meaningful checkpoints.

Large PDFs, ZIP packages, and regenerable binaries SHOULD remain outside ordinary Git history. Their hashes and manifests MAY be tracked.

## 6. Identifiers and event ordering

Stable sortable identifiers SHOULD use ULIDs with type prefixes, including `event_`, `record_`, `intent_`, `turn_`, `session_`, `invocation_`, `revision_`, `fragment_`, `checkpoint_`, and `release_`.

Identifiers MAY be allocated before an operation commits. Abandoned identifiers MUST NOT be reused.

Committed `event_sequence` values MUST be contiguous across active and sealed ledger segments:

- The first committed event sequence is defined by the project schema, normally `1`.
- A sequence is assigned only while holding the exclusive project writer lock immediately before ledger append.
- A write intent MUST NOT reserve an event sequence.
- The next sequence MUST be derived from the verified ledger head; SQLite MAY cache but MUST NOT authoritatively assign it.
- A missing or duplicated committed sequence is an authoritative integrity failure.

## 7. Canonical paths and timestamps

### 7.1 Repository paths

Stored paths MUST:

- Be relative to the project root.
- Use forward slashes.
- Contain no empty, `.` or `..` components.
- Contain no drive prefix, UNC prefix, URI scheme, NUL, or control character.
- Resolve within the project root after platform normalization.
- Be compared using a documented case policy appropriate to cross-platform verification.

Schema work MUST define handling for Windows reserved names, trailing dots/spaces, Unicode-equivalent names, and case collisions. Unsafe or ambiguous paths MUST be rejected before a durable write or archive extraction.

### 7.2 Timestamps

Canonical timestamps MUST:

- Use RFC 3339 UTC.
- Use exactly millisecond precision.
- End in `Z`.
- Use the form `2026-07-17T18:42:10.123Z`.

Wall-clock time is descriptive, not independently trusted. Event sequence supplies authoritative local ordering. Clock rollback MUST NOT permit sequence rollback or event reordering.

## 8. Canonical JSON and hashing

All authoritative JSON MUST use one native canonicalization implementation shared by writing, hashing, verification, backup, release, and report generation.

Before RFC 8785 JSON Canonicalization Scheme processing:

- Text strings MUST be normalized to Unicode NFC.
- Text intended for provider submission MUST be normalized before submission so the retained message matches what was sent.
- Content MUST be encoded as UTF-8 without BOM.
- JSONL records MUST use LF delimiters.
- Undefined, NaN, and infinite values are prohibited.
- Integers MUST be used when floating-point representation is unnecessary.
- Schema-defined numeric bounds MUST be enforced before canonicalization.

SHA-256 digest strings MUST use `sha256:` followed by 64 lowercase hexadecimal characters unless a schema explicitly separates algorithm and digest fields.

Any record containing a hash of itself MUST define the exact canonical identity object and explicitly exclude the hash field. No implementation may infer hash exclusions informally.

Formal Stage 2 fixtures MUST include RFC 8785 vectors plus Thinkloom vectors for NFC, timestamps, paths, numbers, JSONL boundaries, and cross-platform line endings.

## 9. Record mutability

The following records become immutable after creation:

- Invocation request
- Raw provider response retained by policy
- Normalized provider response
- Invocation failure
- Raw transcript turn retained by policy
- Transcript correction and normalization revision
- Model configuration and capability snapshot
- Prompt-template ID/version
- Manuscript revision
- Idea revision
- Edit transaction
- Sealed ledger segment and segment manifest
- Release manifest
- Protected record envelope

Mutable concepts MUST use immutable ordered revisions. `current.json` files and similar pointers MAY exist only as derived, non-authoritative conveniences.

Every staged authoritative record MUST carry or be recoverably associated with `intent_id`, `client_action_id`, `project_id`, record ID, schema version, and record type. Recovery MUST be possible even when SQLite coordination data is unavailable or damaged.

Corrections MUST NOT erase original transcript content when the selected retention policy preserves it. Dispositions MUST be revision records, not updates to a single authoritative file.

## 10. Content and text identity

Content references MUST identify a stable record or revision and its digest. A path alone is insufficient identity.

Text-range references MUST include:

- Document revision ID
- Coordinate system
- Start and end positions
- Exact preimage digest
- Stable fragment ID when available
- Source and destination revision IDs for transformations

Supported coordinate systems MUST be explicit, such as `utf8_byte`, `unicode_scalar`, `utf16_code_unit`, or `editor_position`. Offsets are supporting metadata and MUST NOT be the sole long-term identity.

Meaningful manual editing MUST be grouped into edit transactions rather than keystroke events. A transaction SHOULD close on focus loss, configured idle interval, section change, AI operation, checkpoint, phase change, document close, explicit save, or milestone.

## 11. Retention, export, and encryption policies

These are independent settings:

```text
retention_mode:       minimal | full_private
encryption_mode:      none | protected
default_export_profile: full | sanitized
```

### 11.1 Minimal retention

Minimal provenance is the REQUIRED default for Thinkloom 1.0. It retains final user-approved input, operation purpose, prompt-template identity/hash, input references/hashes, provider/model identity, accepted generated text, disposition metadata, manuscript lineage, checkpoints, and releases.

It MUST NOT retain raw speech hypotheses, complete provider-facing prompts, complete supplied context, unaccepted raw model responses, or provider transport metadata.

### 11.2 Full private retention

Full private provenance MAY retain raw/corrected/normalized transcripts, actual provider-facing messages, supplied context, raw and normalized responses, parameters, provider metadata, rejected output, and correction history. Credentials and prohibited secrets remain excluded.

### 11.3 Sanitized exports

Sanitization is an export profile, not a storage mode. It MUST NOT mutate the project. A sanitized package MUST identify its source chain head, disclose omissions, and include the applied rule set or its hash.

### 11.4 Policy changes

Policy changes apply prospectively and MUST create provenance events. Minimal-to-full cannot reconstruct discarded content. Full-to-minimal does not remove previously retained content. Removal requires the separately confirmed emergency-purge process.

### 11.5 Protected mode

Protected mode MUST NOT ship until portability, recovery, key-loss behavior, and encrypted-backup restoration pass required tests.

Protected records use stable random record IDs independent of ciphertext. Unprivileged verification checks record identity, authenticated envelope metadata, and ciphertext hashes. Authorized verification additionally decrypts and validates the canonical plaintext identity hash.

The plaintext identity digest MUST hash exactly the canonical object containing `schema_version`, `record_id`, `record_type`, and `payload`, excluding the digest and all encryption, path, nonce, and envelope fields.

The required key hierarchy is:

```text
Project data-encryption key
├── device envelope protected by OS-vault material
└── portable recovery envelope protected by a high-entropy recovery key
    or Argon2id-derived recovery key
```

Protected mode MUST require recovery-key verification and a successful test unwrap before activation. Portable backups MUST contain encrypted records, an encrypted SQLite snapshot, recovery envelope, and key metadata, but no plaintext project key or device-vault key.

Operational SQLite MUST NOT become an unprotected duplicate of sensitive protected records. It MUST store references and non-sensitive indexes or use a separately specified protection mechanism for sensitive operational payloads.

## 12. Single-writer and cross-store operation journal

Every provenance mutation MUST pass through one native Rust provenance service. The frontend MUST NOT directly modify ledger segments, chain heads, immutable records, transcript revisions, invocation evidence, dispositions, verification results, or release manifests.

The native service MUST use an exclusive per-project OS-managed writer lock. Persistent lock files alone are insufficient. Provider calls, transcription, Git operations, report generation, and other long-running external work MUST NOT hold this lock.

Every mutation receives a stable `client_action_id`. Retrying the same action MUST return the original committed result rather than create another event.

SQLite MUST maintain a `write_intents` operation journal with these phases:

```text
PREPARED
RECORDS_DURABLE
LEDGER_APPENDED
CHAIN_HEAD_ADVANCED
SQLITE_APPLIED
COMPLETE
QUARANTINED
FAILED
```

The ledger is authoritative after a complete event has been durably appended. SQLite idempotency state is a rebuildable index and MUST be reconstructable from ledger events containing `client_action_id`.

The binding write order and recovery transitions are defined in [STATE-MACHINES.md](STATE-MACHINES.md).

## 13. Ledger segments

The ledger MUST contain one active JSONL segment and zero or more sealed segments. A project writer lock serializes appends and rotation.

A sealed segment manifest MUST bind:

- Schema version
- Segment number
- Previous segment file digest
- First and final event digests
- First and final event sequences
- Event count
- Byte length
- Segment file digest
- Seal timestamp

Sealed segments MUST never be altered automatically. A truncated final line MAY be removed from the active segment only when it was never referenced by the chain head. A complete valid event ahead of the chain head is recoverable by advancing the head after verification.

Rotation MUST stop appends under the writer lock, flush and verify the active segment, create and flush its manifest, move both to sealed storage, create the next active segment, append a sequence-contiguous opening event linked to the prior final event, and advance the head.

Default rotation triggers SHOULD include 10,000 events, 10 MiB, release finalization, and schema-boundary changes. Exact thresholds are configuration, not integrity semantics.

## 14. Model invocation lifecycle

A model invocation is a multi-operation state machine. Before contacting a provider, Thinkloom MUST record the immutable request and `MODEL_INVOCATION_REQUESTED` event under the writer lock, including context record/revision references, model configuration snapshot, prompt-template reference, and the manuscript/idea/conversation heads used.

Provider I/O occurs outside the lock. Incomplete output MAY exist only in bounded application-managed temporary storage and MUST NOT become canonical evidence.

After provider completion, cancellation, or failure, Thinkloom reacquires the lock, records an immutable response or failure, appends the corresponding event, and creates any staged preview. User acceptance or rejection is a separate provenance operation.

If source revisions change while the model runs, the preview MAY be shown, but insertion MUST require revalidation, rebase, or explicit stale-context confirmation. Thinkloom MUST NOT silently insert generation based on stale context.

## 15. Verification

Native verification is authoritative. The UI MUST render native results and MUST NOT independently declare history valid.

The verifier MUST check, as applicable:

- Schema compatibility and canonical JSON
- Contiguous event sequences
- Event digests and previous-event links
- Cross-segment linkage and sealed manifests
- Referenced authoritative record existence and digests
- Active chain head
- Release manifests and release-file bindings
- Source Git commit and tag bindings
- Deterministic derived-index reproducibility
- Protected record envelopes, and plaintext when keys are available

Finding severities are `INFO`, `WARNING`, `ERROR`, and `CRITICAL`. Overall statuses are:

- `VERIFIED`: all authoritative evidence validates.
- `VERIFIED_WITH_WARNINGS`: authoritative evidence validates; a derived or optional component needs attention.
- `INCOMPLETE`: no contradiction was found, but authoritative verification could not finish.
- `FAILED`: authoritative evidence is inconsistent.
- `UNSAFE`: an import or external package violates security policy.

An unavailable historical index generator is a warning, not an incomplete result. A stale index is repairable and MUST NOT invalidate authoritative evidence.

Incremental verification MAY use a cached verified chain head. The cache is non-authoritative. Full verification is REQUIRED for release completion, backup import, explicit deep verification, and integrity recovery.

Release and import gates MUST follow the approved status matrix: `INCOMPLETE`, `FAILED`, and `UNSAFE` block release; unverified imports never enter an active project destination.

## 16. Derived indexes

Derived indexes MUST be reproducible from authoritative records. Deterministic index content MUST use stable sorting, locale-independent comparison, canonical inputs, fixed schema/configuration, no random identifiers, and no current timestamp.

An outer manifest MUST identify the source chain head, event count, generator name/version, configuration digest, and deterministic content digest. Volatile generation metadata MUST remain outside the hashed deterministic content.

Verification rebuilds an index in temporary storage and compares canonical content digests. The stored index is never authoritative.

Physical packing of immutable records is deferred. Content-addressed identity MAY be included in Stage 2 schemas, but authoritative paths MUST NOT be rewritten by an unimplemented packing process.

## 17. Git checkpoints

Git checkpointing occurs after critical provenance writes and outside the provenance writer lock. A checkpoint uses two stages to avoid a self-referential commit:

1. Source commit `C1` binds the frozen publication tree and provenance head `H0`.
2. A `GIT_CHECKPOINT_CREATED` event acknowledges `C1`, producing head `H1`.
3. Audit commit `C2` contains the acknowledgment event and checkpoint manifest but is not referenced by that event.

Git commits MUST be created from a frozen captured tree or isolated temporary Git index. They MUST NOT depend on a live working tree remaining unchanged after the provenance lock is released.

A checkpoint is not user-visible as complete until the acknowledgment event exists. Restore uses publication content from `C1`; an application checkpoint ref MAY point to `C2`. Git failure produces a warning and recoverable checkpoint state, not fabricated completion.

## 18. Backup, import, and SQLite snapshots

Thinkloom MUST use SQLite's online backup API for a consistent snapshot. Copying live database, WAL, or SHM files is prohibited.

A snapshot manifest MUST include SQLite version, database schema version, completion timestamp, file digest, project ID, and provenance chain head.

Backup import MUST occur in application-controlled staging outside the destination. Before activation it MUST enforce archive path/count/size rules, verify the backup manifest and every listed digest, perform full native provenance verification, verify SQLite integrity, verify expected Git structure, confirm project identity, and detect destination conflicts.

Only a verified project may be atomically moved into its destination. `UNSAFE` packages are rejected; `INCOMPLETE` packages remain quarantined; no unverified archive file may be placed into an active project directory.

## 19. Release finalization

Release finalization follows the state machine in [STATE-MACHINES.md](STATE-MACHINES.md). It MUST distinguish a frozen source commit from the later release commit.

The release manifest binds the source commit, source chain head, source manuscript digest, release schema/application versions, sanitization state, and a formally defined release-files Merkle root. It MUST NOT contain the release commit digest.

The release commit contains the generated package and manifest. The release tag points to that commit. Self-referential manifests and hash files MUST be excluded from their own flat digest lists or handled by the formally specified Merkle construction.

Routine large release binaries SHOULD remain outside Git; tracked manifests and digests bind them. Release status transitions MUST be durable and recoverable.

## 20. Temporary model output

Temporary streaming output MUST use an application-private, non-Git, non-backup directory with restrictive permissions and encrypted per-invocation spool files.

Defaults:

```text
Maximum spool per invocation:       8 MiB
Hard configurable ceiling:         64 MiB
Maximum stream duration:            30 minutes
Maximum concurrent spools/project:  4
Maximum aggregate spool/project:    128 MiB
Stale recovery period:               1 hour
```

Limits MUST be enforced before accepting additional bytes. On limit violation Thinkloom stops intake, attempts cancellation, records a bounded failure summary, and does not promote partial output unless the user explicitly preserves it under the retention policy.

Spool keys MUST be wrapped by temporary-storage key material and retained only in protected operational state. Completion or cancellation removes the spool, wrapped temporary key, and directory. Cleanup MUST be described as cryptographic and logical deletion, not guaranteed physical overwrite.

Temporary recovery MUST distinguish local recovery of bounded partial data from resumption of a provider stream; Thinkloom MUST NOT claim a provider request is resumable unless the provider contract supports it.

## 21. Secret filtering and audio

Credentials, authorization headers, cookies, signed credentials, and detected secrets MUST be filtered before the first durable filesystem, SQLite, Git, log, spool, or archive write. Redaction after Git persistence is not sufficient.

Redaction actions MUST be disclosed rather than pretending omitted content never existed. A low-entropy secret MUST NOT be exposed through an unhashed or ordinary plaintext digest that permits trivial guessing.

Audio retention remains prohibited. Project storage, provenance, Git, logs, reports, spools, and backups MUST contain no audio bytes, audio paths, or audio content digests. Permitted transcript retention depends on provenance policy.

## 22. Purge semantics

Sanitized export does not alter project history. Emergency purge is a separate destructive operation requiring strong confirmation.

A purge MAY rewrite affected records, ledger hashes, and Git history only through a defined purge state machine. It MUST create a new chain root, record the superseded chain head when safe, disclose that integrity history was reconstituted, and warn that earlier copies and exports cannot be revoked.

Ordinary editing MUST NOT invoke purge behavior.

## 23. Legacy preview projects

Thinkloom 1.0 MUST detect known preview/experimental project markers and refuse normal opening or editing. It MUST preserve the original project untouched, explain that migration is deferred, offer Show Project Folder, and permit a byte-preserving raw archival ZIP labeled:

```text
Legacy project preservation archive
Not verified or converted by Thinkloom 1.0.0
```

Thinkloom 1.0 MUST NOT import legacy records into schema 1.0, regenerate or verify their provenance under 1.0 rules, create a 1.0 evidence report, modify their Git history, or present them as migrated. Formal legacy reading, conversion, and migration begin after 1.0.0.

## 24. Formal schema inventory for Stage 2

Stage 2 MUST produce JSON Schema Draft 2020-12 documents for:

```text
project-manifest.schema.json
provenance-policy.schema.json
write-intent.schema.json
provenance-event.schema.json
chain-head.schema.json
ledger-segment-manifest.schema.json
record-envelope.schema.json
content-reference.schema.json
prompt-template.schema.json
prompt-template-reference.schema.json
model-configuration-snapshot.schema.json
model-capability-snapshot.schema.json
encrypted-key-envelope.schema.json
project-key-manifest.schema.json
recovery-key-envelope.schema.json
conversation-session.schema.json
transcript-turn.schema.json
transcript-correction.schema.json
transcript-normalization.schema.json
invocation-request.schema.json
invocation-response.schema.json
invocation-failure.schema.json
invocation-state.schema.json
invocation-stream-state.schema.json
invocation-stream-summary.schema.json
disposition-revision.schema.json
idea.schema.json
idea-revision.schema.json
manuscript-revision.schema.json
edit-transaction.schema.json
text-fragment-reference.schema.json
derived-index-manifest.schema.json
verification-report.schema.json
backup-manifest.schema.json
release-manifest.schema.json
release-state.schema.json
sanitized-export-manifest.schema.json
purge-manifest.schema.json
```

Prompt-template and other self-digesting schemas MUST define exact digest identity objects. Migration schemas are deferred until after Thinkloom 1.0.0.

## 25. Required implementation characteristics

The Stage 3 native implementation MUST provide deterministic fault injection at every durable boundary and test at least concurrency, duplicate retries, partial writes, truncated JSONL, ledger/head disagreement, record corruption, stale context, SQLite online backup during editing, unsafe archives, release failure transitions, secret filtering, protected-key loss, and native-verifier/frontend consistency.

The complete required test groups are enumerated in [COMPLETION-CHECKLIST.md](COMPLETION-CHECKLIST.md).

## 26. Explicit deferrals

The following are not required for the Stage 1 specification or initial unanchored provenance implementation:

- Legacy-project migration or conversion before/at 1.0.0
- External trusted timestamping
- Remote transparency logging
- Public hash anchoring
- Hardware-backed signing keys
- Multi-user authority or distributed conflict resolution
- Physical immutable-record packing
- Human-versus-AI contribution percentages

Locally signed releases, external anchors, and hardware-backed keys remain compatible future trust enhancements.
