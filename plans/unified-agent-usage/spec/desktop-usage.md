# Desktop usage

Covers: F15-F17; S7-S8; W5; B2; N14.

## Requirements

### Requirement: sanitized Rust bridge

Rust SHALL expose a versioned, sanitized boltffi projection filtered from the
canonical graph. Swift SHALL render typed DTO fields and invoke broker operations; it
MUST NOT discover accounts, fetch providers, derive quota semantics, or parse display
strings.

### Screen contract: status popover

The native status item and `NSPopover` SHALL support icon-only, worst-provider,
pinned-provider, and bounded-strip modes using Rust-ranked data. The popover SHALL show
the centered official `jackin❯` signature, current/stale/unavailable truth, compact
account summaries, native Refresh, and Open Usage. It SHALL anchor to the clicked
status item on every display and work while the app was inactive.

### Screen contract: Usage window

Production SHALL implement the blessed dark-only grouped Overview/provider detail
reference: native unified titlebar/sidebar/toolbar, absolute-centered official logo,
800×520 shared minimum, 1000×680 default, 1200×760 wide reference, realistic shared-
engine digital rain behind opaque content, compact sidebar meters, account-only
selection for multi-account providers, direct selection for single-account providers,
and no duplicated content title or colored leading rail.

#### Scenario: sidebar collapses

- WHEN the native sidebar collapses
- THEN content expands, system toggle remains operable, and the titlebar logo stays
  absolutely centered and visible.

### Requirement: native accessibility and state truth

System components SHALL own Liquid Glass, toolbar refresh, split behavior, focus and
window chrome. Selection, hover, keyboard focus, reduced transparency, increased
contrast, active/inactive, narrow/wide, multi-account, unavailable, stale,
informational, and secondary-display states SHALL be proven. Sidebar summaries MUST
say Stale/Unavailable instead of asserting a current percentage when appropriate.

#### Scenario: account unavailable

- THEN sidebar and detail both say `Unavailable`
- AND no unqualified current quota/reset claim is shown.

Prototype scenarios and fixtures are references only; production MUST reuse production
architecture and assets, not copy prototype stores or harnesses.

