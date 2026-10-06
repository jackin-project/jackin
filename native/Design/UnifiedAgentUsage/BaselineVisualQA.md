# Baseline Visual QA — Unified Agent Usage

Status: FAILED LEGACY BASELINE. This is evidence about the incumbent source, not
an approved visual baseline or evidence for the successor fixture tuples.

Run: 2026-08-20 against source commit `25844091bd70933df134d9daa5af68b600e3d925`.

## Environment and permissions

- macOS 26.5.2 (`25F84`), macOS SDK 26.5, Xcode 26.6 (`17F113`).
- Interactive graphical session: present.
- Display: built-in Liquid Retina XDR, 3456 × 2234 Retina, 2× backing scale.
- Screen Recording: held, proven by successful window-ID captures.
- Accessibility: held; System Events reported UI elements enabled.
- Automation for system-setting changes: held, proven by successful accessibility-setting captures and restore.
- XCTest UI automation: unavailable during this run; the runner failed before tests with `Timed out while enabling automation mode`.
- Original accessibility-display defaults were absent and were restored. A post-run read confirmed Increase Contrast, Reduce Transparency, Reduce Motion, and Differentiate Without Color were all absent.

## Build and launch proof

- `cargo xtask desktop build --version 0.6.0 --build 1`: passed.
- `cargo xtask desktop verify native/dist/JackinDesktop.app`: passed for the ad hoc development artifact.
- Every accepted image was captured from the running app by resolved window ID. No detached SwiftUI snapshot or rectangle capture is accepted as evidence.
- Accepted image and executable SHA-256 values are recorded in JSON sidecars under the ignored local evidence directory `native/.build/visual-qa/baseline/`.

The retained captures use the repository-owned resolver and capture loop. This
is the exact reproducible launch → activation → window-ID resolution →
`screencapture -l` contract; `capture.sh` rejects an unresolved, non-key,
offscreen, changed, or empty window and writes the resolved metadata sidecar:

```sh
mkdir -p native/.build/visual-qa/baseline native/.build/visual-qa/tools
swiftc -O native/Scripts/VisualQA/window-id.swift \
  -o native/.build/visual-qa/tools/window-id
swiftc -O native/Scripts/VisualQA/notification-drive.swift \
  -o native/.build/visual-qa/tools/notification-drive
swiftc -O native/Scripts/VisualQA/focus-drive.swift \
  -o native/.build/visual-qa/tools/focus-drive

env WINDOW_ID_TOOL="$PWD/native/.build/visual-qa/tools/window-id" \
  NOTIFICATION_DRIVE_TOOL="$PWD/native/.build/visual-qa/tools/notification-drive" \
  FOCUS_DRIVE_TOOL="$PWD/native/.build/visual-qa/tools/focus-drive" \
  native/Scripts/VisualQA/capture.sh native/dist/JackinDesktop.app \
  'jackin❯ desktop' native/.build/visual-qa/baseline/usage-light-F02.png \
  'jackin❯ desktop' --fixture F02-catalog-normal --ui-test --open-usage \
  --window-size 920x620 --appearance light
```

Dark Usage changes the output suffix and `--appearance dark`. The popover uses
fixture `F03-multi-account`, an empty window-name argument, `--open-popover`, and
no `--window-size`. Accessibility settings use the exact guarded wrapper below;
it reads the write back, runs the same capture command, restores every original
value on exit, then an explicit before/after snapshot must have zero diff:

```sh
native/Scripts/VisualQA/state.sh snapshot \
  native/.build/visual-qa/baseline/settings-before.txt
native/Scripts/VisualQA/state.sh with reduce-transparency -- env \
  WINDOW_ID_TOOL="$PWD/native/.build/visual-qa/tools/window-id" \
  NOTIFICATION_DRIVE_TOOL="$PWD/native/.build/visual-qa/tools/notification-drive" \
  FOCUS_DRIVE_TOOL="$PWD/native/.build/visual-qa/tools/focus-drive" \
  native/Scripts/VisualQA/capture.sh native/dist/JackinDesktop.app \
  'jackin❯ desktop' \
  native/.build/visual-qa/baseline/usage-dark-reduce-transparency-F02.png \
  'jackin❯ desktop' --fixture F02-catalog-normal --ui-test --open-usage \
  --window-size 920x620 --appearance dark
native/Scripts/VisualQA/state.sh snapshot \
  native/.build/visual-qa/baseline/settings-after.txt
diff -u native/.build/visual-qa/baseline/settings-before.txt \
  native/.build/visual-qa/baseline/settings-after.txt
```

Increase Contrast substitutes `increase-contrast` and its named output. The
accepted sidecars were ignored local files, so their load-bearing values are
committed here. Every recorded file hash was independently re-read with
`shasum -a 256` and matched its sidecar; all used executable SHA-256
`409897140a0bd412204eb3ff1a120865c82a57a518e3fb971309e1e1271a1892`.

| Capture | UTC | PID / window ID | Points / pixels | Image SHA-256 |
|---|---|---|---|---|
| Light Usage F02 | `2026-08-19T18:17:04Z` | `14097 / 309181` | `920×620 / 1840×1240` | `cf5144caad1def881ed7b7d714d5caada700a21b627aef71a4254cb912fd7426` |
| Dark Usage F02 | `2026-08-19T18:16:06Z` | `12453 / 309141` | `920×620 / 1840×1240` | `f2df74d77b6c973b5fef3a968526436366b59a606dd7c4b3c07e3a77069c97da` |
| Dark popover F03 | `2026-08-19T18:16:25Z` | `12859 / 309170` | `406×546 / 812×1096` | `bb8ff6aa4974363dd04300fbbd53e7d0edb96e1010abb8c091a50bff107a6426` |
| Dark Reduce Transparency F02 | `2026-08-19T18:19:08Z` | `16201 / 309203` | `920×620 / 1840×1240` | `9feb9b148053ff3e56dc7ebcafc1166dceb942114511a6af9953cc96d56670f5` |
| Dark Increase Contrast F02 | `2026-08-19T18:19:20Z` | `16521 / 309211` | `920×620 / 1840×1240` | `a71eeb50d2c6f21cf866f73d0529c9a70fa083d697afbbb03f8dd45a4d1a198f` |

## Accepted captures

| State | Fixture | Geometry | Result |
|---|---|---:|---|
| Usage window, light | Legacy F02 | 920 × 620 | Captured; readable but structurally noisy. |
| Usage window, dark | Legacy F02 | 920 × 620 | Captured; readable but structurally noisy. |
| Focused popover, dark | Legacy F03 | 406 × 546 frame | Captured; clear provider/account focus and quota hierarchy. |
| Usage window, dark, Reduce Transparency | Legacy F02 | 920 × 620 | Captured; system material became opaque and content remained stable. |
| Usage window, dark, Increase Contrast | Legacy F02 | 920 × 620 | Captured; hard failure due to collapsed table layout. |

Two later files with spaces in their names are excluded: their malformed launch arguments produced empty fixture identifiers, wrong 760 × 500 geometry, and identical image hashes. They are not evidence for F00 or F03.

The accepted files were generated from the legacy executable fixture catalog at
the named source commit. They establish incumbent structural failures only. The
selected design must instead create the committed standalone package
`native/Design/Prototypes/UnifiedAgentUsage/`, load the canonical successor
records from `Fixtures.md`, and implement the standard five `--tr-*` launch
arguments with a `default` scenario alias. The incumbent
`VisualQAFixtures.swift` and bespoke launch flags remain legacy-only. No image
from the prototype becomes evidence until the user has walked every scenario,
both appearances, and all declared sizes live and recorded approval in
`SIGNOFF.md`; post-signoff capture belongs to `tailrocks-macos-visual-qa`.

## Findings

### Hard failure — Increase Contrast destroys table relationships

At the same 920 × 620 geometry, provider group placeholders and account values concatenate across visual columns: `— — — —`, `Plus 0% —`, `Max 20× 12% —`, and `Default — 81% —`. The table no longer communicates which account, plan, remaining value, and reset belong together. This violates the required contrast, hierarchy, and non-overlap behavior and blocks release.

The enabling structural condition is visible in `OverviewListView`: Plan, Remaining, and Reset have width contracts, while Provider and Account do not, and provider group rows emit placeholders through every account-specific column. The selected prototype must prove a hierarchy that keeps provider labels out of account-only cells and preserves identity/state widths under Increased Contrast, long labels, and minimum geometry.

### Major — overview hierarchy carries avoidable placeholder noise

Default light and dark captures remain readable, but provider group rows fill account, plan, remaining, and reset columns with em dashes. Single-provider rows such as Amp also resemble data rows with missing fields. The repeated placeholders compete with canonical account rows and make scanning harder than the provider/account model requires.

### Major — account identity wraps before less important metadata contracts

The normal personal and secondary email labels wrap at 920 points while wide plan and reset columns remain reserved, including when reset data is absent. The selected design must protect provider/account identity and explicit state before secondary plan/reset metadata.

### Passed baseline observations

- The system sidebar, titlebar, traffic lights, split behavior, and native controls read as a macOS 26 application in light and dark appearances.
- The popover has a clear provider/account heading, ordered quota windows, explicit values and reset text, and stable footer actions.
- Reduce Transparency produced an opaque native result without layout drift.
- Quota values remained textual, not color-only; no token price, spend history, trend, or launch-blocking action appeared.

## Automated accessibility result

`performAccessibilityAudit` did not execute. The UI-test launch failed while enabling automation mode, before any test case ran. The ignored result bundle is `native/.build/visual-qa/baseline/accessibility.xcresult`. This is a recorded blocker, not a pass. The implementation plan must repair deterministic UI-test lifecycle and run audits for status item, popover, Usage overview/detail, Settings, every unavailable state, and Increased Contrast.

A second focused retry on 2026-08-20 used:

```sh
rtk proxy xcodebuild test \
  -project native/JackinDesktop.xcodeproj \
  -scheme JackinDesktop \
  -destination 'platform=macOS' \
  -parallel-testing-enabled NO \
  -only-testing:JackinDesktopUITests/JackinDesktopUITests/testOverviewPassesAccessibilityAudit \
  -derivedDataPath native/DerivedData \
  -resultBundlePath native/.build/test-results/goal-accessibility-overview.xcresult
```

The build and selected test launch began, but the one test failed after 61.977
seconds: XCUITest could not activate `com.jackin-project.desktop` because its
state remained `Running Background`. `performAccessibilityAudit` again did not
execute. This isolates the current failure to app activation/lifecycle rather
than missing test discovery. The result remains ignored local evidence; no
accessibility pass is claimed. Repairing the harness or app lifecycle is later
implementation work and is forbidden in this read-only planning-preparation
phase.

## Remaining settings retry and restoration proof

Reduce Motion and Differentiate Without Color were each applied on 2026-08-20
with the same verified wrapper. These were the exact capture invocations after
the tools were compiled as above:

```sh
native/Scripts/VisualQA/state.sh snapshot \
  native/.build/visual-qa/baseline/settings-before-extra-states.txt
env WINDOW_ID_TOOL="$PWD/native/.build/visual-qa/tools/window-id" \
  NOTIFICATION_DRIVE_TOOL="$PWD/native/.build/visual-qa/tools/notification-drive" \
  FOCUS_DRIVE_TOOL="$PWD/native/.build/visual-qa/tools/focus-drive" \
  CAPTURE_SETTLE_DELAY_SECONDS=5 \
  native/Scripts/VisualQA/state.sh with reduce-motion -- \
  native/Scripts/VisualQA/capture.sh native/dist/JackinDesktop.app \
  'jackin❯ desktop' \
  native/.build/visual-qa/baseline/usage-dark-reduce-motion-F02.png \
  'jackin❯ desktop' --fixture F02-catalog-normal --ui-test --open-usage \
  --window-size 920x620 --appearance dark

env WINDOW_ID_TOOL="$PWD/native/.build/visual-qa/tools/window-id" \
  NOTIFICATION_DRIVE_TOOL="$PWD/native/.build/visual-qa/tools/notification-drive" \
  FOCUS_DRIVE_TOOL="$PWD/native/.build/visual-qa/tools/focus-drive" \
  CAPTURE_SETTLE_DELAY_SECONDS=5 \
  native/Scripts/VisualQA/state.sh with differentiate-without-color -- \
  native/Scripts/VisualQA/capture.sh native/dist/JackinDesktop.app \
  'jackin❯ desktop' \
  native/.build/visual-qa/baseline/popover-dark-differentiate-without-color-F02.png \
  '' --fixture F02-catalog-normal --ui-test --open-popover --appearance dark
```

Both setting writes passed read-back verification. Both capture commands failed
after 60 activation attempts with `application did not reach requested active
state`; no image or sidecar was accepted. During the Reduce Motion failure,
System Events returned `false, 0, AXApplication, missing value` for frontmost,
window count, role, and subrole, and the window resolver returned `no window
found for owner jackin❯ desktop`. The Differentiate Without Color retry failed
the same activation gate even when requesting the popover. This agrees with the
XCUITest `Running Background` failure and precisely blocks window-ID capture:
the process remains alive but exposes no window to resolve.

```sh
osascript -e \
  'tell application "System Events" to tell application process "JackinDesktop" to get {frontmost, count windows, role, subrole}'
native/.build/visual-qa/tools/window-id 'jackin❯ desktop' \
  'jackin❯ desktop' --json --pid 91936
```

The wrapper printed restoration for every governed key after each failure.
After the respective runs, both exact comparisons exited zero with no diff:

```sh
native/Scripts/VisualQA/state.sh snapshot \
  native/.build/visual-qa/baseline/settings-after-reduce-motion.txt
diff -u native/.build/visual-qa/baseline/settings-before-extra-states.txt \
  native/.build/visual-qa/baseline/settings-after-reduce-motion.txt

native/Scripts/VisualQA/state.sh snapshot \
  native/.build/visual-qa/baseline/settings-after-differentiate-without-color.txt
diff -u native/.build/visual-qa/baseline/settings-before-extra-states.txt \
  native/.build/visual-qa/baseline/settings-after-differentiate-without-color.txt
```

The toggles and restoration are proven; static visual behavior under those two
settings remains unproven. Repairing the activation lifecycle is later
implementation work and remains forbidden in this planning-preparation phase.

## Missing baseline states

The following remain required before final approval:

- F00, F01, and F04–F24 with valid fixture identity and requested geometry or
  task-completion evidence as defined by each fixture.
- Light/dark inactive window and key-window transitions.
- Accepted Reduce Motion and Differentiate Without Color captures after the
  recorded activation-lifecycle failure is repaired.
- Full Keyboard Access and VoiceOver traversal, announcements, order, labels, values, and actions.
- Clear and tinted Liquid Glass appearance, accent colors, icon sizes, scrollbar policies, display scaling, external-display movement, varied wallpaper, and color profiles.
- Minimum 760 × 500 and wide 1200 × 760 evidence with long, right-to-left, CJK, German, and 40-account fixtures.
- Driven status-item-to-popover-to-Usage handoff and retained selection/window restoration.
- Signed, notarized, stapled, quarantined public artifact and Homebrew-installed artifact launch.

## Baseline verdict

FAIL. The incumbent native structure is a credible starting point, but the Increased Contrast table collapse is a hard failure and automated accessibility evidence is absent. No visual direction may be approved until a selected structural prototype removes the failure and passes the complete matrix.
