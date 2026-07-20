# Stage 4 — Conforming project boundary

Status: **Complete for Thinkloom 0.5.4**

Milestone/version rule: **Milestone 4 → 0.5.4**

Project format: **thinkloom-cpl 1.0**
Provenance conformance marker: **cpl-1.0**

## Outcome

Thinkloom 0.5.4 establishes an unambiguous boundary between newly created CPL projects and every earlier preview project. The opener classifies a selected directory by reading `project.json` and the minimum required structure before it invokes recovery, verification, SQLite, Git, or any other mutating service.

Only this exact marker is supported:

```json
{
  "project_format": "thinkloom-cpl",
  "project_format_version": "1.0",
  "provenance_conformance": "cpl-1.0"
}
```

`schemaVersion: "1.0"`, `schema_version: "1.0"`, an incomplete marker, or a differently versioned marker is not sufficient.

## New-project contract

New 0.5.4 projects:

- serialize the formal snake-case project manifest;
- include the exact supported marker;
- bind the initial project manifest and provenance policy as immutable CPL records;
- initialize `records/`, active and sealed `provenance/ledger/` storage, `reports/`, and operational `.app/` storage;
- keep writer locks under `.app/locks/`, same-filesystem staging under `.app/temp/staging/`, recovery quarantine under `.app/recovery/`, and SQLite under `.app/state.sqlite`;
- pass CPL recovery and native verification before becoming editable when reopened.

The marker is necessary but not sufficient. Missing required directories produce `CPL_BLOCKED`; an integrity result other than `VERIFIED` or `VERIFIED_WITH_WARNINGS` also leaves the project unavailable for editing.

## Classification states

| Classification | Meaning | Permitted actions |
| --- | --- | --- |
| `CPL_CONFORMING` | Exact marker, supported schema and required structure; native recovery and verification pass | Normal editable opening |
| `LEGACY_PREVIEW_READ_ONLY` | Marker absent, including v0.5 projects with only `schemaVersion: "1.0"` | Explain, Show Project Folder, preservation archive |
| `UNSUPPORTED_READ_ONLY` | Invalid, partial, or unsupported marker/manifest | Explain and Show Project Folder |
| `CPL_BLOCKED` | Exact marker is present, but structure, schema compatibility, recovery, or integrity gate fails | Explain and Show Project Folder; no editable activation |

Classification itself is read-only. A read-only selection clears the active editable project so subsequent persistence and provenance commands cannot target either the legacy project or a previously open project by mistake.

## Legacy preservation

Thinkloom does not load legacy state into the editor and does not invoke CPL recovery for an unmarked project. The UI exposes only:

- **Show project folder**; and
- **Create preservation archive** for `LEGACY_PREVIEW_READ_ONLY` projects.

The archive must be saved outside the source project. It stores each source file without content transformation, retains directory and symbolic-link entries, and includes this label:

```text
Legacy preview-project preservation archive
Not verified, converted, or CPL-conforming
```

The native implementation inventories the source before and after archive creation. If any entry changes during the operation, it removes the incomplete destination archive and reports a retryable failure. Legacy backup import is explicitly refused.

## Prohibited legacy behavior

Before a future post-1.0 migration specification, Thinkloom 0.5.4 does not:

- add a marker to a legacy project;
- convert or import legacy preview data into CPL;
- synthesize records or ledger events;
- run CPL recovery or represent legacy provenance as verified;
- modify Git history;
- generate CPL evidence or HARP artifacts.

## Acceptance evidence

Native tests prove that:

- `schemaVersion: "1.0"` without the marker is classified as legacy;
- inspection leaves the complete source inventory unchanged;
- exact markers remain blocked until the required layout exists;
- partial and unsupported markers never enter recovery;
- preservation archives reproduce arbitrary binary file bytes while leaving the source inventory unchanged.

Static application acceptance tests additionally enforce inspection-before-recovery ordering, `.app` operational paths, exact marker constants, legacy-only controls, and the absence of `.thinkloom` runtime storage.

## Remaining boundary

Milestone 4 establishes project-format and opener conformance only. Phase 1 typed reconstruction is assigned to Milestone 5 / 0.5.5. Composition transaction lineage, contribution-map generation, HARP generation, dedicated CPL/HARP interfaces, export hardening, and the full release verification matrix remain assigned to later milestones. No HARP or legal conclusion is produced by this milestone.
