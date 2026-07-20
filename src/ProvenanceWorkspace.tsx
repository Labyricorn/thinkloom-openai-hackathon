import { useCallback, useEffect, useMemo, useState } from "react";
import "./provenance.css";

type Classification = "exact" | "degraded" | "stale" | "unverified";
type EvidenceCategory = "evidence_fact" | "user_declaration" | "derived_classification" | "suggested_application_language" | "legal_determination_not_made";

interface NativeFinding { code: string; severity: string; scope: string; message: string }
interface NativeVerification { status: string; event_count: number; record_count: number; findings: NativeFinding[]; chain_head?: { event_id: string; event_sequence: number; event_sha256: string } }
interface ExplorerRecord { record_id: string; record_type: string; path: string; record_sha256: string; subject_ids: string[]; accessible: boolean }
interface ExplorerEvent { event_id: string; event_sequence: number; timestamp: string; event_type: string; actor: string; event_sha256: string; previous_event_sha256: string | null; records: ExplorerRecord[] }
interface MapSegment {
  segment_id: string;
  segment_sequence: number;
  range: { start: number; end: number };
  recorded_origin_kind: string;
  actor_identity_status: string;
  generation_status: string;
  lineage_status: string;
  lineage_reference_ids: string[];
  operation_ids: string[];
  transformation_relationships: string[];
  assertion_ids: string[];
  classification_status: Classification;
}
interface MapBoundary { boundary_id: string; status: string; kind: string; start: number; end: number; message: string }
interface ContributionMapProjection {
  deposit: { deposit_id: string; deposit_sha256: string; manuscript_revision_id: string; manuscript_revision_sha256: string; cpl_chain_head: string; cpl_event_sequence: number; deposit_path: string };
  contribution_map: { contribution_map_id: string; contribution_map_sha256: string; classification_status: Classification; coverage: { denominator: number; recorded_positions: number; coverage_status: string; denominator_definition: string }; segments: MapSegment[]; structural_locators: Array<{ segment_id: string; chapter: number | null; paragraph: number | null; page: number | null }> };
  assertions: Record<string, unknown>[];
  assertion_evaluations: Record<string, unknown>[];
  boundaries: MapBoundary[];
}
interface HarpProjection { harp: Record<string, unknown>; report_directory: string; applicability_status: string; stale_reasons: string[]; generated_files: Array<{ role: string; path: string; sha256: string }> }
interface HarpTrace { statement_id: string; harp_path: string; statement: string; category: EvidenceCategory; segment_ids: string[]; assertion_ids: string[]; evaluation_ids: string[]; record_ids: string[]; trace_note: string }
interface ExplorerProjection { verification: NativeVerification; events: ExplorerEvent[]; composition: { revisionId: string; operationCount: number; spans: unknown[] }; contribution_map: ContributionMapProjection | null; harp: HarpProjection | null; harp_statement_traces: HarpTrace[] }
interface AiDisclosure { provider_id: string | null; model_id: string | null; identity_status: "recorded" | "unknown"; included_expression_segment_ids: string[] }
interface LanguageInput { authorCreated: string; materialExcluded: string; newMaterialIncluded: string; noteToCo: string; registrationTreatmentSuggestions: string[] }
interface HarpPreparation { contributionMap: ContributionMapProjection; aiSystemDisclosures: AiDisclosure[]; existingHarp: HarpProjection | null; policyProfile: { policy_profile_id: string; profile_version: string; profile_sha256: string; official_sources: Array<{ retrieved_on: string }> }; suggestedRegistrationLanguage: { author_created: string; material_excluded: string; new_material_included: string; note_to_co: string | null; registration_treatment_suggestions: string[] }; legalScopeStatement: string }

const wizardSteps = ["Deposit", "Identity", "AI systems", "Classifications", "Boundaries", "Language", "Approve"];
const categoryLabels: Record<EvidenceCategory, string> = {
  evidence_fact: "Evidence fact",
  user_declaration: "User declaration",
  derived_classification: "Derived classification",
  suggested_application_language: "Suggested application language",
  legal_determination_not_made: "Legal determination not made",
};

async function invokeNative<T>(command: string, args: Record<string, unknown> = {}): Promise<T | null> {
  const host = window as unknown as { __TAURI_INTERNALS__?: { invoke: (name: string, payload: Record<string, unknown>) => Promise<T> } };
  return host.__TAURI_INTERNALS__ ? host.__TAURI_INTERNALS__.invoke(command, args) : null;
}

function nativeMessage(error: unknown): string {
  if (typeof error === "object" && error && "message" in error) return String((error as { message: unknown }).message);
  return error instanceof Error ? error.message : String(error);
}

function short(value: unknown, length = 22): string {
  const text = typeof value === "string" ? value : String(value ?? "unknown");
  return text.length > length ? `${text.slice(0, length)}…` : text;
}

function displayName(value: string): string {
  return value.replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase());
}

function StatusBadge({ value }: { value: string }) {
  const normalized = value.toLowerCase();
  return <span className={`provenance-status status-${normalized}`}>{displayName(value)}</span>;
}

function CategoryBadge({ category }: { category: EvidenceCategory }) {
  return <span className={`evidence-category category-${category}`}>{categoryLabels[category]}</span>;
}

export default function ProvenanceWorkspace({ onNotice }: { onNotice: (message: string) => void }) {
  const [mode, setMode] = useState<"explorer" | "wizard">("explorer");
  const [explorer, setExplorer] = useState<ExplorerProjection | null>(null);
  const [preparation, setPreparation] = useState<HarpPreparation | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [selectedEventId, setSelectedEventId] = useState("");
  const [selectedSegmentId, setSelectedSegmentId] = useState("");
  const [selectedTraceId, setSelectedTraceId] = useState("");
  const [step, setStep] = useState(0);
  const [declaredName, setDeclaredName] = useState("");
  const [identityStatus, setIdentityStatus] = useState<"self_declared" | "verified" | "unknown">("self_declared");
  const [acceptedBoundaries, setAcceptedBoundaries] = useState<Set<string>>(new Set());
  const [sanitizationProfile, setSanitizationProfile] = useState<"full_private" | "sanitized">("sanitized");
  const [language, setLanguage] = useState<LanguageInput>({ authorCreated: "", materialExcluded: "", newMaterialIncluded: "", noteToCo: "", registrationTreatmentSuggestions: [] });
  const [approved, setApproved] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const nextExplorer = await invokeNative<ExplorerProjection>("load_cpl_explorer");
      if (!nextExplorer) {
        setError("The native CPL explorer is available in the installed Thinkloom desktop app.");
        return;
      }
      setExplorer(nextExplorer);
      setSelectedEventId((current) => current || nextExplorer.events.at(-1)?.event_id || "");
      setSelectedSegmentId((current) => current || nextExplorer.contribution_map?.contribution_map.segments[0]?.segment_id || "");
      setSelectedTraceId((current) => current || nextExplorer.harp_statement_traces[0]?.statement_id || "");
      if (nextExplorer.contribution_map) {
        const nextPreparation = await invokeNative<HarpPreparation>("prepare_harp");
        if (nextPreparation) {
          setPreparation(nextPreparation);
          setLanguage((current) => current.authorCreated ? current : {
            authorCreated: nextPreparation.suggestedRegistrationLanguage.author_created,
            materialExcluded: nextPreparation.suggestedRegistrationLanguage.material_excluded,
            newMaterialIncluded: nextPreparation.suggestedRegistrationLanguage.new_material_included,
            noteToCo: nextPreparation.suggestedRegistrationLanguage.note_to_co ?? "",
            registrationTreatmentSuggestions: nextPreparation.suggestedRegistrationLanguage.registration_treatment_suggestions,
          });
        }
      } else {
        setPreparation(null);
      }
    } catch (cause) {
      setError(nativeMessage(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => { void refresh(); }, 0);
    return () => window.clearTimeout(timer);
  }, [refresh]);

  const selectedEvent = explorer?.events.find((event) => event.event_id === selectedEventId) ?? null;
  const map = explorer?.contribution_map ?? preparation?.contributionMap ?? null;
  const selectedSegment = map?.contribution_map.segments.find((segment) => segment.segment_id === selectedSegmentId) ?? null;
  const selectedAssertions = useMemo(() => map?.assertions.filter((assertion) => selectedSegment?.assertion_ids.includes(String(assertion.assertion_id))) ?? [], [map, selectedSegment]);
  const selectedEvaluations = useMemo(() => map?.assertion_evaluations.filter((evaluation) => selectedSegment?.assertion_ids.includes(String(evaluation.assertion_id))) ?? [], [map, selectedSegment]);
  const selectedTrace = explorer?.harp_statement_traces.find((trace) => trace.statement_id === selectedTraceId) ?? null;
  const blockingBoundaries = map?.boundaries.filter((boundary) => boundary.status !== "exact") ?? [];
  const hasStaleBoundary = map?.contribution_map.classification_status === "stale" || blockingBoundaries.some((boundary) => boundary.status === "stale");
  const boundariesAccepted = !hasStaleBoundary && blockingBoundaries.every((boundary) => acceptedBoundaries.has(boundary.boundary_id));
  const identityReady = identityStatus === "unknown" || declaredName.trim().length > 0;
  const languageReady = language.authorCreated.length <= 1000 && language.materialExcluded.length <= 1000 && language.newMaterialIncluded.length <= 1000 && language.noteToCo.length <= 2000;
  const canAdvance = [Boolean(preparation && !hasStaleBoundary), identityReady, true, true, boundariesAccepted, languageReady, approved][step];

  const resetApproval = () => setApproved(false);
  const updateLanguage = (field: keyof LanguageInput, value: string) => {
    setLanguage((current) => ({ ...current, [field]: value }));
    resetApproval();
  };

  const freezeDeposit = async () => {
    setBusy(true);
    try {
      await invokeNative("freeze_contribution_map", { request: { pageScalarCapacity: 1800, selectedByHuman: false, arrangedByHuman: false } });
      setAcceptedBoundaries(new Set());
      setApproved(false);
      await refresh();
      onNotice("The current manuscript revision is frozen as the selected HARP deposit.");
    } catch (cause) {
      onNotice(`Deposit could not be frozen: ${nativeMessage(cause)}`);
    } finally {
      setBusy(false);
    }
  };

  const generateHarp = async () => {
    if (!preparation || !approved || !identityReady || !boundariesAccepted || !languageReady) return;
    setBusy(true);
    try {
      const result = await invokeNative<HarpProjection>("generate_harp", {
        request: {
          declaredName: identityStatus === "unknown" ? null : declaredName.trim(),
          identityStatus,
          identityEvidenceReferenceIds: [],
          sanitizationProfile,
          suggestedRegistrationLanguage: {
            authorCreated: language.authorCreated,
            materialExcluded: language.materialExcluded,
            newMaterialIncluded: language.newMaterialIncluded,
            noteToCo: language.noteToCo || null,
            registrationTreatmentSuggestions: language.registrationTreatmentSuggestions,
          },
          userApproved: true,
        },
      });
      if (!result) throw new Error("The native HARP generator did not return a result.");
      await refresh();
      setMode("explorer");
      setStep(0);
      setApproved(false);
      onNotice(`HARP generated in ${result.report_directory}. Open Statement trace to inspect every evidence link.`);
    } catch (cause) {
      onNotice(`HARP could not be generated: ${nativeMessage(cause)}`);
    } finally {
      setBusy(false);
    }
  };

  if (loading && !explorer) return <section className="provenance-loading panel" aria-live="polite"><span className="trace-spinner" />Loading native CPL evidence…</section>;

  return <section className="provenance-workspace">
    <header className="provenance-titlebar">
      <div><span className="eyebrow">HARP + Composition Provenance Ledger</span><h1>Evidence, declarations, and limits</h1><p>Inspect the native ledger or prepare an exact-deposit HARP. Thinkloom reports evidence; it does not decide legal authorship or copyrightability.</p></div>
      <div className="provenance-mode" role="tablist" aria-label="Provenance views">
        <button role="tab" aria-selected={mode === "explorer"} className={mode === "explorer" ? "active" : ""} onClick={() => setMode("explorer")}>CPL explorer</button>
        <button role="tab" aria-selected={mode === "wizard"} className={mode === "wizard" ? "active" : ""} onClick={() => setMode("wizard")}>Prepare HARP</button>
      </div>
    </header>
    {error && <div className="provenance-alert" role="alert">{error}</div>}
    {mode === "explorer" ? <CplExplorer
      explorer={explorer}
      selectedEvent={selectedEvent}
      selectedEventId={selectedEventId}
      setSelectedEventId={setSelectedEventId}
      selectedSegment={selectedSegment}
      selectedSegmentId={selectedSegmentId}
      setSelectedSegmentId={setSelectedSegmentId}
      selectedAssertions={selectedAssertions}
      selectedEvaluations={selectedEvaluations}
      selectedTrace={selectedTrace}
      selectedTraceId={selectedTraceId}
      setSelectedTraceId={setSelectedTraceId}
      onRefresh={() => void refresh()}
    /> : <HarpWizard
      preparation={preparation}
      map={map}
      step={step}
      setStep={setStep}
      declaredName={declaredName}
      setDeclaredName={(value) => { setDeclaredName(value); resetApproval(); }}
      identityStatus={identityStatus}
      setIdentityStatus={(value) => { setIdentityStatus(value); resetApproval(); }}
      acceptedBoundaries={acceptedBoundaries}
      setAcceptedBoundaries={setAcceptedBoundaries}
      language={language}
      updateLanguage={updateLanguage}
      sanitizationProfile={sanitizationProfile}
      setSanitizationProfile={(value) => { setSanitizationProfile(value); resetApproval(); }}
      approved={approved}
      setApproved={setApproved}
      canAdvance={Boolean(canAdvance)}
      hasStaleBoundary={hasStaleBoundary}
      busy={busy}
      onFreeze={() => void freezeDeposit()}
      onGenerate={() => void generateHarp()}
    />}
  </section>;
}

interface ExplorerProps {
  explorer: ExplorerProjection | null;
  selectedEvent: ExplorerEvent | null;
  selectedEventId: string;
  setSelectedEventId: (id: string) => void;
  selectedSegment: MapSegment | null;
  selectedSegmentId: string;
  setSelectedSegmentId: (id: string) => void;
  selectedAssertions: Record<string, unknown>[];
  selectedEvaluations: Record<string, unknown>[];
  selectedTrace: HarpTrace | null;
  selectedTraceId: string;
  setSelectedTraceId: (id: string) => void;
  onRefresh: () => void;
}

function CplExplorer(props: ExplorerProps) {
  const { explorer, selectedEvent, selectedEventId, setSelectedEventId, selectedSegment, selectedSegmentId, setSelectedSegmentId, selectedAssertions, selectedEvaluations, selectedTrace, selectedTraceId, setSelectedTraceId, onRefresh } = props;
  if (!explorer) return <div className="provenance-empty panel"><h2>No native CPL projection</h2><p>Open a conforming CPL 1.0 project to inspect its evidence.</p></div>;
  const map = explorer.contribution_map;
  const locator = map?.contribution_map.structural_locators.find((item) => item.segment_id === selectedSegmentId);
  return <div className="explorer-stack">
    <section className="verification-strip panel" aria-label="Native CPL verification">
      <div><span className="eyebrow">Native verification status</span><div className="verification-line"><StatusBadge value={explorer.verification.status} /><strong>{explorer.verification.event_count} events · {explorer.verification.record_count} immutable records</strong></div><code>{short(explorer.verification.chain_head?.event_sha256, 38)}</code></div>
      <div className="verification-copy"><p>This status comes from the native CPL verifier. The frontend does not manufacture a “valid” declaration.</p>{explorer.verification.findings.slice(0, 2).map((finding) => <small key={`${finding.code}-${finding.scope}`}>{finding.severity}: {finding.message}</small>)}</div>
      <button className="secondary-button" onClick={onRefresh}>Re-verify</button>
    </section>
    <div className="evidence-legend" aria-label="Evidence categories">{(Object.keys(categoryLabels) as EvidenceCategory[]).map((category) => <CategoryBadge key={category} category={category} />)}</div>
    <section className="explorer-grid">
      <article className="panel explorer-column"><div className="explorer-heading"><div><span className="eyebrow">Composition timeline</span><h2>Events and underlying records</h2></div><span>{explorer.events.length}</span></div><div className="explorer-list timeline-list">{[...explorer.events].reverse().map((event) => <button key={event.event_id} className={selectedEventId === event.event_id ? "selected" : ""} onClick={() => setSelectedEventId(event.event_id)}><i>{event.event_sequence}</i><span><strong>{displayName(event.event_type)}</strong><small>{new Date(event.timestamp).toLocaleString()} · {event.actor}</small><code>{short(event.event_id)}</code></span></button>)}</div></article>
      <article className="panel explorer-column"><div className="explorer-heading"><div><span className="eyebrow">Expression lineage</span><h2>Final-text segments</h2></div>{map && <StatusBadge value={map.contribution_map.classification_status} />}</div>{map ? <div className="explorer-list segment-list">{map.contribution_map.segments.map((segment) => <button key={segment.segment_id} className={selectedSegmentId === segment.segment_id ? "selected" : ""} onClick={() => setSelectedSegmentId(segment.segment_id)}><span><strong>Segment {segment.segment_sequence}</strong><small>{displayName(segment.recorded_origin_kind)} · scalars {segment.range.start}–{segment.range.end}</small><code>{short(segment.segment_id)}</code></span><StatusBadge value={segment.classification_status} /></button>)}</div> : <div className="provenance-empty"><p>Freeze a deposit to create final-text lineage.</p></div>}</article>
      <article className="panel explorer-detail"><div className="explorer-heading"><div><span className="eyebrow">Selected evidence</span><h2>{selectedSegment ? `Segment ${selectedSegment.segment_sequence}` : selectedEvent ? `Event ${selectedEvent.event_sequence}` : "Choose an item"}</h2></div></div><div className="detail-scroll">
        {selectedSegment ? <>
          <dl className="evidence-facts"><div><dt>Recorded origin</dt><dd>{displayName(selectedSegment.recorded_origin_kind)}</dd></div><div><dt>Identity status</dt><dd>{displayName(selectedSegment.actor_identity_status)}</dd></div><div><dt>Generation</dt><dd>{displayName(selectedSegment.generation_status)}</dd></div><div><dt>Lineage</dt><dd>{displayName(selectedSegment.lineage_status)}</dd></div><div><dt>Locator</dt><dd>Chapter {locator?.chapter ?? "—"}, paragraph {locator?.paragraph ?? "—"}, page {locator?.page ?? "—"}</dd></div><div><dt>Transformations</dt><dd>{selectedSegment.transformation_relationships.map(displayName).join(", ") || "None recorded"}</dd></div></dl>
          <TraceIds title="Lineage and operations" ids={[...selectedSegment.lineage_reference_ids, ...selectedSegment.operation_ids]} />
          <section className="assertion-section"><h3>Assertions and current evaluations</h3>{selectedAssertions.map((assertion) => { const evaluation = selectedEvaluations.find((item) => item.assertion_id === assertion.assertion_id); return <details key={String(assertion.assertion_id)}><summary><span>{displayName(String(assertion.predicate))}</span><StatusBadge value={String(evaluation?.status ?? "unverified")} /></summary><code>{String(assertion.assertion_id)}</code><p>Reason: {displayName(String(evaluation?.reason_code ?? assertion.reason_code ?? "unknown"))}</p><TraceIds title="Dependencies" ids={Array.isArray(evaluation?.dependency_results) ? (evaluation.dependency_results as Array<Record<string, unknown>>).map((item) => String(item.dependency_id)) : []} /><small>Evaluation {String(evaluation?.evaluation_id ?? "not recorded")}</small></details>})}</section>
        </> : selectedEvent ? <><dl className="evidence-facts"><div><dt>Event type</dt><dd>{displayName(selectedEvent.event_type)}</dd></div><div><dt>Actor</dt><dd>{selectedEvent.actor}</dd></div><div><dt>Event digest</dt><dd><code>{selectedEvent.event_sha256}</code></dd></div><div><dt>Previous digest</dt><dd><code>{selectedEvent.previous_event_sha256 ?? "Chain root"}</code></dd></div></dl><section className="record-list"><h3>Underlying records</h3>{selectedEvent.records.map((record) => <article key={record.record_id}><div><strong>{displayName(record.record_type)}</strong><StatusBadge value={record.accessible ? "exact" : "unverified"} /></div><code>{record.record_id}</code><small>{record.path}</small><TraceIds title="Referenced evidence" ids={record.subject_ids} /></article>)}</section></> : null}
      </div></article>
    </section>
    <section className="panel boundary-panel"><div className="explorer-heading"><div><span className="eyebrow">Evidence boundaries</span><h2>Exact, degraded, stale, and unverified</h2></div><span>{map?.boundaries.length ?? 0}</span></div>{map?.boundaries.length ? <div className="boundary-grid">{map.boundaries.map((boundary) => <article key={boundary.boundary_id}><StatusBadge value={boundary.status} /><strong>{displayName(boundary.kind)}</strong><p>{boundary.message}</p><small>Scalar range {boundary.start}–{boundary.end}</small></article>)}</div> : <p className="boundary-clear">No non-exact contribution-map boundary is currently recorded.</p>}</section>
    <section className="panel statement-trace"><div className="explorer-heading"><div><span className="eyebrow">HARP statement trace</span><h2>Statement → assertion → evaluation → record</h2></div>{explorer.harp && <StatusBadge value={explorer.harp.applicability_status} />}</div>{explorer.harp_statement_traces.length ? <div className="trace-layout"><div className="trace-list">{explorer.harp_statement_traces.map((trace) => <button key={trace.statement_id} className={selectedTraceId === trace.statement_id ? "selected" : ""} onClick={() => setSelectedTraceId(trace.statement_id)}><CategoryBadge category={trace.category} /><strong>{trace.harp_path}</strong><span>{trace.statement}</span></button>)}</div>{selectedTrace && <article className="trace-detail"><CategoryBadge category={selectedTrace.category} /><h3>{selectedTrace.statement}</h3><p>{selectedTrace.trace_note}</p><div className="trace-chain"><TraceIds title="Segments" ids={selectedTrace.segment_ids} /><TraceIds title={selectedTrace.category === "user_declaration" ? "Assertions — not applicable to declaration" : "Assertions"} ids={selectedTrace.assertion_ids} /><TraceIds title={selectedTrace.category === "user_declaration" ? "Evaluations — not applicable to declaration" : "Evaluations"} ids={selectedTrace.evaluation_ids} /><TraceIds title="Underlying records" ids={selectedTrace.record_ids} /></div></article>}</div> : <div className="provenance-empty"><h3>No HARP has been generated</h3><p>Use Prepare HARP to create the first traceable exact-deposit record.</p></div>}</section>
  </div>;
}

function TraceIds({ title, ids }: { title: string; ids: string[] }) {
  return <div className="trace-ids"><strong>{title}</strong>{ids.length ? <div>{ids.map((id) => <code key={id}>{id}</code>)}</div> : <small>None recorded or not applicable.</small>}</div>;
}

interface WizardProps {
  preparation: HarpPreparation | null;
  map: ContributionMapProjection | null;
  step: number;
  setStep: (step: number) => void;
  declaredName: string;
  setDeclaredName: (value: string) => void;
  identityStatus: "self_declared" | "verified" | "unknown";
  setIdentityStatus: (value: "self_declared" | "verified" | "unknown") => void;
  acceptedBoundaries: Set<string>;
  setAcceptedBoundaries: (value: Set<string>) => void;
  language: LanguageInput;
  updateLanguage: (field: keyof LanguageInput, value: string) => void;
  sanitizationProfile: "full_private" | "sanitized";
  setSanitizationProfile: (value: "full_private" | "sanitized") => void;
  approved: boolean;
  setApproved: (value: boolean) => void;
  canAdvance: boolean;
  hasStaleBoundary: boolean;
  busy: boolean;
  onFreeze: () => void;
  onGenerate: () => void;
}

function HarpWizard(props: WizardProps) {
  const { preparation, map, step, setStep, declaredName, setDeclaredName, identityStatus, setIdentityStatus, acceptedBoundaries, setAcceptedBoundaries, language, updateLanguage, sanitizationProfile, setSanitizationProfile, approved, setApproved, canAdvance, hasStaleBoundary, busy, onFreeze, onGenerate } = props;
  const boundaries = map?.boundaries.filter((boundary) => boundary.status !== "exact") ?? [];
  return <div className="wizard-shell">
    <nav className="wizard-progress panel" aria-label="HARP preparation steps">{wizardSteps.map((label, index) => <button key={label} className={index === step ? "active" : index < step ? "complete" : ""} onClick={() => { if (index < step) setStep(index); }} disabled={index > step}><span>{index < step ? "✓" : index + 1}</span><strong>{label}</strong></button>)}</nav>
    <section className="wizard-card panel" aria-live="polite">
      {step === 0 && <><WizardHeading index={step} title="Freeze or select the exact deposit" text="HARP describes one immutable deposit, not the moving manuscript." /><div className="wizard-content">{map ? <div className="deposit-card"><div><StatusBadge value={map.contribution_map.classification_status} /><h3>Selected frozen deposit</h3><code>{map.deposit.deposit_id}</code></div><dl><div><dt>Deposit digest</dt><dd><code>{map.deposit.deposit_sha256}</code></dd></div><div><dt>Manuscript revision</dt><dd><code>{map.deposit.manuscript_revision_id}</code></dd></div><div><dt>CPL source sequence</dt><dd>{map.deposit.cpl_event_sequence}</dd></div><div><dt>Contribution map</dt><dd><code>{map.contribution_map.contribution_map_id}</code></dd></div></dl>{hasStaleBoundary && <div className="wizard-warning">The selected deposit is stale for the active manuscript. Freeze the current revision before continuing.</div>}</div> : <div className="provenance-empty"><h3>No frozen deposit</h3><p>Freeze the current composition revision to produce its exact deposit and contribution map.</p></div>}<button className="secondary-button" disabled={busy} onClick={onFreeze}>{map ? "Freeze current manuscript revision" : "Freeze current manuscript"}</button></div></>}
      {step === 1 && <><WizardHeading index={step} title="Confirm the author identity declaration" text="This is your declaration. Thinkloom does not infer identity from writing activity." /><div className="wizard-content form-stack"><label><span>Identity status</span><select value={identityStatus} onChange={(event) => setIdentityStatus(event.target.value as WizardProps["identityStatus"])}><option value="self_declared">Self-declared</option><option value="verified">Verified by separately recorded evidence</option><option value="unknown">Unknown / not declared</option></select></label><label><span>Declared author name</span><input value={declaredName} disabled={identityStatus === "unknown"} onChange={(event) => setDeclaredName(event.target.value)} placeholder="Name exactly as it should appear" /></label><div className="classification-note"><CategoryBadge category="user_declaration" /><p>The declaration and exact text will be recorded in the HARP approval event.</p></div></div></>}
      {step === 2 && <><WizardHeading index={step} title="Review AI systems used" text="Only systems connected to accepted AI-output segments in this deposit appear here." /><div className="wizard-content">{preparation?.aiSystemDisclosures.length ? <div className="ai-review">{preparation.aiSystemDisclosures.map((item, index) => <article key={`${item.provider_id}-${item.model_id}-${index}`}><StatusBadge value={item.identity_status} /><h3>{item.provider_id ?? "Unknown provider"}</h3><p>{item.model_id ?? "Unknown model"}</p><small>{item.included_expression_segment_ids.length} included expression segments</small><TraceIds title="Included segments" ids={item.included_expression_segment_ids} /></article>)}</div> : <div className="provenance-empty"><h3>No accepted AI-output segment in this deposit</h3><p>The ledger may contain model activity that did not enter the final text; it is not presented as included expression.</p></div>}<div className="classification-note"><CategoryBadge category="evidence_fact" /><p>Provider and model identities come from recorded invocation requests, not frontend inference.</p></div></div></>}
      {step === 3 && <><WizardHeading index={step} title="Review final-text classifications" text="Origin, lineage, transformation, and assertion status remain separate evidence dimensions." /><div className="wizard-content"><div className="classification-table" role="table"><div role="row" className="table-head"><span>Segment</span><span>Recorded origin</span><span>Lineage</span><span>Classification</span></div>{map?.contribution_map.segments.map((segment) => <div role="row" key={segment.segment_id}><code>{segment.segment_sequence}</code><span>{displayName(segment.recorded_origin_kind)}</span><span>{displayName(segment.lineage_status)}</span><StatusBadge value={segment.classification_status} /></div>)}</div><div className="coverage-statement"><CategoryBadge category="derived_classification" /><p>{map?.contribution_map.coverage.recorded_positions} of {map?.contribution_map.coverage.denominator} normalized Unicode scalar positions have recorded origin.</p><small>Provenance coverage is not a human-authorship score.</small></div></div></>}
      {step === 4 && <><WizardHeading index={step} title="Resolve or accept evidence boundaries" text="Non-exact evidence is never hidden. Stale evidence must be resolved; other boundaries require explicit acceptance." /><div className="wizard-content">{boundaries.length ? <div className="boundary-review">{boundaries.map((boundary) => <article key={boundary.boundary_id}><div><StatusBadge value={boundary.status} /><strong>{displayName(boundary.kind)}</strong></div><p>{boundary.message}</p><small>Scalar range {boundary.start}–{boundary.end}</small>{boundary.status === "stale" ? <button className="secondary-button" onClick={onFreeze}>Resolve by freezing current revision</button> : <label className="boundary-accept"><input type="checkbox" checked={acceptedBoundaries.has(boundary.boundary_id)} onChange={(event) => { const next = new Set(acceptedBoundaries); if (event.target.checked) next.add(boundary.boundary_id); else next.delete(boundary.boundary_id); setAcceptedBoundaries(next); }} /><span>I reviewed and accept this boundary as a disclosed limitation.</span></label>}</article>)}</div> : <div className="boundary-success"><StatusBadge value="exact" /><h3>No non-exact boundary requires acceptance</h3><p>Assertions and evaluations remain available in the CPL explorer.</p></div>}</div></>}
      {step === 5 && <><WizardHeading index={step} title="Preview suggested registration language" text="Edit these fields to match your intended application. They are suggestions, not legal advice." /><div className="wizard-content language-grid"><label><span>Author Created</span><textarea value={language.authorCreated} maxLength={1000} onChange={(event) => updateLanguage("authorCreated", event.target.value)} /><small>{language.authorCreated.length} / 1000</small></label><label><span>Material Excluded</span><textarea value={language.materialExcluded} maxLength={1000} onChange={(event) => updateLanguage("materialExcluded", event.target.value)} /><small>{language.materialExcluded.length} / 1000</small></label><label><span>New Material Included</span><textarea value={language.newMaterialIncluded} maxLength={1000} onChange={(event) => updateLanguage("newMaterialIncluded", event.target.value)} /><small>{language.newMaterialIncluded.length} / 1000</small></label><label><span>Note to CO</span><textarea value={language.noteToCo} maxLength={2000} onChange={(event) => updateLanguage("noteToCo", event.target.value)} /><small>{language.noteToCo.length} / 2000</small></label><div className="classification-note wide"><CategoryBadge category="suggested_application_language" /><p>{preparation?.legalScopeStatement ?? "Copyrightability remains a Copyright Office determination."}</p></div></div></>}
      {step === 6 && <><WizardHeading index={step} title="Choose the archive and explicitly approve" text="Generation records this exact identity, language, evidence selection, and archive profile." /><div className="wizard-content approval-layout"><fieldset><legend>Supporting archive</legend><label className="archive-choice"><input type="radio" name="archive" checked={sanitizationProfile === "sanitized"} onChange={() => setSanitizationProfile("sanitized")} /><span><strong>Sanitized supporting archive</strong><small>Omits protected source bodies and private text excerpts while retaining hashes and structural facts.</small></span></label><label className="archive-choice"><input type="radio" name="archive" checked={sanitizationProfile === "full_private"} onChange={() => setSanitizationProfile("full_private")} /><span><strong>Full private supporting archive</strong><small>May retain representative source excerpts inside the local project.</small></span></label></fieldset><div className="approval-summary"><h3>Approval summary</h3><dl><div><dt>Deposit</dt><dd><code>{short(map?.deposit.deposit_sha256, 30)}</code></dd></div><div><dt>Identity</dt><dd>{identityStatus === "unknown" ? "Unknown" : `${declaredName} · ${displayName(identityStatus)}`}</dd></div><div><dt>AI disclosures</dt><dd>{preparation?.aiSystemDisclosures.length ?? 0}</dd></div><div><dt>Accepted boundaries</dt><dd>{acceptedBoundaries.size}</dd></div><div><dt>Archive</dt><dd>{displayName(sanitizationProfile)}</dd></div></dl></div><label className="explicit-approval"><input type="checkbox" checked={approved} onChange={(event) => setApproved(event.target.checked)} /><span><strong>I explicitly approve HARP generation.</strong><small>I reviewed the exact deposit, identity declaration, AI disclosure, contribution classifications, evidence boundaries, suggested application language, and archive profile shown above. I understand Thinkloom does not determine legal authorship, originality, copyrightability, ownership, or registrability.</small></span></label><button className="release-button generate-harp" disabled={!approved || busy} onClick={onGenerate}>{busy ? "Generating and recording…" : "Approve and generate HARP"}</button></div></>}
      <footer className="wizard-actions"><button className="secondary-button" disabled={step === 0 || busy} onClick={() => setStep(Math.max(0, step - 1))}>Back</button><span>Step {step + 1} of {wizardSteps.length}</span>{step < wizardSteps.length - 1 && <button className="primary-button" disabled={!canAdvance || busy} onClick={() => setStep(Math.min(wizardSteps.length - 1, step + 1))}>Continue</button>}</footer>
    </section>
  </div>;
}

function WizardHeading({ index, title, text }: { index: number; title: string; text: string }) {
  return <header className="wizard-heading"><span>{index + 1}</span><div><span className="eyebrow">{wizardSteps[index]}</span><h2>{title}</h2><p>{text}</p></div></header>;
}
