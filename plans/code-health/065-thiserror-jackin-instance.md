# Plan 065: thiserror mid-tranche — jackin-instance

## Status

- **Priority**: P2
- **Effort**: S
- **Depends on**: 037
- **Category**: tech-debt
- **Planned at**: 2026-07-12 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Work

- Add workspace `thiserror` dep to `jackin-instance`
- New `error.rs`:
  - `InstanceError` — host config read, no-parent-dir, unknown agent runtime,
    index missing, join panics (github / per-agent / background)
  - `SyncSourceValidationError` — typed wrapper for sync-source folder checks
- Convert former `anyhow!` / `Result<(), String>` sites to the typed enums
- Console adapter maps `SyncSourceValidationError` → `String` for UI
- `InstanceManifest::agent()` returns `Result<Agent, InstanceError>`

## Measured remaining R-thiserror-mid-tranches

| Crate | Sites (approx) | Status |
|-------|----------------|--------|
| jackin-instance | ~7 | **CLOSED** this plan |
| jackin-isolation | ~14 | still DEFER |
| jackin-docker | ~17 | still DEFER |
| jackin-image | ~23 | still DEFER |
| jackin-config | ~66 | still DEFER |

## Done criteria

- [x] Typed errors in-tree
- [x] jackin-instance lib tests green (215)
- [x] Residual ledger R-thiserror notes instance closed
