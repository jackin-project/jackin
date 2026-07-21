# Plan 010: The Usage window — Liquid Glass sidebar/content restating the Capsule dialog (screens S3 + S4)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. Swift is display-only: the window renders DTO strings verbatim — any temptation to compute, reword, reorder, or "improve" a Rust-provided label is a STOP. The Capsule reference screens quoted below are the acceptance contract: same fields, same strings, same order. When done, update this plan's row in `plans/native-macos-usage-menu-bar/README.md`.
>
> **Drift check (run first)**: `git diff --stat be6fb79e..HEAD -- native crates/jackin-usage-ffi`
> Expected drift: Plans 007–009. Confirm: `native/Sources/JackinDesktop/` exists; `rg -n "overviewRows|estimateCaption" native/Sources/JackinUsageBridge/jackin_usage_ffi.swift` matches. Missing → STOP (dependency not met).

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED (largest new Swift surface; Liquid Glass gates; `LSUIElement` window management)
- **Depends on**: Plan 007 (identity), Plan 008 (FFI: overview rows, estimate caption, format prefs). Plan 009 is not a hard dependency but lands first in program order (shared `PresentationStore` edits — coordinate if run in parallel).
- **Category**: direction
- **Planned at**: commit `be6fb79e`, 2026-07-22

## Why this matters

Roadmap S3/S4: detail moves out of the popover into a **normal macOS window** that restates the Capsule usage dialog natively — glass sidebar (Overview + providers in Capsule tab order), content pane rendering the full provider card field-for-field, honest degradation states, Liquid Glass with centralized fallbacks. This is the heart of jackin❯ Desktop v1: "exactly what the Capsule usage dialog shows in-session, always visible on the host without an open Capsule." Capsule parity is invariant 1: if the window and the Capsule ever disagree on a number, that is a bug by definition.

## Current state

- Scenes (`native/Sources/JackinDesktop/JackinDesktopApp.swift`): hidden keepalive `Window` (id `"keepalive"`), `MenuBarExtra(.window)`, `Settings`. **No detail window exists.**
- `native/Sources/JackinUsageBridge/PresentationStore.swift` — projection is intentionally narrow today and must widen:

  ```swift
  // PresentationStore.swift:9-30 — SurfaceRow/BucketRow (pre-widening)
  public struct SurfaceRow: … { id, label, enabled, statusBarLabel, status,
      accountLabel, planLabel, buckets, updatedLabel, lastError }
  public struct BucketRow: … { label, usedLabel, remainingPercent, resetLabel,
      severity, status }
  ```

  `applySnapshots()` (`PresentationStore.swift:197-267`) maps `UsageViewDto` → `SurfaceRow` and currently **drops**: `username`, `credentialOrigin`, `limitLabel`, `paceLabel`, `usedMoney`/`limitMoney`, `statusSlot`, `resetsAt`, `focusedProvider`, `source`, `confidence`, and Plan 008's `estimateCaption`. All exist on the DTO (`crates/jackin-usage-ffi/src/dto.rs:42-75`) — widening is projection-only, no FFI change.
- `native/Sources/JackinDesktop/SurfaceCard.swift` — existing popover card: header (label + `statusBadgeSymbol`), account/plan line, `BucketGaugeRow` (`Gauge(value: 100 - remainingPercent, in: 0...100).gaugeStyle(.accessoryLinearCapacity).tint(severityTint(...))`, drawn **only** when `remainingPercent != nil` — "Never invent %"), optional last-error line, dim 0.85 on stale/unavailable. Reuse its row anatomy; the window card is a superset.
- `native/Sources/JackinUsageBridge/PresentationHelpers.swift` — `severityTint(_:)` (danger→red, warn→orange, else accent) and `statusBadgeSymbol(_:)` (error/needs-login/needs-secret/unavailable→`exclamationmark.triangle`, stale→`clock`, fresh→nil). The shipped mappings; do not fork.
- `native/Sources/JackinDesktop/GlassFallbacks.swift` — the **only** file allowed `#available(macOS 26, *)` (ArchitectureTests-enforced). Has `chromeBackground(content:)`, `footerBarBackground()`, `statusChipBackground(tint:)`. Window chrome gates (sidebar/toolbar glass) are added here.
- Provider display labels + row content: Plan 008's `overviewRows()` (display_label via the shared remap — OpenAI/Anthropic/xAI/Z.AI) and `snapshot(surfaceId:)` for card content. Sidebar order = `listSurfaces()` order (= `HostSurfaceId::ALL` = Capsule tab order) with "Overview" first.
- Acceptance contract — Capsule reference screens (roadmap, captured 2026-07-22). Provider card field set and layout conventions the window must restate:

  ```text
  │  OpenAI                                        operator@example.com  │
  │  Updated 2m ago                                             Pro 20x  │
  │  Auth: OAuth · ~/.codex/auth.json                                    │
  │──────────────────────────────────────────────────────────────────────│
  │  Session                                                             │
  │  ███████████████…                                                    │
  │  97% left                          Resets in 6d 22h (Jul 28, 17:02)  │
  │  2% in deficit                                                       │
  │  Limit Reset Credits                      3 manual resets available  │
  ```

  Conventions (full list in the roadmap "Conventions jackin❯ Desktop mirrors" section): two-column rows (value left, meta right: provider↔account, updated↔plan, percent↔reset, pace↔prognosis); auth-line variants (`OAuth · ~/.codex/auth.json`, `API key · amp secrets.json`, `API token · env ZAI_API_KEY`); key-only surfaces carry no account line; named/model-scoped windows (`Codex Spark 5-hour`, `All models`, `Fable`) are ordinary buckets; value rows without gauges (`Individual credits: $0.06`); count detail lines (`133 / 4.0K (3.9K remaining)`); money honesty (`100% used` + `Monthly cap: SGD 269.47 spent / SGD 260.00`); reset-credits display-only row; overview rows led by most-constrained fresh bucket with bucket label shown when not the default window (`Anthropic  Fable 68% left`).
- S4 states (roadmap sketch): stale = clock badge + dimmed card + last-good gauges visible; needs secret = status word only, no invented gauge; error = last-good buckets stay + error text in place; over-cap money = `100% used` + cap line; value row = no gauge. All statuses arrive as DTO strings (`fresh/stale/needs_login/needs_secret/unsupported/unavailable/error`).
- Design contract ("Native design reference" roadmap section): inset grouped cards on the panel background; provider section headers (name + muted plan badge) above the card; metric row = label above thin full-width capacity bar, caption below with value left / meta right; fills tint by Rust severity; honest empty rows (em dash + `No data`); typography — system font, bold section titles, muted secondary meta, generous spacing; sidebar rows carry a severity accent (worst-bucket severity per surface — from `overviewRows().severity`).
- S3 window behavior: resizable, remembered frame, ⌘R refresh, Esc closes, toolbar Refresh + Settings, app stays `LSUIElement` (no Dock presence when window open — `LSUIElement` grants this; do not fight activation policy).

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Swift build | `cd native && swift build -c release` | exit 0 |
| Swift tests | `cd native && swift test -c release` | all pass |
| App fixture | `JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/build-usage-menu-bar-app.sh && open native/dist/JackinDesktop.app` | window opens from popover |
| Glass-gate guard | `rg -n "#available\(macOS 26" native/Sources | grep -v GlassFallbacks.swift` | no matches |
| Rust guard (unchanged) | `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` | all pass, no edits |
| Merge readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope:** `native/Sources/JackinDesktop/JackinDesktopApp.swift` (new window scene), new files `native/Sources/JackinDesktop/UsageWindow/…` (e.g. `UsageWindowRoot.swift`, `ProviderCardView.swift`, `OverviewListView.swift`), `native/Sources/JackinDesktop/GlassFallbacks.swift` (new gates only), `native/Sources/JackinDesktop/PopoverRoot.swift` (**one addition only**: an "Open Usage…" footer action opening the window — full popover redesign is Plan 011), `native/Sources/JackinUsageBridge/PresentationStore.swift` (widen `SurfaceRow`/`BucketRow` projection; add `overviewRows` projection), `native/Sources/JackinUsageBridge/PresentationHelpers.swift` (pure mapping additions), `native/Package.swift` (only if new source dirs need target listing — normally not), `native/Tests/**`, operator guide (Usage-window section), roadmap checklist tick, plan/README status rows.

**Out of scope (do NOT touch):**

- Any Rust crate; any FFI change. A missing field means STOP, not a Swift workaround.
- `StatusItemLabel.swift`, `SettingsView.swift` (Plan 009), popover redesign beyond the single Open Usage action (Plan 011).
- New fields the Rust view-model does not carry (roadmap: "it adds no field the Rust view-model does not already carry") — no charts, no multi-account, no external links, no write actions (Buy Credits, claim reset — rejected/deferred).
- `MenuBarExtra` → AppKit migration (standing STOP from Plan 006 carries over).

## Git workflow

- Active feature branch; from `main` propose `feature/desktop-usage-window` and wait for confirmation.
- Signed Conventional Commits, push after every commit. Suggested: `feat(desktop): Liquid Glass usage window restating the Capsule dialog`.

## Steps

### Step 1: Widen the PresentationStore projection

Extend `SurfaceRow` with `username: String?`, `credentialOrigin: String?`, `estimateCaption: String?`; extend `BucketRow` with `limitLabel: String?`, `paceLabel: String?`, `statusSlot: String?`; map them in `applySnapshots()` (fields already on the DTOs — pure plumbing; money crosses pre-formatted inside `usedLabel`/`limitLabel`, so `MoneyDto` needs no projection: confirm the fixture shows `Monthly cap: SGD …` arriving in `limitLabel`; if money arrives only structured, STOP — label composition belongs in Rust/Plan 008, not here). Add `@Published overviewRows` populated from `bridge.overviewRows()` in `applySnapshots()`.

**Verify**: `cd native && swift build -c release` → exit 0; existing tests pass.

### Step 2: Window scene + navigation shell

Add a `Window("jackin❯ Desktop — Usage", id: "usage")` scene (single instance, `openWindow(id:)`-driven) in `JackinDesktopApp.swift`. Content: `NavigationSplitView` — sidebar list: "Overview" + one row per `overviewRows` entry (display label + severity accent dot via `severityTint`), selection state `String?` (surface id or nil = Overview); detail: `OverviewListView` (full-width S2-style rows from `overviewRows` — reuse one row component) or `ProviderCardView(surface:)`. Window: resizable, remembered frame (SwiftUI persists by window id — verify; if not, `.defaultSize` + document actual behavior), toolbar with Refresh (⌘R → `store.refreshAll()`) and Settings buttons, Esc closes (`.keyboardShortcut(.cancelAction)` on a hidden close control or `onExitCommand`), ←/→ or ⇥ moves sidebar selection, `r` refreshes (keyboard parity list, roadmap). Wire popover footer "Open Usage…" (`openWindow(id: "usage")`); clicking an overview row selects that provider. App stays `LSUIElement`.

**Verify**: build + launch; Open Usage opens one window (second invocation focuses, not duplicates); Esc closes; ⌘R refreshes; no Dock icon appears; frame persists across close/reopen (record observed behavior).

### Step 3: Provider card — full Capsule field set

`ProviderCardView` renders, top to bottom, from the widened `SurfaceRow` (all strings verbatim):

1. Identity block: bold provider display label left ↔ `accountLabel` right (+ ` (username)` when present — as separate Text runs, no reformatting); muted `updatedLabel` left ↔ `planLabel` badge right; `Auth: {credentialOrigin}` line when present. Key-only surfaces (no account) simply omit the account run — honest without one.
2. One metric row per bucket (inset grouped card): bucket `label`; thin capacity bar (`Gauge` `.accessoryLinearCapacity`, value `100 - remainingPercent`, tint `severityTint(severity)`) **only when** `remainingPercent != nil`; caption line `usedLabel` left ↔ `resetLabel` right; `paceLabel` left ↔ prognosis right when present (pace strings arrive whole — e.g. `62% in reserve`; render as-is); `limitLabel` line when present (`Monthly cap: …`, `1 week window`, count details); value rows (no percent, has `usedLabel`) render label ↔ value with no bar; rows with neither render em dash + `No data` over an empty track (design contract).
3. `estimateCaption` muted under money rows when present.
4. Status: `statusBadgeSymbol` badge; stale/unavailable dim the card (reuse SurfaceCard's 0.85 pattern); `lastError` rendered in place under the buckets — last-good buckets stay visible (S4).

**Verify**: build + run against live credentials on the dev machine; for each available provider compare the card against `jackin usage` / the Capsule dialog: identical strings for percent, reset, pace, money, auth, plan (spot-check at minimum one OAuth surface, one key-only surface, one money window). Record the comparison in the PR. VoiceOver: each metric row reads as one element.

### Step 4: Liquid Glass chrome, centralized

Add to `GlassFallbacks.swift` (only file with `#available(macOS 26, *)`): sidebar/list glass background helper and toolbar treatment for the window — macOS 26: glass effect, content edge-to-edge under chrome, concentric corner geometry; 14/15: standard `.sidebar` material / plain toolbar. Inset grouped cards use standard grouped-background styling (theme-safe, not glass — glass is chrome only, per Plan 006's rule). Never fork styling per provider view.

**Verify**: `rg -n "#available\(macOS 26" native/Sources | grep -v GlassFallbacks.swift` → no matches; visual pass on macOS 26 (glass) and with Reduce Transparency (fallback) — record results.

### Step 5: Guards + docs

Extend `ArchitectureTests.swift`: include `UsageWindow/` sources in every existing scan (probe imports, availability gates, no percent composition); assert no string literal of a provider display name (`"OpenAI"`, `"Anthropic"`, `"xAI"`, `"Z.AI"`) exists in `Sources/JackinDesktop` (labels must come from Rust — extend the existing provider-name scan). Operator guide: Usage-window section (open, navigate, refresh, close — behavior only). Tick the roadmap checklist item.

**Verify**: `cd native && swift test -c release` → all pass; `cargo xtask docs repo-links && cargo xtask roadmap audit` → exit 0; `cargo xtask ci --fast` → exit 0.

## Test plan

- Swift: ArchitectureTests extension (Step 5); unit tests for any extracted pure mapping (e.g. bucket-row shape selection: gauge / value-row / empty-row given `remainingPercent`/`usedLabel` presence — extract it as a testable function).
- Manual matrix (record in PR): every S4 state — force stale (revoke network), needs-secret (unset env key), error, money over-cap (fixture or live), value row (Amp credits); light/dark; macOS 26 + Reduce Transparency; keyboard parity (←/→, ⇥, r, Esc, ⌘R); window uniqueness + frame persistence; VoiceOver.
- Rust: none (nextest run proves untouched).

## Done criteria

- [ ] Usage window opens from the popover; sidebar = Overview + providers in Capsule order with severity accents; content = full provider card, field-for-field vs the Capsule reference screens.
- [ ] All S4 degradation states render honestly (no invented gauges, last-good visible under errors, dimming never hides data).
- [ ] Glass gates only in `GlassFallbacks.swift`; renders on macOS 14+ and adopts glass on 26.
- [ ] Live spot-check recorded: Desktop strings == Capsule strings for ≥3 providers.
- [ ] ArchitectureTests extended and green; docs audits + `cargo xtask ci --fast` exit 0; guide + roadmap updated.

## STOP conditions

- Dependencies absent (drift check) — 008's exports or 007's rename missing.
- A Capsule field does not arrive over FFI as a finished string (e.g. money cap line only structured, username needs composition) — name the missing/insufficient export; the fix is Plan-008-shaped Rust work.
- The single-instance window cannot be achieved with SwiftUI `Window` + `openWindow` on macOS 14 (duplicate windows or focus failures) — report repro; AppKit window management is an operator decision.
- Rendering parity would require reordering/rewording any Rust string.
- `LSUIElement` + window causes activation problems (window won't key) — report exact behavior; do not flip activation policy silently.

## Maintenance notes

- The window is the template for future detail surfaces; reviewers should reject any per-provider Swift branch (`if provider == …`) — new agents must appear via Rust surface growth with zero Swift edits.
- Plan 011 reduces the popover to the overview strip and routes row-clicks here (`openWindow` + sidebar selection) — keep the selection state settable from outside the window scene.
- Deferred with their features (do not scaffold speculatively): charts/history (needs durable snapshot store, ADR-011), multi-account switcher, external links, drill-in subpages, track marks.
- Reviewer focus: projection widening completeness vs DTO; the "no invented rows/gauges" branches; glass-gate centralization.
