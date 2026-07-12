# Plan 066: thiserror mid-tranche — jackin-isolation

## Status

- **Priority**: P2
- **Effort**: S
- **Depends on**: 037, 065
- **Category**: tech-debt
- **Planned at**: 2026-07-12 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Work

- Add workspace `thiserror` to `jackin-isolation`
- `IsolationError` enum covering preflight, drift, cleanup, purge, state version
- Convert `bail!`/`ensure!`/`anyhow!` sites in materialize, cleanup, state
- Keep public function signatures as `anyhow::Result` (absorb via `?` / `.into()`)

## Done criteria

- [x] Typed errors in-tree
- [x] isolation tests green
- [x] Residual ledger R-thiserror notes isolation closed
