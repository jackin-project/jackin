# TASK-002 — Usage rescues B040 and B044

Read-only inspection of `/Users/donbeave/Projects/jackin-project/jackin`. No checkout, commit, merge, build, or worktree edit.

| Pin | SHA |
| --- | --- |
| P | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| U | `2e4a62da4bc4406764162a267163389a5fb513a7` |
| M | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| B040 | `2abed4ad67d8438ab9eb124b564be6ad5b545847` |
| B044 | `40823a76a3e20b901d5bc410979cee0931fd5c48` |

Neither tip is an ancestor of P (`merge-base --is-ancestor` exit 1). The reviewed set is only `git log --oneline P..TIP`: 17 commits on B040, 10 on B044. Both ranges are linear, one parent each.

**Absent from both P and U: 0 commits.** No path from these 27 commits needs to be rescued as behavior missing from both trees.

Quota provenance (used-only windows, reset watermarks) and typed provider status (`MonitorIssueCode`, numeric HTTP 401/403/429) are not introduced by these commits. Schema 6 and broker wire v9 are not the constants on either tip, and they are not treated as already selected. The revoked-account result-free marker is real and is already on P; U does not have that test.

## How a commit was classified

For each B040 commit, the cumulative patch from merge-base `c45f20b2b24f5c8d41348643546f9d8c1995f8d0` to the B040 tip was compared with the P tip by function body. 341 function bodies match. Zero B040 functions are missing on P. Five bodies differ only because later P commits wrap or extend them (`poll_due` delegates to `poll_due_filtered` and still uses `effective_due_epoch`; `ClockSample::anchored` is public under `test-support`; Claude keychain validation returns a diagnostic and zeroizes). Thirty-three of the 41 patched files are byte-identical at the P tip. The port commit on P is `f3ac746fe5925c3491bee6ae04417333485a173e` (`fix(usage): port reviewed safety updates onto C2 base`), an ancestor of P, and it names this B040 tip.

U was checked by function name and normalized body, not by name alone. A same-named function whose body differs is not treated as this behavior.

For each B044 commit, normalized function bodies were compared with its parent. Differences are `pub(crate)`, signature wrapping, and `super::` depth. No function was added or removed. Parent of the range, `d32e85563648931f80ffab141bb2650f478c2a60`, is an ancestor of P. Those commits relocate code P and U already carry under later crate paths.

## Constants actually on the tips

| Constant | B040 tip | B044 tip | P | U |
| --- | --- | --- | --- | --- |
| `USAGE_BROKER_PROTOCOL_VERSION` | `v8` (unchanged from its base; the range does not edit this const) | `v3` | `v8` | `v9` |
| `USAGE_MONITOR_SCHEMA_VERSION` | not changed by the range | not defined on this tree | `4` | `6` |
| `ACCOUNT_STATE_SCHEMA_VERSION` | `3`, previous `2`, legacy `1` | `1` (moved, not changed) | `3`, previous `2`, legacy `1` | `2`, previous `1` |
| `PROJECTION_STATE_SCHEMA_VERSION` | `3`, previous `2` | `2` (moved, not changed) | `3`, previous `2` | `3`, previous `2` |

B040's wire-v8 `UnverifiedHandle` migration is the behavior in the first two commits. U's wire v9 and monitor schema 6 do not contain those migration tests. B044 does not select either pair; it still has wire v3.

## Revoked account, quota provenance, typed status

Revoked-account behavior to keep, already on P and absent from U:

- `corrupt_revoked_account_is_replaced_with_result_free_uncertainty_marker` in `crates/services/jackin-usage-coordinator/src/tests/case_03.rs`. The tip commit keeps a concrete `Arc<FileAccountStateStore>` so the test reads the persisted marker after catalog rotation. The same body is on P. U has no match.
- `corrupt_removed_claude_state_becomes_result_free_uncertainty_marker` and `catalog_revocation_clears_materialized_data_and_fences_late_result` in `crates/services/jackin-usage-coordinator/src/tests/case_02.rs`. Both are on P. U has no match.
- `cfb2aabd` asserts the terminal view is `UsageRefreshPhase::Failed` with `UsageCoordinationErrorKind::CatalogRevoked` inside `after_apply_purge_failure_restores_preimage_and_catalog`. That assertion is in the identical `case_02.rs` on P.

These commits do not add used-only quota, reset-watermark provenance, `MonitorIssueCode`, or `MonitorProviderReadiness`. Fixture epoch `1_401` is a timestamp inside catalog-revocation tests. P's later typed HTTP status remains the provider-core / Claude collector path already described for U versus P; nothing in this range changes it.

## B040 — 17 commits

Merge-base with P is `c45f20b2b24f5c8d41348643546f9d8c1995f8d0` (`test(usage): use a valid v7 broker envelope`). Every behavior below is on P at the same path unless a later P wrapper is named. U paths are cited only where the normalized body matches.

### `2a93f9375739206447073d9b7e4aafdb5ee16f3a`

`feat(usage): migrate projection identity to v8`

Files: `crates/apps/jackin/tests/usage_broker_e2e/recovery.rs`, `crates/core/jackin-protocol/src/usage_broker.rs`, `crates/core/jackin-protocol/src/usage_broker/tests/case_01.rs`, `crates/services/jackin-usage-broker-publish/src/errors.rs`, `crates/services/jackin-usage-broker-publish/src/publisher.rs`, `crates/services/jackin-usage-broker-publish/src/tests/case_02.rs`, `crates/services/jackin-usage-broker-publish/src/windows.rs`, `crates/services/jackin-usage-broker/src/errors.rs`, `crates/services/jackin-usage-broker/src/projection.rs`, `crates/services/jackin-usage-broker/src/tests/case_02.rs`, `crates/services/jackin-usage-coordinator/README.md`, `crates/services/jackin-usage-coordinator/src/errors.rs`, `crates/services/jackin-usage-coordinator/src/reconcile.rs`, `crates/services/jackin-usage-coordinator/src/state.rs`, `crates/services/jackin-usage-coordinator/src/state/envelope.rs`, `crates/services/jackin-usage-coordinator/src/state/stores.rs`, `crates/services/jackin-usage-coordinator/src/state/tests/case_01.rs`, `crates/ui/jackin-console/src/tui/screens/usage/labels_time.rs`.

Behavior on P at those paths: `UnverifiedHandle` as its own wire kind, startup migration of projection schema 2 before exposure, unknown legacy provider left unwritten, `load_for_broker_migration`. Tests `unverified_identity_has_its_own_wire_kind`, `startup_migrates_projection_v2_before_exposing_identity`, `unknown_legacy_provider_fails_without_replacing_projection_bytes`, `missing_discovery_identity_is_unverified_not_provider_authority`, `projection_v2_is_visible_only_to_the_broker_migration_loader`. P's README and `protocol-v8-contract.md` later say account-state schema 3; the code constant on this tip is already 3 after `5290935c`, and P's envelope file matches the tip.

U has the `UnverifiedHandle` variant under `crates/jackin-protocol/src/usage_broker.rs` and `crates/jackin-usage/src/host/broker.rs`. U does not have the migration tests. U's `migrate_legacy_projection` in `crates/jackin-usage/src/host/broker/publish.rs` is a shorter body. U does have `projection_v2_is_visible_only_to_the_broker_migration_loader` at `crates/jackin-usage/src/coordinator/state/tests.rs` with the same body.

### `855a092c9110081ba0cd988a4e516ab161f60746`

`docs(usage): clarify v8 unverified identity`

Files: `crates/services/jackin-usage/src/tests/case_01.rs`, `crates/services/jackin-usage/src/tests/support.rs`, `plans/claude-unattended-monitor/protocol-v8-contract.md`.

`contract_projection_accepts_unverified_identity_without_claiming_authority` is on P in the same test file. The contract doc's account-schema sentence was updated on P to schema 3. U has neither the test nor this contract section.

### `d3ec39f1fa90542158b3a1c434a7113ea99fb804`

`feat(usage): fence pending probes through catalog rotation`

Files: `crates/services/jackin-usage-coordinator/src/entries.rs`, `finish.rs`, `jobs.rs`, `join.rs`, `lib.rs`, `reconcile.rs`, `refresh.rs`, `tests/case_02.rs`, `tests/support_01.rs`, `upkeep.rs`, `worker.rs`.

On P at the same paths: `ClockSample`, `finish_fenced_provider_attempt`, `has_pending_provider_attempt`, `refresh_runtime_cooldown`, pending-probe fences through catalog rotation. P later publishes `ClockSample` when `test-support` is on; the sample math is the same.

U matches `has_pending_provider_attempt` at `crates/jackin-usage/src/coordinator/reconcile.rs` and `pending_attempt_envelope` at `crates/jackin-usage/src/coordinator/upkeep.rs`. U's `finish_fenced_provider_attempt`, `refresh_runtime_cooldown`, and `catalog_purge_tombstone` bodies differ.

### `49de6c4644cb9e91f9818a0ba47da823be946c55`

`test(usage): inject deterministic coordinator clocks`

Files: `crates/services/jackin-usage-coordinator/src/jobs.rs`, `tests/case_01.rs`, `tests/case_02.rs`, `tests/case_03.rs`, `tests/support_01.rs`, `tests/support_02.rs`.

`system_clock_samples_real_wall_and_monotonic_domains`, `coordinator_with_fake_clock`, and `catalog_coordinator_with_fake_clock` are on P in those files. U has no `system_clock_samples_real_wall_and_monotonic_domains`.

### `812308491eb6c25a46bc9e95c2b911c57d10895b`

`fix(usage): preserve provider cooldown across reloads`

Files: `crates/services/jackin-usage-coordinator/src/entries.rs`, `join.rs`, `tests/case_02.rs`, `tests/support_01.rs`, `plans/unified-agent-usage/spec/broker-refresh.md`.

`restart_reload_fence_blocks_forward_wall_jump_but_not_fresh_accounts` and `unresolved_updating_marker_gets_reload_fence_without_fabricated_invocation` are on P in `tests/case_02.rs`. `broker-refresh.md` matches P byte for byte. U has neither test.

### `5290935c0afb9f9d529ddfd98d38e6014bc53b0e`

`fix(usage): persist uncertain reload fences`

Files: `crates/services/jackin-usage-coordinator/src/finish.rs`, `join.rs`, `state.rs`, `state/envelope.rs`, `state/stores.rs`, `state/tests/case_01.rs`, `state/tests/support.rs`, `tests/case_02.rs`, `upkeep.rs`, `plans/unified-agent-usage/spec/broker-refresh.md`.

Sets `ACCOUNT_STATE_SCHEMA_VERSION` to 3 with previous 2 and legacy 1, and adds `account_state_v2_migration_persists_unresolved_attempt_provenance_atomically`. That test and the envelope constants are on P in the same files. U's account schema is 2 and the test is absent.

### `93a2f31bed9ff7a50c0b4bc02e8aee530e2e0f3c`

`test(usage): wait for fenced state persistence`

File: `crates/services/jackin-usage-coordinator/src/tests/case_02.rs`.

The wait in `catalog_revision_keeps_pending_marker_and_completion_floor_across_readd` observes a durable Idle marker with `success_deadline_epoch == Some(1_320)` instead of treating provider return as commit. That test body is on P in the same file. U does not have the test.

### `51268e7778e6e1e012e7f2a213aefd8100b7f654`

`fix(usage): preserve ambiguous legacy attempt fences`

Files: `crates/services/jackin-usage-coordinator/src/state/envelope.rs`, `state/stores.rs`, `state/tests/case_01.rs`, `tests/case_02.rs`.

`account_state_v1_fresh_queued_migration_does_not_invent_invocation_fence` is on P in `state/tests/case_01.rs`. U does not have it.

### `89732048e7e9525f6cc0be0df48997c1ecf56e14`

`fix(usage): require clean evidence for legacy queued state`

Files: `crates/services/jackin-usage-coordinator/src/state/stores.rs`, `state/tests/case_01.rs`.

A queued legacy row with a retry deadline stays ambiguous (`reload_fence_required` is set only for a fresh queued generation). The store and the extended test are on P in those files. U does not have this test.

### `ec9b026b0c253531bee60ff4749f948a3b74e661`

`fix(usage): fence malformed legacy updating state`

Files: `crates/services/jackin-usage-coordinator/src/state/stores.rs`, `state/tests/case_01.rs`.

`account_state_v1_migration_distinguishes_fresh_queue_from_ambiguous_attempts` is on P in `state/tests/case_01.rs`. U does not have it.

### `616802249595bd343c5704abd3f4ea20cdfe4de1`

`fix(usage): preserve cooldown fences through catalog cleanup`

Files: `crates/services/jackin-usage-coordinator/src/cadence.rs`, `entries.rs`, `reconcile.rs`, `tests/case_02.rs`, `tests/case_03.rs`, `crates/services/jackin-usage-discovery/src/profiles.rs`, `crates/services/jackin-usage-provider-claude/src/keychain.rs`, `crates/services/jackin-usage-provider-claude/src/lease.rs`.

On P: `monotonic_cooldown_deadline_epoch`, `recovered_attempt_deadline_epoch`, `effective_due_epoch`, marker-before-purge and rollback tests (`after_apply_marker_store_failure_restores_preimage_before_any_purge`, `failed_reconcile_does_not_erase_marker_when_store_preimage_was_missing`, `after_apply_purge_failure_restores_preimage_and_catalog`, `later_marker_store_failure_rolls_back_prior_marker_before_any_purge`, `crash_after_durable_cooldown_marker_precedes_purge_and_survives_restart`, `corrupt_removed_claude_state_becomes_result_free_uncertainty_marker`). P's `poll_due` now calls `poll_due_filtered` and still skips revoked and blocked accounts via `effective_due_epoch`.

U matches `monotonic_cooldown_deadline_epoch` and `effective_due_epoch` at `crates/jackin-usage/src/coordinator/entries.rs`. U's `recovered_attempt_deadline_epoch` body differs. The marker and revoked-account tests are absent from U. `non_macos_public_keychain_reader_stays_missing` is on P in `keychain.rs` and absent from U.

### `5db2606eddf58e39fe0a3e8698903e65655d9bf1`

`fix(usage): remove stale cooldown import`

File: `crates/services/jackin-usage-coordinator/src/reconcile.rs`.

Drops the unused `account_cooldown_deadline` import. `reconcile.rs` at the P tip matches the B040 tip. No separate behavior.

### `52b50f1041e7dd43454fbaae74c871b8d261b92c`

`test(usage): coerce crash fixture ports explicitly`

File: `crates/services/jackin-usage-coordinator/src/tests/case_02.rs`.

Port coercions in the crash fixture are in the P copy of this file. U does not have `crash_after_durable_cooldown_marker_precedes_purge_and_survives_restart`.

### `a68757efce321f302a20244460e2e42f53ec5b2f`

`test(usage): explicitly drop injected crash result`

File: `crates/services/jackin-usage-coordinator/src/tests/case_02.rs`.

`drop(coordinator.reconcile_catalog([], 1_002))` inside the crash test is in the P copy of this file.

### `cfb2aabd8eebc3b32111af7b2a4fd250219c75a3`

`test(usage): assert terminal catalog revocation view`

File: `crates/services/jackin-usage-coordinator/src/tests/case_02.rs`.

After purge failure, the catalog view is `Failed` / `CatalogRevoked`. That assertion is in the P copy of `after_apply_purge_failure_restores_preimage_and_catalog`. U does not have this test.

### `e2135912718253454c1dfec4c719ef937dace9e2`

`test(usage): align recovery and stale lease fixtures`

Files: `crates/services/jackin-usage-coordinator/src/entries.rs`, `tests/case_02.rs`, `tests/case_03.rs`, `crates/services/jackin-usage-provider-claude/src/lease/tests.rs`.

`corrupt_revoked_account_is_replaced_with_result_free_uncertainty_marker` and `stale_generation_cannot_read_or_replace_newer_cached_credential` are on P at those paths. U has neither. P later extends `bootstrap_claude_credential_with` with a typed malformed diagnostic; the stale-generation test body still matches the B040 tip.

### `2abed4ad67d8438ab9eb124b564be6ad5b545847`

`test(usage): retain state store for marker assertion`

File: `crates/services/jackin-usage-coordinator/src/tests/case_03.rs`.

Keeps `store.clone()` as `Arc<dyn AccountStateStore>` with the `clone_on_ref_ptr` reason, so the revoked-account test can read the persisted marker. The function body matches P. U does not have the test. P also has two later cadence tests in this file (`selected_cadence_polls_only_the_opted_in_capability`, `selected_cadence_empty_duplicate_and_revoked_inputs_are_safe`); those are P additions around the same test.

## B044 — 10 commits

Merge-base with P is `d32e85563648931f80ffab141bb2650f478c2a60`, an ancestor of P. Each commit splits `crates/services/jackin-usage` and re-exports the old paths. Normalized function bodies match the parent. The rescue subject on the last commit is the same kind of split; `host/broker/tests.rs` gains imports so moved items stay visible, and its function bodies do not change.

Current homes for the moved responsibilities:

| Split | P path | U path |
| --- | --- | --- |
| Claude windows, wave, spend | `crates/services/jackin-usage-provider-claude/src/` (`windows.rs`, `wave.rs`, `spend.rs`) | `crates/jackin-usage/src/usage/claude.rs` |
| Codex, Cursor, Grok, Kimi, and the other providers | `crates/services/jackin-usage-provider-*/` | `crates/jackin-usage/src/usage/<provider>.rs` |
| Coordinator, catalog, state | `crates/services/jackin-usage-coordinator/src/` | `crates/jackin-usage/src/coordinator/` |
| Snapshot schema | `crates/services/jackin-usage-snapshot-store/src/schema.rs` | `crates/jackin-usage/src/usage_snapshot_store.rs` |
| Catalog entries | `crates/services/jackin-usage-discovery/src/capabilities.rs` | `crates/jackin-usage/src/host/` equivalents where the name survives |

Checked examples that exist at both later homes: `claude_denied_view`, `push_claude_dollar_windows`, `kimi_pace_with_over_cap`, `remaining_from_used_percent`, `validate_catalog_entries`, `initialize_schema`. `lock_activation` is moved into `host/broker/ensure.rs` by the last commit and already existed at `d32e855` in `host/broker.rs`. P commit `3f3ddd7c` (`feat(usage): add explicit Claude usage monitoring (#1122)`), which is reachable from P, is the last P commit that touches that function. It is not behavior added by B044.

Wire stays `v3` across the range. Account schema stays `1` and projection schema stays `2`, moved into `coordinator/state/envelope.rs`. That is not schema 6 or wire v9.

### `b5b66a7faf18c323a7e1338a9141e0624e9f650f`

`S7 split: jackin-usage file splits (zai, openrouter, amp, antigravity)`

Files: `usage/amp.rs` plus `amp/{fetch,parse,snapshot,types,views}.rs`; `usage/antigravity.rs` plus `antigravity/{buckets,cli,credits,parse,snapshot,types}.rs`; `usage/openrouter.rs` plus `openrouter/{fetch,parse,snapshot,types}.rs`; `usage/zai.rs` plus `zai/{buckets,fetch,quota,snapshot}.rs`. All under `crates/services/jackin-usage/src/`.

### `39b4d39edc2288421e06479a14a5a4c00139c866`

`S7 split: jackin-usage file splits (minimax, kimi)`

Files: `usage/kimi.rs` plus `kimi/{buckets,fetch,local,snapshot,types}.rs`; `usage/minimax.rs` plus `minimax/{balance,buckets,fetch,region,snapshot,types}.rs`.

### `b950a152e0ea07bcf78d53c5c43c12ba2a49137d`

`S7 split: jackin-usage file splits (cursor, grok)`

Files: `usage/cursor.rs` plus `cursor/{auth,events,period,plan,requests,rest,sand,snapshot,summary,teams}.rs`; `usage/grok.rs` plus `grok/{billing,rpc,snapshot,types,views}.rs`.

### `40dc9004b4a2d2d646995f5de86be11eeae7eebb`

`S7 split: jackin-usage file splits (claude, codex)`

Files: `usage/claude.rs` plus `claude/{cli,credentials,keychain,oauth_types,refresh,snapshot,spend,wave,windows}.rs`; `usage/codex.rs` plus `codex/{credentials,endpoints,oauth,rpc,rpc_types,snapshot,types,views,windows}.rs`.

### `9158cf03423a061e9c1ae69eb3e8ae35387402c7`

`S7 split: jackin-usage file splits (format, view)`

Files: `usage/claude/cli.rs`, `usage/format.rs` plus `format/{cli,labels,numbers,presentation,text}.rs`, `usage/view.rs` plus `view/{cache,compose,meta,refresh,snapshots,status,tabs}.rs`.

### `46a24b4f2b2e081b9fc7a44929e0470ab69b9eab`

`S7 split: jackin-usage file splits (usage)`

Files: `usage.rs` plus `usage/{cache,cache_keys,consts,credentials,diagnostic,fallback,http,io,labels,outcome,surface,transport}.rs`.

### `4191a567471aff3988127af8d781469a7ed43e8a`

`S7 split: jackin-usage file splits (token_monitor, state, accounts)`

Files: `coordinator/state.rs` plus `state/{envelope,sanitize,stores,validate}.rs`; `host/accounts.rs` plus `accounts/{catalog,identity,lifecycle,materialize,views}.rs`; `token_monitor.rs` plus `token_monitor/{discover,monitor,record,session,status,totals}.rs`.

### `123bd4375e8050df9606a4caeda8f18bebd8da3f`

`S7 split: jackin-usage file splits (snapshot_store, projection, publish)`

Files: `host/broker/publish.rs` plus `publish/{errors,merge,publisher,quota,revoke,windows}.rs`; `host/projection.rs` plus `projection/{account,canonical,destination,freshness,groups,runtime}.rs`; `usage_snapshot_store.rs` plus `usage_snapshot_store/{buckets,read,schema,types,upsert,views,write}.rs`.

### `459507ea14b28b5c55edafbd48a8072889fb0330`

`S7 split: jackin-usage file splits (coordinator)`

Files: `coordinator.rs` plus `coordinator/{cadence,catalog,config,construct,entries,errors,finish,jobs,join,outcome,reconcile,refresh,upkeep,worker}.rs`.

### `40823a76a3e20b901d5bc410979cee0931fd5c48`

`rescue: preserve local working state`

Files: `host/broker.rs`, `host/broker/tests.rs`, and `host/broker/{authorize,capabilities,client,config,consts,dispatch,ensure,errors,executor,forwarding,handle,leader,lease,paths,projection,rediscover,runtime,serve,service,startup}.rs`.

This is the broker module split, including `authorize.rs`. Credential-binding and Claude window code moved here already exist at the parent and at the P and U homes above. The commit does not add a new binding rule, freshness rule, or schema.

## Result

0 commits have behavior absent from both P and U. There is no path to preserve from that set.

Coordinator addendum: that sentence covers the 27 commits, not the trees those commits sit on. Parent `d32e85563648931f80ffab141bb2650f478c2a60` is an ancestor of P, and 360 later policy commits follow it. `catalog_cas_rejects_a_stale_rotation_after_a_newer_winner` and `canonical_projection_alias_transition_is_atomic_idempotent_and_fail_closed` are on that parent and absent from the P tip. The 206 B044 names in `b044-fn-names-absent-from-M-P-U.txt` are a deletion question inside P's later history, not a reason to cherry-pick these 10 split commits.

Keep the revoked-account marker tests already on P (`case_03.rs` and the `case_02.rs` revocation assertions). They are absent from U, and they are already in the policy tree, so the B040 tip is not an extra source. Leave schema 6 and wire v9 unselected; neither tip carries that pair.
