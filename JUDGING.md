# Judge Testing Guide

## Supported platform

- Windows 11 x64
- Windows WebView2 runtime (included with current Windows 11 installations)
- No Thinkloom account required

The current hackathon build is unsigned. Windows may show an unknown-publisher or SmartScreen warning. Signing is not required to inspect the source or use the portable release executable, but a judge should follow their organization's security policy.

## Install without rebuilding

Before Devpost submission, replace the pending item below with the public GitHub Release URL and attach both existing artifacts:

- **Pending public release URL**
- `Thinkloom_0.1.0_x64-setup.exe` (NSIS installer)
- `Thinkloom_0.1.0_x64_en-US.msi` (MSI installer)

Current local artifact checksums:

```text
E70F700C04E995685160DDA70FD811912A55DD362A28F63DFBE297040C6A0BE0  Thinkloom_0.1.0_x64-setup.exe
5B2F5072E8A0BB24E34E2AB3FB0EE81574E74D67FCEDB4BC03D0FD4DAEB918C3  Thinkloom_0.1.0_x64_en-US.msi
```

These checksums must be regenerated if either installer is rebuilt.

## Five-minute test path

1. Launch Thinkloom. The bundled sample project, **The Attention Commons**, opens without a login.
2. In **Ideation**, review the conversation and idea provenance. Accept, reject, edit, archive, or create a variant of a suggested idea.
3. Open **Drafting**. Select an idea, edit the structured manuscript directly, and save a named version.
4. Open **Finalization**. Inspect the release checklist and preview-first editorial actions.
5. Open **History** and **Provenance** to inspect the hash-linked creative-process record and contribution relationships.
6. Open **Export** to inspect publication, backup, and sanitized evidence-package options.

## Model-backed features

The sample project and direct editing/history/export surfaces can be inspected without a model credential. To exercise live generation, use one of these entrant-configured options:

- local Ollama at `http://127.0.0.1:11434` (default model `llama3.2`);
- OpenAI with a credential stored in the operating-system vault; or
- an authorized OpenAI-compatible endpoint.

The first cloud request for a project requires explicit in-app approval. Credentials are not included in the repository, project files, logs, or judge build.

## Known limitations

- The build is currently unsigned.
- Windows is the verified hackathon platform; macOS and Linux have not been release-tested.
- Voice input uses an ephemeral system/webview fallback. The planned bundled faster-whisper and Silero VAD pipeline is not included.
- Richer PDF typography, signed clean-install testing, and extended 20,000-word profiling remain release-hardening items.

These limitations do not affect the demonstrated typed ideation, structured drafting, provenance, versioning, or export workflow.
