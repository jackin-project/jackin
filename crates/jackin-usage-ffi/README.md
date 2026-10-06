# jackin-usage-ffi

Synchronous boltffi facade over `jackin-usage` host runtime for the native macOS
agent-usage menu bar. Mirrors TableRock’s `tablerock-ffi` split: Rust owns all
truth; Swift is display-only.

**Limits only** for Desktop DTOs — no token unit prices or historical usage
trends.

## Build

```sh
cargo build -p jackin-usage-ffi --release
cargo nextest run -p jackin-usage-ffi
cargo clippy -p jackin-usage-ffi --all-targets -- -D warnings
```

## boltffi surface (additive desktop v1)

| Method / type | Role |
|---|---|
| `set_format_prefs` | Presentation prefs (`left`/`used`, `countdown`/`exact_clock`) |
| `compact_status_bar_label_for` | Pinned surface compact label |
| `compact_status_bar_strip` | Worst-first multi-surface strip |
| `overview_rows` → `OverviewRowDto` | Popover + Usage-window overview |
| `desktop_inventory` → `DesktopInventoryDto` | Atomic ordered canonical provider/account graph |
| `discovery_diagnostics` → `DiscoveryDiagnosticDto` | Sanitized provider/scope discovery failures; never paths or secrets |
| `next_refresh_label` | Next refresh countdown / due |
| `UsageViewDto.estimate_caption` | Honesty caption when estimated |
| `UsageViewDto.detail_presentation` → `UsageDetailPresentationDto` | Rust-owned Capsule-parity provider-detail card (rows/lines mirror `UsageDetailPresentation`); the Usage window renders it verbatim |

Production `OpenConfig` supplies no paths: Rust derives the operator home, config
root, and data root through `JackinPaths`, exactly like the CLI. Optional data/config
overrides exist only for tests. `jackin-usage` owns credential resolution, opaque
secret handles, discovery, quota shaping, and broker coordination.

Refresh sends broker intent. Rust workers join active generations; Swift never blocks
the main actor. Broker failure preserves last-good quota and never probes directly.

`QuotaBucketDto.status_slot` is exactly `"session"`, `"daily"`, `"weekly"`, or
`"spend"`; Swift renders it without inference.

`QuotaBucketDto` also carries the Rust-owned limits-only presentation
(`remaining_label`, `display_segments`, `display_label`, `meter_percent`), so
Swift renders the segments verbatim. `provider_glance_rows()` (Swift
`providerGlanceRows()`) returns `ProviderGlanceRowDto` — the selected-account-aware
seven-provider Desktop glance rows in canonical order. `OpenConfig.allow_live_probes`
maps to the Rust `HostProbePolicy` (false = smoke/defense mode, no live probes).

`DesktopInventoryDto` carries the seven-provider Rust order, provider chrome, and
self-contained account identity/lifecycle/limits/status fields. OpenCode is absent;
Swift renders without cross-account joins.

`UsageViewDto.detail_presentation` mirrors the Capsule Rust projection. Rows carry
stable IDs/kinds, grouped lines, display copy, meter geometry, and severity; Swift
does not split, join, or reorder them.

## Swift bindings

```sh
cargo xtask desktop bindings
# or: mise run desktop-bindings
```

## XCFramework

```sh
cargo xtask desktop xcframework
# or: mise run desktop-xcframework
```
