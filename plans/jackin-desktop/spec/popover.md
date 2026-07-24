# Popover — Agent Usage preview

## Purpose

The CodexBar-style glance surface: provider tab grid, compact Overview,
per-provider detail with account chips and window bars, Refresh-only
footer. Availability only — no actions (N2), limits only (N3), Capsule
design supremacy (D7), Liquid Glass on chrome only (B2).
Anchors: S5–S10, F4, F8, F10, W1, W3, W4 · Evidence:
research/agent-usage-provider-apis/01 (DTO fields), 10 (CodexBar phrase/UX
provenance), 11 (Amp Free daily); item §Screens reference PNGs

## Requirements

### Requirement: Provider tab grid
The popover SHALL open with a tab grid at top: an Overview tab plus one tab
per enabled provider (icon above name), each provider tab carrying a thin
progress bar under its name (the Rust-selected glance %: weekly for six,
Amp Free daily for Amp); the selected tab SHALL be visibly highlighted.
Covers: S5, S6, F10 · Evidence: item reference-provider-tabs.png; OverviewRowDto (research ch. 01)

#### Scenario: Grid reflects enabled set
- **GIVEN** five enabled providers
- **WHEN** the popover opens
- **THEN** the grid shows Overview + five provider tabs with thin bars; disabled providers absent

#### Scenario: Amp tab bar uses Daily
- **GIVEN** Amp Free reports 61% daily remaining
- **WHEN** the tab grid renders
- **THEN** Amp's thin bar uses 61% from the Rust Daily slot, not a credit
  balance or a Swift-selected fallback

### Requirement: Overview tab
The Overview tab SHALL show exactly one compact row per enabled provider:
provider name, headline availability % (selected account), severity-colored
bar, and reset label/countdown — no deeper detail. The headline uses the
same Rust-selected weekly-or-Amp-daily glance contract as the status item.
Covers: S5 · Evidence: item §Screens Kept "Overview tab"; OverviewRowDto fields (ch. 01)

#### Scenario: Compact rows only
- **GIVEN** providers with data
- **WHEN** Overview renders
- **THEN** each provider occupies one row (headline %, severity color, reset) and no window-level breakdown appears

### Requirement: Provider tab detail
A provider tab SHALL render, top to bottom: account chip row (multi-account
providers; selected chip highlighted), provider header (name, account
email, freshness "Updated …", plan label — all Rust strings), one segmented
bar block per quota window (window label, % left, pace/run-out line, reset
countdown), and provider-supplied credit/reset-credit blocks where the DTO
carries them.
Covers: S6, F4 · Evidence: item reference-codex/claude/amp/grok tab PNGs + Kept list; QuotaBucketDto/AccountDescriptorDto (ch. 01)

#### Scenario: Codex tab
- **GIVEN** Codex with Weekly + Spark Weekly buckets, reset credits, credits balance
- **WHEN** the tab renders
- **THEN** each bucket shows bar, % left, pace line ("13% in deficit · Runs out in 2d 18h" when Rust emits it), reset countdown; Limit Reset Credits and Credits blocks render from DTO fields

#### Scenario: Account chip switch
- **GIVEN** two Codex accounts
- **WHEN** the second chip is clicked
- **THEN** selection persists via `set_selected_account`, tab content, Overview row, and bar % follow (W3)

#### Scenario: Amp daily detail
- **GIVEN** the current Amp text reports 61% remaining today, `Resets
  daily`, individual credits, and workspace credits
- **WHEN** the Amp tab renders
- **THEN** it shows one Amp Free Daily window plus all returned credit
  bounds in Rust order, with no fabricated timestamp or paid-plan label

### Requirement: Popover degradation states
The popover SHALL keep last-known data visible during refresh (no blank
flash); stale data SHALL dim its freshness line; a provider fetch error
SHALL show the Rust-provided error line under that provider's header
without affecting other providers; an empty enabled set SHALL show the
"no agent credentials found" hint.
Covers: S7, S8, S9, S10 · Evidence: item §Screens states; B5; host runtime last-good semantics (ch. 01 Q5)

#### Scenario: One provider errors
- **GIVEN** Claude errors while Codex refreshes fine
- **WHEN** the popover renders
- **THEN** Claude's tab shows its error line with last-good values; Codex is untouched

#### Scenario: Empty state
- **GIVEN** zero enabled providers
- **WHEN** the popover opens
- **THEN** content region renders only the hint line; Refresh footer remains
  available so newly added credentials can be detected (S10)

### Requirement: Refresh
The popover footer SHALL contain exactly one row: Refresh with ⌘R shortcut;
invoking it SHALL request a Rust-side force refresh (`force: true`, the v1
manual-refresh semantics — item interaction "Refresh (⌘R) — force
refresh"), while automatic/timer refreshes SHALL keep honoring the existing
≥60s Rust floor; freshness lines update on completion and failures follow
the degradation states. (Spec corrected 2026-07-24 during planning: the
earlier floor-honoring wording contradicted the item's "force refresh"
interaction and v1's shipped `refreshAll()`.)
Covers: F8, W4 · Evidence: host.rs:425-433 (`refresh(_, force)` floor skip); PresentationStore.swift:308-311 ("Manual Refresh button — bypasses floor."); item §Screens interactions

#### Scenario: Manual refresh forces
- **GIVEN** a refresh completed 20s ago
- **WHEN** ⌘R is pressed
- **THEN** Rust performs the fetch (force path) and freshness lines update from real completion — never fabricated

#### Scenario: Automatic cadence floored
- **GIVEN** the background refresh cadence
- **WHEN** a non-forced refresh fires within 60s of the last
- **THEN** Rust declines it per the floor

### Requirement: Glance navigation
Left-click on a status item SHALL toggle the popover on that provider's
tab; Esc or outside click SHALL dismiss; clicking a provider header row
SHALL open the Usage window focused on that provider (navigation, not an
action button — N2).
Covers: W1, W2 entry · Evidence: item D13; §Screens interactions

#### Scenario: Header click
- **WHEN** the Codex header row is clicked
- **THEN** the Usage window opens focused on Codex and the popover dismisses

## Screen: Popover — Overview tab (S5)

Mockup: item §Screens/"Popover" schematic (left panel) +
reference-popover-overview.png.

- **Regions**: tab grid · compact provider rows · Refresh footer.
- **States**: default | loading (last-known + refresh indicator) | empty
  (S10 hint). Stale/error render per-row (dimmed freshness / status word).
- **Interactions**: tab click → switch (→ "Provider tab grid"); row click →
  that provider's tab; Refresh (→ "Refresh").
- **Navigation**: arrives from status-item left-click; exits via dismiss.

## Screen: Popover — provider tab (S6–S9)

Mockup: item §Screens/"Popover" schematic (right panel) + provider tab PNGs.

- **Regions**: tab grid · account chips (multi-account only) · provider
  header · window bar blocks · credit blocks · Refresh footer.
- **States**: default | loading (S7) | stale (S8) | error (S9) — as drawn
  in the item; all strings Rust-provided.
- **Interactions**: chip click → account select (→ "Provider tab detail");
  header click → Usage window (→ "Glance navigation"); Refresh ⌘R.
- **Navigation**: in from tab grid or status-item left-click; out via
  dismiss or header click → Usage window.
