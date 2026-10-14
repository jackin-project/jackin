# Task 002 schema constant check

Read-only check via `git show` / `git grep` in `/Users/donbeave/Projects/jackin-project/jackin`. No build, commit, or source edit.

Trees:

- M `1c3dd1be4a3b1ed518133ec3899c084c792370ff`
- U `2e4a62da4bc4406764162a267163389a5fb513a7`
- P `fba69d60c2373156dc0dcecee8e085c83776966f`

## Verdicts

| Claim | Verdict |
| --- | --- |
| `USAGE_MONITOR_SCHEMA_VERSION`: M 4, U 6, P 4 | AGREE |
| `USAGE_BROKER_PROTOCOL_VERSION`: M v8, U v9, P v8 | AGREE |
| `ACCOUNT_STATE_SCHEMA_VERSION`: M 2, U 2, P 3 | AGREE |
| U has `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION` v8; M and P do not | AGREE |
| Live acceptance on U is explicitly incomplete in the handoff doc | AGREE |
| P `storage.rs` rejects schema 5 and 6 while U migrates them | DISAGREE |

Wrong claim: U does not migrate schema 6. P `load` rejects both 5 and 6. U migrates schema 5 (`migrate_v5`) and loads schema 6 as the current `USAGE_MONITOR_SCHEMA_VERSION` with no migration.

## USAGE_MONITOR_SCHEMA_VERSION — AGREE

M `crates/jackin-protocol/src/usage_monitor.rs:11`:

```text
pub const USAGE_MONITOR_SCHEMA_VERSION: u16 = 4;
```

U `crates/jackin-protocol/src/usage_monitor.rs:11`:

```text
pub const USAGE_MONITOR_SCHEMA_VERSION: u16 = 6;
```

P `crates/core/jackin-protocol/src/usage_monitor.rs:11`:

```text
pub const USAGE_MONITOR_SCHEMA_VERSION: u16 = 4;
```

## USAGE_BROKER_PROTOCOL_VERSION — AGREE

M `crates/jackin-protocol/src/usage_broker.rs:14`:

```text
pub const USAGE_BROKER_PROTOCOL_VERSION: &str = "v8";
```

U `crates/jackin-protocol/src/usage_broker.rs:14`:

```text
pub const USAGE_BROKER_PROTOCOL_VERSION: &str = "v9";
```

P `crates/core/jackin-protocol/src/usage_broker.rs:14`:

```text
pub const USAGE_BROKER_PROTOCOL_VERSION: &str = "v8";
```

## ACCOUNT_STATE_SCHEMA_VERSION — AGREE

M `crates/jackin-usage/src/coordinator/state/envelope.rs:14`:

```text
pub(crate) const ACCOUNT_STATE_SCHEMA_VERSION: u32 = 2;
```

U `crates/jackin-usage/src/coordinator/state/envelope.rs:14`:

```text
pub(crate) const ACCOUNT_STATE_SCHEMA_VERSION: u32 = 2;
```

P `crates/services/jackin-usage-coordinator/src/state/envelope.rs:14`:

```text
pub(crate) const ACCOUNT_STATE_SCHEMA_VERSION: u32 = 3;
```

## PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION — AGREE

U `crates/jackin-usage/src/host/broker.rs:48`:

```text
const PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION: &str = "v8";
```

`git grep PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION` on M `1c3dd1be` and P `fba69d60` returned no matches.

## Live acceptance on U — AGREE

U `plans/claude-unattended-monitor/claude-code-handoff.md`:

```text
1:# Claude Code quota observer handoff — DRAFT, NOT READY FOR LIVE USE
6:> natural v11 collection generations were recorded, but live acceptance remains
7:> false: in those captures Weekly was current at 92% used, while Session
28:Acceptance is false.
```

M and P contain different handoff files. This verdict is only for the U file above.

## Schema 5 and 6 — DISAGREE

P `crates/services/jackin-usage-broker/src/monitor/storage.rs` `load` (current schema is 4). Versions 5 and 6 hit the reject arm:

```text
95:    let state = match version {
96:        1 => super::legacy::migrate_v1(&bytes)?,
97:        2 | 3 => migrate_v2_or_v3(&bytes, version)?,
98:        USAGE_MONITOR_SCHEMA_VERSION => {
99:            serde_json::from_slice(&bytes).map_err(|_| unavailable())?
100:        }
101:        _ => return Err(unavailable()),
102:    };
```

U `crates/jackin-usage/src/host/broker/monitor/storage.rs` `load` (current schema is 6). Schema 5 is migrated. Schema 6 is the current arm, not a migration:

```text
95:    let state = match version {
96:        1 => super::legacy::migrate_v1(&bytes)?,
97:        2 | 3 => migrate_v2_or_v3(&bytes, version)?,
98:        4 => migrate_v4(&bytes)?,
99:        5 => migrate_v5(&bytes)?,
100:        USAGE_MONITOR_SCHEMA_VERSION => {
101:            serde_json::from_slice(&bytes).map_err(|_| unavailable())?
102:        }
103:        _ => return Err(unavailable()),
104:    };
167:fn migrate_v5(bytes: &[u8]) -> Result<StoreState, MonitorIssue> {
169:    if state.schema_version != 5 {
173:    state.schema_version = USAGE_MONITOR_SCHEMA_VERSION;
```
