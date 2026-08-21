# jackin-usage

Usage, telemetry, and token monitors for the `jackin-capsule` daemon.
Also owns the **Capsule-free host runtime** consumed by the macOS usage menu bar
and `jackin usage host snapshot`.

**Product surfaces (Capsule usage UI, jackin❯ desktop):** **usage limits only** —
remaining/used %, resets, plan/status. **Never** token unit prices or historical
usage/spend trends as product features.

## What this crate owns

- Token monitoring (`token_monitor`) and usage accounting (`usage`) for running agents.
- Host orchestration (`host`) — `HostUsageRuntime` for menu bar / CLI without Capsule.
- Host broker/coordinator (`host/broker`, `coordinator`) — canonical per-account
  generations, bounded provider dispatch, atomic state, shared retry policy, and
  capability-scoped clients.
- The process service executable is owned by `jackin-runtime`; this crate exposes
  the lower-tier broker protocol, coordinator, and client seams only.
- Rust-owned account discovery (`host/discovery`) — read-only global, workspace,
  role, and workspace-role enumeration; explicit profile/protected-source probes;
  pre-source and post-auth account deduplication; sanitized diagnostics.
- Usage snapshot persistence (`usage_snapshot_store`) and token-accounting telemetry (`telemetry`).
- Usage output shaping (`output`).
- Provider probes (`usage/<provider>.rs`). Amp API/CLI share
  `parse_amp_usage_output`; `Amp Free` maps to `StatusSlot::Daily`, while credit
  balances remain detail-only quota bounds.

## Architecture tier and allowed dependencies

**Infrastructure** (capsule-side + host menu-bar observability/accounting). Allowed
inward dependencies: `jackin-core`, `jackin-config`, `jackin-protocol`, and
`jackin-diagnostics`.
No dependency on `jackin-capsule` (which would be circular), `jackin-tui`,
`jackin-console`, `jackin-launch`, or any presentation crate.

boltffi lives in sibling crate `jackin-usage-ffi`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`host.rs`](src/host.rs) · [`host/`](src/host) | Capsule-free host runtime | [`tests.rs`](src/host/tests.rs) |
| [`coordinator.rs`](src/coordinator.rs) · [`coordinator/`](src/coordinator) | Broker-owned single-flight generations and host-only atomic account state | [`tests.rs`](src/coordinator/tests.rs) |
| [`token_monitor.rs`](src/token_monitor.rs) · [`token_monitor/`](src/token_monitor) | token spend monitoring | [`tests.rs`](src/token_monitor/tests.rs) |
| [`usage.rs`](src/usage.rs) · [`usage/`](src/usage) | usage/pricing accounting | [`tests.rs`](src/usage/tests.rs) |
| [`telemetry.rs`](src/telemetry.rs) | telemetry emission | — |
| [`process_telemetry.rs`](src/process_telemetry.rs) | child-process telemetry ownership and redaction | — |
| [`logging.rs`](src/logging.rs) | telemetry-level state and Capsule panic handling | — |
| [`usage_snapshot_store.rs`](src/usage_snapshot_store.rs) · [`usage_snapshot_store/`](src/usage_snapshot_store) | persistent usage snapshot store | [`tests.rs`](src/usage_snapshot_store/tests.rs) |
| [`store_backend.rs`](src/store_backend.rs) | turso SQLite import chokepoint | — |
| [`output.rs`](src/output.rs) | usage output shaping | — |

## Public API

The host broker alone calls providers and writes shared state. Clients join canonical
account generations; timeouts retain ownership. Failure is fail-closed and preserves
last-good quota. Atomic host-only state includes generation, result, failures, and the
provider deadline or shared exponential fallback.

`quota_pace_label` emits the Rust-owned `"<pace> · Runs out in <duration>"`
segment only when the exact projection precedes reset.

Grok decodes ACP billing `config`; server `subscription_tier` owns plan copy,
and prepaid/on-demand values render only as quota bounds.

Host display APIs are presentation-only:

| API | Role |
|---|---|
| `usage::provider_display_label` | Shared Capsule/Desktop provider remap (`Codex`→`OpenAI`, …) |
| `usage::estimate_caption` | Honesty caption for estimated / local-log views |
| `usage::{UsageFormatPrefs,PercentStyle,ResetStyle}` | left/used + countdown/exact-clock prefs |
| `HostUsageRuntime::{set_format_prefs,compact_status_bar_label_for,compact_status_bar_strip}` | Status-item preferences and labels |
| `HostUsageRuntime::{overview_rows,next_refresh_label}` | Overview rows and refresh recency |
| `usage::usage_bucket_presentation` / `usage_display_status_label` | Rust-owned limits-only quota-bucket segments (shared by Capsule + Desktop) |
| `usage::usage_detail_presentation` | Fixed-order Capsule/Desktop detail card |
| `host::HostProviderGlanceRow` / `HostUsageRuntime::provider_glance_rows` | Selected-account-aware seven-provider Desktop glance rows (`DESKTOP_PROVIDER_ORDER`) |
| `HostUsageRuntime::desktop_inventory` | Atomic canonical provider/account groups with complete display fields |
| `host::HostProbePolicy` | `Live` / `Disabled` (smoke-mode probe suppression) |

Canonical identity uses typed provider IDs or stable non-secret handles—not source
ordinals, secrets, agent names, or display labels. Rust publishes current membership
with ICU4X ranks; unresolved evidence stays separate. Desktop filters OpenCode.

## Desktop account contract

Keys hash the canonical surface with a provider subject or account label. Same
email across providers remains distinct. Empty, unknown, presence-only, and
fabricated local-auth labels never become keys.

`desktop_inventory` merges provenance while separating lifecycle from freshness.
Selection accepts only same-surface keys; stale choices clear, and only current
accounts become implicit fallbacks.

Desktop discovery reads global config and every effective workspace/role scope at
open and manual Refresh. Background polling reuses the catalog. Only current
discovery creates membership; history only enriches it. Paths and secrets stay in
Rust. OpenCode and GitHub are outside the seven-provider Desktop catalog.

Capsule discovery is capability-only. The runtime relay exposes only
launch-forwarded capabilities; host catalog/state and in-Capsule credentials do
not cross that boundary.

Each account owns its plan/status, remaining label and geometry, reset phrase
and exact reset, severity, recency, and error. Native clients render all DTO fields
exactly.

## How to verify

```sh
cargo nextest run -p jackin-usage -p jackin-usage-ffi
cargo clippy -p jackin-usage -p jackin-usage-ffi --all-targets -- -D warnings
```
