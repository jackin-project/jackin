# CLI usage

Covers: F11-F12; S9-S10; W2-W3; N13.

## Requirements

### Requirement: bare host read

`jackin usage` SHALL read the canonical host projection and print each canonical
account once. Human output SHALL be deliberately simple: provider heading, account
identity, and compact ordered limit lines. It MUST NOT use meters, cards, animation,
pace/reserve/run-out, raw mode, or interactive chrome.

#### Scenario: human output

- GIVEN multiple OpenAI accounts
- WHEN `jackin usage` succeeds
- THEN OpenAI is printed once as a group
- AND every canonical account appears once with compact limit/reset text.

### Requirement: stable JSON

`jackin usage --format json` SHALL emit the versioned canonical envelope with stable
field and array semantics, explicit lifecycle/freshness/failures, timestamps and IDs.
Unknown major versions SHALL fail closed; additive same-major fields SHALL be allowed
by documented compatibility rules.

#### Scenario: partial failure JSON

- GIVEN one provider fails and another has current or stale last-good data
- THEN stdout contains one valid JSON envelope with both structured outcomes
- AND the command exits zero.

### Requirement: exit and stream contract

Completed empty/unresolved-only and partial results containing current or stale rows
SHALL exit zero. Invalid invocation, invalid envelope, or all current members failing
without last-good SHALL exit nonzero. JSON stdout SHALL remain machine-valid; concise
diagnostics belong on stderr.

### Requirement: instance inspection without bypass

`jackin usage <instance> accounts` and `verify` intent SHALL remain, backed by the
same broker projection and resolved launch configuration. Cache/snapshot/no-refresh/
sync-host-cache forms that create alternate truth SHALL be removed or redefined.

#### Scenario: instance verification

- GIVEN an instance resolves two agents to one canonical account
- THEN verification reports both agent bindings and one canonical account
- AND performs no provider fetch outside the broker.

