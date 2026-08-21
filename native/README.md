# jackin❯ desktop

Native macOS limits display over `jackin-usage-ffi` (boltffi). Product identity is **jackin❯ desktop** (`JackinDesktop.app`, bundle id `com.jackin-project.desktop`). Rust owns probes, provider ordering, accounts, quota semantics, refresh policy, severity, and every domain string. Swift owns AppKit/SwiftUI presentation and OS integration.

Production Swift passes no config, home, or data paths. Rust derives canonical host
paths, loads the global/workspace/role configuration read-only, resolves configured
credential sources, deduplicates accounts, and exports only immutable sanitized
inventory/diagnostic DTOs. Swift never scans configuration or handles credentials.
One host Rust broker owns canonical refresh generations, provider calls, atomic
last-good state, and shared rate-limit deadlines. Desktop sends refresh intent and
renders the returned phase; it never starts a local probe or uses Swift task
cancellation as coordination.

One `desktopProjection` call returns the complete generation: provider groups,
account children, selected identities, quota/detail rows, status-item rows, activity,
and sanitized diagnostics. `PresentationStore` replaces visible state only after that
whole projection decodes. A transient failure preserves the exact last-good rows and
destination; an older generation can never overwrite a newer one.

Product scope is limits only: remaining/used percentages, resets, plan/status, multi-account selection, and provider-supplied quota caps. Never add token unit prices, session-cost estimates, historical spend/usage, trends, sparklines, or aggregate charts.

## Shipping baseline

- Deployment target and release floor: **macOS 26.0**.
- Shipping lane: **Xcode 26.6, macOS 26.5 SDK, Swift 6.3** on GitHub's `macos-26` image, Swift 6 language mode with complete strict concurrency and warnings as errors.
- Forward-validation lane: **Xcode 27 beta / macOS 27 SDK**, nonblocking and scheduled; never the shipping lane.
- Architecture: Apple Silicon (`arm64`) static XCFramework assembly.
- No compatibility branch, custom material, explicit `glassEffect`, or `GlassEffectContainer` exists in production UI.

**Forward-lane exception (dated):** the scheduled nonblocking Xcode 27/macOS 27
build lane does not exist yet. Owner: Release Engineering. Recorded 2026-08-20.
Shipping remains Xcode 26.6 and forward failures do not gate release. The
exception exits when an Xcode 27 runner image is available and the lane is added
at the owning `velnor-actions` native-workflow source (`ci.yml` is generated —
never hand-edited), then regenerated here.

**Post-26.0 API discipline:** every post-26.0 symbol is guarded with
`if #available(macOS 27, *)`, ships a decided native fallback, and names the
minimum-target bump that removes the guard. `UIDesignRequiresCompatibility` is
never a strategy; an architecture test rejects it.

Liquid Glass is owned by the system hosts and standard functional chrome: `NSPopover`, unified `NSToolbar`, `NSSplitViewController`, sidebar/list/table, menus, pickers, buttons, and window titlebars. Quota content uses ordinary `Form`, `List`, `Section`, `LabeledContent`, `Table`, and `ProgressView` surfaces. The status bar remains template monochrome. jackin❯ phosphor appears only as adaptive identity/healthy-state emphasis; warning and danger retain textual state plus system semantic color.

## Native surfaces

### Status items and popover

`StatusBarController` owns native `NSStatusItem` instances selected from the Rust projection. A primary click opens one real transient `NSPopover` focused on that provider. The popover contains:

- a centered, noninteractive generated jackin❯ monogram plus `jackin❯ desktop` identity row;
- provider identity, selected account, and one Rust-owned activity phrase;
- Limits before useful Details without repetition;
- visible Retry actions for global/provider failures;
- a fixed native footer with adjacent icon-only Refresh (Command-R) and Open Usage
  actions at the leading edge, plus a trailing native account menu when multiple
  identities are known. Semantic labels and hover help preserve discoverability while
  the system popover and controls own Liquid Glass.

There is no cross-provider navigation inside the popover. A secondary click opens the fixed native menu: Open Usage Window, Refresh, Quit jackin❯ desktop.

### Usage window

`UsageWindowController` lazily creates and retains one normal `NSWindow`. A native `NSSplitViewController` owns two columns while SwiftUI renders their content:

- sidebar: Overview plus Rust-ordered providers;
- quiet footer: generated `jackin❯ by tailrocks` wordmark;
- Overview: expanded native hierarchical `Table` with provider parents, account
  children, and Provider/Account/Plan or status/Remaining/Reset columns;
- provider detail: selected identity, account menu, Details, Limits, and recovery;
- titlebar: the standard split-view sidebar button in its fixed leading slot;
- detail top accessory: centered `jackin❯ desktop` identity and trailing Refresh.

The standard `.toggleSidebar` item and `NSSplitViewController.toggleSidebar(_:)` responder action are the only visibility authority. Its native width is retained while its accessibility label changes between Show Sidebar and Hide Sidebar, so the control stays stationary through collapse and retained-window reopen. The sidebar owns the full leading structural height. The detail-only native split-item accessory centers the noninteractive product identity over the detail pane and keeps Refresh trailing; no root header or `Usage` heading spans both panes. Reopening preserves valid destination, account, sidebar state, and frame. A removed/disabled provider normalizes to Overview at `PresentationStore`, not in a view-only fallback.

Standard commands: Command-R Refresh, Command-comma Settings, Command-W Close, Control-Command-S Toggle Sidebar.

### Settings

Settings is a standard titled `NSWindow` containing a grouped `Form`. It owns menu-bar display selection, percent/reset preferences, screen-sharing privacy, launch at login, enabled surfaces, and refresh floor. It does not render quota data or create custom Liquid Glass.

## Layout

| Path | Role |
|---|---|
| `../crates/jackin-usage` | Host probes and `HostUsageRuntime` |
| `../crates/jackin-usage-ffi` | Synchronous boltffi facade |
| `Sources/JackinUsageBindings` | Generated boltffi Swift only (never handwritten) |
| `Sources/JackinUsageBridge` | Handwritten sole FFI importer: typed facade, `PresentationStore`, pure projections |
| `Sources/JackinDesktop` | AppKit hosts and SwiftUI native surfaces |
| `Sources/JackinDesktop/VisualQAFixtures.swift` | Explicit synthetic F00–F14 visual-QA states |
| `UITests/JackinDesktopUITests.swift` | Real-host interaction and accessibility audits |

## Build and verify

```bash
mise install

# Build + verify + launch.
mise run desktop

# Individual steps.
mise run desktop-generate
mise run desktop-build -- 0.6.0 1
mise run desktop-verify
mise run desktop-run
```

The default bundle is `native/dist/JackinDesktop.app`. Build/verify/run print its absolute path and `DESKTOP_APP=…`. The app begins as an `LSUIElement` status-item process; opening a normal window temporarily gives it regular app menu/window citizenship.

## Tests

```bash
mise run desktop-ci

mise run desktop-format-check
mise run desktop-lint
mise run desktop-deadcode
mise run desktop-test
mise run desktop-test-ui

cargo xtask desktop test-swift
```

`desktop-ci` is the required macOS PR contract: nonmutating bindings drift
check, Xcode project generation, formatting, SwiftLint, Rust/FFI plus parity
harnesses, app build, counted SwiftPM tests, then fail-closed app verify.
`desktop-merge` adds the UI suite on top; `desktop-scheduled` adds the
dead-code scan. CI and release invoke these exact `mise run desktop-*` task
names — one definition per command.
`desktop-test` covers 291 Rust/FFI tests plus native architecture/parity harnesses. SwiftPM tests protect ownership, navigation normalization, native component confinement, brand tokens, and visual-QA fixture isolation. The UI suite runs the real app host and audits popover, Overview, provider detail, sidebar coordinates, commands, scrolling, recovery, and retained context.

Explicit visual-QA launch flags (`--fixture`, `--open-popover`, `--open-usage`, `--selection`, `--window-size`, `--appearance`) never activate unless `--fixture` is present in argv and never call the bridge or real credentials. Fixture runs carry a persistent visible Fixture badge, and their frozen account/refresh projections exercise immediate selection plus `Updating…` → terminal activity. Environment variables cannot enable fabricated data. Moving fixture code into a debug-only target remains a maintenance follow-up.

## Visual QA

```bash
native/Scripts/VisualQA/capture-final-matrix.sh native/dist/JackinDesktop.app
```

The script rebuilds and verifies the canonical branch-head app, then drives deterministic fixtures through the real popover and Usage-window hosts. Captures use actual window IDs and default to the ignored `native/.build/visual-qa/final/` directory. They are temporary verification output: inspect them, restore any changed system appearance or accessibility settings, and do not commit them. The retained distributable is `native/dist/JackinDesktop.app`.

## Static assembly

One path builds local, PR, and release apps:

1. `mise install` installs pinned tools.
2. `cargo xtask desktop xcframework` creates the arm64 static `target/xcframework/JackinUsage.xcframework` (FFI module `JackinUsageFFI`).
3. `native/Package.swift` consumes it as a binary target.
4. `mise run desktop-build -- <version> <build>` generates bindings/project, builds `JackinDesktop.app`, and ad-hoc signs local/validation output.
5. `mise run desktop-verify` proves bundle architecture, metadata, dependency, and signature shape. Release verification additionally requires Developer ID, notarization, staple, and Gatekeeper acceptance.

After an XCFramework rename or FFI module change, delete `native/DerivedData` before rebuilding — Xcode caches clang module resolution and otherwise fails with stale module errors.

## CI and release contract

| Surface | Contract |
|---|---|
| PR/local validation | macOS 26.0, Xcode 26.6, arm64 static app, tests and bundle verification |
| Secret-free release validation | fixture version, ad-hoc rejection by release verifier, read-only reconciliation |
| Publication | `main`/tag only, environment `release-macos`, GitHub-hosted macOS only |
| Artifact | `jackin-desktop-<VERSION>-aarch64-apple-darwin.zip` plus SHA-256, Sigstore bundle, SBOM, attestation |
| Symbols | `desktop-release` Cargo profile (thin LTO, one codegen unit, line-table debug, no strip); build UUID-checks and archives `native/dist/JackinDesktop.app.dSYM` beside the app, release CI uploads it with the compressed unstripped Rust static library (90-day retention) |
| Homebrew | formula and `Casks/jackin-desktop.rb` in one independently reviewed tap PR |

Required `release-macos` secret names:

- `DEVELOPER_ID_APPLICATION_P12_BASE64`
- `DEVELOPER_ID_APPLICATION_P12_PASSWORD`
- `APP_STORE_CONNECT_API_KEY_P8`
- `APP_STORE_CONNECT_KEY_ID`
- `APP_STORE_CONNECT_ISSUER_ID`

Required repository variables:

- `JACKIN_DEVELOPER_ID_TEAM_ID`
- `JACKIN_DEVELOPER_ID_CERT_SHA256`

Credential material is never committed. CI removes temporary signing/notary material before supply-chain tooling runs. Until an operator provisions these values and performs the first notarized publication/cask proof, validation is complete but public distribution remains externally gated.

## Local notarization rehearsal

```bash
export DEVELOPER_ID_APPLICATION='Developer ID Application: Your Name (TEAMID)'
export NOTARY_PROFILE=jackin-notary
export JACKIN_APP_VERSION=0.6.0 JACKIN_APP_BUILD=1
mise run desktop-build -- 0.6.0 1
mise run desktop-sign-notarize
```

See the [public macOS guide](<../docs/content/(public)/guides/macos-usage-menu-bar.mdx>) and [ADR-011](../docs/content/reference/adrs/adr-011-native-macos-usage-menu-bar.mdx) for operator behavior, architecture, component ownership, and verification boundaries.

## Xcode agent bridge

Manual host-only integration; never part of CI. Setup on the shipping Xcode:

1. In Xcode 26.6, open Settings → Intelligence and enable external agent access.
2. Run `mise run desktop-generate`, then open `native/JackinDesktop.xcodeproj`
   in the running Xcode instance.
3. From the external agent, enumerate the bridge's actually exposed tools
   before depending on any command name.

Boundary: the Xcode bridge supplies project-context, build, test, and preview
operations only. It captures **no** running-app screenshots and drives **no**
interface automation; `native/Scripts/VisualQA` and XCUITest own those
capabilities. A preview or bridge result is never running-app visual evidence.
A headless worker without a running Xcode and open project reports the bridge
as unavailable — never as passed.

Verification checklist (no secrets; re-probe whenever the shipping Xcode pin
changes):

- expected project: `native/JackinDesktop.xcodeproj` (generated, never committed)
- expected scheme: `JackinDesktop`
- expected Xcode build: 26.6 (`17F113`)
- observed tool list: enumerate and record in the session log before use

## Agent responsibility ownership

Exactly one owner per responsibility; explicit invocation remains required for
overlapping aesthetic skills.

| Responsibility | Owner |
|---|---|
| Framework correctness (Swift/AppKit/SwiftUI API use) | `tailrocks-swift-best-practices` |
| Material policy (Liquid Glass ownership rules) | `tailrocks-macos-design` |
| Visual direction (hierarchy, alternatives, anti-references) | `tailrocks-macos-design` |
| Rendering and visual verification | `tailrocks-macos-visual-qa` |
| Project mechanics (generation, pins, gates, lanes) | `tailrocks-swift-project-setup` |
| Design tokens (brand color/type values) | this repository (`Sources/JackinDesktop/BrandColors.swift`) |

### Apple agent skills export — recorded blocker

`native/Vendor/AppleAgentSkills` is intentionally absent. Probed Xcode 26.6
(build `17F113`, the shipping lane) on 2026-08-20: the bundle ships agent
intelligence only as compiled frameworks
(`Contents/PlugIns/IDEIntelligence*.framework`,
`Contents/SharedFrameworks/*Intelligence*.framework`) — there are no
exportable skill documents (`SKILL.md` or equivalent) anywhere in
`Xcode.app`, so there is nothing reviewable to vendor, hash, or pin. The
unsupported-export caveat is therefore the standing state: project-local
agent knowledge comes exclusively from the pinned `tailrocks-*` skills above
and this repository's own docs. Refresh rule: re-probe on every shipping
Xcode change; if a future Xcode exposes a documented skills export, vendor it
read-only with build, export date, and file hashes before use, and never
execute unreviewed bundled scripts or network steps.
