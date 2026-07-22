# Plan 009: Status-item display modes and the v1 Settings window (screens S1 + S6)

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving on. Swift is display-only: every string you render must arrive over FFI from Plan 008's exports — if a step seems to need Swift-side percentage arithmetic, label composition, or provider mapping, that is a STOP condition. When done, update this plan's row in `plans/native-macos-usage-menu-bar/README.md`.
>
> **Drift check (run first)**: `git diff --stat be6fb79e..HEAD -- native crates/jackin-usage-ffi`
> Expected drift: Plans 007 (rename) and 008 (FFI exports). Confirm both landed: `native/Sources/JackinDesktop/` exists and `rg -n "compactStatusBarStrip|setFormatPrefs|overviewRows" native/Sources/JackinUsageBridge/jackin_usage_ffi.swift` matches. Missing either → STOP (dependency not met).

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW–MED (display-only Swift; one OS-integration feature)
- **Depends on**: Plan 007 (renamed sources, logomark icon), Plan 008 (FFI exports)
- **Category**: direction
- **Planned at**: commit `be6fb79e`, 2026-07-22

## Why this matters

Roadmap S1/S6 (spec: `docs/content/docs/roadmap/(operator-surface)/native-macos-usage-menu-bar.mdx`, "Status bar surface" + "Screen inventory"): the status item gains Settings-selectable display modes — Icon only, Focus percent (shipped default), Pinned surface, Multi-surface strip — plus depleted-shows-reset, used↔left and countdown↔clock format flips, and an optional screen-share privacy collapse. Settings grows the matching pickers (S6 sketch). Every label is a Rust string from Plan 008; Swift only persists preferences and picks which Rust string to show.

## Current state

(Line numbers are pre-007 positions in the renamed files; after the rename the content is identical under `native/Sources/JackinDesktop/`.)

- `native/Sources/JackinDesktop/StatusItemLabel.swift` — logomark image (Plan 007) + one optional text driven by `store.showPercentInMenuBar` and `store.compactBarLabel`; dims to 0.45 opacity via `store.allEnabledSurfacesDegraded`. This is the only file that renders the status item.
- `native/Sources/JackinUsageBridge/PresentationStore.swift` — `@MainActor ObservableObject`; `compactBarLabel` refreshed in `applySnapshots()` from `bridge.compactStatusBarLabel()`; `showPercentInMenuBar` persisted in UserDefaults key `"jackin.desktop.showPercent"` (post-007; pre-007 it was `"jackin.usageMenuBar.showPercent"`, `PresentationStore.swift:43-47`); 5 s poll loop `pollOnce()` gated by `bridge.refreshDue()`.
- `native/Sources/JackinDesktop/SettingsView.swift` — grouped `Form`, sections today: Menu bar (show-percent toggle), Login (launch-at-login via `SMAppService.mainApp`, truth read from `.status`), Surfaces (per-surface toggles via `store.setEnabled`), Refresh (floor slider 1–30 min → `store.setRefreshFloorSecs`), About (three captions). Fixed frame 420×520.
- Plan 008 FFI surface available on `UsageMenuBarBridge`: `compactStatusBarLabel()` (focus), `compactStatusBarLabelFor(surfaceId:)` (pinned), `compactStatusBarStrip(max:)`, `setFormatPrefs(prefs:)` (`UsageFormatPrefsDto(percentStyle:resetStyle:)` — strings `"left"|"used"`, `"countdown"|"exact_clock"`), `listSurfaces()` → `[SurfaceDescriptorDto]` (id + label for the pinned-surface picker), `overviewRows()`, `nextRefreshLabel()`.
- Depleted-shows-reset is **inside** the Rust labels (Plan 008 Step 2): when active, the compact label itself reads `Cl resets 1h 21m`. Swift needs no depleted logic; the S6 checkbox "Show reset when depleted" — confirm against Plan 008's landed API: if 008 shipped the depleted branch unconditionally (its default), the checkbox is not a Rust flag and must NOT be faked in Swift by string-parsing. In that case render the S6 sketch without that checkbox and record the divergence in the PR + roadmap note, or add the flag to `UsageFormatPrefs` via a small 008-shaped follow-up commit (Rust + FFI + golden) — prefer the flag; do not parse.
- `native/Tests/JackinUsageBridgeTests/ArchitectureTests.swift` — static source-scan guards: no `URLSession`/`Process(`/`SecItem`, no provider names, `#available(macOS 26` only in `GlassFallbacks.swift`, no computed-percent `Text` interpolation. Extend, never weaken.
- S1 target (roadmap sketch):

  ```text
  j❯ Cl 63%                      Focus percent (default)
  j❯                             Icon only
  j❯ Cx 41%                      Pinned surface
  j❯ Cl 63% · Cx 41% · ZA 12%    Multi-surface strip (cap 3, worst first)
  j❯ Cl resets 1h 21m            Depleted
  j❯  (0.45 opacity)             Every enabled surface degraded
  j❯                             Screen capture active (privacy collapse)
  ```

- S6 target (roadmap sketch): Menu bar section — Display radio (Focus percent / Icon only / Pinned surface + picker / Strip + cap picker), depleted checkbox, Format radios (% left ↔ % used; Countdown ↔ Exact time), screen-share hide toggle; then the shipped Surfaces / Refresh / Login / About sections unchanged.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Swift build | `cd native && swift build -c release` | exit 0 |
| Swift tests | `cd native && swift test -c release` | all pass |
| App fixture | `JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1 ./scripts/build-usage-menu-bar-app.sh && open native/dist/JackinDesktop.app` | launches; modes switchable live |
| Rust guard (unchanged) | `cargo nextest run -p jackin-usage -p jackin-usage-ffi --locked` | all pass, no edits |
| Merge readiness | `cargo xtask ci --fast` | exit 0 |

## Scope

**In scope:** `native/Sources/JackinDesktop/{StatusItemLabel.swift, SettingsView.swift}`, `native/Sources/JackinUsageBridge/PresentationStore.swift` (preference storage + projection of the new FFI accessors), `native/Tests/JackinUsageBridgeTests/**`, operator guide `docs/content/docs/(public)/guides/macos-usage-menu-bar.mdx` (Settings/display-modes section — operator-visible behavior only), roadmap checklist ticks, plan/README status rows.

**Out of scope (do NOT touch):**

- Any Rust crate. If a needed string/flag is missing, that is a Plan 008-shaped change (STOP and report, or land it as a separate Rust commit following Plan 008's step pattern — never improvise in Swift).
- `PopoverRoot.swift` / `SurfaceCard.swift` (Plan 011), window scenes (Plan 010), `GlassFallbacks.swift` (unless a Settings control needs an existing helper — read-only use is fine).
- Packaging/scripts/CI.
- Alerts, notifications, write actions of any kind — v1 is view-only (roadmap "Rejected"/"Deferred" lists).

## Git workflow

- Active feature branch; from `main` propose `feature/desktop-status-item-modes` and wait for confirmation.
- Signed Conventional Commits, push after every commit. Suggested: `feat(desktop): status-item display modes + settings pickers`.

## Steps

### Step 1: Preference model in PresentationStore

Add a display-mode preference (UserDefaults-backed, keys under `jackin.desktop.*`): `displayMode` (`iconOnly | focusPercent | pinnedSurface | strip`; default `focusPercent`), `pinnedSurfaceId: String?`, `stripMax: Int` (default 3, range 1–8 mirroring Rust clamp), `percentStyle` (`left|used`), `resetStyle` (`countdown|exact_clock`), `hideWhileScreenSharing: Bool` (default false). Replace the boolean `showPercentInMenuBar` with `displayMode` (map old semantics: percent-off ≙ `iconOnly`; delete the old key — pre-release, no shim). On init and on change, push format prefs across FFI: `try bridge.setFormatPrefs(UsageFormatPrefsDto(percentStyle:…, resetStyle:…))` — Rust resolves the strings; Swift never rewrites a label. In `applySnapshots()` populate a single published `statusItemText: String` chosen by mode: focus → `compactStatusBarLabel()`, pinned → `compactStatusBarLabelFor(surfaceId:)` (empty/nil → icon-only fallback rendering, honest), strip → `compactStatusBarStrip(max:)`, iconOnly → `""`.

**Verify**: `cd native && swift build -c release` → exit 0.

### Step 2: StatusItemLabel renders modes

`StatusItemLabel.swift` shows the logomark plus `Text(store.statusItemText)` when non-empty, `.monospacedDigit()`, dimming behavior unchanged. Accessibility label: `"jackin Desktop \(statusItemText)"` / `"jackin Desktop"`. No mode logic here beyond "text empty or not" — selection happened in the store.

**Verify**: build + launch fixture; flip all four modes in Settings without relaunch; strip shows worst-first ` · `-separated entries; pinned tracks only the chosen surface; digits don't shift width.

### Step 3: Screen-share privacy collapse (optional feature, off by default)

Pure Swift OS integration (the one class Swift may own — roadmap "Screen-share privacy"): when `hideWhileScreenSharing` is on and macOS reports active capture, `statusItemText` renders as empty (bare icon). Detection: poll `CGSessionCopyCurrentDictionary()` for `kCGSessionScreenIsShared` (a.k.a. `"CGSSessionScreenIsShared"`) inside the existing 5 s `pollOnce()` tick — no new timers, no new frameworks, no ScreenCaptureKit dependency. Gate the check behind the preference so the default path does zero extra work.

**Verify**: with the toggle on, start a screen share (or QuickTime screen recording) → item collapses to icon within one poll tick; stop → text returns. Record the manual result. If `CGSessionCopyCurrentDictionary` does not reflect capture on the tested macOS version, STOP (see conditions) — do not substitute a ScreenCaptureKit probe on your own authority.

### Step 4: Settings — S6 layout

Rebuild the "Menu bar" section of `SettingsView.swift` per the S6 sketch: Display `Picker` (radio style) with the four modes; pinned-surface `Picker` fed by `store.surfaces` labels (ids from `SurfaceDescriptorDto`); strip cap `Picker` (1…8, default 3); "Show reset when depleted" checkbox **only if** the Rust flag exists (see Current state — otherwise omit + record); Format radios (`% left`/`% used`, `Countdown`/`Exact time`); "Hide values while screen sharing" toggle. Keep Surfaces/Refresh/Login/About sections as shipped; About copy per S5/S6: credential-reading explanation stays. Adjust the fixed frame height if the form no longer fits (e.g. 420×640); `.formStyle(.grouped)` stays.

**Verify**: build + launch; every picker persists across relaunch (UserDefaults); flipping `% used` changes gauge captions app-wide on next snapshot application (Rust-resolved — confirm a label literally changes from `97% left` to `3% used` without any Swift string manipulation in the diff).

### Step 5: Guard tests + docs

Extend `ArchitectureTests.swift`: assert no Swift source contains `"% left"` / `"% used"` / `"resets "` string literals or `String(format:` percent composition in `Sources/JackinDesktop` (heuristic in the existing source-scan style — labels must arrive via the store); assert `CGSessionCopyCurrentDictionary` appears only in `PresentationStore.swift`. Add a pure-function unit test for the mode→accessor selection if you extracted it as a testable function (do extract it). Update the operator guide's Settings section (behavior only: modes, format flips, privacy toggle) and tick the roadmap checklist items for status-item modes and S6 pickers.

**Verify**: `cd native && swift test -c release` → all pass; `cargo xtask docs repo-links && cargo xtask roadmap audit` → exit 0; `cargo xtask ci --fast` → exit 0.

## Test plan

- Swift: extended ArchitectureTests (Step 5); unit test for mode-selection function (icon/focus/pinned/strip × empty/non-empty label).
- Rust: none (no Rust edits; nextest run proves it).
- Manual matrix (record in PR): all four modes live-switch; depleted rendering (force with a 0%-remaining fixture account if available, else note); light/dark template tinting; VoiceOver reads mode text; screen-share collapse on/off; relaunch persistence.

## Done criteria

- [ ] All S1 variants renderable by switching Settings only — no relaunch, no Swift-composed strings (diff contains no percent/reset literal composition).
- [ ] Format flips visibly change Rust-delivered labels app-wide.
- [ ] Screen-share collapse works or is STOP-reported with repro.
- [ ] ArchitectureTests extended and green; `swift build`/`swift test -c release` green; `cargo xtask ci --fast` exit 0.
- [ ] Operator guide + roadmap checklist updated; status rows updated.

## STOP conditions

- Plan 007 or 008 not landed (drift check).
- A needed label/flag is missing from the FFI surface (e.g. depleted toggle) and you are tempted to derive it in Swift — report the exact missing export instead.
- `CGSessionCopyCurrentDictionary` proves unreliable for capture detection on macOS 14/26 testing — report observed behavior; the alternative (ScreenCaptureKit / `SCShareableContent`) is an operator decision (new framework dependency).
- Settings form cannot express the S6 layout without abandoning `.formStyle(.grouped)` (design-contract conflict — report with screenshot).

## Maintenance notes

- The mode enum is the extension point for future status-item ideas (per-surface multi-item was explicitly rejected — reviewers should reject any second `MenuBarExtra`).
- If Rust later adds surfaces, the pinned picker auto-grows via `listSurfaces()` — no Swift enum of providers may ever appear (ArchitectureTests provider-name scan enforces).
- Reviewer focus: the diff must show preference plumbing + accessor selection only; any string manipulation of Rust labels is a defect.
