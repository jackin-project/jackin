# jackin-usage-token-monitor

Per-session token-spend monitor: polls provider-local
`JSONL` / `SQLite` logs inside the container and tracks
per-session input/output/cache token totals plus cost
estimates from the provider stream or the static pricing
table.

## What this crate owns

- Polling (`monitor`, `session`, `discover`):
  throttled provider-file walks and totals.
- Provider readers (`amp`, `claude`, `codex`,
  `kimi`, `opencode`): per-vendor log parsing.
- Cost (`pricing`, `totals`, `record`,
  `status`): estimates, accumulators, and
  degraded-read reporting.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | [`tests.rs`](src/tests.rs) |
| `monitor`/`session` | polling + totals | `tests/` cases |
| `amp`…`opencode` | vendor readers | per-module `tests.rs` |

## Public API

`TokenMonitor`, `TokenSession`, `TokenTotals`,
`PollReport`, `SpendAcc`, `ProviderReadDegraded`,
`find_provider_files`, `recompute_spend`, and the
`amp`/`claude`/`codex`/`kimi`/`opencode`/`pricing`
reader modules.

## How to verify

```sh
cargo nextest run -p jackin-usage-token-monitor
cargo clippy -p jackin-usage-token-monitor --all-targets -- -D warnings
```
