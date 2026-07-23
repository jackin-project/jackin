# Architecture invariants (display-only Swift)

## Purpose

The structural contract every UI slice inherits: Rust computes, Swift
displays. Existing v1 gates (architecture tests, glass fallbacks) stay the
enforcement points (A4).
Anchors: F2 · Evidence: research/agent-usage-provider-apis/01 (FFI spine);
native/README.md; crates/jackin-usage-ffi/CLAUDE.md rules

## Requirements

### Requirement: Swift renders Rust strings verbatim
Every usage-derived label, number, percentage, pace/run-out phrase, plan
name, freshness line, and error message visible in any Desktop surface
SHALL originate in Rust (jackin-usage / jackin-usage-ffi DTOs) and be
rendered verbatim by Swift. Static navigation, action, and empty-state copy
fixed verbatim by this spec MAY remain Swift literals because it does not
derive usage information. Swift MAY split composite strings on the existing
"·" separator and apply layout/color, but SHALL NOT compute, reword,
reorder, or derive any usage value.
Covers: F2, B1 · Evidence: existing splitter + arch tests (native/Tests/JackinUsageBridgeTests/ArchitectureTests.swift; research ch. 01 Q4a)

#### Scenario: Arch test guards new surfaces
- **GIVEN** the redesigned popover and multi-item status bar
- **WHEN** `cargo xtask desktop test` runs
- **THEN** the architecture tests pass, proving no Swift-side string synthesis was added

#### Scenario: New DTO fields, same contract
- **GIVEN** new Rust outputs (run-out composite, Grok server plan label, prepaid bucket)
- **WHEN** Swift renders them
- **THEN** the strings appear exactly as the DTO carries them

### Requirement: Coarse sync FFI only
New data needs SHALL extend the existing coarse UniFFI facade (open / list /
set_enabled / refresh / next_events / snapshot / list_accounts /
set_selected_account / shutdown) rather than adding fine-grained callbacks;
DTO extensions mirror protocol views 1:1.
Covers: F2 · Evidence: crates/jackin-usage-ffi/CLAUDE.md (coarse API rule); dto.rs (ch. 01 Q5)

#### Scenario: Multi-item bar needs per-provider labels
- **GIVEN** the status bar needs one label per provider
- **WHEN** the FFI is extended
- **THEN** it exposes a coarse per-surface query (or reuses overview rows), not per-item callbacks

### Requirement: Native Liquid Glass chrome with system fallbacks
The Desktop SHALL use Swift Native UI; on supported macOS versions it SHALL
apply Liquid Glass only to navigation and control chrome while keeping
usage content on standard materials, and on macOS 14/15 or with Reduce
Transparency enabled it SHALL fall back to the existing system-material
path. Any result that cannot match Capsule design SHALL stop for operator
discussion rather than silently diverge (D7).
Covers: B2 · Evidence: item §Quality bar and D7; native/README.md "SDK + Liquid Glass contract"; native/Sources/JackinDesktop/GlassFallbacks.swift

#### Scenario: Supported macOS uses glass only for chrome
- **GIVEN** jackin❯ Desktop runs on a Liquid Glass-capable macOS release with Reduce Transparency disabled
- **WHEN** the status items, Agent Usage preview, and Usage window render
- **THEN** glass appears only on navigation/control chrome and usage content remains on standard materials

#### Scenario: Older macOS and accessibility fallback
- **GIVEN** jackin❯ Desktop runs on macOS 14/15 or Reduce Transparency is enabled
- **WHEN** the same surfaces render
- **THEN** they use the existing system-material fallback without losing content, navigation, or contrast

### Requirement: Limits-only usage presentation
Every jackin❯ Desktop usage surface and its documentation MUST show only
subscription/quota limits: remaining or used percentage, reset countdowns,
plan/status, provider-supplied limit windows, and provider-supplied quota
bounds. It MUST NOT show token unit prices, session-cost estimates,
spend-over-time charts, usage-trend sparklines, token/spend histories,
aggregate-spend donuts, or cost-legend rankings.
Covers: B4 · Evidence: repository AGENTS.md "Usage surfaces = limits only"; item §Must not; research/agent-usage-provider-apis/10-phrase-provenance-and-misc.md (forbidden reference elements)

#### Scenario: Forbidden reference content is absent
- **GIVEN** every enabled provider supplies all fields available to jackin❯
- **WHEN** the status bar, Agent Usage preview, Usage window, release copy, and user documentation are audited
- **THEN** no forbidden price, cost, spend-history, trend, token-history, donut, or ranking element or string is present

#### Scenario: Provider quota bounds remain allowed
- **GIVEN** a provider supplies a money cap, credit balance, or reset-credit count as a quota bound
- **WHEN** that bound is present in the Rust view
- **THEN** the native surface may render the bound without deriving a price, cost history, or spend trend
