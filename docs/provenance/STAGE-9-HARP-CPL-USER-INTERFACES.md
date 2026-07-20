# Stage 9 — HARP and CPL user interfaces

Status: **Complete for Thinkloom 0.5.9**

Milestone/version rule: **Milestone 9 → 0.5.9**

## Delivered boundary

Thinkloom 0.5.9 replaces the simplified provenance preview with two connected native-data views. The CPL explorer displays native verification status, the composition timeline and immutable records, final-text expression lineage, assertions and current evaluations, and exact, degraded, stale, or unverified evidence boundaries. When a HARP exists, its statement trace connects each evidentiary or derived statement to the relevant CPL segments, assertions, evaluations, and records. User declarations identify their approval records and explicitly show that assertions and evaluations are not applicable.

The HARP preparation wizard requires the user to review or create an exact frozen deposit, confirm an identity declaration, inspect AI-system disclosures and contribution classifications, resolve stale evidence or explicitly accept other non-exact boundaries, preview and edit suggested application language, choose a sanitized or full-private supporting archive, and explicitly approve generation. The approval gate resets when approval-relevant inputs change.

## Evidence-language boundary

The interface visibly separates:

- evidence facts read from CPL records;
- user declarations recorded at approval;
- deterministic derived classifications;
- editable suggested application language; and
- legal determinations Thinkloom does not make.

Verification status comes from the native CPL verifier. The frontend does not manufacture a `valid` declaration, derive authorship percentages, or decide authorship, originality, copyrightability, ownership, or registrability.

## Native projections

- `load_cpl_explorer` verifies the project and returns a read-only event, record, composition, contribution-map, HARP, and statement-trace projection.
- `prepare_harp` returns the current frozen map, recorded AI-system disclosures, immutable policy profile, existing HARP, suggested language, and legal-scope statement without generating an artifact.
- `freeze_contribution_map` and `generate_harp` remain the only mutation commands used by the wizard.

The explorer projection exposes structural identifiers and digests required for tracing; it does not expose protected record bodies as a shortcut for frontend classification.

## Acceptance evidence

- Static UI tests cover both views, native command wiring, the evidence categories, the seven preparation steps, explicit approval, and the no-percentage/no-frontend-validity boundary.
- A native integration test creates a CPL project, freezes an exact deposit, generates a HARP, verifies the chain, and proves the HARP claim summary reaches non-empty segment, assertion, evaluation, and underlying-record sets.
- Identity remains visibly classified as a user declaration: it links to approval records while assertion and evaluation links are explicitly not applicable.

Unmarked preview projects retain the preservation-only behavior established in 0.5.4 and cannot be represented as CPL-conforming evidence.
