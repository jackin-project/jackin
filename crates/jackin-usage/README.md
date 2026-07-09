# jackin-usage

Usage, pricing, telemetry, and token monitors for the `jackin-capsule` daemon. Owns the in-capsule accounting of agent token spend/cost and the telemetry store that persists it.

## What this crate owns

- Token monitoring (`token_monitor`) and usage accounting (`usage`) for running agents.
- Telemetry emission + store (`telemetry`, `telemetry_store`, `logging`) — the capsule-side observability tier.
- Usage output shaping (`output`). (`clog!`/`cdebug!` logging infrastructure is re-exported from `jackin-diagnostics`.)

## Architecture tier and allowed dependencies

**Infrastructure** (capsule-side observability/accounting). Allowed inward dependencies: `jackin-core`, `jackin-protocol`, `jackin-diagnostics`. No dependency on `jackin-capsule` (would be circular), `jackin-tui`, `jackin-console`, or any presentation crate. Logging infrastructure (`logging`, `clog!`, `cdebug!`) lives here so both binaries share one tier.

## Structure

- `src/token_monitor.rs` / `src/token_monitor/` — token spend monitoring
- `src/usage.rs` / `src/usage/` — usage/pricing accounting
- `src/telemetry.rs`, `src/telemetry_store.rs` / `src/telemetry_store/` — telemetry + persistent store
- `src/logging.rs` / `src/logging/`, `src/output.rs` — shared logging tier + output shaping

## Public API

Token-monitor + usage-accounting types consumed by `jackin-capsule`; the shared `clog!`/`cdebug!` logging tier re-exported across the workspace. Avoid cloning full usage views during account materialization — serialize from borrowed views/iterators (tracked perf item).

## How to verify

```sh
cargo nextest run -p jackin-usage
cargo clippy -p jackin-usage --all-targets -- -D warnings
```

