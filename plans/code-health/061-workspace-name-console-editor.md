# Plan 061: WorkspaceName on console save/launch services + ConfigEditor writers

## Status

- **Priority**: P2
- **Effort**: M
- **Depends on**: 038, 059, 060
- **Category**: tech-debt
- **Planned at**: 2026-07-12 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Work

1. Console service layer:
   - `providers_for_launch` / `operator_key_present` take `&WorkspaceName`
   - `workspace_save_diff_plan` / `push_env_diff` take `&WorkspaceName`
2. Host apply path parses once and threads `WorkspaceName` into editor APIs.
3. ConfigEditor public writers:
   - `set_workspace_*` auth/sync methods
   - `edit_workspace` / `remove_workspace`
   take `&WorkspaceName`.

## Done criteria

- [x] Service + editor APIs typed
- [x] Call sites compile (token_setup, workspace_cmd, console services, tests)
- [x] `cargo nextest -p jackin-config -p jackin-env -p jackin-console` green
- [x] `lint --strict` green
