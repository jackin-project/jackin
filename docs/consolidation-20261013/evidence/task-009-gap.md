# TASK-009 gap check

Not an approval. No product edit, no cargo, no mbx, no commit, no live provider call, no Keychain read.

| Tree | Path | SHA |
| --- | --- | --- |
| Integration | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` | `738e2cfa95731b8ee59e705f52e38c802c12a3a8` on `integration/branch-consolidation-20261013` |
| Main baseline M | `/Users/donbeave/Projects/jackin-project/wt-baseline-M` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| Policy P | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| Usage repair U | `/Users/donbeave/Projects/jackin-project/wt-baseline-U` | `2e4a62da4bc4406764162a267163389a5fb513a7` |

Named crates on the integration tree match P except one already-landed protocol test. Monitor JSON persistence lives in `jackin-usage-broker` (`src/monitor/storage.rs`), which is the counterpart of U `crates/jackin-usage/src/host/broker/monitor/storage.rs`. `jackin-usage-snapshot-store` is a separate SQLite usage cache. `jackin-usage-store-backend` is the Turso connection chokepoint and holds no monitor schema.

## Monitor schema 4, schema-4 load, schema-6 refusal

ALREADY_PRESENT.

Integration monitor schema is 4 (`crates/core/jackin-protocol/src/usage_monitor.rs:11`). P is the same constant at `crates/core/jackin-protocol/src/usage_monitor.rs:11`. U is 6 (`crates/jackin-protocol/src/usage_monitor.rs:11`).

Load arms on the integration tree (`crates/services/jackin-usage-broker/src/monitor/storage.rs:98-111`) match P (`crates/services/jackin-usage-broker/src/monitor/storage.rs:95-109`): schema 1 migrates, schemas 2 and 3 migrate, schema 4 decodes in place, anything else returns `unavailable()` before `save`. `save` runs only when `version != USAGE_MONITOR_SCHEMA_VERSION` (`storage.rs:107`). `MonitorStore::open` writes a default store only when the file is missing (`operations.rs:17-22`). A schema-6 file takes the `_` arm and stays on disk. `future_store_schema_is_rejected` writes `USAGE_MONITOR_SCHEMA_VERSION + 1` and expects `MonitorStoreUnavailable` (`v2_tests.rs:1788`). A wrong nested event schema is rejected without rewriting the source (`v2_tests.rs:1713`). A failed `save` unlinks only the temporary file after the rename (`storage.rs:309-311`).

## Broker wire v8, v9 mismatch, no previous-protocol constant

ALREADY_PRESENT.

Wire version is `"v8"` (`crates/core/jackin-protocol/src/usage_broker.rs:14`). P is the same at `crates/core/jackin-protocol/src/usage_broker.rs:14`. U is `"v9"` (`crates/jackin-protocol/src/usage_broker.rs:14`). The protocol test locks schema 4, statusline input 2, and wire v8 together (`usage_monitor.rs:1179-1181`).

`jackin-usage-broker-wire` stamps the current constant onto a new lease (`src/lease.rs:39`). Dispatch accepts a header only when it equals that constant (`jackin-usage-broker/src/dispatch.rs:86-91`). A healthy lease with any other protocol is not replaced (`leader.rs:120-125`). `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION` is absent on the integration tree and on P. U defines it as `"v8"` (`crates/jackin-usage/src/host/broker.rs:48`) so a dead v8 lease can be reclaimed after the wire moved to v9. Do not add that constant. v9 stays a protocol mismatch.

## Account state version 3

ALREADY_PRESENT.

Account state schema is 3, previous is 2, legacy is 1 (`crates/services/jackin-usage-coordinator/src/state/envelope.rs:14-16`). That crate is outside the named leaves and was not changed for this check. No account schema 4 was found on the integration tree. Do not add one.

## U schema-6 migration, fingerprint rewrite, and used-only evidence

REJECT.

U `load` migrates 4 and 5 (`storage.rs:98-99`). `migrate_v4` restamps the store to schema 6 and calls `clear_unscoped_provider_projection` (`storage.rs:138-159`). `migrate_v2_or_v3` calls the same drop (`storage.rs:224`). That function is absent on the integration tree. Applying it would delete quota that a schema-4 store still holds.

U fingerprint migration is the same drop. `v4_fingerprint_migration_preserves_account_named_statusline_session_and_drops_provider_data` (`fingerprint_migration.rs:30`) and `v4_migration_preserves_strict_history_and_drops_unscoped_provider_quota` (`provider_diagnostics.rs:1190`) are absent on the integration tree. Current fingerprint keys stay unqualified (`observations.rs:642`, `observations.rs:660`). Rewriting them to U's source-qualified form is a store rewrite. Do not port it.

`MonitorProviderReadiness` (`U usage_monitor.rs:95`), the provider issue codes added beside it (`ProviderRateLimited` through `ProviderNeedsSecret`, `U usage_monitor.rs:759-768`), reset watermarks, and the used-only quota path from `db59c401` are absent. They arrive with schema 6. The pause-barrier fallback in U `evaluation.rs:426-449` is part of that same commit and fills a reset from account fields when the used sample has none. Do not port it. Schema-4 windows already keep used and reset independent (`usage_monitor.rs` test `quota_window_status_keeps_used_and_reset_evidence_independent`).

## Observe-only authority

ALREADY_PRESENT.

An observe-only start has no goal and no policy (`monitor.rs:292-307`). A goal or policy revision on an observe-only config is `ObservationOnly` (`monitor.rs:508-515`). `runnable` is true only for `DispatchGuard` when the evaluation is not blocked or unknown (`evaluation.rs:313-315`, `lifecycle.rs:563-571`). Observe-only dispatch is `NotAuthorized` (`monitor.rs:813-814`, `lifecycle.rs:572-574`). The protocol fixture encodes `purpose: observe_only` and `goal_id: None` (`usage_monitor.rs:1131-1152`). Credential bootstrap is not a monitor RPC (`usage_monitor.rs:1193-1203`).

The offline fixture asserts the same boundary for a session observer and a bound observer: `purpose == observe_only`, `goal_id` null, `dispatch == not_authorized`, `runnable` false (`crates/apps/jackin/tests/usage_monitor_offline.rs:776-783` and `935-943`). A wait for `runnable` stays non-runnable (`usage_monitor_offline.rs:833-845`).

`collector_auth_required_has_stable_wire_code` and `binding_input_defaults_experimental_collector_approval_to_false` exist on M and U and are not separate functions on the integration tree. The same assertions are already inside `v4_monitor_control_shapes_are_tagged_and_secret_free` (`usage_monitor.rs:1104-1111` and `1204-1207`). Do not add a second copy.

## U provider-flow fixture and auth-outcome binary

REJECT.

U `crates/jackin/src/bin/usage-broker-fixture.rs` and `crates/jackin/tests/usage_provider_flow.rs` are absent on the integration tree and on P. The fixture binary calls `run_usage_broker_foreground_bootstrap_with_test_fixture` (`usage-broker-fixture.rs:41`), which is also absent. The flow rewrites a snapshot only when `schema_version == 6` (`usage_provider_flow.rs:1804-1808`), seeds a stale v8 lease (`usage_provider_flow.rs:1321`), and expects a v9 replacement (`usage_provider_flow.rs:1359`). `auth_outcomes.rs:12` starts an observe-only monitor with `--experimental-collector` (`auth_outcomes.rs:83-94`) and then issues provider HTTP. That is not a schema-4, wire-v8, zero-dispatch port.

The non-TTY `interaction_required` gate is already on the real broker binary (`crates/apps/jackin/src/bin/usage-broker/main.rs:325-330`).

Desktop and Capsule already share one updating generation, and waiters already share one rate deadline, in the existing broker e2e: `usage_broker_capsule_refresh_is_same_updating_generation_in_desktop` (`usage_broker_e2e/docker.rs:221`) and `usage_broker_failure_and_rate_deadline_are_identical_for_all_waiters` (`docker.rs:352`). Those tests match P. Do not recreate them under `usage_provider_flow`.

## Snapshot cache, store backend, token monitor, projection

ALREADY_PRESENT.

`jackin-usage-snapshot-store` schema constant is `"4"` (`src/types.rs:11`). Initialization records that value (`src/schema.rs:62-64`). This is the SQLite usage cache, not the monitor JSON store, and it matches P. `jackin-usage-store-backend` opens a local connection and does not version monitor state (`src/lib.rs:1-8`). `jackin-usage-token-monitor` and `jackin-usage-projection` do not define monitor schema, broker wire, `runnable`, or dispatch authority. Nothing in those four crates is a U behavior that can be ported without schema 6 or wire v9.

## Live acceptance

BLOCKED.

U's tip is a docs commit. This check did not run the TASK-009 nextest commands, did not call a provider, and did not read Keychain. Live readiness stays unproved.

## Edit list

Empty. Do not stamp monitor schema 6 or broker wire v9. Do not add `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION`. Do not call `clear_unscoped_provider_projection`. Do not add account schema 4. Do not port the U fixture binary, `usage_provider_flow`, or the used-only quota path.
