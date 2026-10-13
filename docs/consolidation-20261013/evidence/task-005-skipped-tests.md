# TASK-005 skipped tests

Read-only check of the integration worktree. No file in `/Users/donbeave/Projects/jackin-project/jackin` or `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` was edited.

| Item | Value |
| --- | --- |
| Worktree | `/Users/donbeave/Projects/jackin-project/wt-integration-20261013` |
| Branch | `integration/branch-consolidation-20261013` |
| HEAD | `3329780ccf7f17a50db86c4e2463384fe718f650` |
| Restored broker and protocol checks | `010cb38a9af5d69e0d40ad1fd024611534e50978` (13 tests) |
| Restored Claude consent checks | `2ee01c8cf9b86f35bb3ef00e9d7b84787994a5b3` (6 tests) |

`010cb38a` and `2ee01c8c` are ancestors of `3329780`. `git status --porcelain=v1 -uall` on the worktree was empty before and after the search. HEAD stayed `3329780ccf7f17a50db86c4e2463384fe718f650`.

This file records the 15 tests those two commits left unported. It does not approve TASK-005 completion.

## Verification

For each name below, from the worktree:

```text
git grep -n -F <name> HEAD
```

Each command exited 1 and printed nothing. Every name is absent from that tree, so every disposition is EXCLUSION.

## Exclusions

| Test | Reason | Disposition |
| --- | --- | --- |
| `first_provider_error_is_preserved_when_consent_revokes_after_response` | Current collector returns `ConsentRevoked` and drops the provider error. | EXCLUSION |
| `retry_429_metadata_survives_consent_revocation_after_response` | Current collector returns `ConsentRevoked` and drops the provider error. | EXCLUSION |
| `service_stop_and_lease_release_during_blocked_401_reread_skip_retry_and_keep_no_ui_guard` | `ClaudeCollectorLiveness` and the no-UI guard are gone. | EXCLUSION |
| `timed_out_probe_between_401_and_reread_admission_skips_keychain` | `ProbeExpired` and the probe-liveness entry point are gone. | EXCLUSION |
| `cli_http_429_retry_after_delta_seconds_survives_restart` | Harness asserts wire v9; this tree is v8. | EXCLUSION |
| `cli_http_429_retry_after_date_survives_restart` | Harness asserts wire v9; this tree is v8. | EXCLUSION |
| `current_quota_fingerprint_keys_bind_source_and_scope` | Schema 4 accepts `used:` keys and rejects the source-prefixed keys this test wants. | EXCLUSION |
| `v4_migration_preserves_strict_history_and_drops_unscoped_provider_quota` | Bodies assert schema 6. | EXCLUSION |
| `v4_migration_rejects_malformed_event_schema_without_rewriting_source` | Bodies assert schema 6. | EXCLUSION |
| `v5_migration_records_unknown_or_known_none_and_restamps_event_schema` | v5 watermark types are absent. | EXCLUSION |
| `v5_migration_rejects_wrong_nested_schema_and_downgraded_watermark_metadata` | v5 watermark types are absent. | EXCLUSION |
| `rate_limited_provider_preserves_retry_and_fresh_statusline_quota` | `MonitorProviderReadiness` / principal-window types are absent. | EXCLUSION |
| `rate_limit_keeps_original_quota_age_and_does_not_block_fresh_statusline` | `MonitorProviderReadiness` / principal-window types are absent. | EXCLUSION |
| `lower_reset_freshness_uses_the_unique_principal_window_identity` | `MonitorProviderReadiness` / principal-window types are absent. | EXCLUSION |
| `lower_reset_quota_read_fence_rejects_split_and_future_pair_siblings` | `MonitorProviderReadiness` / principal-window types are absent. | EXCLUSION |

15 names, 15 exclusions. None is PRESENT.
