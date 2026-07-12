# Plan 060: WorkspaceName on env operator-resolve APIs (R-038 slice)

## Status

- **Priority**: P2
- **Effort**: M
- **Depends on**: 038, 059
- **Category**: tech-debt
- **Planned at**: 2026-07-12 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Work

Convert jackin-env operator-env layer walk to typed workspace names:

- `build_attributed_layers` (private)
- `has_operator_env` / `has_operator_env_matching`
- `lookup_operator_env_raw`
- `resolve_operator_env` / `_matching` / `_with` / `_with_matching`
- `collect_on_demand_bindings`
- `print_launch_diagnostic` / formatters

Signature: `workspace_name: Option<&WorkspaceName>`.

Call sites: runtime launch pipeline (parse at boundary), console
`providers_for_launch` / `operator_key_present`, env resolve tests.

## Done criteria

- [x] Public resolve APIs take `Option<&WorkspaceName>`
- [x] Launch + console callers compile
- [x] `cargo nextest run -p jackin-env -p jackin-console` green
