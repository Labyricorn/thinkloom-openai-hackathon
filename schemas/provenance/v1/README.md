# Thinkloom provenance schemas 1.0

This generated package is the formal Stage 2 contract for the approved provenance architecture. It targets JSON Schema Draft 2020-12 and ships with 40 schemas, valid fixtures, invalid fixture suites, and deterministic verification vectors.

- `catalog.json` is the machine-readable inventory.
- `*.schema.json` are the formal contracts.
- `registries` defines assertion lifecycle, evaluation status, confidence, evidence, boundary, and reason semantics.
- `fixtures/valid` contains one canonical valid instance per schema.
- `fixtures/invalid` covers required fields, closed-object policy, and populated enum, pattern, and numeric/string/array bounds.
- `vectors` fixes canonicalization, timestamps, paths, JSONL, event and segment chains, self-digests, protected records, key rotation and recovery, retention policy, sanitized export, deterministic indexes, verification reports, canonical assertions and evaluations, dependency invalidation, backups, and release-Merkle behavior.

Regenerate with `npm run provenance:schema:generate`. Verify with `npm run provenance:schema:test`. Generated files must be committed together with generator and vector changes. Runtime adoption is a later implementation stage; presence of this package does not imply the current native writer already conforms.
