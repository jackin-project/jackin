# TASK-007 gap check

Read-only comparison. No implementation, no cargo, no commit. This file does not approve TASK-007.

| Tree | Path | SHA |
| --- | --- | --- |
| Integration | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` | `1d76b4d11e3248d261594f45900f2f807b8c0807` (`1d76b4d1` is HEAD) |
| Main baseline M | `/Users/donbeave/Projects/jackin-project/wt-baseline-M` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| Usage repair U | `/Users/donbeave/Projects/jackin-project/wt-baseline-U` | `2e4a62da4bc4406764162a267163389a5fb513a7` |
| Policy P | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` |

Binding schema on the integration tree: monitor schema 4 (`crates/core/jackin-protocol/src/usage_monitor.rs:11`), broker wire v8 (`crates/core/jackin-protocol/src/usage_broker.rs:14`), account state schema 3 (`crates/services/jackin-usage-coordinator/src/state/envelope.rs:14`). U is monitor schema 6 (`crates/jackin-protocol/src/usage_monitor.rs:11`) and wire v9 (`crates/jackin-protocol/src/usage_broker.rs:14`). Do not move the integration tree to schema 6 or wire v9. Do not restore `ScopedCapability`.

`should_exit_idle` is already the required conjunction. Leave it unchanged.

```518:530:crates/services/jackin-usage-broker/src/serve.rs
fn should_exit_idle(
    now_monotonic: Duration,
    last_activity_monotonic: Duration,
    idle_exit: Duration,
    coordinator_idle: bool,
    has_active_monitor: bool,
    has_authenticated_foreground_guard: bool,
) -> bool {
    now_monotonic.saturating_sub(last_activity_monotonic) >= idle_exit
        && coordinator_idle
        && !has_active_monitor
        && !has_authenticated_foreground_guard
}
```

The call at `serve.rs:190-197` passes `coordinator.is_idle()`, `monitor_store.has_active()`, and `has_authenticated_foreground_guard`. Passive service entry passes `false` (`service.rs:361` and `service.rs:537`). Foreground bootstrap passes `true` (`service.rs:124`). `serve.rs` on the integration tree matches P.

## 1. Foreground liveness / idle exit outside `should_exit_idle` — PORT

Idle exit itself is already wired. The missing piece is owner-end shutdown cleanup outside that predicate.

M and U share the serve-loop shutdown flag with collector liveness and clear the foreground source on stop and on serve exit:

- `wt-baseline-M/crates/jackin-usage/src/host/broker/serve_loop.rs:132-135` takes `collector_liveness.shutdown_flag()` when a collector is present.
- `serve_loop.rs:185-187` breaks on that flag before `should_exit_idle`.
- `serve_loop.rs:158`, `serve_loop.rs:164`, and `serve_loop.rs:231` call `invalidate_foreground_collector` (`serve_loop.rs:250-257`), which deactivates liveness and calls `set_experimental_collector_source(None)`.
- `dispatch_ops.rs:152-158` deactivates liveness on `ServiceStop` and clears the same source.

Integration `serve` allocates a private flag at `serve.rs:130`. `ServiceStop` sets it at `dispatch.rs:135-137` and does not clear `experimental_collector_source`. After the accept loop, `serve.rs:223-233` fences and joins workers and the publisher ticker while that source is still set. `collection_accounts` (`monitor/operations.rs:174-176`) still admits collector work while the source is set. `ServiceStop` (`monitor/operations.rs:87`) does not clear it.

Do not re-port `should_exit_idle`. Do not restore `ClaudeCollectorLiveness`. The private shutdown flag is enough for the loop to leave on `ServiceStop`.

Smallest edit, integration crate `jackin-usage-broker` only:

1. In `dispatch_monitor_operation` (`dispatch.rs:135-137`), when the reply is `ServiceStopped`, also call `monitor_store.set_experimental_collector_source(None)` (`monitor/operations.rs:258`).
2. In `serve` (`serve.rs:223`), before `fenced.store` and before joining workers and the ticker, if `has_authenticated_foreground_guard` then call the same clear. That covers lease-renewal failure (`serve.rs:183-186`) and accept failure (`serve.rs:220`), which leave the loop without `ServiceStop`.

## 2. U coordinator/auth: non-TTY gate and passive observation — ALREADY_PRESENT

Both named behaviors are already on the integration tree and do not depend on schema 6 or wire v9.

Non-TTY `interaction_required` happens before broker launch and state creation:

- `crates/apps/jackin/src/bin/usage-broker/main.rs:325-330` returns `interaction_required` before `run_foreground`.
- `crates/apps/jackin/src/bin/usage-broker/tests.rs:90-95` (`explicit_auth_requires_all_terminal_streams_before_bootstrap`) panics if that path claims a broker or reads Keychain.
- `crates/services/jackin-usage-provider-claude/src/lease.rs:231-232` returns `InteractionRequired` before cache inspection or a Keychain read.
- U's matching gate is `crates/jackin/src/bin/usage-broker/main.rs:312-317` and `crates/jackin-usage/src/usage/claude/lease.rs:248-249`. U's foreground core still claims startup cleanup before bootstrap (`service.rs:276-277`), the same order as integration `service.rs:72-73`. The CLI gate is what keeps a non-TTY caller from reaching that claim.

Passive observation stays separate from task authorization:

- `monitor/evaluation.rs:313-315` sets `runnable` only for `MonitorPurpose::DispatchGuard`.
- `monitor.rs:813-814` sets observe-only dispatch to `MonitorDispatchReadiness::NotAuthorized`.
- U does the same at `monitor/status.rs:380-390` and `monitor/status.rs:518-520`.

REJECT these U follow-ups. They are not on the integration tree, and they are coupled to wire v9 or monitor schema 6:

- `f5163e98` changes `USAGE_BROKER_PROTOCOL_VERSION` from v8 to v9 and `USAGE_MONITOR_SCHEMA_VERSION` from 4 to 5. Later U history is schema 6. `MonitorProviderReadiness` is part of that schema bump.
- `expired_v9_lease_from_another_build_can_be_reclaimed_after_owner_exit` (`wt-baseline-U/crates/jackin-usage/src/host/broker/tests.rs:2868`) and `foreign_lease_recovery_fails_closed_for_live_unknown_future_and_malformed_owners` (`tests.rs:2898`) reclaim or compare the previous wire version. Integration `leader.rs:120-126` refuses every lease whose `protocol_version` is not the current v8. Porting that recovery would adopt the v9 lease rule.

U's `claude_rate_limit_retry_deadline` (`f5163e98`, `coordinator/policy.rs`) is also absent. It does not read schema 6 or wire v9, and it is not one of the two behaviors in this item. This check does not schedule it.

## 3. Exact account/surface/source/material fingerprint authorization — ALREADY_PRESENT

Integration functions, all in `crates/services/jackin-usage-broker/src/authorize.rs`:

- `credential_keys_match` at `authorize.rs:33` (closed alias sets only; account, surface, source, and material fingerprint stay exact).
- `credential_scope_has_matching_proof` at `authorize.rs:93` (`authorize.rs:108-109` requires `proof.source` and `proof.material_fingerprint`).
- `binding_matches_proof` at `authorize.rs:122` (`authorize.rs:142-143`).
- `record_key_binding` at `authorize.rs:146` (`authorize.rs:167` fails a duplicate launch key whose source or fingerprint differs).
- `authorize_credential_binding_group` at `authorize.rs:175`. Mixed profile and env returns `None` at `authorize.rs:194-196`. Missing material returns `None` at `authorize.rs:211-218`. More or fewer than one candidate returns `None` at `authorize.rs:245-247`.

`ScopedCapability` is absent. Do not restore it. `ForwardingRequirement` remains at `authorize.rs:15` without the old single `key` field.

`3ddbc056` `selected_routes_require_exact_source_proofs_and_same_identity`, `grouped_broker_authorization_accepts_sibling_proofs_and_rejects_conflicts`, `mixed_profile_and_env_group_fails_closed_without_changing_pure_profile`, and `launch_scope_fails_closed_on_rotation_repoint_and_mixed_agent_source` assert the same rejections the integration tree already has:

- wrong material, account, source, and identity yield `None` (`tests/case_03.rs:81-148`);
- conflicting fingerprint and duplicate launch-key material yield `None` (`tests/case_03.rs:220-240`);
- mixed profile/env yields `None` while a pure profile still authorizes (`tests/case_03.rs:333-334`);
- rotated, repointed, and mixed material yield `Unauthorized` (`tests/case_01.rs:62-99`).

U's copies of those tests (`host/broker/tests.rs:491`, `tests.rs:1307`, `tests.rs:2195`, `tests.rs:2385`) assert the same outcomes. Their bodies do not need schema 6 or wire v9, and they do not need to be ported again.

U `foreground_discovery_executor_rejects_non_claude_before_credentials_or_collector` (`host/broker/tests.rs:896`) asserts `Unauthorized` with zero credential and collector calls. The integration tree has no test of that name. The rejection is already implemented: `probe_with_scope` (`executor.rs:61-64`) returns `collector_not_authorized()` for a non-Claude capability when `collector_lease` is set, before `probe_foreground_claude`. The test body can be ported without schema 6 or wire v9 by using `collector_lease` instead of U's `claude_collector` callback. That would only add coverage for a check that is already present.

## 4. Revoked-account persisted result-free uncertainty marker — ALREADY_PRESENT

The integration tree already has the test.

`corrupt_revoked_account_is_replaced_with_result_free_uncertainty_marker` is `crates/services/jackin-usage-coordinator/src/tests/case_03.rs:7`. It keeps the concrete `FileAccountStateStore` (`case_03.rs:19-24`), rotates the catalog, and asserts the persisted marker is idle, reload-fenced, and result-free (`case_03.rs:35-41`: `terminal_result`, `last_good`, and `terminal_error` are `None`, and `provider_invoked_at_epoch` is `None`). A memory-store sibling is `case_02.rs:987` (`corrupt_removed_claude_state_becomes_result_free_uncertainty_marker`).

## Edit list

1. `dispatch_monitor_operation` in `crates/services/jackin-usage-broker/src/dispatch.rs:135-137`: on `ServiceStopped`, call `monitor_store.set_experimental_collector_source(None)`.
2. `serve` in `crates/services/jackin-usage-broker/src/serve.rs:223`: before fencing and joining workers, if `has_authenticated_foreground_guard` then call `monitor_store.set_experimental_collector_source(None)`.

Do not edit `should_exit_idle`. Do not restore `ScopedCapability` or `ClaudeCollectorLiveness`. Do not adopt monitor schema 6 or broker wire v9.

## Landed

Commit `738e2cfa` on `integration/branch-consolidation-20261013` (pushed). `should_exit_idle` unchanged.

- `dispatch.rs`: `ServiceStopped` stores shutdown and calls `set_experimental_collector_source(None)`.
- `serve.rs`: when `has_authenticated_foreground_guard` is true, the same clear runs before `fenced.store` and before joining workers.
- Test `dispatch::protocol_version_tests::service_stop_clears_the_experimental_collector_source` passed.
- The same crate's policy test now uses the imported `MonitorEvidenceFreshness::Stale` so `-D unused-qualifications` compiles.

Command: `env -u RUSTUP_TOOLCHAIN mise exec -- mbx +1.99.0 nextest run -p jackin-usage-provider-codex -p jackin-usage-broker -E 'test(codex_over_cap_keeps_raw_label_with_clamped_bar) | test(service_stop_clears_the_experimental_collector_source)'`. Exit 0. 2 passed. This note does not approve TASK-007.
