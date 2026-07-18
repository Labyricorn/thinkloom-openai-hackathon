# Thinkloom Stage 2 Provenance Schema Package

Status: **Complete for Thinkloom 0.4.0**
Schema family: **1.0**
Dialect: **JSON Schema Draft 2020-12**
Runtime conformance: **Not yet implemented**

## Scope

Stage 2 formalizes the approved provenance architecture without changing native application behavior. It provides deterministic contracts and verification evidence for the later native-writer implementation. Migration support remains deferred until after Thinkloom 1.0.0.

## Package

The machine-readable package is in [`schemas/provenance/v1`](../../schemas/provenance/v1/README.md). `catalog.json` identifies all 40 schemas, the schema and application versions, compatibility, fixture locations, and the generator.

Each schema has:

- one valid canonical fixture;
- invalid cases for every top-level required field and the closed-object policy;
- populated nested cases for enum, const, pattern, numeric, string, and array bounds; and
- explicit conditional failures for protected records, recovery envelopes, assertion generations, and exact evaluation rules where ordinary keyword mutation is insufficient.

## Deterministic vectors

The vector suite covers:

- RFC 8785-compatible serialization after Thinkloom Unicode NFC preprocessing;
- prohibited non-finite, undefined, and NFC-colliding values;
- exact millisecond UTC timestamps and repository-relative path restrictions;
- LF-only canonical JSONL, contiguous events, event self-digests, and cross-segment seals;
- prompt-template, protected-record, event, and release self-digest identities;
- minimal and full-private retention behavior;
- device and recovery key envelopes plus key rotation;
- non-mutating sanitized exports and omission-rule disclosure;
- deterministic derived indexes;
- backup and release manifests plus the release-files Merkle root;
- all verification statuses and finding severities;
- immutable assertion envelopes and exact self-digest identity;
- point-in-time exact, degraded, refused, stale, and unverified evaluations; and
- versioned reason, lifecycle, confidence, evidence, boundary, and consumer-decision registries.

## Verification

Regenerate deterministically:

```powershell
npm run provenance:schema:generate
```

Validate schemas, fixtures, and vectors:

```powershell
npm run provenance:schema:test
```

The schema tests are also part of `npm test`. Generated output must be committed with its generator and tests. A clean regeneration is required before changing any formal schema or vector.

## Next boundary

Thinkloom 0.4.0 must not claim native provenance conformance. The next implementation stage replaces the MVP persistence path with the approved single-writer protocol and then executes the durable-boundary fault-injection matrix.
