# Structural Alternatives — Unified Agent Usage

Status: SELECTED — A without H, 2026-08-20, Alexey Zhokhov

All alternatives preserve the settled provider-focused status items, native
`NSPopover`, one retained resizable Usage window, Settings, Rust-owned quota
semantics, canonical account identity, and one broker freshness authority. They
differ in information hierarchy and navigation, not color, glass, radius, or
spacing.

Every preview below uses exact records from [Fixtures.md](Fixtures.md); every
OpenAI multi-account example uses F03 without inventing a tuple.
[AntiReferences.md](AntiReferences.md) records every already-rejected direction
and incumbent failure as a reason/correction/learned-rule entry; pending eligible
directions are added only after human selection.
Alternatives A, B, and G preserve the settled two-pane structure and are
eligible for selection. C, D, E, and F are documented counter-directions and
permanently ineligible in this design round because they remove or add a region
to that structure. H is a popover remix paired with an eligible window
alternative. A design that works only for the normal fixture is ineligible at
prototype review.

## A — Grouped Overview, Provider Detail

```
┌──────────────┬─────────────────────────────────────────────────────┐
│ Overview     │ Provider       Account       Plan   Left  Reset     │
│              │ ▾ OpenAI                                         │
│ Providers    │   personal…    Plus          57%    3d             │
│   OpenAI     │   team…        Plus           0%    3d             │
│   Anthropic  │ ▸ Anthropic                                      │
│   Amp        │                                                     │
│   …          │                                                     │
└──────────────┴─────────────────────────────────────────────────────┘
```

Structure: Overview and providers stay in the native sidebar. Overview uses
provider group rows with canonical account children. Selecting a provider opens
one provider page with a native account picker and all quota windows.

Strengths: least structural churn; immediate all-provider scan; accounts remain
visibly subordinate to providers; provider detail remains calm; strong parity
with CLI and console grouping.

Risks: group rows must span hierarchy instead of rendering placeholder dashes;
table contraction must protect state and identity; two routes to provider detail
must retain one selection.

Minimum-width behavior: hide sidebar on request; group labels span the table;
secondary plan/reset text contracts before account and state.

## B — Hierarchical Navigation Sidebar

```
┌────────────────────┬───────────────────────────────────────────────┐
│ Overview           │ OpenAI · team@example.test                    │
│ ▾ OpenAI           │ Plus                                          │
│   personal@example │ Limits                                        │
│   team@example     │ Weekly       0% left      Resets in 3d        │
│   organization@…   │ Status · Depleted                              │
│ ▸ Amp              │ Status · Updated now                          │
└────────────────────┴───────────────────────────────────────────────┘
```

Structure: provider and canonical account hierarchy moves entirely into the
sidebar. Overview remains the first destination; selecting an account opens its
quota detail directly.

Strengths: one navigation location; no account picker duplication; exact account
context stays visible while inspecting details; keyboard drill-down is direct.

Risks: sidebar becomes dense with long account labels; provider-only state needs
a stable destination; many accounts can crowd navigation; hierarchy duplicates
console list/detail but may reduce desktop overview scanability.

Minimum-width behavior: sidebar remains user-hideable; account labels truncate
with complete accessibility values; selection persists when hidden.

## C — Canonical Account Table First

Eligibility: ineligible counter-direction for this design round.

```
┌────────────────────────────────────────────────────────────────────┐
│ [Overview] [Provider: All ▾]                         Refresh       │
│ Provider     Account             Plan      Left    Reset    State  │
│ OpenAI       personal@…          Plus      57%     3d       Current│
│ OpenAI       team@…              Plus       0%     3d       Depleted│
│ Anthropic    personal@…          Pro       12%     1h       Low    │
├────────────────────────────────────────────────────────────────────┤
│ Selected account limit windows                                    │
└────────────────────────────────────────────────────────────────────┘
```

Structure: no persistent provider sidebar in Overview. One sortable/filterable
native Table owns the window; selection reveals quota windows in a lower detail
region. Provider scope is a toolbar pop-up. Provider-specific pages disappear.

Strengths: densest cross-provider comparison; one row per canonical account is
obvious; works well with a large account inventory; fewer nested levels.

Risks: conflicts with the settled two-pane structure and is ineligible in this
round; lower detail competes with vertical space;
sorting can obscure settled provider order; provider-local errors need clear
table placement.

Minimum-width behavior: at 760–899 points the selected detail replaces the
table after explicit activation and a labeled Back toolbar command returns to
the table; at 900 points and above both regions remain visible. No horizontal
scrolling is required for the primary job. It documents the trade-off but is not
part of the selection ballot.

## D — Three-Column Drilldown

Eligibility: ineligible counter-direction for this design round.

```
┌───────────┬──────────────────┬─────────────────────────────────────┐
│ Providers │ Accounts         │ Quota windows                       │
│ OpenAI    │ personal@…       │ Weekly      57% left   3d           │
│ Anthropic │ personal@…       │ Weekly      12% left   1h           │
│ Amp       │ default          │ Reset       18h                     │
└───────────┴──────────────────┴─────────────────────────────────────┘
```

Structure: provider, account, and quota windows each receive a native split
column. Overview becomes an optional first sidebar destination or a separate
table mode.

Strengths: object relationships are continuously visible; excellent for many
accounts; no account picker; direct keyboard column traversal.

Risks: excessive structure for seven providers and usually few accounts; poor
760-point fit; third column can look like an inspector even though quota windows
are primary content; added divider/restoration complexity.

Minimum-width behavior: below 900 points the account column is replaced by a
native account pop-up in the quota column; at 900 points and above all three
columns remain visible. This creates two intentional layouts and no claim of
automatic system collapse.

## E — Overview Table with Native Inspector

Eligibility: ineligible counter-direction for this design round.

```
┌──────────────────────────────────────┬─────────────────────────────┐
│ Provider / canonical account table   │ Account inspector           │
│ ▾ OpenAI                             │ team@example.test           │
│   personal@…      57%     3d         │ Plan · Plus                 │
│   team@…           0%     3d         │ Weekly · 0% · 3d            │
│ ▸ Anthropic                          │ Status · Depleted           │
└──────────────────────────────────────┴─────────────────────────────┘
```

Structure: Overview stays dominant. Selecting an account updates a trailing
native inspector containing all limit windows and metadata. Provider pages are
optional and account selection remains in the table.

Strengths: compare without leaving Overview; selected row and detail remain
visible; inspector can be toggled and restored; supports multi-selection in a
future nonediting workflow.

Risks: inspector is semantically questionable for primary quota content; system
overlay behavior at narrow widths can obscure the table; adds a third region to
the settled two-pane window; no benefit when only one account exists.

Minimum-width behavior: acceptance is measured with inspector closed. Opening
it at narrow width may overlay by system behavior and must not be called
automatic collapse.

## F — Provider Workspace with Account Source List

Eligibility: ineligible counter-direction for this design round.

```
┌──────────────┬─────────────────────────────────────────────────────┐
│ Overview     │ OpenAI                                              │
│ OpenAI       │ Accounts                 Selected quota windows     │
│ Anthropic    │ personal@…   57%         Selected · team@…          │
│ Amp          │ team@…        0%         Weekly       0% · 3d       │
│ …            │ organization… 88%         Status       Depleted      │
└──────────────┴─────────────────────────────────────────────────────┘
```

Structure: sidebar remains provider-only. Each provider page uses a compact
native account source list beside a quota detail region; Overview remains a
separate grouped table.

Strengths: optimized for multi-account providers; account selection is more
scannable than a pop-up; provider context remains stable; quota detail gains
space.

Risks: creates a split inside a split; single-account providers waste space;
window hierarchy becomes inconsistent between Overview and provider pages;
selection restoration gains another dimension.

Minimum-width behavior: below 860 points the account source list changes to a
native pop-up; at 860 points and above it remains visible. Both intentional
layouts require separate evidence and stable focus/selection restoration.

## G — Attention Queue plus Complete Inventory

```
┌──────────────┬─────────────────────────────────────────────────────┐
│ Attention    │ Needs attention                                    │
│ Overview     │ Anthropic · personal@… · 12% left · resets in 1h   │
│ Providers    │ Kimi · cached · offline                            │
│   OpenAI     ├─────────────────────────────────────────────────────┤
│   …          │ Complete inventory                                 │
└──────────────┴─────────────────────────────────────────────────────┘
```

Structure: a stable Attention destination shows Rust-ranked depleted, stale, or
failed accounts before the complete grouped Overview. Provider pages remain.

Strengths: fastest route to actionable risk; status-item worst-provider mode and
window detail share one urgency model; no client-side ranking if Rust projects
the queue.

Risks: adds another navigation concept; can duplicate rows from Overview;
“attention” must not imply launch blocking or client-owned policy; calm healthy
states gain no benefit.

Minimum-width behavior: queue uses the same row contract as Overview and never
becomes cards or banners.

## H — Compact Focused Popover, Deep Window Handoff

```
Popover                             Usage window
┌─────────────────────────┐         ┌────────┬──────────────────────┐
│ OpenAI · team@…      ▾  │         │Overview│ OpenAI · team@…      │
│ 0% left · resets in 3d  │ Open →  │OpenAI  │ all quota windows    │
│ Depleted/current state  │         │…       │ metadata and errors  │
│ Refresh   Open Usage    │         └────────┴──────────────────────┘
└─────────────────────────┘
```

Structure: the popover deliberately removes secondary metadata and shows only
identity, one Rust-ranked quota summary, explicit state, account picker, and
handoff actions. The retained Usage window uses Alternative A, B, or G for full
detail.

Strengths: strongest adherence to popover scope; fastest glance; reduced
scrolling and keyboard path; complete data remains one action away with exact
provider/account handoff.

Risks: hides multi-window quota detail from the glance; operators may expect the
current popover's complete provider form; selected desktop-window alternative
must be recorded with this one because H is a popover structure, not a complete
window direction.

Minimum-width behavior: not applicable to fixed transient width; 2× strings and
all states must still fit or scroll without moving the footer.

## Incumbent implementation evidence

The running prototype in this repository is real code, and its structure is
recorded here as evidence, not as an approval shortcut:

- `native/Sources/JackinDesktop/UsageWindow/UsageWindowSplitController.swift` —
  `NSSplitViewController` with a 190–280-point sidebar item and detail item.
- `native/Sources/JackinDesktop/UsageWindow/OverviewListView.swift` — native
  `Table` of provider group rows with canonical account children.
- `native/Sources/JackinDesktop/UsageWindow/ProviderDetailView.swift` — native
  `List`/`Section`/`LabeledContent` provider detail with a menu-style account
  `Picker` shown only for multi-account providers.

The incumbent therefore implements alternative A's skeleton: sidebar Overview
plus providers, grouped Overview table, provider detail with account picker.
[BaselineVisualQA.md](BaselineVisualQA.md) records its observed defects
(Increase Contrast collapse, placeholder-filled provider rows, early
account-label wrapping) as the incumbent-failure entries in
[AntiReferences.md](AntiReferences.md). Selecting A means fixing those defects
on the proven structure, not adopting the incumbent as-is. No baseline defect
found so far is structural; none justifies C, D, E, or F.

## Selection record

Human selection: **A without H** — Alexey Zhokhov, 2026-08-20.

Eligible ballot: A, B, or G as the Usage-window hierarchy, optionally remixed
with H for the popover. C, D, E, and F document rejected structural trade-offs
and cannot be selected in this round.

Evidence-led recommendation: A without H. It directly removes the observed
provider-row placeholders and protects account/state columns while preserving
the incumbent sidebar, overview, provider detail, and complete popover. B moves
too many long account labels into navigation; G introduces an urgency
destination without baseline evidence that navigation to depleted/stale rows is
slow; H would remove useful complete quota-window detail from a popover whose
baseline hierarchy already passed visual review. The human selection adopted
this recommendation.

Remix inputs:

- Primary hierarchy: A — Overview and providers in the native sidebar; grouped
  Overview table with canonical account children; provider detail with a native
  account picker shown only for multi-account providers.
- Toolbar/accessory model: retained native `NSToolbar` with the standard
  sidebar tracking item and the detail top accessory hosting product title and
  Refresh, as mapped in [NativeComponentMap.md](NativeComponentMap.md); no
  selection-specific accessory change.
- Minimum-width behavior: A's declared behavior — sidebar hideable on request;
  group labels span the table; secondary plan/reset text contracts before
  account identity and explicit state.
- Popover structure: retained complete popover (identity, full quota windows,
  state, account picker, Refresh/Open Usage footer). H is not remixed.

Winner rationale: A is the structure the running incumbent prototype already
proves buildable; every baseline defect recorded against it is a
row/composition failure, not a structural one, so the fix is targeted rather
than a re-architecture. A removes the observed provider-row placeholder defect
and protects account/state columns while preserving the incumbent sidebar,
Overview, provider detail, and complete popover.

Loser rationale: B moves too many long canonical account labels into
navigation, crowding the sidebar. G adds an attention-queue destination with no
baseline evidence that reaching depleted or stale rows is slow. H removes
complete quota-window detail from a popover whose baseline hierarchy already
passed visual review.

Remaining risks accepted: A's declared risks stand — group rows must span the
hierarchy instead of rendering placeholder dashes; table contraction must
protect state and identity before plan/reset; the two routes to provider detail
must retain one selection. These are tracked as the rejected-incumbent-state
corrections in [AntiReferences.md](AntiReferences.md).
