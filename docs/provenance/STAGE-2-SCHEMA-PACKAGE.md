# Thinkloom Stage 2 Provenance Schema Package

Status: **Complete for Thinkloom 0.5.2**

Schema family: **1.0, additive catalog 1.1**

Dialect: **JSON Schema Draft 2020-12**

CPL runtime target: **Thinkloom 0.6.0**

Runtime conformance: **Not yet implemented**

## Scope

The 0.5.2 package extends the existing provenance schema family with composition-specific records rather than overloading v0.4 assertions or edit transactions. It formalizes the normative HARP/CPL model without changing native application behavior. Existing v0.4 `provenance-assertion` and `assertion-evaluation` instances and semantics remain valid.

Migration support remains deferred until after Thinkloom 1.0.0. Projects created before the Milestone 4 marker remain preview projects and MUST NOT be represented as CPL-conforming merely because these schemas are present.

## Package

The machine-readable package is in [`schemas/provenance/v1`](../../schemas/provenance/v1/README.md). `catalog.json` identifies all 47 schemas, 14 registries, package/schema/application versions, per-entry introduction and compatibility, fixture locations, the v0.6.0 runtime target, and the generator.

The seven additive schemas are:

- `composition-operation.schema.json`
- `expression-segment.schema.json`
- `contribution-map.schema.json`
- `deposit-snapshot.schema.json`
- `registration-policy-profile.schema.json`
- `human-authorship-record.schema.json`
- `harp-export-manifest.schema.json`

Each schema has:

- one valid canonical fixture;
- invalid cases for every top-level required field and the closed-object policy;
- populated nested cases for enum, const, pattern, numeric, string, and array bounds; and
- explicit conditional failures for operation/origin consistency and exact-classification identity, generation, origin, lineage, AI-system identity, coverage, and applicability boundaries.

## Independent registries

The package adds versioned registries for:

- composition operation kinds;
- recorded origin kinds;
- transformation relationships;
- contribution-map layers;
- suggested registration treatments;
- HARP limitation codes;
- HARP explanation codes; and
- composition assertion predicates: `derived_from`, `generated_by`, `modified_by_human`, `selected_by_human`, `arranged_by_human`, and `included_in_deposit`.

Recorded origin, transformation, selection/arrangement, evidentiary evaluation, and suggested registration treatment remain independent. A paste cannot default to direct human input, and selection/arrangement cannot alter text origin.

## Deterministic vectors

In addition to the existing canonicalization, ledger, assertion, privacy, backup, and release vectors, the suite now covers:

- all composition-operation origin rules;
- exact expression-segment classification boundaries;
- refusal of exact HARP classification for unknown origin, identity, generation, lineage, or AI-system identity;
- complete ordered non-overlapping Unicode-scalar contribution-map coverage;
- byte-stable map generation from different input orders;
- contribution-map, policy-profile, HARP, and HARP-manifest self-digest identities;
- exact deposit, manuscript revision, CPL head, policy profile, and export-manifest bindings;
- HARP staleness after a later manuscript revision without invalidating historical deposit integrity;
- explicit approval of editable suggested registration language; and
- prohibited human/AI percentage and legal-score fields.

## Verification

Regenerate deterministically:

```powershell
npm run provenance:schema:generate
```

Validate schemas, fixtures, registries, and vectors:

```powershell
npm run provenance:schema:test
```

The schema tests are also part of `npm test`. Generated output must be committed with its generator and tests. A clean second regeneration must produce no changes.

## Next boundary

Thinkloom 0.5.2 is schema-complete for this milestone but MUST NOT claim native CPL or HARP runtime conformance. Thinkloom 0.5.3 subsequently implemented the native CPL service and its core crash-injection matrix. Thinkloom 0.5.4 then established the exact-marker project boundary; all earlier unmarked projects remain read-only previews.
