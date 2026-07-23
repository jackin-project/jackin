# Usage window (Capsule-parity detail)

## Purpose

The full-detail native window: glass sidebar (Overview + providers in
Capsule tab order) and a content pane restating the Capsule usage dialog
field-for-field. Parity is the contract (D5, B3); CodexBar styling applies
to popover/status bar, not here.
Anchors: S11, S12, F13, W2 · Evidence: plans/native-macos-usage-menu-bar/010-usage-window.md (shipped v1 contract); research ch. 01 (view spine)

## Requirements

### Requirement: Capsule-parity provider card
The Usage window content pane SHALL render the selected provider's full
card with the same fields, same strings, and same order as the Capsule
usage dialog, sourced from the same Rust views; any numeric or textual
divergence for the same account at the same fetch is a defect.
Covers: S11, F13 · Evidence: plan 010 invariant (item D5); B3

#### Scenario: Parity spot-check
- **GIVEN** the Capsule usage dialog shows a Codex card with specific bucket strings
- **WHEN** the Usage window shows Codex for the same account/fetch
- **THEN** every field matches string-for-string in the same order

#### Scenario: New pace composite flows through
- **GIVEN** Rust emits "5% in deficit · Runs out in 3d 1h"
- **WHEN** both the Capsule dialog and the window render
- **THEN** both split the composite into their existing pace/right columns identically

#### Scenario: Amp daily and balances stay in parity
- **GIVEN** Rust emits an Amp Free Daily bucket with `61% left` and
  `Resets daily`, plus individual/workspace credit bounds
- **WHEN** the Capsule dialog and Usage window render the same fetch
- **THEN** both show those fields in identical order and wording, with no
  fabricated exact reset or paid-plan label

### Requirement: Sidebar and window states
The window SHALL show a sidebar with Overview on top and providers in
Capsule tab order; Overview SHALL list overview rows for all enabled
providers; stale/error SHALL render Rust-provided degradation strings
verbatim (error never overwrites last-good); an empty enabled set SHALL
show the hint line. Account chips SHALL appear for multi-account providers
and drive the same selection as the popover.
Covers: S12 · Evidence: item §Screens/"Usage window"; B5

#### Scenario: Sidebar order
- **GIVEN** all seven providers enabled
- **WHEN** the window opens
- **THEN** sidebar lists Overview then providers in the Capsule dialog's tab order

#### Scenario: Window entry paths
- **GIVEN** the app is running
- **WHEN** the operator uses right-click → Open Usage Window, or clicks a popover provider header
- **THEN** the window opens (focused on that provider for the header path) — W2

## Screen: Usage window (S11–S12)

Mockup: item §Screens/"Usage window" schematic.

- **Regions**: glass sidebar (Overview + provider rows) · content pane
  (provider card / overview rows) · account chips (multi-account).
- **States**: default (provider card) | Overview | stale/error (verbatim
  degradation strings) | empty (hint) — all item-drawn.
- **Interactions**: sidebar row click → switch provider (→ "Sidebar and
  window states"); chip click → account select (shared selection);
  standard window close/minimize.
- **Navigation**: in via context menu or popover header (W2); out via
  window close.
