import { useMemo, useState } from "react";
import "./export.css";

interface ExportArtifact {
  role: string;
  path: string;
  sha256: string;
  byteLength: number;
  privacyClassification: string;
}

interface ArchiveVerification {
  status: string;
  archiveSha256: string;
  retainedFileCount: number;
  omissionDisclosureCount: number;
  completenessClaim: string;
  verificationScopeStatement: string;
  findings: string[];
}

interface HarpExportResult {
  exportDirectory: string;
  artifacts: ExportArtifact[];
  sanitizedManifest: {
    omission_rules?: Array<{ category: string; action: string; count: number; retained_binding_sha256: string; disclosure_sha256: string }>;
  };
  sanitizedVerification: ArchiveVerification;
}

async function invokeNative<T>(command: string, args: Record<string, unknown> = {}): Promise<T | null> {
  const host = window as unknown as { __TAURI_INTERNALS__?: { invoke: (name: string, payload: Record<string, unknown>) => Promise<T> } };
  return host.__TAURI_INTERNALS__ ? host.__TAURI_INTERNALS__.invoke(command, args) : null;
}

function message(error: unknown): string {
  if (typeof error === "object" && error && "message" in error) return String((error as { message: unknown }).message);
  return error instanceof Error ? error.message : String(error);
}

function label(value: string): string {
  return value.replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function short(value: string, length = 28): string {
  return value.length > length ? `${value.slice(0, length)}…` : value;
}

export default function HarpExportPanel({ onNotice }: { onNotice: (notice: string) => void }) {
  const [redactIdentifiers, setRedactIdentifiers] = useState(true);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<HarpExportResult | null>(null);
  const [verification, setVerification] = useState<ArchiveVerification | null>(null);
  const sanitizedArchive = useMemo(() => result?.artifacts.find((artifact) => artifact.role === "sanitized_supporting_archive") ?? null, [result]);

  const create = async () => {
    setBusy(true);
    try {
      const next = await invokeNative<HarpExportResult>("export_harp_artifacts", { request: { redactPersonalIdentifiers: redactIdentifiers } });
      if (!next) {
        onNotice("HARP registration exports are available in the installed Thinkloom desktop app.");
        return;
      }
      setResult(next);
      setVerification(next.sanitizedVerification);
      onNotice(`Created six HARP export artifacts in ${next.exportDirectory}.`);
    } catch (error) {
      onNotice(`HARP export could not be created: ${message(error)}`);
    } finally {
      setBusy(false);
    }
  };

  const verify = async () => {
    if (!sanitizedArchive) return;
    setBusy(true);
    try {
      const next = await invokeNative<ArchiveVerification>("verify_harp_sanitized_archive", { archivePath: sanitizedArchive.path });
      if (next) {
        setVerification(next);
        onNotice(next.status === "verified_selective" ? "Sanitized archive retained evidence and omission disclosures verified." : "Sanitized archive verification failed.");
      }
    } catch (error) {
      onNotice(`Sanitized archive could not be verified: ${message(error)}`);
    } finally {
      setBusy(false);
    }
  };

  return <article className="export-card harp-export-card">
    <div className="harp-export-heading">
      <div><span className="export-type">Registration + provenance</span><h2>HARP artifact set</h2><p>Create separate registration, deposit, sanitized, and private artifacts from the current exact HARP.</p></div>
      <span className="privacy-lock">Native export boundary</span>
    </div>
    <div className="artifact-blueprint" aria-label="Six HARP export artifacts">
      {[
        ["Registration worksheet", "Suggested, editable application fields"],
        ["Human-readable HARP", "Evidence narrative and legal limits"],
        ["Machine-readable HARP", "Canonical HARP JSON"],
        ["Deposit copy", "Exact digest-bound manuscript"],
        ["Sanitized archive", "Selective disclosed evidence"],
        ["Full private archive", "Private CPL records and source bodies"],
      ].map(([title, detail], index) => <div key={title}><i>{index + 1}</i><span><strong>{title}</strong><small>{detail}</small></span></div>)}
    </div>
    <div className="privacy-choice">
      <label><input type="checkbox" checked={redactIdentifiers} onChange={(event) => setRedactIdentifiers(event.target.checked)} /><span><strong>Redact the declared author name in sanitized presentations</strong><small>The exact private machine-readable HARP and deposit copy remain separate and unchanged.</small></span></label>
      <p><strong>Private archive warning:</strong> the full archive contains private CPL records, ledger metadata, source bodies, and the exact deposit. Review it before sharing.</p>
    </div>
    <button className="primary-button create-harp-export" disabled={busy} onClick={() => void create()}>{busy ? "Creating and verifying…" : "Create six HARP export artifacts"}</button>

    {result && <div className="export-result" aria-live="polite">
      <div className="artifact-results">{result.artifacts.map((artifact) => <div key={artifact.role}><span><strong>{label(artifact.role)}</strong><small>{label(artifact.privacyClassification)} · {artifact.byteLength.toLocaleString()} bytes</small></span><code title={artifact.sha256}>{short(artifact.sha256)}</code><small>{artifact.path}</small></div>)}</div>
      <section className="omission-disclosure">
        <div><span className="eyebrow">Sanitized omission manifest</span><h3>Every category is disclosed and hash-bound</h3><p>The archive is a selective evidence subset. Verification does not claim the omitted private history is present.</p></div>
        <div className="omission-grid">{result.sanitizedManifest.omission_rules?.map((rule) => <article key={rule.category}><strong>{label(rule.category)}</strong><span>{label(rule.action)} · {rule.count} affected record{rule.count === 1 ? "" : "s"}</span><code title={rule.disclosure_sha256}>{short(rule.disclosure_sha256, 24)}</code></article>)}</div>
      </section>
      {verification && <section className={`archive-verification verification-${verification.status}`}>
        <div><span className="eyebrow">Retained-evidence verification</span><h3>{label(verification.status)}</h3><p>{verification.verificationScopeStatement}</p><small>{verification.retainedFileCount} retained files · {verification.omissionDisclosureCount} omission disclosures</small><code>{verification.archiveSha256}</code>{verification.findings.map((finding) => <p key={finding}>{finding}</p>)}</div>
        <button className="secondary-button" disabled={busy} onClick={() => void verify()}>Re-verify sanitized archive</button>
      </section>}
    </div>}
  </article>;
}
