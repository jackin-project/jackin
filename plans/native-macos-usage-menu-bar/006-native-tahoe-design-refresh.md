# Plan 006: Bring the menu bar app to native Tahoe-grade look and feel

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. Swift stays display-only — if any step seems to require provider logic, HTTP, OAuth, or CLI probing in Swift, that is a STOP condition, not a judgment call. Update this plan's row in `plans/native-macos-usage-menu-bar/README.md` when complete.
>
> **Drift check (run first)**: `git diff --stat 3c49fff0..HEAD -- native crates/jackin-usage-ffi crates/jackin-usage/src/host.rs 'docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx'`
>
> If an in-scope file changed, compare the "Current state" excerpts below with live code before proceeding; a mismatch is a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: 005 preferred first (propagation semantics the UI describes); independent of 001–004
- **Category**: direction / dx
- **Planned at**: commit `3c49fff0`, 2026-07-21
- **Design references**: CodexBar (github.com/steipete/codexbar) and OpenUsage (github.com/robinebers/openusage) — concepts only, clean-room; no code copying, no provider scope import.

## Why this matters

The operator wants the menu bar app to look and behave like a first-class modern macOS citizen — the standard native status item and menus, current (macOS 26 "Tahoe" / Liquid Glass) look and feel — while displaying exactly what jackin❯ itself displays. The shipped shell works but reads as a prototype: the status item is a text-only label up to 48 characters wide (Apple's HIG hides wide extras first when menu bar space runs out), there is no template icon, no Quit or Settings affordance in the popover, no launch-at-login, flat `ProgressView` bars instead of capacity gauges, and no Liquid Glass adoption path. CodexBar and OpenUsage prove the genre's UX conventions; this plan adopts those conventions natively while keeping every number Rust-owned.

## Current state

- `native/Sources/JackinUsageMenuBar/JackinUsageMenuBarApp.swift` — the entire UI today (single file). Key excerpts:

  ```swift
  MenuBarExtra {
      PopoverRoot(store: store)
  } label: {
      Text(compactBarText(store.mergedBarLabel))   // text-only, truncates at 48 chars
          .font(.system(size: 12, weight: .medium, design: .monospaced))
  }
  .menuBarExtraStyle(.window)

  Settings { SettingsView(store: store) }
  ```

  `PopoverRoot` (lines 37-80): header + Refresh button (⌘R), error line, scroll of `SurfaceTile`s, footer caption. **No Quit, no Settings entry point** — for an `LSUIElement` app the popover is the only discoverable surface, so Quit must be present.

  `SurfaceTile` (lines 82-142): card on `Color(nsColor: .controlBackgroundColor)`, status chip colored by raw status string, `statusBarLabel` monospaced text, `BucketBar` rows.

  `BucketBar` (lines 144-178): `ProgressView(value:)` tinted by Rust `severity` (`danger`→red, `warn`→orange, else accent); renders only when Rust supplied `remaining_percent` (comment: "Never invent % — only draw when Rust provided remaining_percent"). Keep that invariant literally.

  `SettingsView` (lines 180-233): surface toggles + refresh-floor slider (floor lives in Rust, clamped ≥60 s). No launch-at-login.

- `native/Sources/JackinUsageBridge/PresentationStore.swift` — `@MainActor ObservableObject`; polls Rust every 5 s (`pollOnce`, lines 125-164); projects UniFFI DTOs into `SurfaceRow`/`BucketRow`; `mergedBarLabel` comes from `bridge.mergedStatusBarLabel()`. All strings/percents originate in Rust.

- `crates/jackin-usage-ffi/src/bridge.rs` — UniFFI object `UsageMenuBarBridge` (methods: `openRuntime`, `refresh`, `snapshot`, `listSurfaces`, `mergedStatusBarLabel`, `nextEvents`, `refreshFloorSecs`, …). New display strings are added here + `dto.rs`, backed by `crates/jackin-usage/src/host.rs` (`HostUsageRuntime`), then bindings regenerated via `scripts/generate-usage-swift-bindings.sh` (never hand-edit `native/Sources/JackinUsageBridge/jackin_usage_ffi.swift` or `native/Generated/`).

- `native/Tests/JackinUsageBridgeTests/ArchitectureTests.swift` — source-scan test: no probes/HTTP/OAuth/providers in handwritten Swift. Extend, never weaken.

- `native/Package.swift` — macOS 14 floor (`platforms`). The floor stays 14 (Plan 001/roadmap decision); Liquid Glass appears automatically when the app is **built with the macOS 26 SDK** and runs on Tahoe, with classic materials on 14/15.

- Design research facts the executor must honor (sourced from Apple HIG "The menu bar", Apple "Adopting Liquid Glass", Bjango menu-bar-extras guide):
  - Status-item icons are **template images** (black + alpha; system recolors for light/dark/selected). 16×16 pt weight-matched glyph inside the 22 pt working area. SF Symbols are template by default.
  - Percent text next to the icon must use **monospaced digits** so 87%→88% doesn't shift neighbors, and must stay short — the system hides wide extras.
  - `glassEffect`/`GlassEffectContainer` are macOS 26.0+ APIs; every `#available(macOS 26, *)` gate goes in **one** centralized fallbacks file (OpenUsage's proven pattern), falling back to `.ultraThinMaterial`/standard styles. Don't put custom material backgrounds behind content inside a glass popover.
  - `Gauge` with `.accessoryLinearCapacity` is the native capacity meter; capacity styles take only the **first** color of a gradient, so threshold color must be computed, not gradient-driven.
  - Launch-at-login: `SMAppService.mainApp.register()/unregister()`; read truth from `SMAppService.mainApp.status`, never a cached bool; default OFF.
  - `SettingsLink` from menu bar extras is unreliable on Tahoe without an existing SwiftUI render tree; if Settings won't open reliably, the documented workaround is a hidden keepalive window declared before the `Settings` scene — verify on 26 before adopting.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Regenerate bindings (only if FFI surface changed) | `cargo build -p jackin-usage-ffi --release && ./scripts/generate-usage-swift-bindings.sh` | exit 0, deterministic diff |
| Rust tests | `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` | all pass |
| Swift build | `cd native && swift build -c release` | exit 0 |
| Swift tests | `cd native && swift test -c release` | all pass incl. extended ArchitectureTests |
| App fixture | `./scripts/build-usage-menu-bar-app.sh && open native/dist/JackinUsageMenuBar.app` | app launches, status item shows icon (+ optional percent) |
| Docs audits | `cargo xtask docs repo-links && cargo xtask roadmap audit && cargo xtask research check` | exit 0 |
| Merge readiness | `cargo xtask ci --fast` | exit 0 |

Run macOS-only commands on macOS with full Xcode (26.x for the Tahoe verification pass). Do not weaken them to pass on Linux.

## Scope

**In scope:** `native/Sources/JackinUsageMenuBar/**` (may split the single file into `StatusItemLabel.swift`, `PopoverRoot.swift`, `SurfaceCard.swift`, `SettingsView.swift`, `GlassFallbacks.swift`), `native/Sources/JackinUsageBridge/PresentationStore.swift` (projection additions only), `native/Tests/**`, `crates/jackin-usage/src/host.rs` + `crates/jackin-usage-ffi/src/{bridge,dto}.rs` (new **display-string** API only, Step 1), regenerated bindings, `native/README.md`, operator guide screenshots/wording, this plan/index status.

**Out of scope (do NOT touch):**

- Provider probes, refresh scheduling, cooldowns, DTO semantics for existing fields — display strings may be *added* in Rust, nothing existing changes meaning.
- Providers beyond `Agent::ALL` + Z.AI/GLM + MiniMax (hard ADR-011 scope; no Cursor/Gemini/Copilot ever).
- Packaging/signing/CI (`Package.swift` linkage, scripts, workflows — Plans 001/003 own them). If a new Swift source file requires a trivial target listing change, that alone is allowed.
- Sparkle, widgets, multi-status-item ("merge icons") layouts, drag-reorder customization — recorded as future ideas, not v1.
- Switching from `MenuBarExtra` to AppKit `NSStatusItem`+`NSPanel` (see STOP conditions).

## Git workflow

- Stay on the active feature branch; if starting from `main`, propose `feature/usage-menu-bar-tahoe-design` and wait for operator confirmation.
- Signed Conventional Commits, push after every commit. No PR unless the operator asks.

## Steps

### Step 1: Add a Rust-owned compact status-item label

The status item needs a short label (worst surface, e.g. `Cl 82%` or just `82%`) instead of the 48-char merged string. Selection of "which number" is presentation policy but must not invent percentages, so compute it in Rust: add `compact_status_bar_label()` to `HostUsageRuntime` (`crates/jackin-usage/src/host.rs`) returning the enabled surface with the highest used percent as a short label (Rust already owns `status_bar_label` composition; follow its formatting conventions, honest `…` / empty when nothing is available), expose it through `crates/jackin-usage-ffi` (`bridge.rs` + `dto.rs`), regenerate bindings, and surface it as `compactBarLabel` in `PresentationStore`.

**Verify**: Rust unit test for the new label (golden-style, covering: normal, tie, all-unavailable, all-disabled); `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` passes; regenerated bindings show a deterministic diff.

### Step 2: Native status item — template icon + optional monospaced percent

Replace the text-only `MenuBarExtra` label with `HStack(icon, optional Text)`: a template-rendered brand glyph (SF Symbol placeholder such as `gauge.with.needle` is acceptable until a custom template asset exists; set `.accessibilityLabel("jackin usage")`) plus optional `Text(store.compactBarLabel).monospacedDigit()`. Settings toggle "Show percent in menu bar" (default ON) switches to icon-only. Dim the icon (opacity, still template) when every enabled surface is stale/unavailable — never colorize the status item.

**Verify**: build + launch fixture; status item shows icon+percent; toggling the setting flips to icon-only without relaunch; digits don't shift width as values change.

### Step 3: Popover redesign — cards, gauges, countdowns, footer actions

Rework `PopoverRoot`/`SurfaceTile` into provider cards following the shared genre conventions (clean-room):

- Per-bucket `Gauge(value:)` with `.gaugeStyle(.accessoryLinearCapacity)`, tint **computed** from Rust `severity` (existing mapping: danger→red, warn→orange, else accent). Value = `100 - remainingPercent` exactly as today; still render nothing when `remainingPercent == nil`.
- Bucket row: label left; used label + reset countdown (`resetLabel`) right, `.monospacedDigit()`.
- Card header: surface label, account + plan secondary line, status conveyed by dimming + an SF Symbol badge (`exclamationmark.triangle` for error/needs-login, `clock` for stale) with text tooltip — not by the raw status-enum string. Last-good numbers stay visible under an error badge (Rust already preserves them; the UI must not hide buckets when `lastError` is set).
- Footer: `updatedLabel`, then persistent actions — Refresh (⌘R), Settings… (⌘,), Quit (⌘Q, `NSApplication.shared.terminate`). Quit is mandatory for an `LSUIElement` app.
- Remove the `controlBackgroundColor` card fill; use the glass-safe backgrounds from Step 4.

**Verify**: build + launch; keyboard: ⌘R refreshes, ⌘, opens Settings, ⌘Q quits, Esc closes popover; VoiceOver reads each card as one element with label/percent/reset (extend the existing `accessibilityElement(children: .combine)` approach).

### Step 4: Centralized Liquid Glass adoption with macOS 14 fallback

Create `native/Sources/JackinUsageMenuBar/GlassFallbacks.swift` — the **only** file allowed to contain `#available(macOS 26, *)` checks — exposing small helpers (e.g. `glassChromeBackground()`, `glassButtonStyle()`) that resolve to Tahoe glass APIs on 26 and `.ultraThinMaterial`/`.bordered` equivalents earlier. Apply glass only to chrome (footer action bar, status chips), never behind card content. Respect Reduce Transparency automatically by using system styles (no custom `NSVisualEffectView`). Keep deployment target macOS 14; document in `native/README.md` that release builds must use the macOS 26 SDK for Tahoe rendering.

**Verify**: `cd native && swift build -c release` on Xcode 26 → exit 0; `rg -n "#available\(macOS 26" native/Sources | grep -v GlassFallbacks.swift` → no matches; app renders correctly on a macOS 14/15 machine or with Reduce Transparency enabled (visual check, note result).

### Step 5: Settings — launch at login + display options

In `SettingsView`: add a "Launch at login" toggle driven by `SMAppService.mainApp` — register/unregister on toggle, read displayed state from `.status`, handle `requiresApproval` by pointing the user at System Settings; default OFF. Add the Step 2 "Show percent in menu bar" toggle (UserDefaults; a display preference, allowed in Swift). Keep surface toggles and the Rust-owned floor slider unchanged. If `SettingsLink`/⌘, fails to open Settings reliably on Tahoe in testing, adopt the hidden-keepalive-window workaround (declare a 1×1 hidden `Window` scene before `Settings`) and record it with a WHY comment.

**Verify**: toggle registers/unregisters (System Settings → General → Login Items shows the app); relaunch reflects true `.status`; Settings opens from the popover on macOS 26.

### Step 6: Guard the architecture and update docs

Extend `ArchitectureTests.swift`: forbid `URLSession`/`Process`/keychain APIs in `JackinUsageMenuBar` sources (if not already covered), assert `#available(macOS 26` appears only in `GlassFallbacks.swift`, and assert `Text(` is never fed by string interpolation computing percentages (keep it a source-scan heuristic consistent with the existing test style). Update `native/README.md` (new file layout, SDK requirement) and the operator guide (Settings additions, launch-at-login, menu bar appearance) — operator-visible behavior only, no internal paths.

**Verify**: `cd native && swift test -c release` → all pass; docs audits → exit 0; `cargo xtask ci --fast` → exit 0.

## Test plan

- Rust: golden tests for `compact_status_bar_label` (Step 1) beside the existing host label tests in `crates/jackin-usage/src/host.rs` / its test module — no invented percent, honest empty states.
- Swift: extended `ArchitectureTests` (Step 6) is the enforcement layer for display-only + centralized availability gates; add a unit test for the severity→tint mapping function and for status→badge mapping (pure functions, extract them to be testable).
- Manual matrix (record in PR): macOS 26 Tahoe (glass), macOS 14 or Reduce Transparency (fallback), light/dark, VoiceOver pass, keyboard shortcuts, percent-toggle, launch-at-login round-trip.

## Done criteria

- [x] Status item: template icon + optional Rust-provided monospaced percent; never wider than icon + 6 characters; dims when stale.
- [x] Popover: capacity gauges, reset countdowns, error/stale badges that never hide last-good data, Refresh/Settings/Quit with shortcuts.
- [x] All `#available(macOS 26, *)` gates live in `GlassFallbacks.swift`; app builds and renders on macOS 14+ and adopts glass on 26.
- [x] Launch-at-login via `SMAppService` with `.status` as source of truth.
- [x] `ArchitectureTests` enforce no-probe + centralized-gates invariants; all Rust/Swift tests pass.
- [x] Docs (native README + operator guide) updated; docs audits and `cargo xtask ci --fast` pass.

## Execution status (honest)

- Implementation + docs + Rust nextest for usage/ffi green; local `swift build -c release` green; `#available` only in `GlassFallbacks.swift`.
- Swift tests proven on GHA `native-usage-menu-bar` (run 29818901390, Swift tests step success). Local host remains CLT-only (no XCTest).
- Local `cargo xtask ci --fast` exit 0.

## STOP conditions

- Any step needs provider/HTTP/OAuth/CLI logic or percentage arithmetic in Swift beyond rendering Rust-provided fields.
- `MenuBarExtra(.window)` proves unable to become key / accept first-click keyboard input on macOS 26 (the known accessory-app activation issue that pushed OpenUsage and CodexBar to AppKit panels). Do not silently migrate to `NSStatusItem`+`NSPanel` — stop and report with the exact repro; that migration is an architecture decision for the operator.
- `SettingsLink` and the keepalive-window workaround both fail to open Settings on 26.
- The FFI surface change in Step 1 would break existing DTO fields or golden tests.
- Bindings regeneration is nondeterministic.

## Maintenance notes

Future surface additions (new agents in Rust) must appear in the UI with zero Swift changes — the card list is driven by `listSurfaces()`; reviewers should reject PRs adding per-provider Swift branches. The CodexBar-style ideas deliberately deferred: multi-account cards, per-provider status items / merge-icons layout editor, notifications on threshold crossings, WidgetKit — each needs its own Rust view support first. When Plan 001's static XCFramework path lands, re-run this plan's build fixtures on both architectures.
