# jackin-usage

Usage, pricing, telemetry, and token monitors for the `jackin-capsule` daemon. Owns the in-capsule accounting of agent token spend/cost and the telemetry store that persists it.

## What this crate owns

- Token monitoring (`token_monitor`) and usage accounting (`usage`) for running agents.
- Telemetry emission + store (`telemetry`, `telemetry_store`, `logging`) — the capsule-side observability tier.
- Usage output shaping (`output`). (`clog!`/`cdebug!` logging infrastructure is re-exported from `jackin-diagnostics`.)

## Architecture tier and allowed dependencies

**Infrastructure** (capsule-side observability/accounting). Allowed inward dependencies: `jackin-core`, `jackin-protocol`, `jackin-diagnostics`. No dependency on `jackin-capsule` (would be circular), `jackin-console`, `jackin-launch`, or any presentation crate (TermRock is only for presentation crates). Logging infrastructure (`logging`, `clog!`, `cdebug!`) lives here so both binaries share one tier.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`token_monitor.rs`](src/token_monitor.rs) · [`token_monitor/`](src/token_monitor) | token spend monitoring | [`tests.rs`](src/token_monitor/tests.rs) |
| [`usage.rs`](src/usage.rs) · [`usage/`](src/usage) | usage/pricing accounting | [`tests.rs`](src/usage/tests.rs) |
| [`telemetry.rs`](src/telemetry.rs) | telemetry emission | — |
| [`telemetry_store.rs`](src/telemetry_store.rs) · [`telemetry_store/`](src/telemetry_store) | persistent telemetry store | [`tests.rs`](src/telemetry_store/tests.rs) |
| [`store_backend.rs`](src/store_backend.rs) | turso SQLite import chokepoint | — |
| [`logging.rs`](src/logging.rs) · [`logging/`](src/logging) | shared logging tier (`clog!`/`cdebug!`) | [`tests.rs`](src/logging/tests.rs) |
| [`output.rs`](src/output.rs) | usage output shaping | — |

## Public API

Token-monitor + usage-accounting types consumed by `jackin-capsule`; the shared `clog!`/`cdebug!` logging tier re-exported across the workspace. Avoid cloning full usage views during account materialization — serialize from borrowed views/iterators (tracked perf item).

## How to verify

```sh
cargo nextest run -p jackin-usage
cargo clippy -p jackin-usage --all-targets -- -D warnings
```

