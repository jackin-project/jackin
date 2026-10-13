# Task 002 schema decision

Read-only check of `/Users/donbeave/Projects/jackin-project/jackin`. No clone edit, commit, build, or `cargo` run. Constants below were re-read with `git grep` / `git show` on the named tips.

| Tip | SHA | Subject |
| --- | --- | --- |
| M main | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` | `fix(usage): retain foreground broker for auth lease (#1124)` |
| U usage | `2e4a62da4bc4406764162a267163389a5fb513a7` | `docs(usage): update v11 live handoff gates` |
| P policy | `fba69d60c2373156dc0dcecee8e085c83776966f` | `docs: record Velnor d35 lock audit` |

P is the layered tree (`crates/core`, `crates/services`, `crates/apps`). M and U keep the flat crate layout.

## Verified constants

| Constant | M | U | P |
| --- | --- | --- | --- |
| `USAGE_MONITOR_SCHEMA_VERSION` | `4` at `crates/jackin-protocol/src/usage_monitor.rs:11` | `6` at `crates/jackin-protocol/src/usage_monitor.rs:11` | `4` at `crates/core/jackin-protocol/src/usage_monitor.rs:11` |
| `USAGE_BROKER_PROTOCOL_VERSION` | `"v8"` at `crates/jackin-protocol/src/usage_broker.rs:14` | `"v9"` at `crates/jackin-protocol/src/usage_broker.rs:14` | `"v8"` at `crates/core/jackin-protocol/src/usage_broker.rs:14` |
| `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION` | absent (`git grep` on this SHA returned no hits) | `"v8"` at `crates/jackin-usage/src/host/broker.rs:48` | absent (`git grep` on this SHA returned no hits) |
| `ACCOUNT_STATE_SCHEMA_VERSION` | `2` at `crates/jackin-usage/src/coordinator/state/envelope.rs:14` | `2` at the same path, line 14 | `3` at `crates/services/jackin-usage-coordinator/src/state/envelope.rs:14` |
| Previous account schema | `PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION = 1` at envelope.rs:15 | same, line 15 | `PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION = 2` at envelope.rs:15; `LEGACY_ACCOUNT_STATE_SCHEMA_VERSION = 1` at envelope.rs:16 |
| `USAGE_STATUSLINE_INPUT_SCHEMA_VERSION` | `2` at `usage_monitor.rs:14` on each tip (P under `crates/core/jackin-protocol`) | same | same |
| `PROJECTION_STATE_SCHEMA_VERSION` | `3` at `crates/jackin-usage/src/coordinator/state/envelope.rs:28` | `3` at the same path, line 28 | `3` at `crates/services/jackin-usage-coordinator/src/state/envelope.rs:26` |

Statusline input schema 2 and projection envelope 3 are already the same on all three tips. This decision does not bump either one.

## Monitor load match arms

M `load` is `crates/jackin-usage/src/host/broker/monitor/storage.rs:61`. Current schema is 4, so the current arm is a direct decode. Schema 1 is migrated. Schemas 2 and 3 are migrated. Anything else, including 5 and 6, is refused before `save`.

```91:97:crates/jackin-usage/src/host/broker/monitor/storage.rs
    let state = match version {
        1 => super::legacy::migrate_v1(&bytes)?,
        2 | 3 => migrate_v2_or_v3(&bytes, version)?,
        USAGE_MONITOR_SCHEMA_VERSION => {
            serde_json::from_slice(&bytes).map_err(|_| unavailable())?
        }
        _ => return Err(unavailable()),
    };
```

P `load` is `crates/services/jackin-usage-broker/src/monitor/storage.rs:65`. The arms match M. `USAGE_MONITOR_SCHEMA_VERSION` is 4, so 5 and 6 fall through to `_`.

```95:101:crates/services/jackin-usage-broker/src/monitor/storage.rs
    let state = match version {
        1 => super::legacy::migrate_v1(&bytes)?,
        2 | 3 => migrate_v2_or_v3(&bytes, version)?,
        USAGE_MONITOR_SCHEMA_VERSION => {
            serde_json::from_slice(&bytes).map_err(|_| unavailable())?
        }
        _ => return Err(unavailable()),
    };
```

U `load` is `crates/jackin-usage/src/host/broker/monitor/storage.rs:62`. Current schema is 6. It migrates 5 and also migrates 1, 2, 3, and 4. Versions in `2..6` are rejected first if they already carry schema-6 reset metadata.

```92:103:crates/jackin-usage/src/host/broker/monitor/storage.rs
    if (2..USAGE_MONITOR_SCHEMA_VERSION).contains(&version) {
        reject_v6_reset_metadata_in_legacy_snapshot(&bytes)?;
    }
    let state = match version {
        1 => super::legacy::migrate_v1(&bytes)?,
        2 | 3 => migrate_v2_or_v3(&bytes, version)?,
        4 => migrate_v4(&bytes)?,
        5 => migrate_v5(&bytes)?,
        USAGE_MONITOR_SCHEMA_VERSION => {
            serde_json::from_slice(&bytes).map_err(|_| unavailable())?
        }
        _ => return Err(unavailable()),
    };
```

U's schema-4 migration is not a lossless read of M's store. The comment and the drop are:

```135:138:crates/jackin-usage/src/host/broker/monitor/storage.rs
/// V4 predates provider-refresh diagnostics and local/source attribution.
/// Preserve goal and event history, restamp nested status schemas, and fail
/// closed by dropping broker quota values that lack source-bound ownership.
fn migrate_v4(bytes: &[u8]) -> Result<StoreState, MonitorIssue> {
```

```159:159:crates/jackin-usage/src/host/broker/monitor/storage.rs
    clear_unscoped_provider_projection(&mut state);
```

`migrate_v5` (`storage.rs:167`) records that a schema-5 account-wide reset mark has no source provenance. `git grep` for `StatuslineResetWatermark`, `statusline_reset_watermarks`, `clear_unscoped_provider_projection`, and `legacy_reset_admission` returned no matches on M `1c3dd1be` or P `fba69d60`.

## 1. Selected monitor schema and broker wire

Selected current monitor schema: **4**. Selected broker wire: **v8**.

The integration starts from P's layered tree and must read M's on-disk monitor state. M writes schema 4. P already decodes schema 4 on the current arm and does not rewrite it (`storage.rs:104` saves only when `version != USAGE_MONITOR_SCHEMA_VERSION`).

Do not stamp U's schema 6 or wire v9 onto P.

U does have a behavior P lacks: source-bound provider-quota ownership and reset-watermark provenance (provider-refresh diagnostics and local/source attribution, described on `migrate_v4`). That behavior is not a reason to take U's migration. M's schema-4 bytes do not carry the provenance, and `migrate_v4` fail-closes by dropping unscoped quota (`clear_unscoped_provider_projection` at line 159). Applying it would replace M's readable quota with a dropped projection. P's schema-4 decode keeps those values. U's migration is not the only way to preserve a behavior present in M's store, because that behavior is not in the store.

Rollback: keep refusing every monitor schema newer than 4, including 5 and 6, on the `_ => return Err(unavailable())` arm. Do not add `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION`. A non-v8 socket stays a protocol mismatch and a healthy foreign lease is not replaced.

## 2. Schema-4 store, schema-6 store, v8 socket, v9 socket

On the selected P code (schema 4, wire v8):

| Input | Result |
| --- | --- |
| Schema-4 monitor store | Direct load. No migration rewrite. |
| Schema-6 monitor store | Refusal. The file stays. |
| v8 socket | Current wire. Accepted when the header equals `"v8"`. |
| v9 socket | Refusal. Not migrated. |

Schema 4 hits `USAGE_MONITOR_SCHEMA_VERSION` and `serde_json::from_slice`. The following `save` runs only for a migrated older schema.

Schema 6 hits `_ => return Err(unavailable())` inside `load`, before `save`. `MonitorStore::open` propagates that error and writes a default store only when the file is missing:

```16:23:crates/services/jackin-usage-broker/src/monitor/operations.rs
        let directory = storage::open_store_dir(data_dir)?;
        let state = if let Some(state) = storage::load(&directory)? {
            validate_store_state(&state)?;
            state
        } else {
            let state = StoreState::default();
            storage::save(&directory, &state)?;
            state
        };
```

M's `future_store_schema_is_rejected` (`crates/jackin-usage/src/host/broker/monitor/tests.rs:4255`) writes `USAGE_MONITOR_SCHEMA_VERSION + 1` and expects `MonitorStoreUnavailable`. On M and P that future value is 5. Schema 6 takes the same arm.

A v8 request matches P's gate:

```86:91:crates/services/jackin-usage-broker/src/dispatch.rs
fn request_header_matches_current_protocol(
    protocol_version: &str,
    request_build_id: &str,
    build_id: &str,
) -> bool {
    protocol_version == USAGE_BROKER_PROTOCOL_VERSION && request_build_id == build_id
```

`USAGE_BROKER_PROTOCOL_VERSION` is `"v8"`. A v9 header fails that equality and is `protocol_error()`. The leader does not replace a healthy incompatible lease:

```120:125:crates/services/jackin-usage-broker/src/leader.rs
        if existing.protocol_version != USAGE_BROKER_PROTOCOL_VERSION
            || existing.build_id != build_id
        {
            // A healthy incompatible endpoint is never replaced by an
            // activator; the client will receive protocol_mismatch.
            return Ok(None);
```

U's `"v8"` previous constant does not make v8 frames current on U. U dispatch still requires the current version (`crates/jackin-usage/src/host/broker/dispatch_ops.rs:74`). The previous constant only lets a dead owner's stale v8 lease be reclaimed (`broker.rs:46-48` and the `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION | USAGE_BROKER_PROTOCOL_VERSION` arm at `broker.rs:1485`). That reclaim is not selected. Taking v9 would refuse a live M v8 socket.

Last-good state on corruption stays in place:

- Monitor `load` returns `unavailable()` for bad JSON or an unknown schema and does not call `save`. A failed `save` unlinks only the temporary file after `renameat` / `fsync` of `state.json` (`storage.rs:302-306`). The previous `state.json` is not replaced by a partial write.
- Account load returns `StateStoreError::Corrupt` for an unreadable body or a schema outside the selected arms, and that return happens before `self.store`, so the account file is not rewritten.
- `finish_success` is the writer of `last_good` (`crates/services/jackin-usage-coordinator/src/finish.rs:100`). `finish_failure_in_state` sets `terminal_result = None` and `terminal_error` (`finish.rs:178-180`) and does not assign `last_good`.
- Projection envelope 3: a version above 3 returns `SchemaMigrationRequired` with no quarantine, so the file stays (`stores.rs`, `if schema_version > current`). Unreadable projection bytes are renamed aside by `quarantine` (`stores.rs:228-231`) and are not replaced with an empty envelope. The load comment at `stores.rs:105-107` says valid older and future versions remain in place.

## 3. Account state

Keep P's account-state schema **3**. Do not invent a new version.

M and U treat schema 2 as current and schema 1 as the only migrated previous version. They reject anything else, including 3:

```316:337:crates/jackin-usage/src/coordinator/state/stores.rs
        let (envelope, migrated) = match schema_version {
            PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION => {
                // v1 recorded queue admission as the attempt start. ...
                envelope.schema_version = ACCOUNT_STATE_SCHEMA_VERSION;
                (envelope, true)
            }
            ACCOUNT_STATE_SCHEMA_VERSION => (
                serde_json::from_value(value).map_err(|_| StateStoreError::Corrupt)?,
                false,
            ),
            _ => return Err(StateStoreError::Corrupt),
        };
```

U's account match at `crates/jackin-usage/src/coordinator/state/stores.rs:316` is the same pair of arms. On both tips `ACCOUNT_STATE_SCHEMA_VERSION` is 2 and `PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION` is 1.

P migrates the schema M and U treat as current. Schema 2 is `PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION`. Schema 1 is the legacy arm. Schema 3 is the direct load. Anything else is `Corrupt` and is not stored.

```352:376:crates/services/jackin-usage-coordinator/src/state/stores.rs
            PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION => {
                // v2 did not retain uncertainty after recovering an Updating
                // record without a real invocation timestamp. Preserve that
                // provenance explicitly in v3 so another restart cannot lose
                // the conservative monotonic reload fence.
                // ...
                envelope.schema_version = ACCOUNT_STATE_SCHEMA_VERSION;
                (envelope, true)
            }
            ACCOUNT_STATE_SCHEMA_VERSION => (
                serde_json::from_value(value).map_err(|_| StateStoreError::Corrupt)?,
                false,
            ),
            _ => return Err(StateStoreError::Corrupt),
```

The integration must include a test that a version-2 account file still loads. P already has that test: `account_state_v2_migration_persists_unresolved_attempt_provenance_atomically` in `crates/services/jackin-usage-coordinator/src/state/tests/case_01.rs:149`. It writes `schema_version` = `PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION` (2), calls `store.load`, and asserts the loaded and durable schema equal `ACCOUNT_STATE_SCHEMA_VERSION` (3) at lines 170-177. Keep that test. Do not add an account schema 4.

## 4. Non-goals

- No live provider calls. This check did not call a provider, and the integration decision does not authorize one.
- No copying of install paths or PIDs from any tip.
- U live acceptance remains unproved. U's tip is a docs commit (`2e4a62da`, `docs(usage): update v11 live handoff gates`). Schema 6 / v9 is not selected, and this note is not a live-acceptance result.
