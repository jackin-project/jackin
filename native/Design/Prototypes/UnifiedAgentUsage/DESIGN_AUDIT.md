# Liquid Glass design audit — Unified Agent Usage prototype

Date: 2026-08-20

## Platform baseline

| Contract | Value |
|---|---|
| Minimum deployment target | macOS 26.0 (`Package.swift`) |
| Shipping SDK / Xcode | macOS SDK 26.5 / Xcode 26.6 (17F113) |
| Forward-validation SDK / Xcode | Not configured; macOS 27 symbols are forbidden |
| Forward-only fallback | Keep the macOS 26 standard component; add guarded 27 behavior only in a dedicated validation lane |

Local probes reported Swift 6.3.3 and an arm64 macOS 26.0 target.

## Supplied-artifact inventory

| Artifact | Audit use |
|---|---|
| `Package.swift` | Deployment target and resource contract |
| `DesignSystem/Brand.swift` | Dark color, type, spacing, identity, provider marks, digital rain |
| `App/` | AppKit window, split view, native toolbar, status items, popover, menus |
| `Features/Usage/` | Sidebar, overview cards, provider detail, meters |
| `Features/Popover/` / `Features/Settings/` | Focused popover and Settings reference views |
| `Domain/` | Immutable reference models, navigation, semantic category/order |
| `Harness/Fixtures.swift` | F00–F29 state, scale, localization, RTL, and accessibility fixtures |
| `Tests/UnifiedAgentUsageProtoTests/` | Pure launch, navigation, state-truth, and ordering contracts |
| `Regions.md` / `SIGNOFF.md` | Structural region and operator acceptance contracts |

No embedded repository instruction changed the skill decision order.

## Layer classification

| Region | Layer | Component | Glass source |
|---|---|---|---|
| Window title bar and toolbar | FUNCTIONAL / structural | `NSWindow` + `NSToolbar` | Automatic system material |
| Sidebar | FUNCTIONAL / structural | `NSSplitViewController` sidebar + SwiftUI `List(.sidebar)` | Automatic system material |
| Sidebar wordmark | FUNCTIONAL / structural identity | Image inside sidebar plane | Sidebar material only; no effect |
| Overview stage and provider cards | CONTENT | `ScrollView` + adaptive card grid | Opaque semantic content colors; never glass |
| Provider and account card rows | CONTENT | Buttons with plain style | No glass |
| Provider detail | CONTENT | Authored dossier `ScrollView` + adaptive limit modules | No glass |
| Quota meters | CONTENT | Deterministic SwiftUI shapes | No glass |
| Popover shell | FUNCTIONAL / transient | `NSPopover` | Automatic system material |
| Popover detail | CONTENT inside transient host | Compact priority-ranked quota glance | Opaque content; no glass |
| Popover action cluster | FUNCTIONAL / transient | Two standard SwiftUI Buttons in one `GlassEffectContainer` | `.glass` + one `.glassProminent` |
| Settings | CONTENT inside window | Native grouped `Form` | No glass |
| Menu bar items and menus | FUNCTIONAL / structural + transient | `NSStatusItem`, `NSMenu` | Automatic system material |

Every region classifies cleanly. No glass exists in content.

## Decision-order record

1. Standard components satisfy the window, toolbar, sidebar, list, form,
   popover, menu, status-item, picker, toggle, and sheet-like presentation
   needs. They remain authoritative.
2. No custom toolbar, split-view, sheet, popover background, blur, or bezel
   exists to delete.
3. The popover footer is a composition of two standard buttons. Its explicit
   glass styles are appropriate because it is transient functional chrome; one
   shared container batches the cluster.
4. No custom bar or overlay is required.
5. No raw `glassEffect`, `NSGlassEffectView`, or custom glass surface is
   justified or shipped.

## Mechanics

| Check | Result | Evidence |
|---|---|---|
| Modifier order | PASS | No raw `glassEffect`; standard button styles own capture order |
| Container batching | PASS | Popover's two glass buttons share one `GlassEffectContainer(spacing: 8)`; interior stack spacing is also 8 |
| Nesting / overlap | PASS | No nested or independently overlapping glass surfaces |
| Corner concentricity | PASS | Single controls use system-derived capsules; no numeric glass radius |
| Tint count | PASS | Exactly one prominent action in the popover bar: Open Usage |
| Variant choice | PASS | Standard regular glass only; no `clear` variant |
| Toolbar command parity | PASS | Refresh and sidebar toggle also exist in the View menu |
| Icon accessibility | PASS | Refresh and Open Usage icon buttons have explicit labels and help |
| Motion | PASS in code | Stable stage with 150ms inner move/fade and 140ms refresh fade; no geometry/blur morph; Reduce Motion returns identity |

## Availability

The used Liquid Glass symbols—`GlassEffectContainer`, `.glass`, and
`.glassProminent`—are macOS 26.0 and match the deployment target. No guard is
required. No macOS 27 beta, visionOS-only, UIKit-only, or unavailable toolbar
symbol is present.

Blocked spellings were searched and are absent:
`glassBackgroundEffect`, `toolbarOverflowMenu`,
`topBarPinnedTrailing`, `containerConcentric`,
`effectIsInteractive`, `prominentGlass`, and `clearGlass`.

## Custom-surface records

### Popover action cluster

| Field | Record |
|---|---|
| Layer | FUNCTIONAL / transient |
| Why no earlier component sufficed | A native popover and standard Buttons do suffice; step 3 only composes them into one compact action cluster |
| Container | One `GlassEffectContainer`, spacing 8; interior HStack spacing 8 |
| Variant | Regular for Refresh; prominent regular for the sole primary Open Usage action |
| Shape | System button-style capsule; no numeric radius; concentric derivation does not apply to a single free-floating capsule |
| Availability | macOS 26.0, equal to minimum target |
| Reduce Transparency | System Button and NSPopover substitutions; no app-painted material |
| Reduce Motion | No glass morph; app-authored opacity animations resolve to identity/no animation |
| Verified | Dark launch stability and process-local reduction fixtures |
| Blocked | Real Clear/Tinted setting, real Reduce Transparency, hover, inactive window, focus-ring and VoiceOver inspection require operator visual QA |

### Toolbar Refresh control

| Field | Record |
|---|---|
| Layer | FUNCTIONAL / structural |
| Why no earlier component sufficed | A standard `NSToolbarItem` owns the command, symbol, placement, and system material; no custom hosted control is used |
| Container | System toolbar grouping; single button, so no app container |
| Variant | Regular; never prominent or tinted independently |
| Shape | System button-style capsule; no numeric radius |
| Availability | macOS 26.0, equal to minimum target |
| Reduce Transparency | System toolbar/Button substitution |
| Reduce Motion | 140ms opacity swap is disabled; no morph or blur animation |
| Verified | Dark launch stability and refresh fixture behavior |
| Blocked | Real accessibility settings, inactive-window rendering, hover and focus-ring inspection require operator visual QA |

## Anti-pattern gate

| Anti-pattern | Result | Mechanism evidence |
|---|---|---|
| Glass in content | PASS | Cards, rows, meters, Lists and Forms use opaque/system content material; functional/content distinction remains visible |
| Glass-on-glass | PASS | No nested sampling; popover sibling controls share one container |
| Custom bar/split/popover backgrounds | PASS | None; scroll-edge and content-derived adaptation remain system-owned |
| Hard-coded glass radii | PASS | No numeric glass radius |
| Tint abuse | PASS | One prominent action; semantic content colors never create extra glass primaries |
| Missing accessibility substitutions | PASS in architecture | All material is system component material; app-authored motion is removed |
| Unbatched effects | PASS | Only multi-control explicit cluster is batched |
| iOS API leakage | PASS | No cross-platform-only symbol |
| Wrong modifier order | PASS | No raw modifier |
| Compatibility-key strategy | PASS | No `UIDesignRequiresCompatibility` |
| Mid-merge spacing | PASS | Container and interior spacing equal 8 |
| Raw effect on Button | PASS | Standard glass button styles used |

## Color system

jackin❯ desktop is dark-only. Every authored color resolves through
`JackinBrand`; light endpoints remain implementation-inaccessible rather than a
second product appearance. The active content hierarchy is:

- stage: blue-black #101618;
- cards: raised graphite #162022;
- inset modules: #1C2728;
- hover: #202D2E;
- boundary: #343D3F;
- strong boundary: #465254;
- meter track: #293335;
- healthy/brand: phosphor #5CF07A;
- selected navigation: deep green #173B2B with #E9F7ED text;
- metadata: #ADB5B2;
- quiet metadata: #AAB2AF;
- warning: #FFC15A;
- danger: #FF7B72;
- brand wash: #16372A.

Status-bar and content severity share the same dark endpoints. Increase
Contrast strengthens the authored card edge from 1pt to 1.5pt. State always pairs
color with a symbol or label. System glass keeps its content-derived color;
brand color enters it only through the standard accent and the sole prominent
action.

WCAG contrast against the dark card ground:

| Token | Ratio |
|---|---:|
| Phosphor | 11.24:1 |
| Metadata | 7.94:1 |
| Quiet metadata | 7.67:1 |
| Warning | 10.31:1 |
| Danger | 6.59:1 |

## Typography, rhythm, hierarchy, and multi-account

- Type ramp: 28pt monospaced overview metric, 20pt monospaced detail metric,
  10–11pt monospaced technical labels, system headline/callout body.
- Rhythm: 28pt page insets, 28pt dossier section gaps, 20pt overview-card
  insets, 16pt module insets, and the authored 4/8/12/16/20/24 primitive scale.
- Scan: provider → hero remaining percentage → meter → reset.
- The preferred overview card grid remains. It is content, never glass, and its
  custom opaque boundary is justified by provider grouping and fast scanning.
- F25 keeps five accounts inside one provider card with restrained dividers.
- Usage detail uses a two-column adaptive instrument grid at comfortable width
  and one column at the minimum. The popover consumes the same semantic records
  through a separate compact presentation using the same explicit semantic category order with
  overflow disclosure.

The visual thesis is **graphite instrument + phosphor signal**. It borrows the
industrial neutral planes, calibrated density, technical micro-labels, crisp
boundaries, and sparse accent discipline visible in
[Oxide](https://oxide.computer/), while retaining original jackin❯ identity,
native macOS structure, and provider marks. No Oxide asset or page composition
is copied.

## Provider detail coverage

The prototype fixtures mirror the complete limits-only projection currently
normalized by `crates/jackin-usage/src/usage/`. Detail pages render every bucket
in explicit semantic category order while preserving source order inside each
category; they do not collapse provider data into a generic weekly row. The
prototype fixture owns this metadata; production receives final order from Rust.

| Provider | Detailed output represented |
|---|---|
| OpenAI / Codex | Account, plan, auth origin, session and weekly limits, provider-named additional limits, limit-reset credits, credit balance |
| Anthropic / Claude | Account, plan, auth origin, session, all-model weekly, every named model-scoped weekly limit, extra-usage cap, provider dollar-budget windows |
| Amp | Account, inferred Amp Free plan, auth origin, daily allowance, individual credit balance, every named workspace balance |
| xAI / Grok | Account, server plan tier, auth origin, billing-cycle limit, prepaid extra-usage credits, bounded on-demand usage cap |
| Z.AI / GLM | Plan, auth origin, short token window, primary token window, MCP count limit |
| Kimi | Auth origin, rate-limit window, weekly coding limit, provider counts and resets |
| MiniMax | Plan, auth origin, general five-hour and weekly windows, every available model interval, provider counts and resets |

The Rust detail contract additionally supports distinct username, freshness,
last-good data plus error, meter severity, pace/run-out projection, exact reset,
and unavailable/auth-required states. Fixtures exercise the user-visible forms.
New provider-supplied named/model windows are already planned by the generic
bucket projection: they append as clear items without a Swift provider matrix.
No token price, cost history, or usage trend is represented or planned.

## Motion and transitions

Overview/provider navigation keeps the stage and native glass chrome stable,
then moves the incoming content 5pt while fading it over 150ms ease-out.
Account changes inside one provider update in place. Refresh glyph/spinner state
uses a 140ms ease-out opacity swap; overview hover uses 120ms ease-out with an
immediate lower-opacity pressed state. No scale, spring, geometry, glass morph,
blur, or glow exists. Both real Reduce Motion and the prototype reduction
contract remove app-authored animation.

System window, menu, popover, hover, press, and focus transitions remain
system-owned. Sidebar selection geometry is authored inside the native sidebar
material so the dominant navigation signal remains jackin❯ green instead of
the user's unrelated system accent.

## Acceptance-gate evidence

| Axes | Status |
|---|---|
| Dark-only appearance | PASS; production forces `.darkAqua`, prototype rejects non-dark appearance input, and canonical F02/F04/F25 captures are pixel-reviewed |
| Localization / RTL / long strings | PASS for launch/render stability via F11 and F19 variants |
| Reduce Motion / Transparency flags | PASS for process-local launch stability; real settings remain unverified |
| Clear / Tinted Liquid Glass | BLOCKED pending operator visual QA; no read API exists |
| Auto appearance and live appearance switch | NOT APPLICABLE; dark-only product decision |
| macOS 27 Liquid Glass slider / Show Borders | BLOCKED; no macOS 27 validation lane |
| Increase Contrast / Differentiate Without Color | BLOCKED pending real-setting visual QA; code paths and redundant symbols are present |
| Accent/highlight palette matrix | NOT APPLICABLE; jackin❯ owns one dark phosphor accent system |
| Active/inactive window | BLOCKED pending operator visual QA |
| Sidebar sizes, scroll bars, displays, scale, wallpaper, color profiles | BLOCKED pending operator visual QA |
| Minimum/full-screen layout | Minimum sizes launch clean; full-screen interaction pending operator QA |
| VoiceOver, Voice Control, Full Keyboard Access, focus ring | BLOCKED pending operator accessibility QA |
| Hover | BLOCKED; macOS 26 outside-toolbar glass hover defect is known |

## Automated stability evidence

The current dark-only canonical matrix covers F02 overview at 1000 × 680 and
1200 × 760, F04 four-limit detail, F25 dense multi-account detail, F11 long copy
at 800 × 520, F09 unavailable/auth-required, F06 stale/informational, process-
local Reduce Transparency and Increased Contrast, and native sidebar collapse.
Every capture has a WindowServer sidecar proving active application, key window,
full on-screen containment, frame size, app executable hash, and image hash.

`mise run desktop-prototype-build`, `mise run desktop-test`, and `git diff
--check` are the automated gates. Independent design review remains the final
visual acceptance authority; this audit does not self-certify approval.
