# jackin-usage

Usage, telemetry, and token monitors for the `jackin-capsule` daemon.
The macOS usage menu bar and `jackin usage status --monitor ID` consume
broker-owned publications through the host projection and broker client APIs.

**Product surfaces (Capsule usage UI, jackin❯ desktop):** **usage limits only** —
remaining/used %, resets, plan/status. **Never** token unit prices or historical
usage/spend trends as product features.

## What this crate owns

- Token monitoring (`token_monitor`) and usage accounting (`usage`) for running agents.
- Host presentation (`host`) — a credential-free projection adapter and broker client exports.
- Host broker/coordinator (`host/broker`, `coordinator`) — canonical per-account
  generations, bounded provider dispatch, atomic state, shared retry policy, and
  capability-scoped clients.
- The process service executable is owned by `jackin-runtime`; this crate exposes
  the lower-tier broker protocol, coordinator, and client seams only.
- Broker-owned account discovery (`jackin-usage-discovery` and `jackin-usage-broker`) —
  host config and credential resolution stay in the broker process; consumers receive
  canonical projections and sanitized diagnostics.
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
| [`host.rs`](src/host.rs) · [`host/`](src/host) | broker and projection consumer facade | — |
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

The host broker alone discovers credentials, calls providers, and writes shared state.
Clients read canonical publications or use broker operations that preserve shared
generation ownership. Timeouts do not cancel provider work. Failure is fail-closed and
preserves last-good quota. Atomic host-only state includes generation, result, failures,
and the provider deadline or shared exponential fallback.

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
| `usage::usage_bucket_presentation` / `usage_display_status_label` | Rust-owned limits-only quota-bucket segments (shared by Capsule + Desktop) |
| `usage::usage_detail_presentation` | Fixed-order Capsule/Desktop detail card |
| `host::HostUsageProjectionRuntime` | Canonical provider/account groups, typed metric details, and persisted canonical account selection |

Canonical identity uses typed provider IDs or stable non-secret handles—not source
ordinals, secrets, agent names, or display labels. Rust publishes current membership
with ICU4X ranks; unresolved evidence stays separate. Desktop filters OpenCode.

## Desktop account contract

The projection adapter persists the broker's canonical account id for each
selected surface. A missing selected id stays explicitly unavailable and never
falls back to a sibling account. Typed provider metric groups and broker issues
remain intact through native presentation.

The broker reads global config and effective workspace/role scopes. Native and CLI
consumers read broker publications; they do not discover credentials or provider
accounts. Only current broker discovery creates membership; history only enriches it.
OpenCode and GitHub are outside the seven-provider Desktop catalog.

Capsule relay capabilities are resolved inside the host broker from launch-forwarded
source metadata and exact staged credential fingerprints. The relay pins only the
capabilities returned by that broker operation; host catalog/state and in-Capsule
credential values do not cross the boundary. Declarations requiring interaction remain
unavailable to unattended discovery. The broker projection reports the typed issue;
the current explicit auth-preparation flow covers Claude Keychain credentials only and
does not prepare `OpRef` environment sources.

Each account owns its plan/status, remaining label and geometry, reset phrase
and exact reset, severity, recency, and error. Native clients render all DTO fields
exactly.

## How to verify

```sh
cargo nextest run -p jackin-usage -p jackin-usage-ffi
cargo clippy -p jackin-usage -p jackin-usage-ffi --all-targets -- -D warnings
```
