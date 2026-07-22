# jackin-usage

Usage, telemetry, and token monitors for the `jackin-capsule` daemon.
Also owns the **Capsule-free host runtime** consumed by the macOS usage menu bar
and `jackin usage host snapshot`.

**Product surfaces (Capsule usage UI, jackin❯ Desktop):** **usage limits only** —
remaining/used %, resets, plan/status. **Never** token unit prices or historical
usage/spend trends as product features.

## What this crate owns

- Token monitoring (`token_monitor`) and usage accounting (`usage`) for running agents.
- Host orchestration (`host`) — `HostUsageRuntime` for menu bar / CLI without Capsule.
- Usage snapshot persistence (`usage_snapshot_store`) and token-accounting telemetry (`telemetry`).
- Usage output shaping (`output`).

## Architecture tier and allowed dependencies

**Infrastructure** (capsule-side + host menu-bar observability/accounting). Allowed
inward dependencies: `jackin-core`, `jackin-protocol`, and `jackin-diagnostics`.
No dependency on `jackin-capsule` (which would be circular), `jackin-tui`,
`jackin-console`, `jackin-launch`, or any presentation crate.

UniFFI lives in sibling crate `jackin-usage-ffi`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`host.rs`](src/host.rs) · [`host/`](src/host) | Capsule-free host runtime | [`tests.rs`](src/host/tests.rs) |
| [`token_monitor.rs`](src/token_monitor.rs) · [`token_monitor/`](src/token_monitor) | token spend monitoring | [`tests.rs`](src/token_monitor/tests.rs) |
| [`usage.rs`](src/usage.rs) · [`usage/`](src/usage) | usage/pricing accounting | [`tests.rs`](src/usage/tests.rs) |
| [`telemetry.rs`](src/telemetry.rs) | telemetry emission | — |
| [`logging.rs`](src/logging.rs) | telemetry-level state and Capsule panic handling | — |
| [`usage_snapshot_store.rs`](src/usage_snapshot_store.rs) · [`usage_snapshot_store/`](src/usage_snapshot_store) | persistent usage snapshot store | [`tests.rs`](src/usage_snapshot_store/tests.rs) |
| [`store_backend.rs`](src/store_backend.rs) | turso SQLite import chokepoint | — |
| [`output.rs`](src/output.rs) | usage output shaping | — |

## Public API

Token-monitor and usage-accounting types consumed by `jackin-capsule`.
`host::HostUsageRuntime` for jackin❯ Desktop and the host CLI.

Host display extensions (plan 008; presentation-time only, not persisted):

| API | Role |
|---|---|
| `usage::provider_display_label` | Shared Capsule/Desktop provider remap (`Codex`→`OpenAI`, …) |
| `usage::estimate_caption` | Honesty caption for estimated / local-log views |
| `usage::{UsageFormatPrefs,PercentStyle,ResetStyle}` | left/used + countdown/exact-clock prefs |
| `HostUsageRuntime::set_format_prefs` | Apply presentation prefs |
| `HostUsageRuntime::compact_status_bar_label_for` | Pinned compact status-item label |
| `HostUsageRuntime::compact_status_bar_strip` | Worst-first multi-surface strip |
| `HostUsageRuntime::overview_rows` | Overview rows for popover + Usage window |
| `HostUsageRuntime::next_refresh_label` | `Next update in …` / `Next update due` |

Avoid cloning full usage views during account materialization — serialize from borrowed views/iterators.

## How to verify

```sh
cargo nextest run -p jackin-usage -p jackin-usage-ffi
cargo clippy -p jackin-usage -p jackin-usage-ffi --all-targets -- -D warnings
```

