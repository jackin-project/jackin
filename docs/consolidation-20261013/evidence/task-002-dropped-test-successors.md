# TASK-002 dropped-test successors on the policy tip

Read-only search of `/Users/donbeave/Projects/jackin-project/jackin`. No checkout, no edit, no `cargo`.

| Tree | Commit |
| --- | --- |
| Main (M) | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| Policy (P) | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| Usage (U) | `2e4a62da4bc4406764162a267163389a5fb513a7` |

The 34 names are the dropped set in `task-002-test-identities.md`. The four names under "Tests absent from the policy tree" in `integration-constraints.md` are in that set and are marked below.

## Result

| Class | Count |
| --- | ---: |
| EXACT_ON_P | 0 |
| RENAMED_ON_P | 0 |
| ABSENT | 34 |

`git grep` of the 34 identifiers on `fba69d60` returned no hits (exit 1). A second `git grep` for `fn` plus each name also returned no hits. No name is EXACT_ON_P.

Each name was then searched once, with `git grep -F -n` on `fba69d60`, for one needle taken from its M body (the 22 names on M and U) or its U body (the 12 names only on U). A policy hit counts as RENAMED_ON_P only when that same assertion sits in a different test. A shared error phrase or status fragment in a test that checks a different outcome does not.

## On M and U (22)

Needles are from M. U has the same function names.

| Function | Class | Needle | M source |
| --- | --- | --- | --- |
| `service_status_exposes_secret_free_foreground_collector_mode` | ABSENT | `claude-source-account` | `crates/jackin-protocol/src/usage_monitor/tests.rs:21` |
| `monitor_and_broker_owned_relay_operations_have_v8_wire_shapes` | ABSENT | `bounded event wait expired` | `crates/jackin-protocol/src/usage_broker/tests.rs:175` |
| `future_store_schema_is_rejected` | ABSENT | `future monitor schema must be rejected` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:4263` |
| `v2_store_migrates_explicitly_to_current_schema` | ABSENT | `v2-migration-event` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:3609` |
| `v2_store_rejects_wrong_nested_event_schema_without_rewriting_source` | ABSENT | `v2-wrong-event-schema` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:3654` |
| `v3_migration_rejects_wrong_or_future_nested_event_schema_without_rewriting_source` | ABSENT | `v3-wrong-event-schema` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:3837` |
| `fresh_quota_callbacks_do_not_refresh_same_or_omitted_model_evidence` | ABSENT | `the reset-grace barrier remains latched because usage did not fall` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:6448` |
| `live_lease_owner_blocks_expired_takeover_and_can_renew_after_waking` | ABSENT | `the lifetime lock continues to protect an owner past the next expiry` | `crates/jackin-usage/src/host/broker/tests.rs:2106` |
| `expired_lease_is_reclaimed_after_dead_owner_releases_lifetime_lock` | ABSENT | `dead owner's expired lease can be reclaimed` | `crates/jackin-usage/src/host/broker/tests.rs:2187` |
| `failed_later_purge_prewrites_lazy_success_tombstone_before_purge` | ABSENT | `expected pending marker write first, got {event:?}` | helper `crates/jackin-usage/src/coordinator/tests.rs:3535`; body is the call at `:3798` |
| `failed_later_purge_during_revision_reset_restores_pending_marker` | ABSENT | `pending attempt marker must survive rollback` | helper `crates/jackin-usage/src/coordinator/tests.rs:3763`; body is the call at `:3803` |
| `failed_later_purge_with_missing_pending_preimage_preserves_restart_fence` | ABSENT | `share fake clock with the restart fixture` | helper `crates/jackin-usage/src/coordinator/tests.rs:3613`; body is the call at `:3808` |
| `failed_later_purge_prewrites_lazy_retry_after_tombstone_before_purge` | ABSENT | `provider rate limited` | helper `crates/jackin-usage/src/coordinator/tests.rs:3684`; body is the call at `:3813` |
| `revoked_consent_before_first_request_skips_provider_and_keychain` | ABSENT | `provider_http_status: None` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:252` |
| `first_provider_error_is_preserved_when_consent_revokes_after_response` | ABSENT | `retry_after_seconds: Some(120)` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:342` |
| `revoked_consent_after_401_skips_keychain_reread_and_preserves_401` | ABSENT | `provider_http_status: Some(401)` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:397` |
| `revoked_consent_after_keychain_reread_skips_retry_and_preserves_401` | ABSENT | `resolved.access_token.as_str(), "old-token"` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:462` |
| `stop_before_retry_admission_skips_fetch_after_changed_credential_reread` | ABSENT | `changed credential reaches the retry admission gate` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:545` |
| `revoked_generation_cannot_replace_cache_or_clear_newer_generation` | ABSENT | `newer-session-token` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:610` |
| `service_stop_and_lease_release_during_blocked_401_reread_skip_retry_and_keep_no_ui_guard` | ABSENT | `finish the blocked reread without waiting in service teardown` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:732` |
| `retry_429_metadata_survives_consent_revocation_after_response` | ABSENT | `response_received_at_epoch: Some(1_000)` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:801` |
| `typed_403_and_unchanged_401_never_trigger_a_second_fetch` | ABSENT | `same-token` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:821` |

The four `failed_later_purge_*` bodies are single calls to `failed_later_purge_case` and contain no quoted string. Each needle is one string from the helper that call reaches. The missing-preimage needle is the clippy reason in `assert_missing_pending_restart_fence`; that helper's asserts (`Failed`, `retry_at_epoch == Some(1_300)`, zero calls) have no message string.

`integration-constraints.md` already recorded these four as absent from `fba69d60`: `live_lease_owner_blocks_expired_takeover_and_can_renew_after_waking`, `expired_lease_is_reclaimed_after_dead_owner_releases_lifetime_lock`, `revoked_consent_after_401_skips_keychain_reread_and_preserves_401`, and `future_store_schema_is_rejected`. This pass agrees. Their names and their needles are absent.

## Only on U (12)

| Function | Class | Needle | U source |
| --- | --- | --- | --- |
| `cli_http_429_retry_after_delta_seconds_survives_restart` | ABSENT | `delta-seconds` | `crates/jackin/tests/usage_provider_flow.rs:955` |
| `cli_http_429_retry_after_date_survives_restart` | ABSENT | `Tue, 14 Nov 2023 22:38:22 GMT` | `crates/jackin/tests/usage_provider_flow.rs:966` |
| `timed_out_probe_between_401_and_reread_admission_skips_keychain` | ABSENT | `ClaudeFetchError::ProbeExpired` | `crates/jackin-usage/src/usage/claude/lease/tests.rs:1004` |
| `current_quota_fingerprint_keys_bind_source_and_scope` | ABSENT | `statusline:used:five_hour:session:account` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:40` |
| `rate_limited_provider_preserves_retry_and_fresh_statusline_quota` | ABSENT | `operator-local-rate-limited-diagnostics` | `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs:11` |
| `v4_migration_preserves_strict_history_and_drops_unscoped_provider_quota` | ABSENT | `dropping unscoped provider quota preserves the account-wide reset maxima` | `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs:1256` |
| `v4_migration_rejects_malformed_event_schema_without_rewriting_source` | ABSENT | `reject malformed V4 event schema` | `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs:1298` |
| `lower_reset_freshness_uses_the_unique_principal_window_identity` | ABSENT | `operator-local-principal-window-identity` | `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs:1878` |
| `lower_reset_quota_read_fence_rejects_split_and_future_pair_siblings` | ABSENT | `read-fence-watermark` | `crates/jackin-usage/src/host/broker/monitor/tests/provider_diagnostics.rs:2413` |
| `v5_migration_records_unknown_or_known_none_and_restamps_event_schema` | ABSENT | `migration preserves event contents and changes only nested schema version` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:4158` |
| `v5_migration_rejects_wrong_nested_schema_and_downgraded_watermark_metadata` | ABSENT | `invalid V5 state must not migrate` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:4245` |
| `rate_limit_keeps_original_quota_age_and_does_not_block_fresh_statusline` | ABSENT | `provider diagnostics remain visible independently of fresh statusline quota` | `crates/jackin-usage/src/host/broker/monitor/tests.rs:7683` |

`delta-seconds` also appears on U in `local_http_429_retry_after_survives_broker_restart_and_catalog_rotation` (`crates/jackin-usage/src/host/broker/tests.rs:1471`). That name is outside this 34. The string is still absent on P. `ClaudeFetchError::ProbeExpired` also appears on U in production `claude.rs` and in `timed_out_probe_cannot_retry_after_blocked_401_reread_resumes` (`lease/tests.rs:895`). The spelling is absent on P, including that production path.

## Policy hits that are not renames

Three needles occur on P. Each hit is a different test or production arm. None is RENAMED_ON_P.

`provider rate limited`, needle for `failed_later_purge_prewrites_lazy_retry_after_tombstone_before_purge`, is a shared rate-limit message. On P it appears in:

- `probe` at `crates/apps/jackin/tests/usage_broker_e2e/docker.rs:204`
- `coordinator_failure_shares_retry_deadline_and_last_good` at `crates/services/jackin-usage-coordinator/src/tests/case_01.rs:345`
- `coordinator_rate_limit_without_provider_deadline_uses_shared_backoff` at `crates/services/jackin-usage-coordinator/src/tests/case_02.rs:137`
- `catalog_revision_retains_retry_after_across_restart` at `crates/services/jackin-usage-coordinator/src/tests/case_02.rs:517`
- `failed_catalog_reset_write_restores_retry_state_and_catalog` at `crates/services/jackin-usage-coordinator/src/tests/case_02.rs:613`
- `cadence_shared_retry_after_wins_over_periodic_due` at `crates/services/jackin-usage-coordinator/src/tests/case_03.rs:318`

The purge-order assertion `expected pending marker write first, got {event:?}` and `pending attempt marker must survive rollback` are absent on P.

`provider_http_status: None`, needle for `revoked_consent_before_first_request_skips_provider_and_keychain`, also appears in production (`executor.rs:106`, `collector.rs:83`, `collector.rs:158`) and in `consent_revoked_after_success_discards_the_provider_view` (`collector.rs:415`). That test asserts one request and that a successful request must not reread Keychain. The M test asserts `fetch_count == 0` and `reread_count == 0`.

`provider_http_status: Some(401)`, needle for `revoked_consent_after_401_skips_keychain_reread_and_preserves_401`, also appears in production `fetch_claude_with_one_401_reread` and `first_401_if_consent_current`, and in `consent_revoked_after_missing_keychain_reread_discards_first_401_view` (`collector.rs:481`). That test reads `ClaudeKeychainRead::Missing` and then asserts consent revoked with status 401. The M test asserts `reread_count == 0`.

The assertion `resolved.access_token.as_str(), "old-token"` from the keychain-reread sibling is absent on P.
