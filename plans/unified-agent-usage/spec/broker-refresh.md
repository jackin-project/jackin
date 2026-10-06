# Broker and refresh

Covers: F5-F7; W6; N3.

## Requirements

### Requirement: one durable authority

A process-independent per-user broker SHALL exclusively own provider I/O, canonical
discovery revisions, freshness, retry deadlines, last-good persistence, and generation
publication. CLI, Console, Capsule, runtime relay, FFI, Swift, diagnostics, and caches
MUST NOT become alternate provider or freshness authorities.

#### Scenario: activating client exits

- GIVEN a CLI starts the broker and requests a refresh
- WHEN the CLI exits
- THEN the broker survives and completes or records that generation
- AND another client reads or joins the same authority.

### Requirement: joined work

Concurrent requests for the same canonical account/provider generation SHALL join one
in-flight operation. Cancellation of one waiter MUST NOT cancel work required by other
waiters. No caller SHALL queue a duplicate post-flight refresh.

#### Scenario: four surfaces refresh together

- GIVEN CLI, Console, Capsule, and desktop request refresh concurrently
- THEN provider I/O occurs once per due canonical work item
- AND every caller receives the same generation ID.

### Requirement: adaptive cadence

The broker SHALL own the settled automatic cadence: 2 minutes during direct
interaction, 5 minutes during recent activity, 15 minutes while idle, and 30 minutes
during long idle or Low Power Mode. Opening a surface SHALL request refresh only when
policy says due. Manual refresh SHALL join active work and obey provider retry limits.

#### Scenario: popover opens before deadline

- WHEN the popover opens with current last-good data before the success deadline
- THEN it reads immediately
- AND no provider request starts.

### Requirement: recoverable persistence

Broker state SHALL use atomic writes, explicit protocol/schema versions, authenticated
service ownership, catalog revision replacement, persisted retry/success deadlines,
and immutable last-good generations. Crash or corrupt state SHALL fail closed and
recover without fabricating current data.

#### Scenario: provider refresh fails

- GIVEN a valid last-good generation exists
- WHEN the next provider refresh fails
- THEN the last-good windows remain immutable and marked stale with age and cause
- AND other successful providers publish in the same new generation.

