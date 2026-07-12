# Plan 062: WorkspaceName on AppConfig require/edit/remove

## Status

- **Priority**: P2
- **Effort**: S-M
- **Depends on**: 038, 061
- **Category**: tech-debt
- **Planned at**: 2026-07-12 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Work

`AppConfig::require_workspace`, `edit_workspace`, and `remove_workspace` take
`&WorkspaceName`. ConfigEditor already delegates typed edit; resolve Saved
workspace load and CLI/token sites parse at the boundary.

## Done criteria

- [x] AppConfig CRUD APIs typed
- [x] Call sites compile (resolve, token_setup, workspace_cmd, token_cmd)
- [x] jackin-config + env + console tests green; lint --strict green
