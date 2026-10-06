# Capsule usage

Covers: F14; S11-S13; W4; B7; N5-N7.

## Requirements

### Requirement: resolved launch membership

Capsule Usage SHALL include only agents in the current fully resolved launch
configuration and every launch-forwarded deduplicated canonical account. Global host
discovery, unresolved configuration, or capability alone MUST NOT create rows.

#### Scenario: two agents share an account

- THEN Overview contains each agent/account pair required for launch context
- AND the underlying canonical account is not duplicated in shared data.

### Requirement: existing modal grammar

The surface SHALL retain the Capsule Usage dialog, Overview plus resolved-agent tabs,
conditional account tabs only when an agent has multiple canonical accounts,
Capsule-style provider meters/details, responsive narrow rows, two-axis scrolling,
focus reversal, footer hints, and one joined refresh command.

### Requirement: independent lifecycle and quota state

Before first session, an eligible agent SHALL carry typed `agent_uninitialized`.
Available quota previews MAY coexist without clearing it. First successful session
initialization SHALL clear only the lifecycle error. Quota availability MUST NOT
authorize or block launch/session actions.

#### Scenario: preview before first session

- GIVEN usage resolves but the agent has never initialized
- THEN quotas render with `agent_uninitialized`
- AND launch remains available.

### Requirement: truthful empty and failures

A fully resolved configuration with zero agents SHALL show
`No agents configured for this Capsule.` with only Overview and no Retry. Resolution,
provider refresh, no-capability, stale last-good, and initialized states SHALL remain
distinct and use Rust-owned copy.

