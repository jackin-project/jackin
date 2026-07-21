# Plan 012 acceptance record (2026-07-22)

Built on HEAD of `docs/native-macos-usage-menu-bar` after plans 007–011.

## Bundle

- `JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/build-usage-menu-bar-app.sh` → OK
- `./scripts/verify-usage-menu-bar-app.sh native/dist/JackinDesktop.app` → OK
- Plist: `com.jackin-project.desktop`, `Jackin Desktop`, `JackinDesktop`, `LSUIElement=true`
- `cargo nextest run -p jackin-usage -p jackin-usage-ffi -p jackin-capsule --locked` → all pass

## Capsule parity (structural)

Desktop and Capsule share one truth path: `jackin-usage` view shaping → protocol DTOs → Capsule TUI **or** UniFFI `UsageViewDto` / `overview_rows` / compact labels. Provider display remap is a single `provider_display_label` (Capsule re-exports it). Goldens in `host/tests.rs` / `usage/tests.rs` / `bridge/tests.rs` pin compact strip, overview headlines, format prefs, estimate caption, and depleted forms.

| Surface / field group | Desktop source | Capsule source | Match basis |
|---|---|---|---|
| Identity (provider label) | `SurfaceRow.label` / overview `displayLabel` via remap | Capsule tab strip via `usage_provider_display_label` | Shared remap |
| Account / plan / auth | `accountLabel`, `planLabel`, `credentialOrigin` from DTO | Same DTO fields in dialog | Shared snapshot |
| Bucket % / reset / pace / money | `usedLabel`, `resetLabel`, `paceLabel`, `limitLabel` | Same bucket fields | Shared snapshot |
| Compact status item | `compactStatusBarLabel` / `For` / `Strip` | N/A (Desktop-only surface) | Rust goldens |
| Overview rows | `overviewRows` | Capsule Overview tab status labels | Shared remaining/reset helpers |
| Estimate caption | `estimateCaption` | Same confidence/source rules | Shared `estimate_caption` |
| Status words | DTO `status` storage labels | Same | Shared enum labels |

**Live side-by-side gap (honest):** this executor environment did not drive a full interactive Capsule TUI + Desktop UI for every live provider/money window. Structural parity + goldens stand; plan 004 production proof should re-run live spot-checks on Apple Silicon after notarized install.

## S1–S6 + design contract

| Screen | Result | Notes |
|---|---|---|
| S1 status item modes | pass | focus / icon / pinned / strip; depleted via Rust; dim 0.45; **cold launch** opens `HostUsageRuntime` from `StatusItemLabel`/`keepalive` `onAppear` (no menu click required) |
| S1 privacy collapse | pass | `CGSessionCopyCurrentDictionary` gated by prefs |
| S2 glance popover | pass | overview strip + menu footer; detail cards removed |
| S3 Usage window | pass | `NavigationSplitView`, Open Usage…, severity sidebar |
| S4 provider card states | pass | no invented gauges; last-good + error text; estimate caption |
| S5 empty / first-run | pass | static copy + Open Settings; no credential harvest UI |
| S6 Settings pickers | pass | display modes + format prefs; depleted checkbox omitted (unconditional Rust depleted branch — recorded in 009) |
| Floating panel / glass | pass-with-note | `MenuBarExtra(.window)` system chrome may retain arrow/corners; inner panel uses glass helper |
| Menu-row footer / next-refresh | pass | `nextRefreshLabel` + updated caption |
| Architecture guards | pass | no Swift usage-string composition on display surfaces; Settings may use `% left`/`% used` as **format-picker chrome only** (allowlisted in `ArchitectureTests`); glass only in `GlassFallbacks`. Local host is CLT-only (`import XCTest` unavailable) — static scan of the test body + display sources re-verified after 009/012; CI `native-usage-menu-bar` job runs `swift test -c release` on full Xcode |

## Roadmap prose honesty (plan 012 Step 3)

- Open work §4 rewritten from **open** unshipped bullets to **done** shipped phrasing (identity, modes, popover, Usage window); activation residual remains only in Open work §3 + checklist Activation item.
- Item `**Status**` line and overview index state v1 UI complete with ops residual only.

## Residual (named only)

1. **Plan 003 activation** — `mode=publish` blocked on Apple secrets in GitHub environment `release-macos`.
2. **Plan 004** — production proof + roadmap page retirement after first notarized ZIP + operator-merged `jackin-desktop` cask.
