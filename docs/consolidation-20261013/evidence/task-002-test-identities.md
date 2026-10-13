# TASK-002 test identities (source names only)

Read-only. No compile, no `cargo`, no `nextest`, no edits under the three worktrees. Nothing here is a runtime pass or fail.

| Tree | Path | HEAD |
| --- | --- | --- |
| M | `/Users/donbeave/Projects/jackin-project/wt-baseline-M` | `1c3dd1be` |
| P | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60` |
| U | `/Users/donbeave/Projects/jackin-project/wt-baseline-U` | `2e4a62da` |

Collected with Homebrew ripgrep 15.2.0 (`/opt/homebrew/bin/rg`). The mise `rg` shim does not run in the policy worktree (that tree asks for mise 2026.10.7; the installed mise is 2026.10.4). Attribute scan: `#[test]`, `#[tokio::test]`, `#[rstest]`, then the next `fn`. `crates/` and `native/`, excluding `target/`, `vendor/`, and `fuzz/`.

| Tree | Extracted test fns | Unique names |
| --- | ---: | ---: |
| M | 7473 | 7436 |
| P | 7503 | 7466 |
| U | 7514 | 7477 |

No `#[rstest]` function exists in those trees. Each tree has three `#[rstest]` / `case::` hits, and they are string literals in `jackin-xtask` (`test_layout/tests.rs`, `docs/specs.rs`), not tests.

## Scope

A name is in scope when the function name (and, where noted, the path) matches one of these topics. Presence is by function name anywhere in that tree. A move that keeps the name is not a loss.

| Topic | Name rule |
| --- | --- |
| Broker idle / liveness | `idle`, `liveness`, `sleep_jump`, `keepalive`, `spawned_ticker_`, `ticker_collector_preserves_minimum_attempt_floor`, `lease_renewal_due_uses_wall_or_monotonic_elapsed`, `fenced_connection_workers_drop_queued`, under broker / serve-loop / coordinator / lease paths. UI, clipboard, agent-status, and caffeinate idle tests are out. |
| Provider status | `401` / `403` / `429`, `provider_error`, `scope_restriction`, `keychain_status`, `rate_limited`, `consent_revok`, `typed_status`, `auth_status_mapping`, plus the Claude lease/collector consent and generation tests named below. |
| Quota windows | `quota` in the name, or `window` in a monitor / provider / projection / protocol path. |
| Monitor wire / schema | `wire` or `schema` in protocol, usage-broker, or monitor paths, plus the three usage-monitor wire tests called out below. |
| Locks | `_lock` / `locked` / `locks` / `inode` / `live_lease_owner` / `lifetime_lock` / `repo_lock`. `bounded_locked` and `clock` are out. |
| Purge | `purge` in the name. |
| Model / effort | `effort`, `model_override`, or `reasoning_effort`. |
| Diagnostic redaction / retry | `redact` or `scrub` in the name, or `retry` under diagnostics, telemetry, OTLP, or redact paths. Provider HTTP retry stays under provider status. |

In-scope unique names: M 263, P 270, U 276.

## Counts

| Set | Count |
| --- | ---: |
| Names only in M (absent from P and U) | **0** |
| Names only in U (absent from P and M) | **12** |
| Names only in P (absent from M and U) | **40** |
| Names in M or U whose name is absent from P | **46** |
| Of those 46, same assertion found nearby on P (rename or absorption) | 12 |
| Of those 46, looked nearby and the assertion is not there (dropped) | **34** |

Per topic, names on M or U that are absent from P:

| Topic | M | P | U | Absent from P |
| --- | ---: | ---: | ---: | ---: |
| Broker idle / liveness | 5 | 6 | 6 | 0 |
| Provider status | 22 | 24 | 26 | 17 |
| Quota windows | 55 | 66 | 61 | 9 |
| Monitor wire / schema | 16 | 9 | 19 | 14 |
| Locks | 52 | 51 | 52 | 2 |
| Purge | 29 | 29 | 29 | 5 |
| Model / effort | 26 | 26 | 26 | 0 |
| Diagnostic redaction / retry | 60 | 61 | 60 | 0 |

Broker idle, model/effort, and diagnostic redaction/retry contribute no missing M/U names. `ticker_collector_preserves_minimum_attempt_floor_and_retry_after` is on P and U, not on M. `debug_redacts_scope_labels` is the one redaction name only on P.

`spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch` is on all three. On P it still asserts an active monitor blocks idle exit and that the foreground guard blocks idle exit (`should_exit_idle`’s last flag is `has_authenticated_foreground_guard` on P and `has_foreground_liveness` on M).

- M `crates/jackin-usage/src/host/broker/serve_loop/tests.rs:373`
- P `crates/services/jackin-usage-broker/src/serve/tests.rs:376`
- U `crates/jackin-usage/src/host/broker/serve_loop/tests.rs:589`

Cleanup / `ensure_absent` test names were compared separately (path contains `cleanup-absent` or the name contains `ensure_absent` or `purge` under runtime). That set is 81 names on M, P, and U, with no name present on only one tree.

## Renamed, not dropped

Same assertion is on P under another name. These 12 are part of the 46 absent names and part of the 40 P-only names (the new name). They are not in the dropped list.

| M/U name | M path | P name and path |
| --- | --- | --- |
| `broker_conflict_has_a_stable_wire_name` | `crates/jackin-protocol/src/usage_broker/tests.rs:50` | `broker_conflict_has_a_stable_sanitized_wire_name` at `crates/core/jackin-protocol/src/usage_broker/tests/case_01.rs:45`. Both assert the encoded kind is `"broker_conflict"`. |
| `local_source_identity_kind_has_an_explicit_v8_wire_name` | `crates/jackin-protocol/src/usage_broker/tests.rs:61` | `local_source_identity_has_its_own_wire_kind` at `crates/core/jackin-protocol/src/usage_broker/tests/case_01.rs:53`. Both encode and decode `"local_source_handle"`. |
| `unverified_identity_kind_has_an_explicit_v8_wire_name` | `crates/jackin-protocol/src/usage_broker/tests.rs:74` | `unverified_identity_has_its_own_wire_kind` at `crates/core/jackin-protocol/src/usage_broker/tests/case_01.rs:65`. Both encode and decode `"unverified_handle"`. |
| `collector_auth_required_has_stable_wire_code` | `crates/jackin-protocol/src/usage_monitor/tests.rs:35` | Absorbed into `v4_monitor_control_shapes_are_tagged_and_secret_free` at `crates/core/jackin-protocol/src/usage_monitor.rs:1043`, which asserts `MonitorIssueCode::CollectorAuthRequired` encodes as `"collector_auth_required"`. The M message `"experimental collection requires foreground setup"` is not in that P test. |
| `binding_input_defaults_experimental_collector_approval_to_false` | `crates/jackin-protocol/src/usage_monitor/tests.rs:46` | Same P test. A binding object without `experimental_collector_approved` still decodes as false. |
| `classify_claude_keychain_status_maps_denial_and_absence` | `crates/jackin-usage/src/usage/tests.rs:4315` | `classify_claude_keychain_status_distinguishes_denial_absence_and_consent` at `crates/services/jackin-usage-provider-claude/src/tests/case_04.rs:48`. Same arms: `-128` and `-25293` denied, `-25300` missing, `-25308` consent, `-1` missing. |
| `scope_restriction_requires_typed_http_403` | `crates/jackin-usage/src/usage/claude/tests.rs:64` | `scope_restriction_requires_typed_http_403_and_keeps_status_in_error` at `crates/services/jackin-usage-provider-claude/src/tests/case_01.rs:7`. Same strings: `"Claude OAuth usage request failed: status 403"` and `"Claude OAuth usage decode failed: payload mentions 401"`. |
| `cli_contract_policy_approval_defaults_strict_sgd_and_never_infers_quota_only` | M `crates/jackin/src/cli/usage/tests.rs:840`; U `crates/jackin/src/cli/usage/tests.rs:887` | `policy_approval_defaults_strict_sgd_and_never_infers_quota_only` at `crates/apps/jackin/src/cli/usage/tests.rs:488`. Seven assert lines match. |
| `metric_groups_project_window_spend_and_plan` | `crates/jackin-usage/src/host/projection/tests.rs:269` | `account_projects_window_spend_and_plan_groups` at `crates/services/jackin-usage-projection/src/tests/case_01.rs:92`. Same weekly/spend slot and money-limit asserts. |
| `typed_401_allows_one_changed_exact_source_retry` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:159` | `changed_exact_source_is_reread_and_retried_once_after_401` at `crates/services/jackin-usage-provider-claude/src/collector.rs:332`. One reread, tokens `old-token` then `new-token`. |
| `revoked_consent_after_first_success_rejects_provider_result` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:261` | `consent_revoked_after_success_discards_the_provider_view` at `crates/services/jackin-usage-provider-claude/src/collector.rs:388`. One request, no Keychain reread, consent revoked with no HTTP status. |
| `failed_purge_after_removal_restores_catalog_and_preimage` | M `crates/jackin-usage/src/coordinator/tests.rs:3466`; U `crates/jackin-usage/src/coordinator/tests.rs:3603` | `after_apply_purge_failure_restores_preimage_and_catalog` at `crates/services/jackin-usage-coordinator/src/tests/case_02.rs:758`. Purge failure returns `Unavailable`, restores the preimage, and leaves the phase `Completed`. Fixture type differs (`TombstoneWriteFailureStore` vs `AfterApplyFailureStore`). |

U has the same function names as M for every row above, at the M path unless a U line is shown.

## Dropped rather than renamed

34 names. Fixture strings and the asserted outcome were searched on P. A nearby P test that checks a different outcome is noted and is not treated as the same test.

### On M and U, absent from P (22)

| Function | Path |
| --- | --- |
| `service_status_exposes_secret_free_foreground_collector_mode` | `crates/jackin-protocol/src/usage_monitor/tests.rs:11` |
| `monitor_and_broker_owned_relay_operations_have_v8_wire_shapes` | `crates/jackin-protocol/src/usage_broker/tests.rs:126` |
| `future_store_schema_is_rejected` | M `crates/jackin-usage/src/host/broker/monitor/tests.rs:4255`; U `.../monitor/tests.rs:4800` |
| `v2_store_migrates_explicitly_to_current_schema` | M `crates/jackin-usage/src/host/broker/monitor/tests.rs:3604`; U `.../monitor/tests.rs:3711` |
| `v2_store_rejects_wrong_nested_event_schema_without_rewriting_source` | M `crates/jackin-usage/src/host/broker/monitor/tests.rs:3649`; U `.../monitor/tests.rs:3757` |
| `v3_migration_rejects_wrong_or_future_nested_event_schema_without_rewriting_source` | M `crates/jackin-usage/src/host/broker/monitor/tests.rs:3831`; U `.../monitor/tests.rs:3983` |
| `fresh_quota_callbacks_do_not_refresh_same_or_omitted_model_evidence` | M `crates/jackin-usage/src/host/broker/monitor/tests.rs:6333`; U `.../monitor/tests.rs:6878` |
| `live_lease_owner_blocks_expired_takeover_and_can_renew_after_waking` | M `crates/jackin-usage/src/host/broker/tests.rs:2077`; U `.../broker/tests.rs:2680` |
| `expired_lease_is_reclaimed_after_dead_owner_releases_lifetime_lock` | M `crates/jackin-usage/src/host/broker/tests.rs:2160`; U `.../broker/tests.rs:2763` |
| `failed_later_purge_prewrites_lazy_success_tombstone_before_purge` | M `crates/jackin-usage/src/coordinator/tests.rs:3798`; U `.../coordinator/tests.rs:3935` |
| `failed_later_purge_during_revision_reset_restores_pending_marker` | M `crates/jackin-usage/src/coordinator/tests.rs:3803`; U `.../coordinator/tests.rs:3940` |
| `failed_later_purge_with_missing_pending_preimage_preserves_restart_fence` | M `crates/jackin-usage/src/coordinator/tests.rs:3808`; U `.../coordinator/tests.rs:3945` |
| `failed_later_purge_prewrites_lazy_retry_after_tombstone_before_purge` | M `crates/jackin-usage/src/coordinator/tests.rs:3813`; U `.../coordinator/tests.rs:3950` |
| `revoked_consent_before_first_request_skips_provider_and_keychain` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:217` |
| `first_provider_error_is_preserved_when_consent_revokes_after_response` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:303` |
| `revoked_consent_after_401_skips_keychain_reread_and_preserves_401` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:354` |
| `revoked_consent_after_keychain_reread_skips_retry_and_preserves_401` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:412` |
| `stop_before_retry_admission_skips_fetch_after_changed_credential_reread` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:467` |
| `revoked_generation_cannot_replace_cache_or_clear_newer_generation` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:579` |
| `service_stop_and_lease_release_during_blocked_401_reread_skip_retry_and_keep_no_ui_guard` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:638` |
| `retry_429_metadata_survives_consent_revocation_after_response` | M `crates/jackin-usage/src/usage/claude/lease/tests.rs:748`; U `.../lease/tests.rs:1019` |
| `typed_403_and_unchanged_401_never_trigger_a_second_fetch` | M `crates/jackin-usage/src/usage/claude/lease/tests.rs:817`; U `.../lease/tests.rs:1088` |

Why these are not renames:

- `service_status_exposes_secret_free_foreground_collector_mode` uses `"claude-source-account"`. That string is absent on P.
- `monitor_and_broker_owned_relay_operations_have_v8_wire_shapes` asserts monitor `service_status` / `service_stopped`, `resolve_relay_capabilities`, `wait_timeout`, and `relay_capabilities`. Those asserts are not in P protocol tests. Only the trailing `reconcile_catalog` rejection is nearby, in `caller_supplied_catalog_reconciliation_is_not_a_wire_operation` (`crates/core/jackin-protocol/src/usage_broker/tests/case_01.rs:117`).
- `future_store_schema_is_rejected`, both `v2_store_*` tests, and `v3_migration_rejects_wrong_or_future_nested_event_schema_without_rewriting_source`: the fixture strings (`future monitor schema must be rejected`, `v2-migration-event`, `v2-wrong-event-schema`, `v3-wrong-event-schema`) are absent on P. P monitor tests cover other migrations (`v1_disk_migration_rewrites_and_reopens_without_granting_dispatch` and similar), not these cases.
- `fresh_quota_callbacks_do_not_refresh_same_or_omitted_model_evidence`: `"session-model-field-ttl"` is absent on P.
- `live_lease_owner_blocks_expired_takeover_and_can_renew_after_waking` asserts a live lock blocks takeover after wall-clock expiry and that the owner can renew after waking. P `expired_live_owner_lock_prevents_successor_takeover` (`crates/services/jackin-usage-broker/src/tests/case_05.rs:133`) only fences takeover. The renew-after-wake messages are absent. `claim_leader_at` is absent on P.
- `expired_lease_is_reclaimed_after_dead_owner_releases_lifetime_lock` asserts that dropping the live owner lets the same build reclaim, and that another build cannot. Those messages are absent on P. `stale_lease_reclaimed` appears in `stale_lease_does_not_reclaim_a_live_foreign_socket`, which is a different case.
- The four `failed_later_purge_*` wrappers call `failed_later_purge_case` with revision-reset, missing-preimage, and retry-after flags. Accounts `a-pending-account`, `m-terminal-cooldown-account`, and `z-later-purge-account` are absent on P. Nearby P tests (`later_marker_store_failure_rolls_back_prior_marker_before_any_purge`, `crash_after_durable_cooldown_marker_precedes_purge_and_survives_restart`) use other stores and accounts.
- `revoked_consent_before_first_request_skips_provider_and_keychain` asserts zero fetches and zero Keychain reads. No P Claude test asserts that zero-call case.
- `first_provider_error_is_preserved_when_consent_revokes_after_response` asserts the result stays `Provider(HttpStatus { status: 429, retry_after_seconds: Some(120), response_received_at_epoch: Some(1_000) })`. Nearby `consent_revoked_after_provider_error_discards_error_and_keeps_only_status` asserts `ConsentRevoked { provider_http_status: Some(429) }` instead.
- `retry_429_metadata_survives_consent_revocation_after_response` asserts that same `retry_after_seconds: Some(120)` provider error after a 401 reread. Nearby `consent_revoked_after_retry_discards_provider_error_and_keeps_only_status` asserts `ConsentRevoked { provider_http_status: Some(429) }` and does not keep the retry metadata.
- `revoked_consent_after_401_skips_keychain_reread_and_preserves_401` asserts reread count 0 and `ConsentRevoked { provider_http_status: Some(401) }`. P `claude_401_does_not_implicitly_reread_same_source_or_retry` asserts `NeedsLogin` with no source reread, which is a different outcome. P `consent_revoked_after_missing_keychain_reread_discards_first_401_view` does reread Keychain (`Missing`) and then reports consent revoked.
- `revoked_consent_after_keychain_reread_skips_retry_and_preserves_401` asserts one fetch, one reread of a replacement payload, cache still the original token, and `ConsentRevoked { Some(401) }`. That cache-preserved-401 outcome is not in the P collector tests.
- `stop_before_retry_admission_skips_fetch_after_changed_credential_reread` and `service_stop_and_lease_release_during_blocked_401_reread_skip_retry_and_keep_no_ui_guard` have no matching stop/lease-release asserts on P.
- `revoked_generation_cannot_replace_cache_or_clear_newer_generation` is a paused-write race: a stale generation must not replace `"newer-session-token"`. P `stale_generation_cannot_read_or_replace_newer_cached_credential` is a sequential generation check, not that race.
- `typed_403_and_unchanged_401_never_trigger_a_second_fetch` asserts a typed 403 with zero rereads, then an unchanged 401 that rereads once and stops at `fetch_count == 2` with status 401. P `forbidden_and_rate_limited_responses_never_trigger_keychain_reread` covers 403 and 429 with one request and no reread. It does not assert the unchanged-401 one-reread stop.

### Only on U, absent from M and P (12)

These are the “only in U” count. Their fixture strings (`delta-seconds`, `operator-local-rate-limited-diagnostics`, `operator-local-principal-window-identity`, `operator-local-lower-reset-read-fence`, `read-fence-watermark`, `acct-v5-reset-known`, `broker_projection:used:five_hour:account`) are absent on P.

| Function | Path |
| --- | --- |
| `cli_http_429_retry_after_delta_seconds_survives_restart` | `crates/jackin/tests/usage_provider_flow.rs:952` |
| `cli_http_429_retry_after_date_survives_restart` | `crates/jackin/tests/usage_provider_flow.rs:962` |
| `timed_out_probe_between_401_and_reread_admission_skips_keychain` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:923` |
| `current_quota_fingerprint_keys_bind_source_and_scope` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:34` |
| `rate_limited_provider_preserves_retry_and_fresh_statusline_quota` | `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs:7` |
| `v4_migration_preserves_strict_history_and_drops_unscoped_provider_quota` | `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs:1191` |
| `v4_migration_rejects_malformed_event_schema_without_rewriting_source` | `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs:1280` |
| `lower_reset_freshness_uses_the_unique_principal_window_identity` | `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs:1876` |
| `lower_reset_quota_read_fence_rejects_split_and_future_pair_siblings` | `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs:2403` |
| `v5_migration_records_unknown_or_known_none_and_restamps_event_schema` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:4087` |
| `v5_migration_rejects_wrong_nested_schema_and_downgraded_watermark_metadata` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:4183` |
| `rate_limit_keeps_original_quota_age_and_does_not_block_fresh_statusline` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:7586` |

P `fake_http_429_preserves_numeric_and_http_date_retry_after` (`crates/services/jackin-usage-provider-core/src/tests/case_04.rs:327`) is a provider-core HTTP-date case, not these CLI restart tests. `"delta-seconds"` does not appear on P.

## Names only on P (40)

Ten of the twelve renames above are new P-only names. The other two (`collector_auth_required_has_stable_wire_code` and `binding_input_defaults_experimental_collector_approval_to_false`) were absorbed into `v4_monitor_control_shapes_are_tagged_and_secret_free`, which is already on M and U, so they do not add a P-only name. The other 30 P-only names are:

| Function | Path | Topic |
| --- | --- | --- |
| `caller_supplied_catalog_reconciliation_is_not_a_wire_operation` | `crates/core/jackin-protocol/src/usage_broker/tests/case_01.rs:117` | monitor wire |
| `expired_live_owner_lock_prevents_successor_takeover` | `crates/services/jackin-usage-broker/src/tests/case_05.rs:133` | locks |
| `after_apply_marker_store_failure_restores_preimage_before_any_purge` | `crates/services/jackin-usage-coordinator/src/tests/case_02.rs:653` | purge |
| `later_marker_store_failure_rolls_back_prior_marker_before_any_purge` | `crates/services/jackin-usage-coordinator/src/tests/case_02.rs:811` | purge |
| `crash_after_durable_cooldown_marker_precedes_purge_and_survives_restart` | `crates/services/jackin-usage-coordinator/src/tests/case_02.rs:870` | purge |
| `catalog_purge_prevents_restart_resurrection` | `crates/services/jackin-usage-coordinator/src/tests/case_02.rs:1842` | purge |
| `consent_revoked_after_provider_error_discards_error_and_keeps_only_status` | `crates/services/jackin-usage-provider-claude/src/collector.rs:423` | provider status |
| `consent_revoked_after_missing_keychain_reread_discards_first_401_view` | `crates/services/jackin-usage-provider-claude/src/collector.rs:455` | provider status |
| `consent_revoked_after_retry_discards_provider_error_and_keeps_only_status` | `crates/services/jackin-usage-provider-claude/src/collector.rs:489` | provider status |
| `forbidden_and_rate_limited_responses_never_trigger_keychain_reread` | `crates/services/jackin-usage-provider-claude/src/collector.rs:534` | provider status |
| `one_401_reread_replaces_only_the_selected_generation_once` | `crates/services/jackin-usage-provider-claude/src/lease/tests.rs:124` | provider status |
| `stale_generation_cannot_read_or_replace_newer_cached_credential` | `crates/services/jackin-usage-provider-claude/src/lease/tests.rs:150` | provider status |
| `claude_http_auth_scope_and_rate_failures_keep_typed_status` | `crates/services/jackin-usage-provider-claude/src/tests/case_04.rs:99` | provider status |
| `claude_401_does_not_implicitly_reread_same_source_or_retry` | `crates/services/jackin-usage-provider-claude/src/tests/case_04.rs:156` | provider status |
| `claude_http_timeout_and_transport_failures_stay_typed_and_do_not_retry` | `crates/services/jackin-usage-provider-claude/src/tests/case_04.rs:234` | provider status |
| `fake_http_429_preserves_numeric_and_http_date_retry_after` | `crates/services/jackin-usage-provider-core/src/tests/case_04.rs:327` | provider status |
| `provider_error_keeps_timeout_transport_and_decode_distinct_from_status_text` | `crates/services/jackin-usage-provider-core/src/tests/case_04.rs:355` | provider status |
| `debug_redacts_scope_labels` | `crates/services/jackin-usage-discovery/src/issues.rs:106` | redaction |
| `claude_oauth_response_maps_windows_to_buckets` | `crates/services/jackin-usage-provider-claude/src/tests/case_02.rs:60` | quota windows |
| `claude_oauth_limits_array_skips_unnamed_scoped_window` | `crates/services/jackin-usage-provider-claude/src/tests/case_02.rs:258` | quota windows |
| `claude_oauth_limits_array_backfills_missing_legacy_windows` | `crates/services/jackin-usage-provider-claude/src/tests/case_02.rs:273` | quota windows |
| `claude_codename_dollar_window_is_surfaced` | `crates/services/jackin-usage-provider-claude/src/tests/case_03.rs:54` | quota windows |
| `claude_oauth_response_accepts_window_aliases` | `crates/services/jackin-usage-provider-claude/src/tests/case_03.rs:117` | quota windows |
| `unknown_window_pool_kept_as_detail_row` | `crates/services/jackin-usage-provider-antigravity/src/tests/case_01.rs:157` | quota windows |
| `minimax_exhausted_and_unlimited_windows_render` | `crates/services/jackin-usage-provider-minimax/src/tests/case_01.rs:87` | quota windows |
| `window_label_comes_from_duration` | `crates/services/jackin-usage-provider-muse/src/tests/case_01.rs:47` | quota windows |
| `zai_peak_window_follows_weekday_and_hour` | `crates/services/jackin-usage-provider-zai/src/tests.rs:51` | quota windows |
| `opencode_missing_window_and_negative_percent_stay_errors` | `crates/services/jackin-usage-provider-opencode/src/tests/case_01.rs:108` | quota windows |
| `parity_antigravity_two_family_windows_groups_and_credits` | `crates/services/jackin-usage-projection/src/tests/case_01.rs:312` | quota windows |
| `parity_stale_partial_windows_issues_and_retry` | `crates/services/jackin-usage-projection/src/tests/case_03.rs:7` | quota windows |

Those 30 plus these 10 rename targets are the 40 names only on P: `broker_conflict_has_a_stable_sanitized_wire_name`, `local_source_identity_has_its_own_wire_kind`, `unverified_identity_has_its_own_wire_kind`, `classify_claude_keychain_status_distinguishes_denial_absence_and_consent`, `scope_restriction_requires_typed_http_403_and_keeps_status_in_error`, `policy_approval_defaults_strict_sgd_and_never_infers_quota_only`, `account_projects_window_spend_and_plan_groups`, `changed_exact_source_is_reread_and_retried_once_after_401`, `consent_revoked_after_success_discards_the_provider_view`, `after_apply_purge_failure_restores_preimage_and_catalog`. Paths for those 10 are in the rename table.

Wait, I said 28 then listed 30. Let me count the table rows... I'll count in the file I wrote.

Looking at my table:
1 caller_supplied
2 expired_live
3 after_apply_marker
4 later_marker
5 crash_after
6 catalog_purge
7 consent_revoked provider_error
8 consent_revoked missing keychain
9 consent_revoked retry
10 forbidden
11 one_401
12 stale_generation
13 claude_http_auth
14 claude_401
15 claude_http_timeout
16 fake_http_429
17 provider_error_keeps
18 debug_redacts
19 claude_oauth maps
20 skips_unnamed
21 backfills
22 codename
23 accepts aliases
24 unknown_window
25 minimax
26 window_label
27 zai
28 opencode
29 parity_antigravity
30 parity_stale

That's 30. Plus 10 rename targets = 40. I wrote "the other 28" which is wrong. I need to fix that sentence to 30.

Also I said "Includes the 12 rename targets" but only 10 of the 12 create new P-only names. The prose later corrects this. I should fix the opening sentence so it isn't contradictory.

Let me fix those two sentences with StrReplace.
