# Canonical projection

Covers: F1-F4, F8, F17; W1-W6; N1, N4, N8-N10.

## Requirements

### Requirement: one versioned Rust projection

The system SHALL publish one immutable, versioned Rust-owned projection containing
provider, canonical account, ordered quota windows, lifecycle, freshness, last-good
time, and structured failures. Every surface SHALL adapt this projection without
parsing display strings.

#### Scenario: atomic generation

- GIVEN providers finish at different times
- WHEN a refresh generation publishes
- THEN readers observe either the previous complete generation or the new complete generation
- AND never a mixed generation.

### Requirement: evidence-based canonical identity

Canonical identity MUST prefer provider account IDs, then provider-stable non-secret
handles. Proven aliases SHALL merge deterministically. Unresolved evidence SHALL be
typed provisional identity. Source ordinals MUST NOT be durable IDs.

#### Scenario: duplicate discovery

- GIVEN the same provider account is discovered through global, role, workspace, and workspace-role sources
- WHEN the graph is built
- THEN exactly one canonical account is emitted
- AND provenance enriches that account without creating rows.

### Requirement: deterministic membership and order

Current read-only configuration discovery SHALL own host membership. Durable history
MUST NOT resurrect absent members. Providers SHALL use the settled provider-only names
and order: OpenAI, Anthropic, Amp, xAI, Z.AI, Kimi, MiniMax, OpenCode. Accounts SHALL
sort in Rust with a pinned ICU4X collator using runtime locale and deterministic `und`
fallback, case-insensitive full display label, then stable ID. The locked ICU/data
version plus `und`, English, Turkish, and Vietnamese goldens SHALL freeze ranks;
adapters SHALL consume serialized ranks and never sort.

#### Scenario: refresh does not reorder accounts

- GIVEN severity, freshness, or remaining percentage changes
- WHEN a new projection publishes
- THEN provider and account navigation order remains stable.

### Requirement: selection preservation

Typed destinations SHALL be Overview, single-account provider, or account. A
multi-account provider SHALL never itself be selectable. Selection SHALL survive
refresh by stable identity. A removed selection SHALL return to Overview and show
`Selected account is no longer available.` persistently until acknowledged or a new
destination is selected.

#### Scenario: selected account disappears

- WHEN current discovery removes the selected account
- THEN no sibling account is silently selected
- AND Overview is selected with the persistent notice.

### Requirement: Rust-owned presentation semantics

Rust SHALL own labels, state words, rounding, countdowns, units, semantic category,
severity, missing-plan fallback, summary selection, and final window ordering. Overview
SHALL use the first real ranked window: long-range weekly/daily/monthly, model, session,
then other. It MUST NOT invent provider aggregates or reorder account navigation.

#### Scenario: visible provider label

- GIVEN an OpenAI account resolved through a runtime agent
- THEN rich and simple surfaces display `OpenAI`
- AND do not display the runtime or agent name as provider identity.
