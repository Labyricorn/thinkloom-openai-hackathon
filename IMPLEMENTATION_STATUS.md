# Thinkloom implementation status

Status date: 2026-07-19

Release convention: provenance Milestone N maps to application version `0.5.N`; this progression is assumed for subsequent milestones unless explicitly overridden.

## Completed locally

- M0 engineering baseline: native-only React/TypeScript interface, Tauri 2 shell, typed command errors, Windows packaging configuration, and build/type/lint/unit checks.
- M1 project and provenance foundation: project layout, SQLite schema, atomic canonical writes, rotating snapshots, SHA-256 event chain, chain verification, hidden Git checkpoints, and canonical rebuild inputs.
- M2 provider/privacy boundary: Ollama, OpenAI and compatible request paths; OS vault secrets; provider test; local/cloud status; first-cloud-use approval; and retry-preserving failure state.
- M3 ideation and idea curation: typed turns, one focused question, challenge modes, suggestion review, direct save, edits, variants, merge, tags, archive/rejected history, and drafting sets.
- M4 drafting workspace: TipTap structured editor, dominant three-panel layout, selective context, staged generation, all acceptance destinations, partial acceptance, manual editing, undo/redo, and autosave.
- M5 finalization/style/history: editorial action previews, editable style traits and disallowed habits, saved versions, comparison summary, restore, release checkpoints, provenance timeline, and contribution relationships.
- M7 export/backup/evidence: Markdown, HTML, PDF, text, backup ZIP, evidence ZIP, sanitization disclosure, hashes, manifest, atomic finalization, and strict ZIP import validation.
- Provenance Stage 2 contract (v0.3.0): 38 JSON Schema Draft 2020-12 schemas, valid/invalid fixture suites, canonical JSON and JSONL vectors, event-chain vectors, protected-record/key-rotation vectors, retention-policy vectors, and a formal release-files Merkle construction.
- Canonical assertion envelopes (v0.4.0): immutable provenance assertions, point-in-time evaluations, six semantic registries, independent confidence dimensions, evidence classes, stable reason codes, and deterministic dependency-invalidation vectors.
- HARP/CPL normative amendment (v0.5.1): one unambiguous CPL ledger meaning; exact-deposit, non-authoritative HARP projection; integrity-only cryptographic claims; contribution-dimension separation; explicit user approval; U.S. literary Standard Application policy profile; HARP staleness; prohibited UI claims; and the v0.5.x preview-project boundary.
- Composition/HARP schema extension (v0.5.2): seven additive schemas, eight independent semantic registries, exhaustive valid/invalid fixtures, deterministic contribution-map and exact-deposit HARP vectors, self-digest identities, staleness checks, v0.4 assertion compatibility, and declared v0.6.0 schema compatibility.
- Native CPL service (v0.5.3): modular Rust provenance boundary; NFC plus RFC 8785-compatible canonicalization; prefixed sortable IDs; exact millisecond timestamps; OS-level exclusive writer locks; stable action idempotency; durable SQLite intents; same-filesystem immutable-record staging; active and sealed ledger segments; chain-head recovery and quarantine; rebuildable indexes; structured native verification reports; and deterministic crash injection across record, ledger, head, SQLite, and rotation boundaries.
- Conforming project boundary (v0.5.4): exact CPL 1.0 marker; formal project manifest; conforming `records/`, segmented `provenance/ledger/`, `reports/`, and `.app/` layout; inspection-before-recovery classification; verified-only editable activation; read-only legacy refusal; Show Project Folder; byte-preserving preservation archives; and explicit legacy backup-import rejection.
- Phase 1 CPL routing (v0.5.5): typed native commands and operation-specific immutable records for turns, assistant responses, sessions, context, ideas, drafting-paper changes, distillation, external material, voice transcription, and provider lifecycle; request-before-I/O ordering; response/failure capture; writer-lock-free provider I/O; and deterministic UI reconstruction from canonical records and ledger events.
- Manuscript composition and lineage (v0.5.6): instrumented TipTap transaction capture at meaningful boundaries; typed origins for direct human, transcription, AI, paste/import, restoration, and unattested expression; shared drafting/finalization path; Unicode-scalar lineage preserving AI preimages through human revision; invocation-bound partial AI dispositions; writer-lock-atomic preimage validation; and deterministic manuscript/span replay from canonical composition records.
- Contribution-map projection (v0.5.7): exact frozen Markdown deposits; deterministic bytewise span normalization and structural splitting; complete non-overlapping Unicode-scalar coverage; stable ancestry through split/merge; chapter, paragraph, and fixed-layout page locators; independent inclusion/selection/arrangement assertions with current evaluations; explicit stale, degraded, unverified, and unattested boundaries; and coverage wording that cannot be presented as a human-authorship percentage.
- Deterministic HARP generation (v0.5.8): explicit approval recorded before generation; canonical exact-deposit HARP and manifest records; one-page summary, visual contribution map, representative transformations, AI-system disclosure, coverage/limitations, suggested registration language, verification material, and supporting archive manifest; shared report metadata; sanitization-aware excerpts; idempotent regeneration; and deterministic manuscript, deposit, policy, assertion, dependency, approval, and verification staleness.
- HARP/CPL user interfaces (v0.5.9): native verification and composition timeline; immutable-record inspection; final-expression lineage; assertion/evaluation detail; explicit exact, degraded, stale, and unverified boundaries; HARP statement-to-record traces; seven-step preparation wizard; evidence/declaration/derived/suggested/legal category separation; stale resolution and non-exact acceptance gates; archive selection; and resettable explicit generation approval.
- HARP export, privacy, and security (v0.5.10): separate registration worksheet, human-readable HARP, canonical machine HARP, exact deposit copy, sanitized archive, and full-private archive; current-HARP and exact-deposit gates; allowlisted sanitized content; user-selected identity redaction; seven HARP-specific omission categories with per-disclosure and aggregate hashes; retained-file and source-binding verification; explicit selective-completeness scope; non-mutating CPL export; and private-archive warnings.
- Verification matrix and release safety (v0.5.11): executable coverage for every required schema, canonicalization, concurrency, crash-recovery, rotation, composition-lineage, voice, staleness, Unicode, native/frontend consistency, sanitized disclosure, and prohibited-wording scenario; origin-preserving selection/arrangement; authoritative release blocking for incomplete, failed, or unsafe verification; and packaged Windows executable/MSI/NSIS launch verification.
- Product surface: Sites, Next, Vinext, Cloudflare, and browser-companion code and configuration have been removed. Tauri is the sole application target.

## External release gates

These items cannot be honestly certified from this workspace alone and need project-owner decisions or assets:

1. Voice runtime: provide or approve the faster-whisper model size and Silero VAD ONNX model distribution. The interface currently uses ephemeral webview speech recognition as a functional fallback and retains no audio, but the required bundled local voice pipeline is not yet present.
2. Local-model acceptance baseline: confirm the Ollama model used for release testing. The implementation default is `llama3.2`.
3. Signing and distribution: provide the Windows code-signing certificate and choose the installer distribution channel. MSI/NSIS configuration is present, but signed release artifacts cannot be produced without credentials.
4. Secondary platforms: decide whether macOS and Linux are part of the first signed release. Their keychain, microphone, installer, and packaging gates require platform runners.
5. PDF typography: the native exporter creates a valid dependency-free PDF. Approve whether the release should instead bundle a native print engine for richer typography.
6. Diagnostics policy: confirm whether opt-in diagnostic reports are allowed. Current diagnostics are local, redacted, and exclude prompts, responses, credentials, and audio.

## Remaining hardening

- Run microphone/VAD/transcription integration tests once the approved model assets are supplied.
- Run signed clean-install, upgrade, downgrade-warning, and uninstall tests on each release platform.
- Run full 20,000-word interaction and fault-injection profiling on packaged release hardware.
