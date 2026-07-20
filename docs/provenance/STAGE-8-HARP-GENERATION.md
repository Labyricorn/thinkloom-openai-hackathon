# Stage 8 — Deterministic HARP generation

Status: **Complete for Thinkloom 0.5.8**

Milestone/version rule: **Milestone 8 → 0.5.8**

## Delivered boundary

Thinkloom 0.5.8 adds a deterministic native Human Authorship Record of Provenance generator over the exact frozen deposit and contribution map delivered in 0.5.7. The `generate_harp` command requires explicit approval of the identity declaration, suggested registration language, and sanitization profile. It records that approval before generation, then records the HARP, export manifest, and complete generation bundle as immutable CPL records. `load_harp` reconstructs the latest record and reevaluates its applicability against current canonical state.

The generator is evidence projection code. It does not call an LLM, classify legal authorship, infer originality, compute a human percentage, or decide copyrightability or registration scope.

## Generated report set

Each generation creates a deterministic `reports/harp/{harp_id}/` tree containing:

- a one-page human-authorship summary;
- a visual SVG final-text contribution map;
- representative transformation comparisons;
- an AI-system and model disclosure;
- provenance coverage and limitations;
- suggested **Author Created**, **Material Excluded**, **New Material Included**, and **Note to CO** language;
- canonical machine-readable `harp.json`;
- a verification report;
- the canonical contribution map used by the reports;
- a supporting archive manifest; and
- a schema-valid HARP export manifest.

Every generated report carries the exact deposit digest, manuscript revision and digest, CPL chain head and sequence, HARP/schema/application versions, policy-profile ID/version/digest/retrieval date, sanitization profile, and a statement that copyrightability remains a U.S. Copyright Office determination.

## Determinism and disclosure

HARP identity is derived from canonical immutable dependencies: the deposit, manuscript revision, contribution map, source CPL binding, bundled read-only policy profile, assertion set, declared dependency set, and approved request. Repeating an identical request replays the same approval action and returns the existing byte-identical HARP without adding ledger events.

Accepted-AI-output segments are joined to recorded invocation requests to disclose provider and model identity. Missing identity is represented as `unknown` and prevents `exact` evidentiary status. Representative transformations are selected by stable operation ID; the `sanitized` profile omits source excerpts while retaining hashes and structural facts.

Coverage always uses normalized Unicode scalar positions. It describes provenance coverage and is never presented as an authorship percentage.

## Staleness

Loading HARP reevaluates its dependencies without rewriting the historical record. The result becomes visibly stale when any of the following changes or becomes unavailable:

- active manuscript revision;
- frozen deposit bytes or digest;
- contribution-map digest or classification;
- registration policy-profile digest;
- immutable assertion set;
- assertion or lineage dependency set;
- approved HARP request; or
- native CPL verification state.

The historical exact-deposit HARP remains inspectable while its current-work applicability changes to `stale` with deterministic reason codes.

## Acceptance evidence

The native Rust suite generates the complete artifact tree from a verified CPL project, verifies exact status, checks idempotent regeneration and stable event count, then edits the manuscript and verifies deterministic staleness. Additional tests mutate each dependency class, validate the bundled policy digest and application compatibility, prohibit model/network/classifier dependencies, verify every report and shared metadata field, and validate every generated Stage 2 schema fixture.

Milestone 8 supplies the native generator and artifact contract. The guided approval/preview user interface remains assigned to Milestone 9.
