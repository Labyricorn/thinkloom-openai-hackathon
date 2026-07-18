# Thinkloom implementation status

Status date: 2026-07-17

## Completed locally

- M0 engineering baseline: native-only React/TypeScript interface, Tauri 2 shell, typed command errors, Windows packaging configuration, and build/type/lint/unit checks.
- M1 project and provenance foundation: project layout, SQLite schema, atomic canonical writes, rotating snapshots, SHA-256 event chain, chain verification, hidden Git checkpoints, and canonical rebuild inputs.
- M2 provider/privacy boundary: Ollama, OpenAI and compatible request paths; OS vault secrets; provider test; local/cloud status; first-cloud-use approval; and retry-preserving failure state.
- M3 ideation and idea curation: typed turns, one focused question, challenge modes, suggestion review, direct save, edits, variants, merge, tags, archive/rejected history, and drafting sets.
- M4 drafting workspace: TipTap structured editor, dominant three-panel layout, selective context, staged generation, all acceptance destinations, partial acceptance, manual editing, undo/redo, and autosave.
- M5 finalization/style/history: editorial action previews, editable style traits and disallowed habits, saved versions, comparison summary, restore, release checkpoints, provenance timeline, and contribution relationships.
- M7 export/backup/evidence: Markdown, HTML, PDF, text, backup ZIP, evidence ZIP, sanitization disclosure, hashes, manifest, atomic finalization, and strict ZIP import validation.
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

- Replace the current MVP provenance persistence path with the approved [Thinkloom 1.0 Stage 1 normative provenance subsystem](docs/provenance/README.md). The specification is complete; formal schemas, fixtures, native implementation, and fault-injection conformance remain future stages.

- Run microphone/VAD/transcription integration tests once the approved model assets are supplied.
- Run signed clean-install, upgrade, downgrade-warning, and uninstall tests on each release platform.
- Run full 20,000-word interaction and fault-injection profiling on packaged release hardware.
