# Stage 10 — Export, privacy, and security

Status: **Complete for Thinkloom 0.5.10**

Milestone/version rule: **Milestone 10 → 0.5.10**

## Delivered artifacts

The native HARP export service creates six separate artifacts from one current, exact-deposit HARP:

1. registration worksheet;
2. human-readable HARP;
3. canonical machine-readable HARP;
4. exact deposit copy;
5. sanitized supporting archive; and
6. full private archive.

The exact machine-readable HARP and deposit remain unchanged private artifacts. User-selected author-name redaction applies only to sanitized human-readable presentations. A stale HARP or a deposit whose bytes no longer match its recorded digest cannot be exported.

## Sanitized archive boundary

The sanitized archive is assembled from an allowlist of derived presentation and structural-evidence files. It never copies raw records or ledger segments and excludes the deposit and full machine-readable HARP. Its omission manifest always addresses:

- private conversations;
- rejected model output;
- credentials and authorization material;
- personal identifiers selected for redaction;
- internal paths;
- provider metadata not required for disclosure; and
- protected source bodies.

Each omission entry contains its category, action, affected-record count, retained-evidence binding digest, and disclosure digest. The manifest separately hashes the ordered omission-rule set.

## Verification and completeness

Before publication, the native verifier reopens the sanitized ZIP and checks:

- every required omission category is present;
- every disclosure digest and the aggregate rules digest recompute;
- every retained file exists and matches its byte length and SHA-256 binding; and
- the CPL chain head, HARP digest, and deposit digest match the expected source bindings.

Success is reported as `verified_selective`, never as complete. The manifest declares `selective_disclosed_subset` and states that verification covers disclosed retained evidence only. It does not claim that omitted private history is present or that provenance integrity decides identity or legal authorship.

Export writes only derived files under `exports/harp/`. It does not append a CPL event, rewrite a record, advance the source chain, or alter the exact deposit. The full private archive carries an explicit sharing warning and includes the exact deposit, private HARP artifacts, records, ledger segments, and generated report material.

## Acceptance evidence

- A native integration test generates an exact HARP, creates all six artifacts, and proves the CPL chain head is unchanged.
- The test confirms the sanitized ZIP contains no raw records, ledger segments, deposit copy, or exact machine HARP and that selected identity text is absent.
- The same test confirms the full private archive retains the deposit, records, and ledger.
- Schema vectors validate the selective completeness claim and recompute every omission-disclosure digest.
- UI tests cover artifact separation, redaction selection, privacy warnings, omission disclosure, and re-verification.

Unmarked preview projects retain the preservation-only boundary and cannot use HARP export commands.
