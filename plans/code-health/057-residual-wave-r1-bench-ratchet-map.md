# Plan 057: Residual wave R1 — materialize bench, export-volume ratchet, map-check

> Execute three write-disjoint residual-ledger rows from [DISPATCH.md](DISPATCH.md)
> wave R1 without design-gated work.

## Status

- **Priority**: P1
- **Effort**: M
- **Depends on**: 014, 017, 044, 049; residual ledger
- **Category**: perf / gates
- **Planned at**: 2026-07-11 on `chore/rust-code-health-roadmap`
- **Status**: DONE (in-tree on `chore/rust-code-health-roadmap`)

## Residuals closed

| Residual ID | Work |
|-------------|------|
| R-014-materialize-bench | `UsageCache` path inject + `materialize_accounts` criterion bench |
| R-export-volume-ratchet | `ratchet.toml` family `export-volume` + provider reading 044 constants |
| R-map-metadata-gate | `cargo xtask docs map-check` — every workspace package named in Codebase Map |

## Done criteria

- [x] Bench compiles: `cargo check -p jackin-usage --benches`
- [x] Host path not required for materialize (temp path seam)
- [x] `cargo xtask lint ratchet` green with export-volume family
- [x] `cargo xtask docs map-check` green
- [x] RESIDUAL_LEDGER marks the three rows CLOSED
