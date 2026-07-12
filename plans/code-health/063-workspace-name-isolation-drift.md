# Plan 063: WorkspaceName on isolation list + drift detect

## Status

- **Priority**: P2
- **Effort**: S
- **Depends on**: 038, 062
- **Category**: tech-debt
- **Planned at**: 2026-07-12 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Work

- `list_records_for_workspace(data_dir, &WorkspaceName)`
- `detect_workspace_edit_drift(..., &WorkspaceName, ...)`
- Call sites: workspace_cmd edit drift, console drift check

## Done criteria

- [x] APIs typed
- [x] isolation list_records + runtime drift tests green
- [x] lint --strict green
