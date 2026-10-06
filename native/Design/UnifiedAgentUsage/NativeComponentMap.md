# Native Component Map — Unified Agent Usage

## Accepted dark-only prototype composition

This section supersedes earlier “no custom content region” language where it
conflicts with the selected prototype. AppKit still owns window, split view,
toolbar, status items, popover, menus, resize, and native chrome.

| Composition | Why standard control alone is insufficient | System-owned behavior | Accessibility/substitution | Production boundary |
|---|---|---|---|---|
| Sidebar destination well and inline meter | The destination must show account/provider quota at glance and use jackin❯ selection independent of unrelated system accent. | Native sidebar plane, split behavior, scrolling, pointer delivery, keyboard focus chain. | Full row label/value; meter supplemental; explicit selected trait and focus outline. Reduce Transparency leaves opaque content; Increase Contrast strengthens outline. | Rust supplies destination, percentage, state. Swift draws only geometry. Multi-account headers are not destinations. |
| Quota modules | Dense label/value/reset/meter scanning is materially clearer than generic form rows. | Scroll, text rendering, focus, links, toolbar commands. | Each module combines complete label/value/reset/state; color is redundant. Opaque under Reduce Transparency. | Rust supplies finished strings, category, state, final order. Swift never parses labels or reorders production DTOs. |
| Digital-rain stage | No standard control expresses restrained jackin❯ atmosphere. | Window/chrome material and accessibility settings remain system-owned. | Noninteractive and accessibility-hidden; removed under Reduce Transparency; static/disabled under Reduce Motion. | Swift-only background, never behind native chrome or inside data modules. |
| Centered wordmark | Native title text cannot preserve the official artwork at the absolute window center through sidebar collapse. | Titlebar, traffic lights, toolbar layout, resizing. | One labeled heading; no duplicate page title. | Small titlebar host only; AppKit lifecycle reinstalls after collapse/resize. |

Selection state is model-owned and exposed semantically even where its well is
authored. Keyboard destination order is Overview, each account under a
multi-account provider, then each single-account provider in canonical Rust
order. Quota category/order is explicit DTO metadata: `longRange`, `model`,
`general`, `session`, `other`; display-label inspection is forbidden.

Status: DRAFT. No custom visible component is proposed.

Every visible region carries exactly one classification.

- `NATIVE` — standard component, semantic configuration only. The design never
  specifies internal appearance.
- `NATIVE-COMPOSED` — product-specific arrangement of standard components.
- `CUSTOM` — requires a completed custom component contract.

## Map

| Region | Class | Component / API | Placement | Allowed customization | Forbidden customization |
|---|---|---|---|---|---|
| Provider status items | NATIVE-COMPOSED | `NSStatusItem`, `NSStatusBarButton`, template `NSImage` | System menu bar | Rust-ranked item presence, provider symbol, short remaining label, accessibility description, autosave identity | Custom status-bar background, glass, oversized pill, app-owned menu-bar layout |
| Status-item context menu | NATIVE-COMPOSED | `NSMenu`, `NSMenuItem` presented explicitly for right click | Anchored to status item | Titles, enabled state, standard shortcuts | Assigning `NSStatusItem.menu`, custom menu panel, hidden canonical commands, pointer-only rows |
| Focused glance host | NATIVE | `NSPopover` with system behavior | Anchored to clicked status item | Transient behavior, semantic size, SwiftUI content | Custom floating panel without proven focus defect, custom material, manual screen coordinates |
| Popover title | NATIVE-COMPOSED | `HStack`, template image, `Text` | Popover header | Product name, fixture marker in QA only | Decorative glass plate, gradient, custom blur |
| Popover provider content | NATIVE-COMPOSED | SwiftUI `Form`, `Section`, `LabeledContent`, `ProgressView`, `ContentUnavailableView`, `Label`, `Link` | Popover content area | Rust-owned strings, ordering, labels, accessibility summaries | All-provider card grid, custom scroll view behavior, custom quota semantics |
| Popover account selection | NATIVE | SwiftUI `Picker` using pop-up style | Popover footer | Canonical account rows and selected account key | Source-path choices, duplicate account entries, custom dropdown |
| Popover footer actions | NATIVE-COMPOSED | `Button`, `Spacer` | Bottom of popover | Refresh/Open Usage labels, enabled/busy state, native keyboard equivalents | Floating action capsules, icon-only ambiguous actions, separate refresh authority |
| Usage window | NATIVE | `NSWindow` managed by `NSWindowController`/delegate | Unique retained titled window | Title, minimum/default size, restoration identifier, standard collection behavior | Custom frame, traffic lights, shadow, titlebar material, fixed canvas |
| Usage split structure | NATIVE-COMPOSED | `NSSplitViewController`, sidebar/detail `NSSplitViewItem` | Full-height Usage content | Sidebar min/ideal/max widths and restoration | Hand-painted divider, custom resize interaction, automatic overlay promise |
| Usage toolbar | NATIVE-COMPOSED | `NSToolbar`, standard sidebar tracking item | Unified titlebar | Semantic item placement and standard labels | Manual overflow, custom bezel, status text that looks like a button |
| Detail top accessory | NATIVE-COMPOSED | `NSSplitViewItemAccessoryViewController` hosting SwiftUI | Top of detail split item | Product title, Refresh command, busy value | Custom glass bar, duplicated window toolbar, non-menu toolbar action |
| Usage sidebar | NATIVE-COMPOSED | SwiftUI `List` with `.sidebar`, `Section`, `Label` | Leading split item | Overview and provider destinations in Rust order | Custom sidebar background, card rows, source/config entries, hard-coded icon color |
| Overview inventory | NATIVE-COMPOSED | SwiftUI `Table`/`DisclosureGroup` or the selected native outline-equivalent composition | Detail pane when Overview selected | Provider group hierarchy, canonical account rows, native columns, stable Rust IDs | Stack-painted table, placeholder dash columns on group headers, duplicate accounts |
| Provider/account detail | NATIVE-COMPOSED | SwiftUI `List`/`Form`, `Section`, `LabeledContent`, `Picker`, `Link` | Detail pane when provider selected | Rust-owned rows, account picker, provider-local error and Retry | Custom limit cards, charts, client-side value ranking/formatting |
| Empty/loading/global failure | NATIVE-COMPOSED | `ContentUnavailableView`, `ProgressView`, `Button` | Detail or popover content area | Actionable copy and exact recovery action | Modal alert for recoverable state, blank frame, layout-shifting overlay |
| Provider-local stale/error state | NATIVE-COMPOSED | `Label`, `Text`, `Button` inside native section/row | Adjacent to affected provider/account | Semantic symbol/color plus explicit text and last-good age | Color-only warning, global alert, erasing last-good values |
| Settings host | NATIVE | SwiftUI `Settings` scene/window | App menu, Command-Comma | Stable panes and window title | Custom settings panel, modal sheet, missing menu command |
| Settings content | NATIVE-COMPOSED | `Form`, `Section`, `Picker`, `Toggle`, `Slider`, `Text`, `Link` | Settings window | Presentation choices, identifiers, accepted/error state | Custom switches/sliders, silent mutation failure, provider credentials |
| Main command model | NATIVE-COMPOSED | `NSMenu`, `NSMenuItem`, responder actions | System menu bar | Standard menu order, stable disabled items, shortcuts, checked state | Hidden context-only commands, command palette replacement |
| Scrollbars and focus rings | NATIVE | System list/table/form scrolling and focus | Every scrollable/focusable region | None beyond semantic focus order | Hidden indicators, hand-drawn focus, pointer-only affordance |
| App icon | NATIVE-COMPOSED | Icon Composer `.icon` artifact | Finder, Dock when active, system UI | Layered product artwork and six system appearances | Baked blur, shadow, highlight, or glass refraction |
| C provider filter and table/detail replacement | NATIVE-COMPOSED | `Picker`, `Table`, toolbar `Button`, native content replacement | C only | Provider scope and labeled Back action | Gesture-only Back, custom segmented chrome, client-side sorting |
| D provider/account/quota columns | NATIVE-COMPOSED | `NSSplitViewController`, `NSSplitViewItem`, `List`, `Picker` | D only | Three visible columns at 900+ points; account picker below | Claimed automatic collapse, custom divider, hidden selection |
| E account inspector | NATIVE-COMPOSED | SwiftUI `inspector`, `Table`, native toolbar toggle | E only | Account detail and explicit visibility | Treating primary quota content as incidental metadata, custom overlay |
| F account source list | NATIVE-COMPOSED | `List`, `Picker`, `Form` | F provider page only | List at 860+ points; picker below | Undeclared adaptive threshold, duplicate account selection models |
| G attention queue | NATIVE-COMPOSED | `List`, `Section`, `Label` | G destination only | Rust-ranked rows and complete inventory handoff | Swift ranking, launch-blocking language, cards or banners |

## Platform and availability

| API family | Availability relative to macOS 26 floor | Rule |
|---|---|---|
| `NSStatusItem`, `NSStatusBarButton`, `NSMenu`, `NSPopover`, `NSWindow`, `NSToolbar`, `NSSplitViewController` | Predates floor | Use system behavior; no compatibility branch. |
| SwiftUI `Settings`, `List`, `Table`, `Form`, `Picker`, `ProgressView` | Predates floor | Use current macOS 26 behavior and system styling. |
| `ContentUnavailableView` | Predates floor | Owns empty and globally unavailable presentation. |
| `NSSplitViewItemAccessoryViewController` | macOS 26 | Allowed because deployment floor is macOS 26; verify scroll-edge interaction. |
| `NSGlassEffectView`, `GlassEffectContainer`, `glassEffect` | macOS 26 but not selected | Do not use while standard components satisfy the design. Any future use needs a new component contract and evidence. |
| `SMAppService` | Predates floor | Owns launch-at-login state; failure remains contextual in Settings. |
| macOS 27 APIs | Above floor / beta | Forbidden for this plan. |

## Region contracts

### Focused glance host

Region: Focused glance host
Component: `NSPopover`
Placement: anchored to the clicked `NSStatusItem.button`
Symbol: provider-owned status symbol from the current Rust-ranked item
Label: provider and canonical account identity
States: default, key, inactive, transient dismissal, loading, current, stale,
refreshing, provider error, global unavailable, empty
Material: system-provided; no custom appearance
Keyboard: explicit initial first responder, native key loop, explicit Escape
cancel, and focus return to the originating status item
Menu command: Window > Usage opens the persistent equivalent
Accessibility role: system popover/window with labeled native children
Resize behavior: fixed semantic glance size; content scrolls, window does not
become a workspace

### Usage window and split

Region: Usage window and split
Component: `NSWindow`, `NSSplitViewController`, sidebar/detail
`NSSplitViewItem`
Placement: retained unique application window
Symbol: none at window level
Label: jackin❯ desktop
States: key, main, inactive, minimized, resized, sidebar shown/hidden
Material: system-provided; no custom appearance
Keyboard: Command-W, Control-Command-S, native split/list focus traversal
Menu command: Window > Usage; View > Show/Hide Sidebar
Accessibility role: standard window with split group and labeled panes
Resize behavior: 800 × 520 minimum, 1000 × 680 typical, 1200 × 760 wide;
continuous native divider tracking

### Usage toolbar and detail accessory

Region: Usage toolbar and detail accessory
Component: `NSToolbar`, standard sidebar tracking item,
`NSSplitViewItemAccessoryViewController`, SwiftUI `Button`
Placement: unified titlebar and top of detail split item
Symbol: `sidebar.left`/system-owned sidebar item; `arrow.clockwise` for Refresh
Label: Show/Hide Sidebar; Refresh
States: default, hover, pressed, disabled, keyboard-focused, inactive window,
refresh in progress
Material: system-provided; no custom appearance
Keyboard: Control-Command-S and Command-R
Menu command: matching View-menu commands
Accessibility role: toolbar button and button with “In progress” value
Resize behavior: system toolbar overflow; Refresh remains reachable through menu

### Usage sidebar

Region: Usage sidebar
Component: SwiftUI `List` with `.sidebar`
Placement: leading split item
Symbol: `rectangle.grid.2x2` for Overview; stable provider marks for providers
Label: Overview and Rust-owned provider names
States: default, selected, hover, keyboard-focused, inactive window, disabled
only when destination is genuinely unavailable
Material: system-provided; no custom appearance
Keyboard: arrows move selection; Tab/Shift-Tab traverse panes
Menu command: navigation has no duplicate command; Usage window command opens
the current selection
Accessibility role: labeled sidebar/list and list rows
Resize behavior: 190–280 points; user width restored; may be hidden at narrow
width

### Overview inventory

Region: Overview inventory
Component: native `Table` with disclosure/group hierarchy, subject to selected
alternative
Placement: Usage detail pane
Symbol: disclosure indicators and provider marks only where native
Label: Provider, Account, Plan/Status, Remaining/Used, Reset, State
States: group, current account, stale last-good, refreshing, partial error,
missing value, selected, keyboard-focused, inactive window
Material: system-provided; no custom appearance
Keyboard: arrows navigate rows/disclosures; Return activates account detail;
Space toggles disclosure where standard
Menu command: View > Refresh; no table action lacks a menu equivalent
Accessibility role: table/outline, rows, cells, disclosure controls with one
concise row summary
Resize behavior: provider group rows span hierarchy rather than emitting empty
placeholder cells; secondary columns contract before identity/state; no overlap

### Provider/account detail

Region: Provider/account detail
Component: `List` or `Form`, `Section`, `LabeledContent`, `Picker`, `Link`
Placement: Usage detail pane
Symbol: semantic provider and state symbols
Label: selected provider, account, plan/status, quota-window labels, values,
reset, source/confidence, update age
States: current, stale, refreshing last-good, depleted, missing, error,
permission denied, offline
Material: system-provided; no custom appearance
Keyboard: native list/form/picker traversal and activation
Menu command: View > Refresh; Window > Usage
Accessibility role: native form/list rows with explicit combined values
Resize behavior: values wrap or truncate with complete accessibility text; limit
identity and state survive minimum width

### Settings

Region: Settings
Component: SwiftUI `Settings`, `Form`, `Picker`, `Toggle`, `Slider`
Placement: system Settings window
Symbol: only unambiguous SF Symbols if panes are introduced
Label: every setting's complete user-facing purpose
States: accepted, applying, disabled by dependency, operation error,
requires system approval, inactive window
Material: system-provided; no custom appearance
Keyboard: Command-Comma opens; standard control traversal and values
Menu command: App > Settings
Accessibility role: system window/form and native controls with identifiers,
labels, values, and contextual error text
Resize behavior: system Settings sizing; text must remain unclipped at 2× English

## Composition contracts

### Status-item set

Region: Provider status items
Composition: stable `NSStatusItem` controls derived from the Rust-ranked bounded
status projection, each using a template provider symbol and short text
Hierarchy and proportion: icon-only, worst-provider, pinned-provider, or a
maximum one-to-three provider strip according to the accepted setting
Spacing roles between components: system status-bar spacing only
Containers deliberately NOT added: no enclosing status pill, custom menu-bar
panel, border, or glass background

Event and focus contract: the `NSStatusBarButton` sends left- and
right-mouse-up events to one target/action. Left click toggles the anchored
popover. Right click calls native menu presentation explicitly; the
`NSStatusItem.menu` property remains unset so it cannot suppress the primary
action. On popover open the hosting view enters the key loop and the first
meaningful control becomes first responder. Escape closes through an explicit
cancel command and returns focus to the originating status item. Opening Usage
moves focus to its restored selection. F21 proves pointer, keyboard, dismissal,
and focus restoration.

### Popover shell

Region: Popover title, provider form, and footer
Composition: compact title, one focused-provider form, then Refresh/Open Usage
and canonical-account picker in a fixed footer
Hierarchy and proportion: provider/account identity first, limit windows second,
metadata third, provider error adjacent, persistent actions last
Spacing roles between components: native form sections and standard inline/footer
spacing
Containers deliberately NOT added: no nested glass cards, dashboard grid,
provider carousel, tab strip, or custom bottom bar

### Provider-local feedback

Region: stale, refreshing, partial error, permission, and offline status
Composition: semantic `Label`/`Text`, retained quota values, update age, and one
native Retry button when useful
Hierarchy and proportion: usable quota remains primary; state and recovery stay
adjacent without covering content
Spacing roles between components: native section/row spacing
Containers deliberately NOT added: no modal alert, toast queue, floating banner,
or separate error card

## CUSTOM regions — detail

None. Provider marks and the app icon are assets hosted by native system
components, not custom controls. Existing quota meters must remain native
`ProgressView` semantics or be removed; no custom interactive meter is approved.

## Decision order evidence

| Region | 1 standard | 2 background removed | 3 composition | 4 system extension point | 5 custom |
|---|---|---|---|---|---|
| Overview hierarchy | Native Table/List available | No custom background needed | Provider group plus account rows is expressible | Native disclosure/outline behavior covers hierarchy | Not chosen |
| Quota window | `LabeledContent` and `ProgressView` available | No card/background needed | Text, value, reset compose in native row | Native accessibility values and tint semantics cover state | Not chosen |
| Status item | `NSStatusItem` available | System menu bar owns background | Template symbol plus text is sufficient | Autosave identity and `NSMenu` cover product behavior | Not chosen |
| Popover | `NSPopover` available | System material is sufficient | SwiftUI Form provides content structure | AppKit anchoring/dismissal/focus behavior covers need | Not chosen |
| Settings | SwiftUI `Settings` and native controls available | System window/form own appearance | Sections express product grouping | `SMAppService` owns login-item state | Not chosen |
