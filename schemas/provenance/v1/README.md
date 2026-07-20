# Thinkloom provenance schemas 1.0

Package release: **Thinkloom 0.5.2**

CPL runtime target: **Thinkloom 0.6.0**

This generated additive package is the formal schema contract for the approved provenance architecture, composition lineage, and HARP projection. It targets JSON Schema Draft 2020-12 and ships with 47 schemas, valid fixtures, invalid fixture suites, versioned semantic registries, and deterministic verification vectors.

- `catalog.json` declares package, schema-family, per-schema introduction, and v0.6.0 compatibility without claiming native CPL conformance.
- `*.schema.json` are closed formal contracts. The seven composition/HARP records extend rather than overload v0.4 assertions or edit transactions.
- `registries` keeps recorded origin, transformation, selection/arrangement, evidentiary evaluation, and suggested registration treatment independent.
- `fixtures/valid` contains one canonical valid instance per schema.
- `fixtures/invalid` covers required fields, closed-object policy, populated constraints, and HARP-specific unknown-origin, identity, generation, and lineage boundaries.
- `vectors` fixes canonicalization, event and segment chains, self-digests, protected records, deterministic contribution maps, exact-deposit HARP binding, staleness, policy profiles, assertions/evaluations, backups, sanitization, and release-Merkle behavior.

Existing v0.4 provenance-assertion and assertion-evaluation semantics remain valid. Regenerate with `npm run provenance:schema:generate`. Verify with `npm run provenance:schema:test`. Generated files must be committed together with generator and vector changes. Package presence does not confer CPL project-format conformance. Thinkloom 0.5.4 established the exact CPL 1.0 project marker and opener boundary. Thinkloom 0.5.5 adds typed, replayable Phase 1 commands, and Thinkloom 0.5.6 adds deterministic manuscript composition lineage, and Thinkloom 0.5.7 adds the frozen-deposit contribution-map projection, and Thinkloom 0.5.8 adds deterministic exact-deposit HARP generation, and Thinkloom 0.5.9 adds connected native CPL exploration and gated HARP preparation, and Thinkloom 0.5.10 adds separate registration artifacts plus verifiable sanitized and full-private HARP archives, and Thinkloom 0.5.11 adds the executable verification matrix, native release gate, origin-preserving arrangement checks, and packaged Windows end-to-end gate, without converting unmarked v0.5.x preview projects.
