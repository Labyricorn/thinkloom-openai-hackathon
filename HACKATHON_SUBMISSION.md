# OpenAI Build Week Submission

Status date: July 17, 2026

## Submission identity

- Project: Thinkloom
- Entrant: Christopher Chambers (individual)
- Track: Apps for Your Life
- Created: July 16, 2026, during the submission period
- Primary Codex build task: `019f6edd-de3d-78d2-8b6f-67af5fe6c44c`
- `/feedback` session ID: `019f6edd-de3d-78d2-8b6f-67af5fe6c44c`
- Public repository: `https://github.com/Labyricorn/thinkloom-openai-hackathon`
- Public release/testing URL: `https://github.com/Labyricorn/thinkloom-openai-hackathon/releases/tag/v0.1.0`
- Public YouTube demo URL: **pending**
- License: GNU Affero General Public License v3.0 only

## Submission description

Thinkloom is a native, local-first writing studio for people who want AI assistance without surrendering control of either their prose or their creative process. It guides one writer through three reversible phases: ideation, drafting, and finalization.

In Ideation, the writer explores a subject through conversation and decides which suggested ideas to accept, reject, edit, merge, archive, or carry into a drafting set. In Drafting, selected ideas sit beside a structured manuscript editor and an assistant workspace. Generated prose is always staged for review; nothing enters the manuscript until the writer explicitly accepts all or part of it. In Finalization, the writer can preview editorial changes, save restorable versions, inspect history, and produce a release.

Thinkloom is local-first by design. Projects use SQLite for live state and canonical Markdown, JSON, and JSONL files for portability and recovery. An append-only SHA-256 chain records meaningful creative events, while hidden Git checkpoints support versions and restoration without exposing Git terminology in the interface. The app can export Markdown, HTML, PDF, plain text, complete backups, and sanitized authorship-evidence packages. Provider credentials remain in the operating-system vault, cloud use requires project-level approval, and audio is not retained.

Christopher Chambers created Thinkloom with Codex using GPT-5.6 Sol at High reasoning effort. Christopher made the central product, audience, privacy, and design decisions. Codex accelerated the work by converting the MVP specifications into an interactive framework and implementation plan, implementing the Tauri/React/Rust application, integrating structured editing and native persistence, debugging packaging, and running the verification suite. When an early implementation introduced a hosted companion, Christopher rejected that boundary and Codex removed the hosted stack, leaving Tauri as the sole product surface.

Thinkloom demonstrates a specific form of human-AI collaboration: the system can help develop both the software and the writing, while the person keeps explicit control over product direction, manuscript changes, and the evidence of how the work evolved.

## Feature summary

- Reversible Ideation, Drafting, and Finalization phases
- Explicit accept/reject/edit/variant/merge controls for suggested ideas
- TipTap/ProseMirror structured manuscript editing with Markdown round-tripping
- Preview-first full and partial acceptance for model output
- Local Ollama, OpenAI, and authorized OpenAI-compatible providers
- OS-vault credentials and explicit first-cloud-use approval
- SQLite state, canonical files, atomic writes, rotating snapshots, and hidden checkpoints
- Append-only SHA-256 provenance verification and contribution relationships
- Named versions, comparison, restore, and release checkpoints
- Publication, backup, and sanitized evidence exports
- Strict ZIP import path, symlink, entry-count, and expanded-size validation

## Compliance checklist

- [x] Individual entrant eligibility and lack of conflict confirmed by Christopher Chambers
- [x] Project created during the submission period
- [x] Codex and GPT-5.6 Sol use confirmed by the entrant
- [x] Dated Git history exists for July 16-17, 2026
- [x] Apps for Your Life track approved by the entrant
- [x] Text description prepared
- [x] README includes the required Codex collaboration account and decision split
- [x] Windows application builds and launches
- [x] Type checking, linting, frontend tests, and Rust tests pass
- [x] Judge installation/testing guide prepared
- [x] Demo narration and shot list prepared
- [x] Entrant confirms sole ownership of the original project name, icon, example content, and other original assets
- [x] AGPL-3.0-only license selected and added
- [x] `/feedback` is submitted from the primary build task and the resulting session ID is added
- [x] GitHub CLI is installed and authenticated
- [x] Public repository is created and its URL is verified
- [x] Installers are attached to a public release available through the end of judging
- [ ] Demo is recorded with audio, kept under three minutes, uploaded publicly to YouTube, and linked
- [ ] Demo is reviewed for unlicensed music, third-party material, private data, and incidental trademarks
- [ ] Devpost submission is created and all required fields are completed
- [ ] Repository, video, build, and testing links are tested in a signed-out/private browser window
- [ ] Submission is finalized before July 21, 2026 at 5:00 p.m. Pacific

## Devpost fields still requiring the entrant

- Public YouTube URL
- Any profile information requested by Devpost
- Final agreement to the Official Rules when submitting
