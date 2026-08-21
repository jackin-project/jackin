# Swift Best-Practices Review — Unified Agent Usage

Status: REVIEWED — REMEDIATION REQUIRED

Review date: 2026-08-20

Mode: read-only review under `tailrocks-swift-best-practices`. Generated UniFFI
source is evidence, not a handwritten remediation target. (The FFI generator has
since migrated to boltffi 0.30.1; UniFFI references below describe the reviewed
snapshot.)

## Executive verdict

The shell has a strong product boundary: live numbers and provider policy come
from Rust; AppKit is confined to status items, windows, menus, and the native
split/toolbar; identifiers exist across the main Usage and popover paths; and
Swift 6 complete strict concurrency builds. It is not implementation-ready yet.

The root enabling condition behind the highest-risk findings is one oversized
`ObservableObject` serving simultaneously as lifecycle owner, command launcher,
FFI adapter client, settings persistence layer, navigation state, fixture
runtime, and every feature snapshot. Its public synchronous methods hide
unstructured tasks. That makes operation ownership, cancellation, ordering,
typed recovery, and focused invalidation impossible to prove locally.

## Isolation inventory

| Type family | Intended category | Current evidence | Verdict |
|---|---|---|---|
| `PresentationStore` | Main-actor interface state | `@MainActor ObservableObject` at `PresentationStore.swift:15-17` | Correct actor; observation and responsibility are too broad. |
| Projection rows and navigation values | Sendable values | Nested rows and `UsageWindowModel` conform to `Sendable` | Pass; identities need one cleanup noted below. |
| `RefreshScheduler` | Actor-isolated exclusive FFI facade | Serial queue + lock + `@unchecked Sendable` at `RefreshScheduler.swift:6-23` | Current isolation category is not encoded. Mutable invalidation/queue state requires an actor with a dedicated executor or equivalent proven owner; the raw generic closure also leaks FFI ownership. |
| AppKit controllers | Main-actor interface ownership | `@MainActor` on status bar, app delegate, split, toolbar, menus, and windows | Pass. |
| Generated FFI types | Generator-owned | Generated `@unchecked Sendable`, traps, and force operations | Exclude from handwritten lint; fix only by pin/upgrade/generator configuration and a drift gate. |

## P0 findings

### SWIFT-01 — unowned operations can mutate after shutdown or lose user ordering

Evidence:

- `stripMax`, percent/reset preferences, open, provider enable, account selection,
  refresh-floor changes, and manual/provider refresh all create unstructured tasks
  without retaining them: `PresentationStore.swift:397-433,562-643,657-699,758-813`.
- `shutdown()` cancels only polling and fixture refresh, then marks the scheduler
  invalid (`645-655`). An already-running open or mutation can return afterward
  and write `isOpen`, snapshots, preferences, or errors.
- The polling loop ignores sleep cancellation and calls `pollOnce()` once more
  before returning (`826-833`).
- The scheduler rejects queued work after invalidation, but cannot stop an
  already-running synchronous bridge call (`RefreshScheduler.swift:41-75`).
- Request/generation guards protect snapshot replacement only
  (`PresentationStore.swift:879-897`); they do not protect lifecycle flags,
  persisted settings, selection intent, or operation-specific errors.

Failure classes:

- close during cold open → stale completion reopens presentation state;
- two rapid account or format changes → task scheduling/queue admission can invert
  user intent;
- shutdown during poll sleep → one post-shutdown poll attempt and spurious error;
- a late failed mutation overwrites a newer successful operation's global error.

Implementation:

1. Replace fire-and-forget entry points with one main-actor command owner. Keep
   explicit task slots by semantic class: lifecycle, preferences, account
   selection per provider, provider enable per provider, refresh floor, manual
   refresh, polling, and fixture animation.
2. Increment a lifecycle epoch on open and shutdown. Every post-`await` mutation
   must verify its captured epoch and operation generation before applying.
3. Assign a monotonically increasing intent revision synchronously on the main
   actor before launching preference/selection work. Latest-wins cancellation and
   post-await guards protect Swift state, but cannot undo an older synchronous FFI
   setter that has already committed.
4. Enforce gesture order at admission: the bridge actor records the latest
   revision per mutation key, skips superseded queued commands before calling FFI,
   and preserves queue order for an already-running older command followed by its
   newer successor. If that cannot be proven, add a Rust-validated mutation
   revision and reject stale setters there.
5. Preserve broker-owned refresh coalescing: one UI refresh command submits one
   intent; Swift never adds provider single-flight or retry timing.
6. Make the polling loop catch cancellation or check it immediately after sleep;
   never call `pollOnce()` after cancellation.
7. Cancel every owned task on shutdown/deinit and clear it only when the same task
   generation completes.

Required tests:

- delayed open completion after shutdown cannot set `isOpen` or publish an error;
- delayed older percent/reset/account mutation cannot replace newer state;
- rapid mutations admitted in deliberately reversed task order still leave the
  final Rust projection equal to the last user gesture after the queue drains;
- cancelled polling sleep invokes zero bridge operations;
- cancellation while a synchronous bridge call is in flight drops the result but
  does not deadlock the serial queue;
- repeated refresh gestures still submit once per gesture and rely on Rust broker
  joining;
- fixture animation cancellation leaves the terminal projection unchanged.

### SWIFT-02 — bridge failure type and recovery are erased

Evidence:

- Generated `UsageBridgeError` distinguishes rejected code/message, contained
  panic, runtime unavailable, and resync required
  (`jackin_usage_ffi.swift:2995-3034`).
- Every catch funnels to `report(_:userMessage:)`, logs the dynamic error, and
  stores one generic string (`PresentationStore.swift:1179-1182`).
- The single `lastError` has no operation, affected provider/account, recovery
  action, lost-data statement, or retry eligibility.
- Settings mutations surface no inline error. With existing usage rows, the
  global error is not visible in the popover/Usage empty-state branch.
- Percent/reset `didSet` persists to `UserDefaults` before Rust accepts the value
  (`PresentationStore.swift:413-433`).

Implementation:

1. Define a handwritten `UsageOperation` and `PresentationFailure` value. Preserve
   every generated error case plus rejected code, operation, provider/account
   context, recoverability, and safe diagnostic identifier.
2. Map it to localized `errorDescription`, `failureReason`, and
   `recoverySuggestion`; attach an exact recovery command such as retry open,
   retry refresh, revert setting, or resync projection.
3. Keep provider last-good/error strings from the Rust projection as domain
   state. Do not collapse operational FFI failure into that same channel.
4. Make Rust the committed source for Rust-owned percent/reset preferences.
   Present pending state locally, persist only an accepted projection, and revert
   on rejection. Keep truly presentation-only settings in Swift.
5. Add feature-scoped error state: global lifecycle, provider/account, Settings
   row, and refresh banner. Success clears only the matching operation failure.
6. Never expose credential paths, raw tokens, or unsanitized provider responses.
7. Remove `NSLog` and raw `localizedDescription` presentation. Use unified
   `Logger` categories, deliberate privacy annotations, and the typed localized
   mapping. Log once where failure becomes actionable; tests must prove every
   user-derived diagnostic and provider/account value remains private.

Required tests:

- exhaustive mapping for every `UsageBridgeError` case and broker coordination
  code;
- each user-facing failure includes what happened, whether last-good data remains,
  and a working recovery action;
- failed preference mutation restores accepted value and does not persist the
  rejected value;
- Settings displays and clears the exact row failure;
- newer operation success does not clear an unrelated provider failure.

### SWIFT-03 — launch-at-login binding can issue a second, opposite command

Evidence:

- The `launchAtLogin` state is both `Toggle` command input and the reported
  `SMAppService` status (`SettingsView.swift:12,74-80`).
- `applyLaunchAtLogin` performs the Apple mechanism, then assigns
  `launchAtLogin = status == .enabled` (`SettingsView.swift:148-160`).
- A successful registration that enters `.requiresApproval` therefore writes
  `false` back into the same observed value. Its `onChange` can call
  `unregister()` immediately, reversing the user's request.
- The view owns the Apple mechanism directly, so it cannot distinguish hydration,
  pending intent, accepted status, approval-required status, and rollback.

Implementation:

1. Introduce a main-actor `LoginItemService` protocol that executes only the
   `SMAppService` mechanism. Inject it into a small typed Settings state owner.
2. Model reported status, requested intent, pending operation, approval required,
   and typed failure separately. A custom binding sends one intent; status
   reconciliation never recursively sends another.
3. On `.requiresApproval`, preserve the requested-on intent, show the native
   System Settings recovery path, and do not unregister.
4. Keep product policy and retry/recovery decisions out of the mechanism adapter.

Required tests: enabled, disabled, requires approval, register failure,
unregister failure, and external status change; each gesture causes exactly one
mechanism call and reconciliation causes zero.

### SWIFT-04 — the generated handle has no compile-time single owner

Evidence:

- `RefreshScheduler` owns `UsageMenuBarBridge`, but its public generic `run`
  accepts any closure over the raw handle (`RefreshScheduler.swift:19-43`).
- Generated bindings, scheduler, store, and presentation models share one module
  (`native/Package.swift:21-34`). Any handwritten file in that module can bypass
  the intended facade.
- Current source convention is good—views do not import/call FFI—but convention
  is not an architecture boundary.

Implementation:

1. Apply the target split specified in `SwiftProjectReadiness.md`.
2. Replace the GCD/lock owner with an actor isolated to a dedicated serial
   executor, then expose only a typed `package` API across the generated-bindings
   and handwritten-bridge targets: `open`, `projection`, `refresh`,
   `setEnabled`, `setSelectedAccount`, `setFormatPreferences`, `setRefreshFloor`,
   event drain, and shutdown. Remove closure-based raw access.
3. Configure SwiftPM and XcodeGen with the same package-name access boundary so
   `package` visibility compiles in both graphs; no raw generated type appears in
   a package API signature.
4. The main-actor presentation store owns this one facade. The raw generated
   handle physically remains inside the bridge actor because this synchronous ABI
   may block on Keychain/provider work; this is the explicit blocking-ABI exception
   to the literal main-actor-handle store pattern. No second facade exists, and no
   view, fixture, feature model, or AppKit controller names a generated type.
5. Prefer actor isolation over `@unchecked Sendable`. If the pinned generator
   forces an unchecked conformance at the private boundary, document the exact
   invariant and keep it generated-only.
6. Add source-graph tests that fail on any second owner/import.

Required tests:

- compile-time module dependency test;
- concurrent fake calls prove serial execution and exactly-once continuation
  completion;
- invalidation before enqueue, while queued, and while in flight;
- typed error propagation through every facade method.

### SWIFT-05 — fixture projection bypasses the bridge boundary and duplicates Rust ranking

Evidence:

- `applyQIFixture` accepts status-bar rows optionally and otherwise calls the
  Swift `selectStatusBarGlanceRows` filter/cap (`PresentationStore.swift:712-735`).
- The helper filters zero percent and caps rows in Swift
  (`PresentationHelpers.swift:17-32`), duplicating a live policy that the comments
  correctly say Rust owns.
- The canonical successor fixture packet requires exact Rust-shaped records; the
  executable F00–F14 catalog is still a declared legacy predecessor.
- `VisualQAFixtures.swift:719-727,934-942` directly constructs generated
  `UsageIdentityPresentationDto` values outside the bridge/store boundary.

Implementation:

1. Make `statusBarGlanceRows` mandatory in every fixture projection. Delete the
   Swift ranking/filter fallback and its policy test.
2. Add pure value initializers for presentation rows; generated DTO conversion
   remains inside the typed bridge target.
3. Generate or serialize canonical fixture envelopes through Rust projection
   builders, then decode the exact produced DTOs for Swift fixtures.
4. Keep Swift-only fixture controls limited to presentation state: selected
   surface/account, requested window, geometry, appearance, and animation phase.
5. Add parity tests that compare fixture IDs, provider order, accounts, row IDs,
   values, state, and status-bar membership to committed Rust-produced envelopes.

Acceptance: live and fixture modes use the same already-resolved projection
shape; Swift contains no provider ranking, filter, cap, deduplication, or quota
policy.

### SWIFT-06 — availability enforcement currently forbids the correct pattern

Evidence:

- The deployment floor is macOS 26.0.
- `ArchitectureTests.testLatestOnlySourcesHaveNoMacOSCompatibilityBranches`
  rejects every `#available(macOS` occurrence (`ArchitectureTests.swift:71-79`).
- Best-practices policy requires guards for any symbol introduced after 26.0,
  with a decided fallback and removal condition.

Implementation:

1. Replace the blanket text ban with a policy that rejects compatibility branches
   below the 26.0 floor but permits forward-only guards above it.
2. Require a nearby fallback and `Remove when minimum target reaches macOS X`
   marker for each approved guard.
3. Keep beta-only API use out of the shipping design unless the stable SDK lane
   confirms it and the guard contract is present.
4. Add an explicit ban on `UIDesignRequiresCompatibility`.

Required tests: accepted 26.1/27 guarded fixtures, rejected unguarded symbol
registry entry, rejected pre-26 compatibility branch, rejected trapping fallback,
and rejected compatibility key.

### SWIFT-13 — behavior-driving bridge strings are not exhaustively typed

Evidence:

- `BucketRow.severity`, `percentStyle`, and `resetStyle` are raw strings
  (`PresentationStore.swift:97,412-429`). The format values are persisted before
  the bridge accepts them and can contain values outside the Rust contract.
- `severityTint(_:)` treats every unknown severity as the normal phosphor color
  (`PresentationHelpers.swift:9-14`). A newly added or malformed critical state
  therefore degrades silently instead of failing closed.
- The current review requires typed failures and settings rollback, but does not
  close the broader class of behavior-driving string contracts at the Swift/Rust
  boundary.

Implementation:

1. Define small handwritten semantic enums for every behavior-driving bridge
   value consumed by Swift, beginning with severity, percent style, and reset
   style. Keep Rust wire values explicit in one exhaustive boundary mapper.
2. Keep the generated DTO strings inside the generated target and typed facade.
   SwiftUI/AppKit feature state must never switch on raw bridge strings.
3. Treat an unknown value as a typed schema/projection failure with last-good
   state preserved and a resync/update recovery. Never render it as a normal or
   healthy state.
4. Give display-only Rust-owned strings a distinct wrapper or naming convention
   so static source-graph tests can distinguish verbatim copy from semantic input.
5. Use the typed format values with the accepted-projection persistence and
   rollback contract in SWIFT-02; do not create a second preference authority.

Required tests: exhaustive known-value mapping, unknown severity failure,
unknown format-value rollback, last-good preservation, and a source-graph test
that rejects raw string switching or comparison outside the typed facade.

## P1 findings

### SWIFT-07 — observation is app-wide and derived work runs during `body`

Evidence:

- `PresentationStore` carries more than twenty `@Published` properties, so any
  object change invalidates all `@ObservedObject` readers
  (`PresentationStore.swift:340-438`).
- Sidebar and detail recreate `UsageWindowModel` during body evaluation
  (`UsageWindowRoot.swift:21-29,110-118`).
- Provider detail and popover filter detail rows during rendering
  (`ProviderDetailView.swift:50,69`; `PopoverRoot.swift:155-163`).
- Limit lines allocate an enumerated array during rendering
  (`ProviderDetailView.swift:167`; `PopoverRoot.swift:289`).
- Overview constructs its tree through a computed property read by `body`
  (`OverviewListView.swift:29-34`).
- Provider and brand asset lookup scans bundles, checks the filesystem, and
  decodes images on paths called from view bodies (`ProviderMarks.swift:15-72`;
  `JackinBrandIdentity.swift:10-34,52-55`).

Implementation:

1. Migrate new presentation state to `@MainActor @Observable`; keep infrastructure
   and task slots ignored by Observation.
2. Expose feature-scoped immutable values: status bar, popover, usage sidebar,
   overview, provider detail, Settings, and failures. A change in one must not
   invalidate every surface.
3. Build those values once when a Rust projection or native selection changes.
   Store metadata/limit row partitions and stable line IDs in the value model.
4. Keep `body` declarative: no filtering, sorting, tree construction, formatter
   creation, array enumeration, file work, or FFI.
5. Load immutable provider/brand assets once through a catalog-backed or cached
   loader. Inject the loader in tests and prove each asset decodes at most once.
6. Measure update counts and body duration in the SwiftUI instrument before and
   after; verify a background refresh that changes one provider does not rebuild
   unrelated Settings or window subtrees.

### SWIFT-08 — popover state ownership and identity are unstable

Evidence:

- The public `PopoverRoot` initializer constructs `PopoverPresentationState`, but
  the property is `@ObservedObject` (`PopoverRoot.swift:43-59`). SwiftUI may
  recreate the view value and therefore recreate sequence/scroll ownership.
- The internal status-bar path injects a long-lived state object, but the public
  initializer leaves a second ownership rule.
- Scroll-reset identity uses mutable, nonunique `accountLabel`
  (`PopoverRoot.swift:10-21,155-162,222-232`). Two canonical accounts with the
  same label fail to reset; a label edit for one account resets spuriously.
- Existing tests freeze label-based behavior at
  `PopoverPresentationTests.swift:70-94` instead of canonical identity.

Implementation:

- Prefer one rule: the status-bar controller owns and injects the presentation
  state; remove the self-creating initializer. If self-creation remains required,
  use Observation with `@State` or initialize `@StateObject` explicitly.
- Key scroll reset by `(surfaceId, accountKey, presentationSequence)` and carry
  the selected canonical account key in popover view state.
- Test that parent view reconstruction preserves presentation sequence, only a
  new popover presentation resets scroll once, and same-label/different-key
  accounts remain distinct.

### SWIFT-09 — accessibility coverage is incomplete and exceptions can mask regressions

Evidence:

- Main Usage/popover controls generally have stable identifiers.
- `SettingsView` supplies labels but no `settings.*` identifiers for display
  mode, pinned provider, max providers, percent/reset styles, privacy, login,
  provider toggles, refresh floor, or inline failure (`SettingsView.swift:15-145`).
- Status-item buttons set labels but no stable accessibility identifiers
  (`DesktopAppDelegate.swift:116-153`).
- The native sidebar toolbar button receives a label/help but no identifier
  (`UsageWindowSplitController.swift:122-135`).
- The secondary-click status menu contains Open Usage, Refresh, and Quit, but
  accessory mode hides the application menu and exposes no keyboard/VoiceOver
  route to show that context menu (`StatusItemMenuModel.swift:26-32` and
  `DesktopAppDelegate.swift:192-225`).
- Audit exceptions accept all anonymous parent/child issues, all generic groups,
  several broad missing-description/action cases, and explicit visual IDs without
  an OS/build expiry (`JackinDesktopUITests.swift:673-856`).
- Baseline execution never reached `performAccessibilityAudit` because automation
  mode timed out; no current audit result is a pass.

Implementation:

1. Commit an accessibility contract table for every interactive element with
   label, value, role, focus order, identifier, keyboard path, and menu equivalent;
   mark native-default fields explicitly rather than omitting them.
2. Add stable identifiers to every Settings control, provider toggle, status item,
   menu command, retry action, account picker, and external link.
3. Surface non-color state text and explicit values for progress/refresh controls;
   use `Idle` rather than an empty accessibility value.
4. Replace broad audit handlers with exact fingerprints containing audit type,
   element role/identifier or full system-host signature, Xcode/macOS build,
   linked evidence, and removal date. Unknown or changed fingerprints fail closed.
5. Add driven focus-order tests for popover, Usage overview/detail, Settings, and
   menus; add VoiceOver manual scripts for F00–F24 and long/localized/RTL states.
6. Add an accessible `Show Menu` action or equivalent keyboard path from each
   status item so Open Usage, Refresh, and Quit remain keyboard/VoiceOver
   reachable while the app is an accessory.
7. Repair deterministic UI automation and rerun the four supported macOS audit
   types on the real host under default, Increase Contrast, and Reduce
   Transparency.

Current interactive accessibility inventory:

| Interactive family | Label | Value | Role | Focus order | Identifier | Keyboard/menu path |
|---|---|---|---|---|---|---|
| Provider and fallback status buttons | Present from Rust/fallback copy | Missing explicit semantic value | Native button | System menu-bar order; not driven | Missing | Primary activation only; context menu has no proven keyboard/VoiceOver open path |
| Status context-menu Open Usage / Refresh / Quit | Present | Not applicable | Native menu item | System menu order after opening | Missing | `r`/`q` exist inside the pointer-opened menu; entry path incomplete |
| Popover global Retry | Present | Not applicable | Native button | Not driven | Present | Native button focus/activation |
| Popover provider Retry | Present | Not applicable | Native button | Not driven | Present | Native button focus/activation |
| Popover Refresh | Present | Refreshing or idle state needs explicit value | Native button | Not driven | Present | Command-R and context-menu Refresh |
| Popover Open Usage | Present | Not applicable | Native button | Not driven | Present | Default action and context-menu Open Usage |
| Popover account picker | Present | Native selected account | Native pop-up button | Not driven | Present | Native picker keyboard behavior |
| Usage sidebar Overview/provider rows | Present | Selection native; no explicit value required | Native list rows | Visual order; not driven | Present | Native list navigation |
| Overview provider/account rows | Present composite labels | Native cell values | Native table rows | Tree/table order; not driven | Present | Native table navigation |
| Usage account picker | Present | Native selected account | Native pop-up button | Not driven | Present | Native picker keyboard behavior |
| Usage global/provider Retry | Present | Not applicable | Native button | Not driven | Present | Native button focus/activation |
| Usage provider-page link | Present | URL destination implicit | Native link | Not driven | Present | Native link activation |
| Usage Refresh | Present | Empty when idle; incomplete | Native button | Not driven | Present | Command-R and View → Refresh |
| Sidebar toolbar toggle | Present and stateful | Show/Hide in label | Native toolbar button | Native toolbar order; not driven | Missing | Control-Command-S and View menu |
| Settings display/pinned/max/percent/reset controls | Present | Native selected value | Native pickers | Not driven | Missing | Native control behavior; no per-control menu command required |
| Settings privacy/login/provider toggles | Present | Native on/off | Native switches | Not driven | Missing | Native control behavior |
| Settings refresh-floor slider | Present | Minutes announced by companion text; explicit slider value unverified | Native slider | Not driven | Missing | Native slider keyboard behavior |
| Application menu commands | Present | Not applicable | Native menu items | System menu order | Mostly missing | Standard shortcuts exist for Settings, Close, Sidebar, Refresh, Usage, Quit |

“Native” is not shorthand for verified. Final evidence must record the actual role
and value exposed by XCTest/Accessibility Inspector, drive focus order, and turn
every Missing or Not driven cell into Present or a justified not-applicable case.

### SWIFT-10 — AppKit bridges need explicit capability/lifecycle contracts

Evidence:

- `NSStatusItem`/secondary-click routing, real `NSPopover`, responder-chain
  integration, and the native split/toolbar accessory have named AppKit capability
  needs. Whole-app Settings, command, and retained-window ownership exceed that
  narrow boundary until each has a stable-SwiftUI gap or migrates.
- Status-item event monitors are retained and removed during item teardown
  (`DesktopAppDelegate.swift:196-218,345-359`).
- The split controller and toolbar state the native shape but not the specific
  stable-SwiftUI capability gap and replacement condition
  (`UsageWindowSplitController.swift:8-35,74-149`).
- The KVO callback creates an unowned main-actor task
  (`UsageWindowSplitController.swift:83-91`).
- `StatusItemMenuRouter` is public and stores UI/store/`NSApp` closures without
  actor isolation (`StatusItemMenuModel.swift:35-72`).
- The executable entry point relies on `MainActor.assumeIsolated` around the
  entire AppKit lifecycle without stating the startup invariant
  (`JackinDesktopApp.swift:11-22`).

Implementation:

1. At each AppKit type, name the capability SwiftUI 26 lacks and the condition
   under which the bridge is removed.
2. Record lifecycle explicitly: controller owner, observation/event-monitor
   creation, identity lifetime, and teardown.
3. Own/cancel the KVO-hop task or replace it with a synchronous main-actor-safe
   delivery mechanism whose queue invariant is documented.
4. Keep all inputs typed and all outputs callbacks/actions; no shared mutable
   reference or business logic enters an AppKit controller.
5. Make the router internal and `@MainActor`, including its callbacks. Use a
   supported main-actor entry-point shape or document and test the AppKit-main-
   thread invariant instead of an unchecked isolation assumption.
6. Keep `NSStatusItem`/`NSPopover` and required split-accessory bridges. Move
   Settings, commands, and retained window ownership toward SwiftUI scenes unless
   a named stable-SwiftUI capability gap justifies each AppKit owner.

### SWIFT-11 — stable selection and row identity have latent stale/collision cases

Evidence: `BucketRow.id` equals its label (`PresentationStore.swift:89-91`). A
provider can expose two windows with the same localized label, and a label can
change across projection/localization. The current primary views use Rust
`UsageDetailRow.rowId`, so this is latent rather than observed.

`reconcileSelections()` validates Usage and popover selection but never validates
`overviewSelectionID` (`PresentationStore.swift:1139-1159`). Removing a selected
row can leave the native table bound to a nonexistent ID.

Implementation: carry a Rust-owned stable bucket/window ID through the FFI boundary and
remove label identity. Clear an overview selection only when its canonical
provider/account ID disappears; preserve it across reorder and refresh. Add
duplicate-label, label-change, removal, and reorder tests proving state,
selection, and animation continuity.

### SWIFT-12 — native chrome and recovery copy have no localization system

Evidence:

- Settings labels, descriptions, status notes, and errors are hard-coded in
  `SettingsView.swift:15-159`.
- Application menus and window titles are hard-coded in
  `AppMainMenu.swift:43-283`.
- Popover empty/error/action/accessibility chrome is hard-coded in
  `PopoverRoot.swift:117-143,207-217,318-371`.
- No String Catalog ownership or localization test is recorded for these Swift
  presentation strings.

Implementation:

1. Add a String Catalog for Swift-owned native chrome, accessibility wording,
   recovery explanations, menu commands, and Settings copy. Use typed localization
   keys at feature boundaries and locale-aware `FormatStyle` for Swift-owned dates,
   numbers, and Apple mechanism status.
2. Preserve the project override that Rust owns finished provider usage labels,
   quota formatting, countdowns, stale markers, plan/status copy, and money-cap
   units. Swift must not relocalize, split, join, or reinterpret those strings.
3. Add pseudo-localized, German, CJK, and right-to-left tests/captures, including
   menu key equivalents, VoiceOver announcements, long Settings labels, and the
   800 × 520 minimum Usage window.
4. Add a source gate that rejects new user-facing Swift literals outside the
   catalog or an explicitly reviewed fixture/test scope.

Acceptance: all Swift-owned user-facing and accessibility copy resolves from the
catalog in production; Rust-owned usage strings remain byte-identical across CLI,
TUI, Capsule, FFI, and desktop fixtures.

### SWIFT-14 — Settings uses a rigid frame instead of adaptive native sizing

Evidence: `SettingsView` forces a 420 × 640 frame
(`SettingsView.swift:134`). This makes the form insensitive to 2× text expansion,
long localization, right-to-left layout, display scaling, and future system
control metrics. The localization finding asks for long-string captures but does
not define a sizing remediation or no-clipping criterion.

Implementation:

1. Remove the fixed width and height. Give the Settings scene a tested minimum
   size plus content-driven ideal size, and let the native window remain user
   resizable where the scene API supports it.
2. Put only the form content that can legitimately exceed the available height
   in a system scroll container; do not scale, truncate, or compress labels to
   preserve the old dimensions.
3. Persist window geometry through the scene/window system, clamp restored
   geometry to the current display, and do not let geometry become business or
   bridge state.
4. Verify standard and 2× expanded text, pseudo-localized, German, CJK,
   right-to-left, and supported display-scaling layouts at minimum and ideal
   sizes. All labels, recovery text, values, toggles, and focus rings must remain
   visible or reachable by scrolling.

Acceptance: no fixed two-axis Settings frame remains; the window opens at a
native useful ideal size, honors its tested minimum, restores valid geometry,
and has zero clipped or unreachable content across the required layout matrix.

## Existing strengths to preserve

- Rust owns provider detection, account deduplication, quota semantics, refresh
  policy, cache, ordering, and finished usage strings.
- The production shell does not issue provider HTTP/OAuth/CLI probes.
- Snapshot request/generation guards already reject stale projection replacement.
- Selection identities use provider surface IDs and canonical account keys.
- Running views use system lists, tables, forms, pickers, links, menus, toolbar,
  split view, status items, and popovers; no custom blur/material renderer exists.
- Main toolbar Refresh has command/menu parity, and sidebar behavior routes through
  the responder chain.
- AppKit ownership is explicit and event-monitor teardown exists.
- Generated source is excluded from handwritten format/lint policy.

## Generated-code policy

Do not hand-edit `jackin_usage_ffi.swift`. Its force operations, generated traps,
and `@unchecked Sendable` conformances are owned by the pinned FFI generator. The
implementation plan must:

1. keep generated code in its own target;
2. regenerate under the exact CLI/crate pin;
3. enforce drift in CI;
4. audit generator release notes before upgrades;
5. exercise cancellation/error behavior at the handwritten facade;
6. replace the FFI generator only in a dedicated migration if generator defects
   block the required invariants.

## Verification gate

The Swift remediation is complete only when all hold:

- Swift 6 complete strict-concurrency build has no suppressed handwritten
  diagnostics.
- Every unstructured task has a retained owner, cancellation path, lifecycle,
  post-await freshness guard, and cancellation test.
- Rapid mutation tests prove the final Rust projection matches the last user
  gesture after all synchronous bridge work drains.
- Exactly one facade owns generated FFI; source-graph tests reject bypasses.
- Operational errors stay typed through presentation and expose exact recovery.
- Every behavior-driving bridge value is exhaustively typed; unknown values fail
  closed while preserving last-good state.
- No Rust-owned preference persists before acceptance.
- Feature-scoped observable state replaces app-wide invalidation.
- No sorting, filtering, tree construction, array allocation, formatter work, or
  FFI occurs in `body`.
- Every interactive element has recorded label, value, role, focus order,
  identifier, keyboard path, and menu parity.
- Accessibility exceptions are fingerprinted, versioned, and fail closed.
- Swift-owned chrome, recovery, and accessibility copy resolves from the String
  Catalog while Rust-owned usage strings remain unchanged.
- Settings uses adaptive native sizing and passes the no-clipping layout matrix.
- Failure-path, cancellation, ordering, duplicate-identity, settings-revert,
  focus, UI, and accessibility tests pass.
- Final running-app visual QA has no hard failure.

## Review verdict

REMEDIATION REQUIRED. The architecture is directionally sound, but task lifetime,
typed recovery, compile-time FFI ownership, fixture parity, availability policy,
observation scope, render purity, localization, and accessibility proof must be
closed before the implementation can claim Swift best-practices compliance.
