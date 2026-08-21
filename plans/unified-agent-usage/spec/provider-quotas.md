# Provider quota contracts

Covers: F3, F8-F10; Q1-Q3; N2, N8, N11-N12.

## Requirements

### Requirement: supported provider matrix

The host SHALL support OpenAI, Anthropic, Amp, xAI, Z.AI, Kimi, MiniMax, and OpenCode.
Desktop SHALL exclude OpenCode by filtering the canonical graph. Each adapter SHALL
document trusted authentication, stable identity evidence, limit windows, reset and
failure semantics. Browser-cookie decryption/import, authenticated WebViews, and
scraping are forbidden.

#### Scenario: desktop projection

- GIVEN all eight host providers exist
- THEN host CLI/Console include all eight in settled order
- AND desktop includes the first seven with no second discovery path.

### Requirement: limits only

Adapters SHALL expose subscription/quota bounds only: remaining or used percentage,
reset, plan/status, provider-supplied windows, and money caps when the provider exposes
them as a quota bound. Token unit prices, session-cost estimates, spend history/trends,
charts, and cost rankings MUST NOT enter the projection.

#### Scenario: Codex individual limit

- GIVEN Codex supplies a monthly `individual_limit`
- THEN it is represented as a money-cap quota window with Rust-owned units
- AND not as spend history or price telemetry.

### Requirement: window identity and ordering

Semantic identity SHALL use provider evidence and the researched classifier for each
provider, resilient to renamed, malformed, or duration-missing slots. Provider/source
window order SHALL be retained for detail, while Overview summary uses canonical
category ranking. Unlike windows MUST NOT aggregate.

#### Scenario: malformed slot

- WHEN a slot label is renamed and duration is absent
- THEN the adapter emits the researched typed fallback or explicit unresolved window
- AND does not guess a different semantic category.

### Requirement: truthful derived states

`Not started` SHALL appear only with explicit provider evidence. `Runs out in N d`
SHALL appear only on rich current detail when evidence and confidence satisfy the
specified model; it SHALL be absent for stale/untouched/weak-confidence data and all
simple CLI output.

#### Scenario: 100 percent remains

- GIVEN a window reports 100% remaining without a provider start signal
- THEN it is not labeled `Not started`.

