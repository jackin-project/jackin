# Production mapping — Unified Agent Usage

Status: authoritative implementation handoff; visual blessing remains recorded only in
[SIGNOFF.md](SIGNOFF.md).

The prototype is the interaction and visual reference for jackin❯ desktop. Its
fixture records, `ProtoStore`, launch harness, and scenario menu are not production
DTOs and must not be copied. Rust remains the source of truth for stable IDs,
provider/account identity, visible strings, state/freshness, semantic quota category,
and final row order. Swift adapts layout only and never parses display strings.

AppKit owns the window, split view, toolbar, popover, status items, native chrome,
focus chain, and display-local anchoring. Authored cards and meters are opaque content
presentation, never simulated glass.

| Concept | Prototype reference | Production owner | Reuse / reimplement / never copy |
|---|---|---|---|
| Navigation destination | `SidebarSelection`, `ProtoStore.sidebarDestinations` | Rust projection plus `UsageNavigationContext` and `PresentationStore` | Reimplement typed provider/account destinations; never copy fixture selection state. |
| Multi-account grouping | `SidebarView` provider header and account rows | Rust canonical provider/account graph | Reuse hierarchy: header is taxonomy, accounts are destinations. Never make a multi-account provider selectable. |
| Single-account provider | `SidebarView.providerRow` | Rust graph plus production sidebar | Reuse direct provider destination only when exactly one account exists. |
| Account selection | `ProtoStore.navigate` normalization | `PresentationStore` | Preserve stable account key across refresh; normalize removed accounts in the model, not a view. |
| Sidebar meters | `SidebarView.sidebarMeter` | Rust-owned percentage/state; Swift geometry | Reimplement compact accessible meter; adjacent text owns meaning. Never derive percentage or state. |
| Provider identity/detail | `ProviderDetailView`, `ProviderDetailSections` | `UsageDetailPresentation` | Reuse hierarchy and responsive composition; production consumes finished Rust strings and ordered rows. |
| Quota category/order | `ProtoQuotaCategory`, `ProtoQuotaOrdering` | Rust DTO and projection | Prototype fixtures carry explicit category. Production receives final order; never parse labels or reorder in Swift. |
| Stale/unavailable | `ProtoState`, `exposesQuotaSummary`, fixture states | Rust freshness/state DTO | Reuse explicit words/symbols. Unavailable suppresses current quota claims; stale labels cached data and age. |
| Digital rain | `JackinStageBackground` | Swift authored atmosphere | Reimplement as noninteractive background; remove under Reduce Transparency and freeze/disable under Reduce Motion. Never place behind native chrome. |
| Centered titlebar brand | `ProtoShell.installCenteredBrand` | `UsageWindowController` | Reimplement through titlebar host centered on the window, surviving collapse/resize. |
| Refresh | standard `NSToolbarItem` in `UsageWindowToolbar` | production `UsageWindowToolbar` | Native AppKit command only; no custom glass button. Menu command remains equivalent. |
| Popover anchoring | `ProtoShell.togglePopover` | `StatusBarController` | Anchor to clicked `NSStatusBarButton` on its display; activate before show and make popover key. |
| Window metrics | `ProtoUsageWindowMetrics` | production `UsageWindowMetrics` | One source per target: minimum 800×520, default 1000×680, wide reference 1200×760. |
| Keyboard/accessibility | typed destination order, selected traits, focus/hover states | native list/focus model plus explicit account destinations | Reimplement semantic selection and arrow order; preserve full accessibility labels and visible focus. |
| Popover/settings harness | `ProtoShell`, `ProtoFixtures`, QA flags | none | Never copy. Production keeps its real lifecycle, broker, persistence, and settings owners. |

## Refactor sequence

1. Add Rust/FFI semantic category, final order, account destination, and sidebar
   summary state.
2. Adapt production presentation models without changing native shell ownership.
3. Reimplement shell reference behaviors: metrics, centered identity, Refresh,
   collapse, and display-local popover.
4. Reimplement Usage content and account-only navigation.
5. Add atmosphere and accessibility substitutions last.

Each step stops only after its model tests, Swift tests, runtime evidence, and
roadmap evidence named in `roadmap/unified-agent-usage/README.md` pass.
