# Status bar (menu bar items + context menu)

## Purpose

The always-visible glance: one menu bar item per enabled provider, selected
account glance % (weekly for six providers; Amp Free daily for Amp), plus
the right-click context menu that is the app's action home. Replaces v1's
single-item modes with per-provider items.
Anchors: S1, S2, S3, S4, F1 · Evidence:
research/agent-usage-provider-apis/01 (overview/compact labels), item
§Screens/"macOS status bar item"

## Requirements

### Requirement: One item per enabled provider
The app SHALL render one status bar item per auto-detected enabled provider,
each showing the provider's template (monochrome) icon plus one percentage:
weekly-limit % left for Codex, Claude, Grok, z.ai, Kimi, and MiniMax; the
server-reported Amp Free daily % left for Amp. It SHALL show no other
number, no stacked dual percentages, and no severity color in the bar.
Covers: S1, F1 · Evidence: item §Decisions D4/D8/D12/D15; research ch. 01,
11 (Amp daily)

#### Scenario: Three enabled providers
- **GIVEN** Codex, Claude, z.ai are enabled with weekly buckets at 57/74/31% left
- **WHEN** the menu bar renders
- **THEN** three items show "⊙ 57%", "✳ 74%", "Z 31%" style icon+percent, monochrome

#### Scenario: Account switch reflects in bar
- **GIVEN** Codex has two accounts and the operator selects the second in the popover
- **WHEN** the selection lands (existing `set_selected_account` FFI)
- **THEN** the Codex bar % changes to the second account's weekly % without restart

#### Scenario: Amp uses daily, never a false weekly label
- **GIVEN** Amp Free reports 61% remaining today with a daily cadence
- **WHEN** the menu bar renders
- **THEN** the Amp item shows its icon and `61%`
- **AND** Rust selected the semantic Daily bucket; no monthly balance,
  individual credit, or workspace credit was relabeled as weekly/daily

### Requirement: Degradation display in the bar
A provider item SHALL never disappear while the provider is enabled: on
stale/error the last-known % renders dimmed; before any successful fetch the
item SHALL show "–" in place of the percentage. It SHALL also show "–" when
a successful provider response lacks that provider's required glance window
(for example a paid-only Amp response with balances but no Amp Free daily
line); detail surfaces still show the returned bounds.
Covers: S2, S3 · Evidence: item §Screens states (decided 2026-07-24); B5

#### Scenario: Fetch fails after success
- **GIVEN** Grok showed 48% and the next refresh errors
- **WHEN** the bar re-renders
- **THEN** "48%" persists, visually dimmed, and the item stays in place

#### Scenario: Never fetched
- **GIVEN** a provider enabled this launch with no completed fetch
- **WHEN** the bar renders
- **THEN** its item shows the icon and "–"

#### Scenario: Amp response has balances but no daily line
- **GIVEN** Amp is enabled and returned individual credits but no Amp Free
  daily quota
- **WHEN** the bar renders
- **THEN** the Amp item remains present with `–`, while its detail tab keeps
  the returned credit balance

### Requirement: Item interactions
Left-click on a provider item SHALL toggle the popover opened on that
provider's tab; right-click SHALL open a context menu with exactly three
rows: Open Usage Window, Refresh, Quit.
Covers: S1, S4 · Evidence: item §Decisions D13; §Screens interactions

#### Scenario: Left-click focuses provider
- **WHEN** the operator left-clicks the Claude item
- **THEN** the popover opens with the Claude tab selected

#### Scenario: Right-click menu
- **WHEN** the operator right-clicks any provider item
- **THEN** a menu shows Open Usage Window, Refresh, Quit — nothing else

## Screen: macOS status bar item (S1–S3)

Mockup: roadmap item §Screens/"macOS status bar item" — layout intent.

- **Regions**: per-provider item = template icon + percent text.
- **States**: default (icon + weekly % left, or Amp Free daily % left) |
  stale/error (dimmed last-known, item persists) | never-fetched or
  required-glance-window unavailable ("–"). All drawn in the item.
- **Interactions**: left-click → popover on provider tab (→ "Item
  interactions"); right-click → context menu (→ "Item interactions").
- **Navigation**: app entry point; exits to popover or context menu.

## Screen: Status item context menu (S4)

Mockup: item §Decisions "Usage window entry" (three rows; specified here).

- **Regions**: menu rows top-to-bottom: Open Usage Window · Refresh · Quit.
- **States**: default only (menu is static; Refresh uses the same forced
  Rust path as popover Refresh — see popover.md "Refresh"; only automatic
  refreshes honor the ≥60s floor).
- **Interactions**: Open Usage Window → Usage window (W2); Refresh →
  refresh flow (W4); Quit → app terminates.
- **Navigation**: arrives from right-click on any provider item; exits to
  Usage window or dismissal.
