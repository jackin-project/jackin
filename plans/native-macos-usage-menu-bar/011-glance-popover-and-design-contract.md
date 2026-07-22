# Plan 011: Reduce the glance popover to the overview strip and land the v1 design contract (screens S2 + S5)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. Swift is display-only — all row text comes from `overviewRows()` / `nextRefreshLabel()`; composing, truncating, or reformatting a Rust string is a STOP. When done, update this plan's row in `plans/native-macos-usage-menu-bar/README.md`.
>
> **Drift check (run first)**: `git diff --stat be6fb79e..HEAD -- native crates/jackin-usage-ffi`
> Expected drift: Plans 007–010. Confirm the Usage window exists (`native/Sources/JackinDesktop/UsageWindow/` present, window id `"usage"`) and `rg -n "overviewRows|nextRefreshLabel" native/Sources/JackinUsageBridge/jackin_usage_ffi.swift` matches. Missing → STOP (dependency not met).

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW–MED (removes the popover's detail cards — the window must already carry them, hence the dependency)
- **Depends on**: Plan 008 (overview rows, next-refresh label), Plan 010 (Usage window hosts the detail the popover loses). Plan 009 ordering preferred (shared store edits).
- **Category**: direction
- **Planned at**: commit `be6fb79e`, 2026-07-22

## Why this matters

Roadmap S2/S5: the popover becomes a **glance** — one overview line per enabled surface (mirroring the Capsule Overview tab), menu-style footer actions with key equivalents, an `Updated … · Next update in …` footer, and the honest empty/first-run state. Detail lives in the Usage window (Plan 010); keeping full cards in the popover would duplicate the rendering surface and drift. The design contract ("Native design reference", roadmap 2026-07-22 screenshot review) fixes how it feels: detached floating panel, menu-row footer, generous rhythm — while every string stays a Capsule-parity Rust string. Ordering note: this plan intentionally runs **after** 010 so no intermediate commit loses detail visibility (popover cards are removed only once the window shows them).

## Current state

- `native/Sources/JackinDesktop/PopoverRoot.swift` — today: header (`Text("jackin❯ Desktop")` post-007 + Refresh button), optional error line, `ScrollView`/`LazyVStack` of full `SurfaceCard`s (`maxHeight: 420`), footer:

  ```swift
  // PopoverRoot.swift:57-88 (pre-007 numbering) — footer to be replaced
  HStack(spacing: 12) {
      Text(footerUpdatedLabel) …
      Button("Refresh") { store.refreshAll() }.keyboardShortcut("r", …)
      SettingsLink { Text("Settings…") }.keyboardShortcut(",", …)
      Button("Quit") { NSApplication.shared.terminate(nil) }.keyboardShortcut("q", …)
  }
  .background { GlassFallbacks.footerBarBackground() }
  ```

  `footerUpdatedLabel` (`PopoverRoot.swift:90-97`) falls back to `"Rust owns probes · Swift display only"`. Plan 010 added an "Open Usage…" action.
- `native/Sources/JackinDesktop/SurfaceCard.swift` — the full card + `BucketGaugeRow`. After this plan the popover no longer uses it; the Usage window's `ProviderCardView` (Plan 010) is the detail renderer. Delete `SurfaceCard.swift` only if nothing else references it (Step 2) — dead display code is a defect, not a keepsake (repo dead-code lints cover Rust, not Swift; enforce by hand).
- Store: `overviewRows` published (Plan 010 Step 1), `bridge.nextRefreshLabel()` exported (Plan 008) but not yet projected.
- S2 target (roadmap sketch — layout intent, strings literal):

  ```text
  ┌────────────────────────────────────────────────┐
  │ jackin❯ Desktop                                │
  ├────────────────────────────────────────────────┤
  │ OpenAI      97% left · Resets in 6d 22h        │
  │ Anthropic   Fable 68% left · Resets in 2d 12h  │
  │ Amp         fresh                              │
  │ MiniMax     unsupported                        │
  ├────────────────────────────────────────────────┤
  │ ▤ Open Usage…                                  │
  │ ↻ Refresh                                  ⌘R  │
  │ ⚙ Settings…                                ⌘,  │
  │ ⏻ Quit                                     ⌘Q  │
  ├────────────────────────────────────────────────┤
  │ Updated 2m ago · Next update in 4m             │
  └────────────────────────────────────────────────┘
  ```

  Row anatomy: display label left; `headline` (`97% left` / `Fable 68% left`) + ` · ` + `reset_label` when numeric; bare `status_word` (`fresh`, `unsupported`, `needs secret`…) otherwise. Clicking a row opens the Usage window focused on that provider.
- S5 target (roadmap sketch) — shown when no surface is enabled or no credentials resolve:

  ```text
  │ jackin❯ Desktop                                  │
  │ No usage surfaces enabled.                       │
  │ jackin❯ Desktop reads the credentials your agent │
  │ CLIs already store — sign in with an agent, then │
  │ enable its surface in Settings.                  │
  │                                 Open Settings…   │
  ```

  No credential-harvesting UI: no login forms, no token paste. Missing credentials stay Rust `needs login` / `needs secret` / `unavailable` rows.
- Design contract items this plan owns (roadmap "Native design reference"): detached floating panel (own rounded surface, large continuous-corner radius, elevated material, soft shadow, no popover arrow — as far as `MenuBarExtra(.window)` allows; see STOP); menu-style footer actions (icon + label rows, right-aligned key equivalents — not button clusters); pinned footer (muted `Updated … · Next update in …` left; version/menu pill right is **deferred** — no Rust version field exported; do not fake one); typography/rhythm (system font, bold titles, muted meta, generous spacing); honest empty rows.
- Severity accents on rows: `overviewRows().severity` → `severityTint` dot/accent per row (same mapping as the window sidebar).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Swift build | `cd native && swift build -c release` | exit 0 |
| Swift tests | `cd native && swift test -c release` | all pass |
| App fixture | `JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/build-usage-menu-bar-app.sh && open native/dist/JackinDesktop.app` | popover = strip + menu footer |
| Glass-gate guard | `rg -n "#available\(macOS 26" native/Sources | grep -v GlassFallbacks.swift` | no matches |
| Rust guard (unchanged) | `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` | all pass, no edits |
| Merge readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope:** `native/Sources/JackinDesktop/PopoverRoot.swift` (rewrite), `native/Sources/JackinDesktop/SurfaceCard.swift` (delete if unreferenced after the rewrite), `native/Sources/JackinDesktop/GlassFallbacks.swift` (panel-surface helper additions only), `native/Sources/JackinUsageBridge/PresentationStore.swift` (project `nextRefreshLabel`; combined footer string arrives whole from Rust if 008 exported it combined — otherwise render the two Rust strings side by side with a literal ` · ` separator, which is layout, not label composition), `native/Tests/**`, operator guide (popover section + first-run), roadmap checklist ticks (glance popover; S5), plan/README status rows.

**Out of scope (do NOT touch):**

- Any Rust crate / FFI. Missing string → STOP.
- `StatusItemLabel.swift`, `SettingsView.swift` (Plan 009), `UsageWindow/` internals (Plan 010) beyond calling `openWindow(id: "usage")` with a selected surface.
- Version label / consolidated menu pill in the footer (deferred — needs a Rust build-info export; record, don't fake).
- `MenuBarExtra` → `NSStatusItem`+`NSPanel` migration (standing STOP).
- Alerts, notifications, write actions (v1 view-only).

## Git workflow

- Active feature branch; from `main` propose `feature/desktop-glance-popover` and wait for confirmation.
- Signed Conventional Commits, push after every commit. Suggested: `feat(desktop): glance popover overview strip + first-run state`.

## Steps

### Step 1: Project the footer strings

In `PresentationStore.applySnapshots()`: publish `nextRefreshLabel` from `bridge.nextRefreshLabel()`. Footer line = `updatedLabel` (first enabled surface's, as today's `footerUpdatedLabel` does) and `nextRefreshLabel` rendered as two Text runs separated by ` · `. Drop the `"Rust owns probes · Swift display only"` fallback — with no data the S5 state shows instead.

**Verify**: `cd native && swift build -c release` → exit 0.

### Step 2: Rewrite PopoverRoot as the glance strip

Structure: header (`jackin❯ Desktop` title only — Refresh moves to the footer menu rows); rows: `ForEach(store.overviewRows)` — display label (medium weight) left, severity-tinted accent dot, headline + optional ` · ` + reset (secondary color) right-aligned or trailing, `status_word` alone for non-numeric rows; row is a `Button` → `openWindow(id: "usage")` + select that surface (the mechanism Plan 010 exposed); hover highlight; error line stays above the footer when `store.lastError != nil`. Footer: menu-style rows (design contract) — `Open Usage…`, `Refresh ⌘R`, `Settings… ⌘,`, `Quit ⌘Q` — icon + label left, key equivalent right, full-width hover highlight; then the pinned `Updated … · Next update in …` caption. Remove `SurfaceCard` usage; delete `SurfaceCard.swift` + `BucketGaugeRow` if now unreferenced (`rg -n "SurfaceCard|BucketGaugeRow" native/Sources native/Tests` → only the deletion diff). Keep ⌘R/⌘,/⌘Q shortcuts working while the popover is open.

**Verify**: build + launch; rows match the S2 sketch against live data (compare strings with `jackin usage` output); row click opens the window focused on that provider; all three shortcuts fire; VoiceOver reads each row as one element ("OpenAI, 97% left, resets in 6 days 22 hours").

### Step 3: Floating-panel styling

In `GlassFallbacks.swift` add a panel-surface helper: macOS 26 — glass panel background, continuous-corner radius on the popover content; earlier — elevated material equivalent. Apply as the popover root background. Stay within what `MenuBarExtra(.window)` allows — if the system chrome (arrow/square corners) cannot be restyled from inside the view, apply the contract to the inner surface and record the limitation in the PR; do **not** migrate to `NSPanel` (STOP list). Generous spacing/typography pass per the contract (bold title, regular values, muted meta, no dense grid).

**Verify**: glass-gate guard command → no matches; visual pass macOS 26 + Reduce Transparency + light/dark; record.

### Step 4: S5 empty / first-run state

When `overviewRows` is empty (no enabled surfaces) — or every surface is disabled — render the S5 copy verbatim (static UI copy, allowed in Swift; it is not usage data) with an `Open Settings…` action (`SettingsLink`). Rows with `needs login`/`needs secret` are **not** empty-state triggers — they render as normal status-word rows (honest degradation, S2).

**Verify**: disable all surfaces in Settings → S5 state appears with working Open Settings; re-enable → strip returns without relaunch.

### Step 5: Guards + docs

Extend `ArchitectureTests.swift`: assert `SurfaceCard` no longer exists (or is unreferenced) if deleted; assert the popover source contains no `Gauge(` (detail belongs to the window); existing scans cover the rest. Operator guide: rewrite the popover walkthrough (glance rows, Open Usage, first-run). Tick roadmap checklist items (glance popover overview strip; S5 within the S1–S6 item as applicable).

**Verify**: `cd native && swift test -c release` → all pass; `cargo xtask docs repo-links && cargo xtask roadmap audit` → exit 0; `cargo xtask ci --fast` → exit 0.

## Test plan

- Swift: ArchitectureTests additions (Step 5); unit test for the row-variant selection (numeric+reset / numeric only / status-word) if extracted as a pure function (extract it).
- Manual matrix (record in PR): live strip vs Capsule Overview tab string-for-string; empty state round-trip; row click focus correctness for ≥3 providers; shortcuts; VoiceOver; macOS 26 glass + 14/Reduce Transparency; light/dark.
- Rust: none (nextest proves untouched).

## Done criteria

- [ ] Popover = title + overview rows + menu-row footer + updated/next-refresh caption; no detail cards remain in the popover.
- [ ] Row strings byte-match Capsule Overview conventions (Rust-delivered; no Swift composition in the diff).
- [ ] Row click opens the Usage window focused on the clicked provider.
- [ ] S5 first-run state renders with no credential-harvesting UI.
- [ ] Glass gates centralized; ArchitectureTests green; docs audits + `cargo xtask ci --fast` exit 0; guide + roadmap updated.

## STOP conditions

- Dependencies absent (drift check): no Usage window, or `overviewRows`/`nextRefreshLabel` missing from bindings.
- A row needs data `overviewRows()` doesn't carry (e.g. the exact-clock right column if the operator asks for it here) — Rust-shaped change, report.
- `MenuBarExtra(.window)` cannot host the floating-panel look at all (system chrome fully overrides) — report with screenshots; panel migration is an operator decision.
- Deleting `SurfaceCard.swift` breaks a reference outside this plan's scope.

## Maintenance notes

- The popover and the window's Overview pane render the same `overviewRows` — one row component, two hosts. Reviewers should reject a second row-shaping path.
- Footer version + consolidated menu pill: deferred until a Rust build-info/version FFI export exists (record as a future 008-shaped extension).
- After this plan, S1–S6 are fully implemented; Plan 012 runs the parity acceptance and closes the roadmap items.
