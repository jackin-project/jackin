# Anti-Reference Corpus — Unified Agent Usage

Status: SELECTION RECORDED — A without H, 2026-08-20

This corpus records rejected states, why they failed, the correction, and the
rule that must survive implementation. Human selection on 2026-08-20 (Alexey
Zhokhov) chose alternative A without H; the unselected eligible directions B,
G, and H are appended below with the recorded human decision rationale.

## Rejected structural directions

| Anti-reference | Status | Why rejected | Required correction | Learned rule |
|---|---|---|---|---|
| Alternative C — canonical-account table first | Rejected and ineligible this round | Removes the settled persistent provider sidebar, makes provider order a filter/sort concern, and replaces the retained two-pane hierarchy at minimum width. | Retain Overview plus provider destinations in the sidebar; keep canonical accounts subordinate to providers and keep Rust ordering immutable. | A dense table is not automatically clearer when it erases the product object hierarchy. |
| Alternative D — three-column drilldown | Rejected and ineligible this round | Adds a third permanent region for a seven-provider inventory, fails the 760-point envelope, and changes account navigation into a different control below 900 points. | Keep one sidebar and one detail region; use a native account picker inside provider detail when needed. | Do not add a split merely to keep every relationship simultaneously visible. |
| Alternative E — native inspector | Rejected and ineligible this round | Treats primary quota windows as incidental metadata, adds a third region, and permits narrow-width overlay over the comparison surface. | Keep quota windows in primary detail content reached by stable provider/account selection. | Inspectors are for secondary properties, not the main reason the window exists. |
| Alternative F — provider workspace with nested account source list | Rejected and ineligible this round | Creates a split inside a split, wastes space for single-account providers, and introduces two layouts plus a second account-selection model. | Keep one canonical account-selection owner and one provider-detail composition across window sizes. | Responsive substitution must not duplicate navigation state or create two interaction models. |
| Alternative B — hierarchical navigation sidebar | Rejected by human selection 2026-08-20 (A without H) | Moves too many long canonical account labels into navigation, crowding the sidebar and weakening all-provider overview scanning. | Keep the sidebar to Overview plus provider destinations; canonical accounts live in the grouped Overview table and the provider-detail account picker. | Navigation structure must not absorb content identity that belongs in the content region. |
| Alternative G — attention queue plus complete inventory | Rejected by human selection 2026-08-20 (A without H) | Adds an urgency destination with no baseline evidence that reaching depleted or stale rows is slow; duplicates Overview rows and adds a navigation concept. | Surface urgency through the settled Rust-ranked provider-focused status items and explicit row states inside the grouped Overview. | Do not add a destination for a bottleneck the running baseline has not demonstrated. |
| Alternative H — compact focused popover, deep window handoff | Rejected by human selection 2026-08-20 (A without H) | Removes complete quota-window detail from a popover whose baseline hierarchy already passed visual review, hiding multi-window data from the glance. | Retain the complete popover: identity, full quota windows, explicit state, account picker, and Refresh/Open Usage footer. | Do not shrink a surface that already passes review to satisfy an unproven scope rule. |

## Rejected incumbent states

| Anti-reference | Status | Why rejected | Required correction | Learned rule |
|---|---|---|---|---|
| Increased Contrast overview collapse | Hard failure in the legacy running baseline | Provider labels, account values, plan, percentage, and reset text concatenate because provider/account identity lacks protective width behavior and provider group rows populate account-only columns with placeholders. | Group rows span hierarchy; identity/state survive before plan/reset; minimum-width and Increased Contrast fixtures prove zero overlap. | Accessibility appearance changes are layout inputs, not a cosmetic afterthought. |
| Placeholder-filled provider rows | Rejected legacy hierarchy | Repeated em dashes make provider groups resemble broken account records and compete with real values. | Use native group/disclosure semantics and leave account-only columns structurally absent on provider rows. | Missing data and non-applicable structure are different states; do not render both as placeholder noise. |
| Early account-label wrapping | Rejected legacy contraction order | Canonical account identity wraps while secondary plan/reset columns retain unnecessary width. | Contract optional metadata before provider/account identity and explicit state; expose complete accessibility text. | Protect object identity before descriptive metadata. |

## Rejected generic Mac directions

| Anti-reference | Status | Why rejected | Required correction | Learned rule |
|---|---|---|---|---|
| Card-grid usage dashboard | Rejected | Flattens provider/account/window hierarchy, adds equal visual weight, and encourages trend/spend decoration forbidden by the limits-only contract. | Native list/table hierarchy for inventory and native form/list detail for quota windows. | Monitoring work needs selection and comparison structure, not a wall of metric cards. |
| Custom-painted glass, blur, pills, window chrome, or sidebar | Rejected | Duplicates system-owned macOS 26 material and forfeits automatic contrast, transparency, focus, metric, and future-platform behavior. | Standard AppKit/SwiftUI structure and controls; no custom material while native components satisfy the job. | Do not draw what the operating system owns. |
| Fixed-canvas desktop layout | Rejected | Pretends the Mac window cannot resize and hides failures at the 800 × 520 minimum, long text, display scaling, and toolbar overflow. | Continuous native resizing across minimum, typical, and wide sizes with stable focus and selection. | A Mac design is a behavior envelope, not one screenshot size. |

## Evidence

- [Structural alternatives](Alternatives.md) — complete direction descriptions,
  strengths, risks, and eligibility.
- [Legacy baseline visual QA](BaselineVisualQA.md) — running-app failure evidence.
- [Experience brief](ExperienceBrief.md) — archetype, hierarchy, density, and
  out-of-scope boundaries.
- [Native component map](NativeComponentMap.md) — system-owned replacements and
  forbidden customizations.
- [Apple-native research](../../../research/agent-usage-platform/02-apple-native-design.md)
  — primary-source component and material constraints.
