# Plan 059: WorkspaceName on config roles resolve APIs (R-038 slice)

## Status

- **Priority**: P2
- **Effort**: M
- **Depends on**: 038
- **Category**: tech-debt
- **Planned at**: 2026-07-11 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Work

Convert auth-mode resolution boundaries to typed workspace names:

- `resolve_mode` / `resolve_mode_with_trace`
- `resolve_github_mode`
- `resolve_sync_source_dir`
- `build_github_env_layers`

Signature: `workspace: Option<&WorkspaceName>` (`None` = global-only layer walk).

Call sites updated in: jackin-config tests, jackin app config auth show,
jackin-runtime launch pipeline / core / runtime / auth_error / tests,
jackin-console auth_config.

Validation error messages use `name.as_str()` Debug so operators still see
`workspace "foo"` rather than `WorkspaceName("foo")`.

## Done criteria

- [x] Roles resolve APIs take `Option<&WorkspaceName>`
- [x] Production + test call sites compile
- [x] `cargo nextest run -p jackin-config` green (263 tests)
