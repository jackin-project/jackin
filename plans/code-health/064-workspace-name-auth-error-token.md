# Plan 064: WorkspaceName on auth_error traces + token_setup expiry/revoke

## Status

- **Priority**: P2
- **Effort**: S-M
- **Depends on**: 038, 060, 058
- **Category**: tech-debt
- **Planned at**: 2026-07-12 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Work

### auth_error (jackin-runtime)

- `build_mode_resolution(cfg, agent, Option<&WorkspaceName>, role)`
- `build_env_layer_states(cfg, Option<&WorkspaceName>, role, env_var)`
- Launch core reuses one `workspace_opt` for mode/env/github/resolve/expiry

### token_setup (jackin-env)

- `run_revoke` / `run_revoke_with_runner` take `&WorkspaceName`
- `expiry_cache_path`, `write_expiry_stamp`, `clear_expiry_stamp`, `expiry_days_for_launch` take `&WorkspaceName`
- CLI boundary parses clap string → WorkspaceName

### materialize_workspace residual (measured DEFER)

- `materialize_workspace(..., workspace_name: &str, ...)` **cannot** take
  `WorkspaceName`: for CurrentDir/Path loads, `ResolvedWorkspace.label` is a
  **filesystem path** (workdir), not a config stem. IsolationRecord.workspace
  stores that dual-semantics string. Converting would reject ad-hoc path labels.

## Done criteria

- [x] Auth trace helpers typed
- [x] Token revoke + expiry cache path helpers typed
- [x] instance/env/runtime focused tests green
- [x] Residual ledger updated (R-038 partial + materialize note)
