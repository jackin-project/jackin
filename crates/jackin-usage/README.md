# jackin-usage

Usage, pricing, telemetry, and token monitors for the `jackin-capsule` daemon. Owns the in-capsule accounting of agent token spend/cost and the usage snapshot store that persists it.

## What this crate owns

- Token monitoring (`token_monitor`) and usage accounting (`usage`) for running agents.
- Usage snapshot persistence (`usage_snapshot_store`) and token-accounting telemetry (`telemetry`).
- Usage output shaping (`output`).

## Architecture tier and allowed dependencies

**Infrastructure** (capsule-side accounting). Allowed inward dependencies: `jackin-core`, `jackin-protocol`, `jackin-diagnostics`. No dependency on `jackin-capsule` (would be circular), `jackin-tui`, `jackin-console`, or any presentation crate.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`token_monitor.rs`](src/token_monitor.rs) · [`token_monitor/`](src/token_monitor) | token spend monitoring | [`tests.rs`](src/token_monitor/tests.rs) |
| [`usage.rs`](src/usage.rs) · [`usage/`](src/usage) | usage/pricing accounting | [`tests.rs`](src/usage/tests.rs) |
| [`telemetry.rs`](src/telemetry.rs) | telemetry emission | — |
| [`logging.rs`](src/logging.rs) · [`logging/`](src/logging) | telemetry-level state and Capsule panic handling | [`tests.rs`](src/logging/tests.rs) |
| [`usage_snapshot_store.rs`](src/usage_snapshot_store.rs) · [`usage_snapshot_store/`](src/usage_snapshot_store) | persistent usage snapshot store | [`tests.rs`](src/usage_snapshot_store/tests.rs) |
| [`store_backend.rs`](src/store_backend.rs) | turso SQLite import chokepoint | — |
| [`output.rs`](src/output.rs) | usage output shaping | — |

## Public API

Token-monitor and usage-accounting types consumed by `jackin-capsule`. Avoid cloning full usage views during account materialization — serialize from borrowed views/iterators (tracked perf item).

## How to verify

```sh
cargo nextest run -p jackin-usage
cargo clippy -p jackin-usage --all-targets -- -D warnings
```
