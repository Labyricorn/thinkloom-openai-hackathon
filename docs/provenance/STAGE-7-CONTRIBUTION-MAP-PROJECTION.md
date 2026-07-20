# Stage 7 — Contribution-map projection

Status: **Complete for Thinkloom 0.5.7**

Milestone/version rule: **Milestone 7 → 0.5.7**

## Delivered boundary

Thinkloom 0.5.7 adds a deterministic native contribution-map generator over the 0.5.6 composition ledger. Finalizing a release freezes the exact replayed manuscript as an immutable Markdown deposit, binds it to the manuscript revision and pre-projection CPL chain head, and records the deposit snapshot, contribution map, assertions, evaluations, and complete projection bundle as canonical CPL records.

The native `freeze_contribution_map` and `load_contribution_map` commands also expose the projection independently of the later HARP interface. Repeating a request against the same revision and configuration reuses the existing frozen map instead of creating a time-dependent duplicate.

## Deterministic coverage

The generator sorts source spans with bytewise ordering, validates complete source coverage, and normalizes equivalent adjacent source splits before projection. It then splits expression at structural and fixed-layout boundaries while retaining the composition ancestry ID, lineage references, and operation references. Projected ranges are ordered, non-overlapping, and cover every Unicode scalar position in the frozen deposit exactly once.

Map identity excludes only `contribution_map_sha256`. IDs, configuration, layout profile, segment boundaries, assertions, and map digest are derived from canonical inputs; generation time, locale, database index order, and presentation sorting do not affect the map bytes.

## Structural locators

Chapter and paragraph locators are derived from the frozen Markdown structure. Page locators use the recorded layout profile's fixed Unicode-scalar capacity, defaulting to 1,800 positions. Locators are derived metadata: the authoritative anchors remain the deposit digest, manuscript revision, scalar range, and stable segment ancestry.

## Independent evidence dimensions

Recorded origin and transformation remain properties of expression lineage. `included_in_deposit`, `selected_by_human`, and `arranged_by_human` are separate assertions; selection and arrangement assertions are emitted only when their explicit request declarations are true. Every emitted assertion has a point-in-time evaluation bound to a chain head and dependency results.

The map never converts provenance coverage into an originality or human-authorship percentage. Its denominator is explicitly defined as all normalized Unicode scalar positions in the exact frozen deposit. `recorded_positions` counts positions with a recorded origin other than `unattested`.

## Visible boundaries

The native projection exposes range-addressed boundaries for:

- `unattested` expression with no recorded origin;
- `degraded` lineage or missing frozen-deposit evidence;
- `unverified` CPL or provenance evidence; and
- `stale` deposit or manuscript-revision dependencies.

Loading a frozen map re-evaluates its immutable assertions against current canonical state. A later composition revision makes the map stale without rewriting the frozen map record.

## Acceptance evidence

The Rust suite verifies byte-identical maps for reordered canonical inputs, normalization of equivalent source splits, structural and page splitting with stable ancestry, explicit unattested coverage, exact-map reuse, missing-deposit degradation, inconclusive-verification boundaries, and deterministic staleness after a later manuscript revision. Static integration tests verify native commands, release freezing, canonical record types, coverage invariants, locator layers, independent assertions, and all boundary states.

Milestone 7 does not generate HARP or make copyrightability conclusions. Deterministic HARP generation remains assigned to Milestone 8.
