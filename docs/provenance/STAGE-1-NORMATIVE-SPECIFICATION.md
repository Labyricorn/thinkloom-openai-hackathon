# Thinkloom Stage 1 Normative Provenance Specification

Status: **Approved architecture baseline for formal schema work**  
Document release: **Thinkloom 0.5.1**

Runtime conformance target: **Thinkloom 0.6.0; project-format conformance 1.0**

Provenance schema family: **1.0**

Migration support: **Deferred until after Thinkloom 1.0.0**

## 1. Purpose and precedence

This specification defines the authority, persistence, integrity, privacy, recovery, verification, backup, release, Composition Provenance Ledger (CPL), and Human Authorship Record of Provenance (HARP) contracts for Thinkloom provenance.

It supersedes the provenance-specific transaction order, single-ledger layout, mutable-record assumptions, live-database snapshot method, and release-binding sequence in the earlier MVP architecture and implementation plans. It does not supersede their product requirements, native Tauri boundary, preview-first generation model, user-control requirements, accessibility requirements, or prohibition on retained audio.

Thinkloom 0.5.1 includes this normative HARP/CPL amendment and preserves the existing formal Stage 2 schemas, canonical assertion envelopes, and verification vectors, but its native writer is not represented as CPL-conforming. Full conformance begins only after the native implementation and required fault-injection tests are complete.

## 2. Normative language

The words **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, **SHOULD NOT**, and **MAY** are normative.

- **MUST/MUST NOT** identify a conformance requirement.
- **SHOULD/SHOULD NOT** identify a strong recommendation that requires a documented reason when not followed.
- **MAY** identifies permitted optional behavior.

## 3. Correct product claim

Thinkloom provenance is:

> A local, transactionally coordinated, tamper-evident creative-process record with configurable retention, native verification, recoverable storage, and reproducible release manifests.

The correct HARP/CPL product claim is:

> A Human Authorship Record of Provenance (HARP) is a reproducible evidence projection for an exact deposit, backed by cryptographically verifiable integrity evidence in Thinkloom's Composition Provenance Ledger (CPL).

Thinkloom MUST NOT claim that local provenance is tamper-proof, an independently trusted timestamp, conclusive legal proof, or a quantitative measure of human versus AI authorship.

Cryptographic verification establishes only the integrity and internal linkage of the retained bytes within the verifier's stated scope. It MUST NOT, without separately identified evidence, be represented as verification of a person's identity, the truth of a user declaration, legal authorship, originality, copyrightability, ownership, or registrability. A signature or local key proves control of that key; it proves identity only when the key has a separately verified identity binding.

The strongest valid claim without an external anchor is:

> The system can detect changes relative to a previously retained chain head, signed release, or external anchor.

### 3.1 Normative product terms

The following meanings are exclusive and binding throughout product copy, schemas, reports, tests, and implementation:

- **Composition Provenance Ledger (CPL)** is the product-facing name for the single canonical provenance ledger defined by §§12–13. CPL is not a new ledger, side ledger, report, database, or replacement for the existing ledger. A conforming project has one CPL whose events bind immutable records, assertions, and evaluations under §4.
- **CPL record** is an immutable authoritative record referenced by a CPL event. The term does not include SQLite rows, UI state, derived indexes, generated reports, or uncommitted temporary data.
- **Deposit** is the exact file selected by the user for a registration submission or other declared evidentiary purpose and frozen in a deposit snapshot. A manuscript revision, editor state, export recipe, or filename alone is not a deposit.
- **Human Authorship Record of Provenance (HARP)** is a non-authoritative, deterministic, reproducible projection of CPL records and evaluations bound to one exact deposit. The word “Authorship” in the product name describes the subject of the evidence record; it is not a Thinkloom determination that any expression is legally authored, original, or copyrightable.
- **Recorded origin** describes what the CPL records about how expression entered the composition. It is an evidence classification, not a legal conclusion.
- **Transformation relationship** describes how expression changed or derived from earlier expression.
- **Selection/arrangement overlay** describes recorded relationships among surviving expression or structural units. It is independent of their recorded origin and MUST NOT be encoded as a text-origin category.
- **Suggested registration treatment** is editable application language produced under a versioned policy profile. It is neither an evidence fact nor a legal determination.
- **Legal determination** includes copyrightability, originality, authorship, ownership, and registrability. Thinkloom and HARP do not make these determinations.

No second meaning, abbreviation expansion, or competing product label MAY be used for CPL or HARP in a conforming product surface.

### 3.2 Prohibited claims and required UI terminology

Thinkloom, CPL, HARP, exports, and marketing MUST NOT state or imply:

- A “human percentage,” “AI percentage,” authorship score, originality score, copyrightability score, or equivalent quantitative legal proxy
- “Copyright verified,” “copyrightable,” “originality proven,” “proven human author,” “legally human-authored,” “registration-ready,” “Copyright Office approved,” or “authorship certified”
- That typing time, edit count, word count, retained-word ratio, prompt count, or any other activity metric determines originality or authorship
- That a paste, import, self-declaration, cryptographic signature, or successful integrity check by itself proves identity or human authorship
- That `VERIFIED` or `exact` means anything beyond the explicitly named integrity or evidentiary scope

Product surfaces MUST distinguish and label these categories: **Evidence fact**, **User declaration**, **Recorded origin**, **Transformation relationship**, **Selection/arrangement overlay**, **Evidentiary evaluation**, **Suggested registration language**, and **Copyright Office determination**. The UI MAY use “integrity verified” only with the verified scope and chain head visible. It MUST use “self-declared identity” unless stronger identity evidence and its verification method are present.

## 4. Authority hierarchy

The following hierarchy is binding:

1. **Immutable filesystem records, canonical provenance assertions, point-in-time assertion evaluations, and the canonical provenance ledger (product name: CPL)** are authoritative evidence when bound by CPL event references.
2. **Canonical publication files and manuscript revisions** are authoritative publication content when bound by ledger references.
3. **Release manifests** are authoritative bindings for a completed release.
4. **SQLite** stores operational state, UI state, write intents, idempotency indexes, and rebuildable query indexes.
5. **Git** stores meaningful milestone history and release state but is not the provenance authority.
6. **Derived indexes, contribution maps, HARP documents, and generated reports** are disposable, reproducible projections. They are non-authoritative even when their bytes and source bindings verify.

SQLite MUST NOT be the only location of an evidentiary fact. A valid ledger MUST take precedence over contradictory SQLite state. Git failure MUST NOT invalidate an otherwise valid provenance ledger.

A HARP MUST NOT add, promote, or repair an evidentiary fact. Every factual HARP statement MUST trace to a CPL record, assertion, evaluation, or explicit user declaration bound by a CPL event. If the required basis is unknown, unattested, stale, degraded, or unverified, the HARP MUST preserve that boundary rather than infer a favorable classification.

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
│   ├── transformations/
│   └── composition/
├── provenance/
│   ├── schema/
│   ├── ledger/active/
│   ├── ledger/sealed/
│   ├── indexes/
│   ├── integrity/
│   └── report-config/
├── releases/
├── deposits/
├── reports/
│   └── harp/
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

Stable sortable identifiers SHOULD use ULIDs with type prefixes, including `event_`, `record_`, `intent_`, `turn_`, `session_`, `invocation_`, `revision_`, `segment_`, `deposit_`, `harp_`, `policy_`, `fragment_`, `checkpoint_`, `release_`, `assertion_`, and `evaluation_`.

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
- Provenance assertion
- Assertion evaluation

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

Composition provenance MUST keep these dimensions independent:

1. Recorded origin
2. Transformation relationship
3. Selection/arrangement overlay
4. Evidentiary evaluation
5. Suggested registration treatment

Recorded-origin values MUST distinguish at least recorded direct human input, human expressive input mediated by transcription, accepted AI output, imported or pasted material, system restoration, and unattested expression. Paste or import MUST NOT default to recorded direct human input. Human modification of AI-origin material MUST retain the AI preimage lineage and the later human operations; it MUST NOT rewrite the earlier origin as human. Unknown identity, generation, origin, or lineage MUST remain unknown or unattested and MUST NOT validate as an exact classification.

## 11. Retention, export, and encryption policies

These are independent settings:

```text
retention_mode:       minimal | full_private
encryption_mode:      none | protected
default_export_profile: full | sanitized
```

### 11.1 Minimal retention

Minimal provenance is the REQUIRED default for a CPL 1.0-conforming project. It retains final user-approved input, operation purpose, prompt-template identity/hash, input references/hashes, provider/model identity, accepted generated text, disposition metadata, manuscript lineage, checkpoints, and releases.

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
- Provenance assertions
- Assertion evaluations

Finding severities are `INFO`, `WARNING`, `ERROR`, and `CRITICAL`. Overall statuses are:

- `VERIFIED`: all authoritative evidence validates.
- `VERIFIED_WITH_WARNINGS`: authoritative evidence validates; a derived or optional component needs attention.
- `INCOMPLETE`: no contradiction was found, but authoritative verification could not finish.
- `FAILED`: authoritative evidence is inconsistent.
- `UNSAFE`: an import or external package violates security policy.

An unavailable historical index generator is a warning, not an incomplete result. A stale index is repairable and MUST NOT invalidate authoritative evidence.

Incremental verification MAY use a cached verified chain head. The cache is non-authoritative. Full verification is REQUIRED for release completion, backup import, explicit deep verification, and integrity recovery.

Release and import gates MUST follow the approved status matrix: `INCOMPLETE`, `FAILED`, and `UNSAFE` block release; unverified imports never enter an active project destination.

### 15.1 Canonical provenance assertions and evaluations

A promoted fact or artifact relationship MUST be represented by an immutable `provenance-assertion` record rather than inferred from a derived index, UI state, or application-specific internal structure. An assertion states what was claimed, which generation and lifecycle phase it belongs to, which subsystem produced it, which retained evidence supports it, and which dependencies can invalidate its use.

An assertion MUST contain:

- Stable assertion, project, subject, and object identities
- A machine-readable predicate
- A source anchor naming a prior authoritative event, its sequence, and its event digest
- Explicit source-generation coordinates
- The lifecycle phase at which the relationship was asserted
- Producer subsystem and application version
- A provenance basis and retained evidence references
- Structured invalidation dependencies with expected digests, expected generations, evidence class, and required/advisory role
- A stable reason code
- An exact assertion self-digest identity excluding only `assertion_sha256`

The event that records an assertion MUST reference the already-written assertion record and digest. The assertion source anchor MUST identify basis evidence and MUST NOT identify the recording event when that would create a circular digest dependency.

Time-dependent conclusions MUST NOT mutate the assertion. They MUST be stored as immutable `assertion-evaluation` records evaluated against an explicit chain head, event sequence, and project epoch. A later evaluation supersedes an earlier conclusion for current use without erasing or rewriting it.

Assertion evaluation statuses are:

- `exact`: all required provenance, generation, integrity, identity, chronology, derivation, authorship, and completeness inputs are known, compatible, and sufficient for the asserted scope.
- `degraded`: the assertion remains usable only with an explicitly bounded limitation.
- `refused`: policy, authorization, unsupported compatibility, or another mandatory gate prohibits a conclusion.
- `stale`: a dependency digest, generation, or source anchor no longer matches the evaluation target.
- `unverified`: required evaluation has not completed or mandatory evidence is unavailable without a demonstrated contradiction.

Every evaluation MUST assess these dimensions independently: `integrity`, `identity`, `chronology`, `derivation`, `authorship`, and `completeness`. Dimension values are `exact`, `degraded`, `unverified`, or `not_applicable`. Numeric confidence scores and human-versus-AI percentages MUST NOT be used.

For compatibility with the existing v0.4 assertion registry, the wire-level dimension name `authorship` is retained. In this specification it means only the sufficiency of recorded evidence for the assertion's explicitly bounded authorship-related predicate. It MUST NOT be displayed or interpreted as legal authorship, copyrightability, originality, or a HARP conclusion; an `exact` value means only that the retained evidence satisfies that declared predicate and scope.

An `exact` evaluation MUST have no uncertainty boundary, MUST contain valid results for every non-shadow dependency, and MUST contain no `degraded` or `unverified` confidence dimension. Shadow evidence never contributes authority and MAY remain unevaluated without changing exactness. A non-exact evaluation MUST identify the exact boundary, affected dimensions, dependencies when applicable, and a stable reason code. Unknown provenance, source generation, compatibility, or confidence MUST NOT silently produce an `exact` evaluation.

Evidence classes are `mandatory_live`, `mandatory_retained`, `advisory`, and `shadow`. Missing or incompatible mandatory evidence prohibits `exact`. Advisory or shadow evidence MAY degrade completeness or another named dimension but MUST NOT silently change an authoritative assertion.

Assertion and evaluation reason codes, lifecycle phases, statuses, dimensions, evidence classes, and boundary kinds MUST come from versioned machine-readable registries included in the schema package. Consumers MUST decide usability from these canonical records and registries rather than reopening producer-specific internal state.

Derived indexes and reports MAY project assertions and the latest applicable evaluations, but remain non-authoritative consumers. They MUST NOT synthesize an `exact` result when no valid authoritative evaluation exists.

### 15.2 Deposit snapshots and authoritative locators

A HARP MUST bind one immutable deposit snapshot containing at least:

- Deposit ID and deposit-snapshot schema version
- Exact deposit-file digest and byte length
- Deposit media type and sanitized display name
- Bound manuscript revision ID and revision digest
- CPL chain head digest and event sequence used for the projection
- Layout-profile ID and digest when page locators are emitted
- Creation timestamp, application version, and schema versions

The deposit revision, deposit digest, and stable expression-segment ID are authoritative locators. Chapter, paragraph, line, and page numbers are derived locators. A page number MUST identify its layout profile and MUST NOT be used as the sole identity of expression or as an authoritative range boundary.

Freezing a deposit does not freeze the working manuscript. Any edit or restoration after the bound deposit snapshot, or any change to the selected deposit bytes, creates a newer dependency state and MUST make the HARP `stale` for the current work. Staleness does not falsify or invalidate a previously verified historical HARP for its exact bound deposit; the UI MUST show both facts. A stale HARP is immutable and MUST be regenerated from a new deposit snapshot rather than updated in place.

### 15.3 HARP content and generation

HARP generation MUST be deterministic and MUST NOT use an LLM to classify authorship, decide originality or copyrightability, fill an evidence gap, or formulate a legal conclusion. Identical canonical inputs, policy profile, generator version, and sanitization profile MUST produce byte-identical machine-readable HARP content.

A canonical HARP MUST include:

- HARP ID, schema version, application version, and generator version
- Exact deposit binding required by §15.2
- CPL chain head and sequence used for projection
- A complete contribution-map coverage statement with an explicit denominator and coordinate unit
- Recorded-origin, transformation, and selection/arrangement layers kept separate
- Current applicable assertion evaluations and visible exact, degraded, stale, unverified, unknown, and unattested boundaries
- AI systems and models recorded as used for included expression, with unavailable identity represented as unknown
- Representative transformations linked to their CPL basis
- Suggested `Author Created`, `Material Excluded`, and `New Material Included` language when supported by the selected policy profile
- Every user declaration, including author identity, labeled as a declaration and identified as self-declared unless stronger evidence is present
- Limitation, sanitization, and omission disclosures
- The policy-profile ID, version, source retrieval date, and source digests or stable source references
- A verification report and manifest binding every emitted artifact and digest
- A clear statement that the U.S. Copyright Office, not Thinkloom, determines copyrightability and registration scope

Coverage MUST describe only the scope of recorded evidence. For example, it MAY state that a percentage of normalized Unicode scalar positions has recorded origin. It MUST NOT relabel that denominator as “human,” “human-authored,” “original,” or “copyrightable.” Selection and arrangement MUST appear as relationship overlays and MUST NOT change the recorded-origin layer.

Suggested registration language MUST be generated only after the user selects a deposit and policy profile, reviews the evidence classifications and limitations, edits or accepts the proposed text, and explicitly approves generation. Approval MUST bind the exact approved strings and their source HARP inputs in CPL. Thinkloom MUST NOT submit an application, assert that the language is legally sufficient, or silently replace user-approved language when a policy profile changes.

### 15.4 HARP verification and traceability

HARP verification is a scoped integrity operation. It MUST verify artifact hashes, manifest bindings, deposit bytes, CPL source head and records, schema and generator compatibility, policy-profile binding, sanitization disclosures, and deterministic regeneration when the required generator is available.

Every factual claim and suggested treatment in a HARP MUST expose a trace path to the supporting CPL event, record, assertion, evaluation, and any user approval. A successful verification MUST be labeled **HARP integrity verified** and MUST show the exact deposit digest. It MUST NOT be labeled **authorship verified**.

The following states are independent and MUST NOT be collapsed:

- CPL verification status under §15
- HARP integrity verification status
- HARP applicability: `current` or `stale`
- Evidence boundary: `exact`, `degraded`, `stale`, `unverified`, `unknown`, or `unattested`
- Policy-profile currency: `current`, `superseded`, or `unavailable`
- User approval status for suggested registration language

### 15.5 Initial U.S. Copyright Office policy profile

The initial policy profile MUST be a versioned, read-only profile scoped to **United States / literary work / Standard Application**. It MUST identify its effective application scope and MUST refuse to produce application-field suggestions for unsupported jurisdictions, work classes, group registrations, or application types.

The initial profile MUST cite, at minimum, the following official sources with retrieval date `2026-07-19`:

- [Copyright Registration Guidance: Works Containing Material Generated by Artificial Intelligence](https://www.copyright.gov/ai/ai_policy_guidance.pdf), effective March 16, 2023
- [Copyright and Artificial Intelligence, Part 2: Copyrightability](https://www.copyright.gov/ai/Copyright-and-Artificial-Intelligence-Part-2-Copyrightability-Report.pdf), January 2025
- [Standard Application Help: Author](https://www.copyright.gov/eco/help-author.html)
- [Standard Application Help: Limitation of Claim](https://www.copyright.gov/eco/help-limitation.html)
- [37 C.F.R. § 202.3](https://www.copyright.gov/title37/202/37cfr202-3.html)

The profile MUST encode field terminology separately from evidentiary classifications and MUST support suggested text for `Author Created`, `Material Excluded`, `New Material Included`, and, when appropriate, `Note to CO`. The `Author Created` and `New Material Included` suggestions MUST remain mutually consistent. The profile MUST disclose recorded use of AI and MUST support describing human contributions and excluding more-than-de-minimis AI-generated content as directed by the cited guidance, while leaving case-specific judgment and the final wording to the user and the Office. Every suggestion screen and generated worksheet MUST state that the text is suggested, editable, not legal advice, and subject to Copyright Office review.

Policy rules MUST NOT convert CPL activity metrics or recorded-origin categories into copyrightability conclusions. Prompts, selection/arrangement, and modifications MAY be described as evidence only; their legal sufficiency MUST remain undetermined. A published profile is immutable. A policy update creates a new profile version; it MUST NOT silently change or regenerate an existing HARP. A HARP bound to a superseded profile remains historically reproducible and MUST visibly disclose that profile status.

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

## 23. Legacy preview projects and CPL conformance boundary

Only a project with the exact, supported conformance marker below MAY be treated as CPL-conforming:

```json
{
  "project_format": "thinkloom-cpl",
  "project_format_version": "1.0",
  "provenance_conformance": "cpl-1.0"
}
```

The marker is necessary but not sufficient: schema compatibility, required CPL structure, startup recovery, and native verification gates still apply. A `schemaVersion: 1.0` field or any preview-era marker MUST NOT be interpreted as this conformance marker.

Thinkloom 0.6 and later MUST detect projects created by any v0.5.x release or earlier and other known preview/experimental project markers before any project mutation. It MUST refuse normal opening, editing, provenance regeneration, CPL verification, and HARP generation. It MUST preserve the original project untouched, explain that migration is deferred until after Thinkloom 1.0.0, offer Show Project Folder, and permit a byte-preserving raw archival ZIP labeled:

```text
Legacy preview-project preservation archive
Not verified, converted, or CPL-conforming
```

Thinkloom MUST NOT import legacy records into CPL schema 1.0, synthesize CPL events from legacy state, regenerate or verify their provenance under CPL rules, create a HARP or CPL evidence report, modify their Git history, or present them as migrated or partially conforming. Formal legacy reading, conversion, and migration begin only after Thinkloom 1.0.0 and require a future normative migration specification.

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
provenance-assertion.schema.json
assertion-evaluation.schema.json
verification-report.schema.json
backup-manifest.schema.json
release-manifest.schema.json
release-state.schema.json
sanitized-export-manifest.schema.json
purge-manifest.schema.json
composition-operation.schema.json
expression-segment.schema.json
contribution-map.schema.json
deposit-snapshot.schema.json
registration-policy-profile.schema.json
human-authorship-record.schema.json
harp-export-manifest.schema.json
```

Prompt-template, provenance-assertion, contribution-map, registration-policy-profile, human-authorship-record, HARP export-manifest, and other self-digesting schemas MUST define exact digest identity objects. Migration schemas are deferred until after Thinkloom 1.0.0.

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
