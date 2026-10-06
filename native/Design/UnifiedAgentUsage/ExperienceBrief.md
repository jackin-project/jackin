# Experience Brief — Unified Agent Usage

Status: APPROVED
Approved by: Alexey Zhokhov
Approved on: 2026-08-20

## Platform baseline

- Deployment target: macOS 26 only; no older-appearance or compatibility lane.
- Design verification baseline: macOS 26.5.2, Xcode 26.6, macOS SDK 26.5,
  Retina 2×. The final gate repeats on the newest stable macOS 26/Xcode 26
  available when implementation lands.
- Linked-SDK behavior is intentional: system AppKit and SwiftUI components own
  Liquid Glass. No fallback implementation or custom material is designed.
- APIs introduced after the deployment floor require an availability entry in
  the native component map before use. Beta macOS 27 APIs are out of scope.

## User

Developers and operators who run several AI coding agents through jackin❯ and
need to decide, at a glance, which configured provider account has usable
subscription or quota capacity. They understand providers, accounts, plans, and
reset windows; they should not need to understand discovery sources, broker
generations, or credential storage.

## Primary job

Determine the current quota state of a configured provider account, including
its relevant limit windows and reset timing, without triggering duplicate
provider work.

## Archetype

Dominant archetype: monitoring and operations workspace.

Secondary archetypes, and which window or mode each owns:

- Menu-bar companion: provider-focused status items and the transient glance
  popover.
- Preferences utility: the Settings window.

Primary object of this window: a canonical provider account and its ordered
quota-limit windows.

Why this archetype and not the adjacent dashboard archetype: the work is
selection, comparison, and inspection of a small live inventory. It does not
contain historical analytics, trends, charts, or aggregate spend.

The failure this archetype attracts, and how the design avoids it: monitoring
interfaces tend to become card grids or dense walls of equal-priority metrics.
This design keeps one selected provider/account relationship, uses native
list/table hierarchy, gives state labels precedence over decoration, and keeps
the popover intentionally small.

## Primary objects

- Provider: one Rust-owned usage surface in the seven-provider desktop catalog.
- Canonical account: one deduplicated provider identity with provenance hidden
  from normal presentation.
- Quota limit window: an ordered Rust-owned row containing its label, remaining
  or used convention, reset text, and optional provider-supplied money cap.
- Provider/account state: loading, refreshing, current, stale last-good,
  partial error, permission denied, offline, or unavailable.
- Presentation preference: menu-bar mode, pinned provider, percent convention,
  reset convention, privacy behavior, enabled desktop surfaces, refresh floor,
  and launch-at-login state.

People inspect and select these objects. They do not create, rename, merge, or
delete provider accounts from jackin❯ desktop.

## Information hierarchy

Visible immediately:

- Current provider and canonical account.
- Most relevant Rust-ranked remaining/used value.
- Current, refreshing, stale, or failed state in text and symbol form.
- Reset label when supplied.

Contextual:

- Every quota window for the selected account.
- Plan/status, source/confidence, last update, and recoverable provider error.
- Other canonical accounts for the provider.

Inspector: none in the incumbent direction. Quota windows are primary content,
not secondary properties, so they remain in the detail pane.

Separate window or sheet:

- Retained Usage window for all-provider overview and provider/account detail.
- Settings window for presentation and launch behavior.
- No modal sheet for quota detail or recoverable errors.

Progressively disclosed:

- Account rows beneath provider rows in Overview.
- Technical error detail only where Rust supplies an actionable diagnostic.
- Settings that apply only to the selected display mode.

## Actions

| Action | Frequency | Consequence | Placement | Menu command | Shortcut |
|---|---|---|---|---|---|
| Open focused glance | constant | safe | Status item primary click | Window > Usage provides full equivalent | Menu-bar keyboard navigation |
| Open Usage | frequent | safe | Popover footer and Window menu | Window > Usage | System-discoverable menu shortcut |
| Select provider | frequent | safe | Usage sidebar | None; selection is content navigation | Arrow keys, Return |
| Select account | frequent | safe | Native account picker or account row | None; selection is content navigation | Native picker/list keys |
| Expand provider accounts | occasional | safe | Overview disclosure | None; disclosure is content navigation | Native disclosure keys |
| Refresh all | occasional | safe | Usage accessory and popover footer | View > Refresh | Command-R |
| Retry one provider | occasional | safe | Contextual provider error | View > Refresh Provider when context exists | Return/Space on focused button |
| Toggle sidebar | occasional | safe | Standard toolbar item | View > Show/Hide Sidebar | Control-Command-S |
| Open Settings | rare | safe | App menu | App > Settings | Command-Comma |
| Show status-item context menu | rare | safe | Status item secondary click | Equivalent commands remain in main menus | Menu-bar keyboard navigation |
| Quit | rare | safe | App menu and status context menu | App > Quit | Command-Q |

Primary action: inspect the selected account's complete quota windows. No
prominent button is needed to advance this read-only job; selection is the
primary interaction. Refresh is secondary because cached last-good data remains
useful and broker freshness is automatic.

## Window model

One menu-bar application with:

- Stable provider-focused `NSStatusItem` controls.
- One transient system `NSPopover` for focused glance and account switching.
- One lazily created, retained, unique, resizable Usage `NSWindow`.
- One system Settings scene/window.

The application is dark-only. The titlebar always shows the official absolute-
centered jackin❯ wordmark, including when the sidebar is hidden. Refresh is a
standard AppKit toolbar item. A restrained digital-rain layer may appear only
behind authored content and never replaces native chrome.

Opening Usage promotes normal app menus. Closing its titled window returns the
application to accessory menu-bar behavior without terminating the process.

## Input

Pointer: precise native hit targets, standard hover/pressed states, selectable
rows, disclosures, pop-up buttons, and scrollbars. No hover-only action.

Keyboard workflow:

1. Open Usage through the Window menu or its shortcut.
2. Move through Overview and destinations in the sidebar. A provider with
   multiple accounts is a nonselectable group header and exposes account
   destinations; a single-account provider is directly selectable. Every
   selectable destination includes a supplemental remaining-quota meter.
3. Move through provider/account rows and disclosures with native table/list
   navigation.
4. Activate account selection or Retry with Return/Space.
5. Refresh with Command-R, toggle the sidebar with Control-Command-S, close with
   Command-W, and open Settings with Command-Comma.
6. Escape dismisses the transient popover; focus never becomes trapped.

Trackpad: system scrolling only. No custom or redefined gestures.

Drag and drop: out of scope; there is no transferable or reorderable content.

Context menus: status item exposes Open Usage, Refresh, and Quit. Content rows
need a context menu only if a future row-specific command exists; no empty menu
is added for appearance.

Status-item event routing: never assign `NSStatusItem.menu`, because that takes
over the button's click behavior. The status-bar button sends left- and
right-mouse-up events to one AppKit action. Left click toggles the anchored
`NSPopover`; right click explicitly presents the native `NSMenu`. Keyboard menu
bar activation exposes the same commands.

Focus routing: on popover open, the hosting view becomes key and the first
meaningful control receives focus according to the native key loop. Escape is
an explicit cancel action that closes the transient popover and returns focus to
the originating status item. Opening Usage transfers focus to the retained
window's selected sidebar/content row; closing it returns normal menu-bar app
behavior. Focus restoration and the key loop are tested, not assumed from
construction APIs.

Services / system integrations in scope: menu-bar extra, app menu commands,
launch at login, window restoration, Keychain consent surfaced through
Rust-owned diagnostics, direct Developer ID distribution, and Homebrew cask
installation.

## Continuity

Restore the Usage window's position, size, sidebar width/visibility, selected
provider, selected canonical account, provider disclosure state, and overview
selection. Preserve these independently from transient popover selection where
the user's explicit handoff does not replace the retained Usage context.

Restore the last Settings pane/state through system ownership. Preserve
presentation preferences only after Rust or the owning system service accepts
them; a rejected mutation must remain visible beside the initiating control.

## Latency targets

| Interaction | Target |
|---|---|
| Pointer or key acknowledgement | Immediate, same frame |
| Selection change | Immediate; no provider request required |
| Menu opening | Immediate |
| Window resize | Continuous with no row overlap or label collision |
| Cached projection opening | First usable frame without network wait |
| Refresh intent | Immediate busy acknowledgement; one broker request |
| Provider completion | Progressive per-provider replacement; other navigation stays live |
| Long work | Stable layout plus progress; leaving the view never creates a second provider request |

## Recovery

Undo: not applicable to read-only usage data. Preference mutations either commit
after acceptance or revert visibly to their accepted value.

Confirmation: none of the current actions are destructive. Do not add alert
confirmation to Refresh, account selection, window close, or preference changes.

Autosave: system window-frame autosave plus explicit selection/sidebar state.

Version history: not applicable.

Error recovery:

- Preserve last-good quota values when refresh fails.
- Label stale data and its age; never silently present it as current.
- Place provider-specific Retry beside the provider failure.
- Use `ContentUnavailableView` with Retry only when no usable projection exists.
- Keep permission/login-item failures beside the relevant Settings control.
- Never block unrelated providers, navigation, or launch behavior.

## Density

High-density professional, restrained by a calm menu-bar companion. The retained
window favors compact native rows and aligned columns for sustained scanning.
The popover favors one focused provider and a short set of related tasks; it is
not a miniature all-provider workspace.

## Window sizes

Minimum usable: 800 × 520 points. The selected destination, account, complete
quota labels, state text, Refresh, sidebar toggle, keyboard focus, and scrollbar
must remain functional. No columns may collapse into concatenated text under
Increase Contrast.

Typical: 1000 × 680 points.

Wide: 1200 × 760 points.

As width decreases, the user may close the native sidebar. Supporting metadata
wraps or truncates with accessible full text before quota identity, state, or
value is dropped. The Usage detail does not automatically become an overlay and
does not promise an inspector collapse behavior the system does not provide.

## States

Empty: “No providers detected,” why the inventory is empty, and the next place
to configure or start an agent. No empty provider rows are fabricated.

Loading: native indeterminate progress in reserved content space. Navigation and
window chrome remain stable.

Normal: provider/account hierarchy, current state, all Rust-owned quota windows,
and reset labels.

Very large dataset: seven providers, 40 canonical accounts, and eight
limit windows per selected account. Scrolling remains native and selection
stable across refresh.

Long values: 2× English provider, account, plan, limit, reset, and error strings;
mixed CJK/Arabic text; one unbroken technical identifier. Primary identity and
quota state remain discoverable without overlap.

Missing values: explicit em dash or Rust-owned fallback for missing plan,
remaining value, reset, or limit detail; missing data never appears as zero.

Error: provider-local inline status and Retry when other data exists; global
unavailable content only when no usable projection exists.

Offline: stale last-good data with age, offline explanation, and Retry.

Permission denied: provider or Settings-local explanation naming the blocked
system permission and a recovery action when available.

Destructive operation pending: not applicable. A fixture remains in the visual
matrix to prove no destructive affordance has been introduced accidentally.

## Accessibility risks

VoiceOver: provider, account, plan, value, reset, and state must form concise
ordered summaries. Status-item images require exact descriptions. Grouping must
not hide children or create anonymous containers.

Keyboard-only operation: status item, popover controls, all Usage destinations,
account selection, disclosures, Retry, Refresh, sidebar toggle, Settings, and
Quit must be reachable without pointer input.

Focus visibility: system focus rings remain visible in key and inactive windows;
selection must remain identifiable without color or hover.

Reduce Motion: no custom navigation animation. Standard system transitions own
motion and adapt through the system setting; no app-defined spatial, spring, or
blur transition is approved.

Reduce Transparency: standard system material becomes opaque. No custom blur or
background may bypass the setting.

Increase Contrast: rows and column separation remain legible. The current
baseline's collapsed spacing/concatenated overview text is a release-blocking
defect to eliminate.

Color independence: stale, warning, depleted, refreshing, and failed states use
text/symbol/value changes in addition to semantic color.

## Localization risks

Text expansion: test every visible string at 2× English length and preserve
complete accessible values for truncated labels.

Right-to-left: standard list, form, table, picker, toolbar, and split behavior
mirror through the system. Percent signs, provider identifiers, account IDs, and
mixed-direction text require explicit fixture coverage.

Locale-sensitive numbers, dates, units: Rust owns quota formatting semantics;
the Swift shell displays supplied strings verbatim. Native Settings values and
system dates use locale-aware formatting.

## Out of scope

- Provider authentication, credential editing, or account merging.
- Provider HTTP/OAuth/CLI probing from Swift.
- A second cache, retry scheduler, identity model, or freshness authority.
- OpenCode in jackin❯ desktop; it remains part of host CLI/TUI inventory.
- Token prices, per-token cost, session cost, historical usage/spend, trends,
  sparklines, burn rate, aggregate-spend charts, or provider cost ranking.
- Launch authorization or blocking based on quota state.
- Custom-painted glass, custom window chrome, custom sidebar, or custom table.
- iOS-style navigation, fixed canvases, card-grid dashboards, or modal quota
  detail.

## Evidence inputs

- [Apple-native design evidence](../../../research/agent-usage-platform/02-apple-native-design.md)
- [Codebase architecture and failure modes](../../../research/agent-usage-platform/01-codebase-architecture.md)
- [Reference implementations and delivery directions](../../../research/agent-usage-platform/03-reference-implementations.md)
- [Current native shell rules](../../AGENTS.md)
