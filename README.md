# Thinkloom

Thinkloom is a native, local-first writing studio that helps one writer move from conversation to ideas, draft, revision, and a versioned release while preserving an inspectable creative-process record.

The application is built with Tauri 2, React, TypeScript, Rust, SQLite, canonical Markdown/JSON/JSONL files, hidden Git checkpoints, operating-system credentials, and atomic exports. There is no hosted or browser companion build.

## Implemented workflow

- Reversible Ideation, Drafting, and Finalization phases
- Typed conversation, challenge levels, push-to-talk transcription, and optional visible speech output
- Suggested ideas with explicit accept/reject, editing, variants, archiving, source links, drafting sets, and merges
- TipTap/ProseMirror structured manuscript editor with canonical Markdown round-tripping, undo/redo, headings, lists, selection replacement, and cursor insertion
- Persisted preview-first generation states with retry-safe provider errors and partial acceptance
- Ollama, OpenAI, and OpenAI-compatible provider profiles; credentials use the operating-system vault
- Local/Cloud/Mixed status and project-scoped cloud approval
- SQLite live state, atomic canonical files, seven rotating recovery snapshots, and a hidden Git repository per project
- Append-only SHA-256 provenance journal with chain-head verification and a contribution relationship view
- Named versions, restore controls, release checkpoints, and tags using non-Git language in the UI
- Markdown, HTML, PDF, plain text, sanitized evidence ZIP, and complete project backup ZIP generation
- ZIP import path, symlink, file-count, and expanded-size validation
- Responsive, keyboard-navigable, screen-reader-labeled UI with reduced-motion and dark-mode support

## Development

Requirements: Node.js 22.13 or newer, Rust 1.77.2 or newer, Git, and the Windows WebView2 runtime for the primary desktop target.

```powershell
npm install
npm run dev
npm run build
npm run tauri -- dev
npm run tauri -- build
```

Quality checks:

```powershell
npm run typecheck
npm run lint
npm test
cd src-tauri
cargo fmt --check
cargo test
```

## Project storage

A Thinkloom project is self-contained. Canonical files live under `manuscript/`, `ideas/`, `conversations/`, `provenance/`, and `style/`. Live SQLite state and rotating snapshots are under `.thinkloom/` and are excluded from the project's hidden Git history. Audio retention is always false; no audio file extension is created by the native service.

## Provider setup

Ollama defaults to `http://127.0.0.1:11434` and model `llama3.2`. OpenAI and compatible credentials are entered in Settings and saved through Windows Credential Manager, macOS Keychain, or Linux Secret Service. The first cloud operation in each project requires explicit approval.

See [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) for release gates that require external models, signing credentials, or additional platform validation.
