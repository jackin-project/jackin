# TASK-002 security invariants

Reviewer: independent of the implementer. Read-only. No files in the three trees were modified, nothing was built, and no live credentials, tokens, or terminal bytes were read.

Trees:

| Tree | Path | Commit |
| --- | --- | --- |
| Main (M) | `/Users/donbeave/Projects/jackin-project/wt-baseline-M` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| Policy (P) | `/Users/donbeave/Projects/jackin-project/wt-baseline-P` | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| Usage (U) | `/Users/donbeave/Projects/jackin-project/wt-baseline-U` | `2e4a62da4bc4406764162a267163389a5fb513a7` |

Working trees were clean at those commits. Verdicts are from the functions below, not from planning notes.

## Verdicts

| Invariant | Verdict |
| --- | --- |
| 1. Idle exit and zero provider calls | **PASS** |
| 2. Typed HTTP status authority | **PASS** |
| 3. Lock, owner, and purge guards | **GAP** |
| 4. Diagnostic redaction | **PASS** |
| 5. Observe-only results | **PASS** |

**Highest-risk gap:** Policy `claim_existing_lease` in `crates/services/jackin-usage-broker/src/leader.rs` will replace a non-document lease when the file text parses as a dead process id, and `claim_leader` no longer rechecks that the locked descriptor is still the same non-symlink inode as the path. Main and Usage refuse unknown formats and recheck `lease_path_matches_open_file` before accepting or rewriting the lease. Coordination, repo-lock, and purge guards in Policy still match Main.

## 1. Idle exit and zero provider calls — PASS

`should_exit_idle` returns true only when the idle interval has elapsed, the coordinator is idle, no non-stopped monitor is present, and foreground liveness is absent. Any one of those keep-alive inputs keeps the result false. A passive broker (coordinator idle, no monitor, no foreground guard, interval elapsed) still exits.

| Tree | Function | File |
| --- | --- | --- |
| M | `should_exit_idle` | `crates/jackin-usage/src/host/broker/serve_loop.rs` (lines 496–508; call at 198–205 passes `monitor_store.has_active()` and `collector_liveness.is_some()`) |
| U | `should_exit_idle` | `crates/jackin-usage/src/host/broker/serve_loop.rs` (lines 564–576; same call shape at 217–224) |
| P | `should_exit_idle` | `crates/services/jackin-usage-broker/src/serve.rs` (lines 518–530; call at 190–198) |

`has_active` is `active_monitor_count`, which counts monitors whose `stopped_at_epoch` is absent (M `monitor.rs` 1194–1203, U `monitor.rs` 1381–1390, P `monitor.rs` 1190–1199).

Foreground presence:

- M and U pass `collector_liveness.is_some()`. Foreground bootstrap installs that object in `run_usage_broker_foreground_bootstrap_core` (`service.rs` in both trees). Background startup leaves it `None`.
- P replaced the object with `ServeConfig.has_authenticated_foreground_guard`. `run_usage_broker_foreground_bootstrap` passes `true` (`service.rs` line 124). `ensure_usage_broker_with_executor` passes `false` (line 537).

Split risk: low. The predicate is the same. Policy’s flag is a startup boolean, not a live liveness object, but the foreground process sets it for the whole serve, which matches Main’s `is_some()` check. Policy did not drop the keep-alive.

Unavailable or unauthorized state returns before a provider call:

- Store `Unavailable` or `Corrupt` fails in `ensure_loaded` and is returned from `request_refresh_with_scope` before a probe job is queued. M and U: `coordinator/join.rs` `ensure_loaded`, `coordinator/refresh.rs` `request_refresh_with_scope`. Policy: same functions in `crates/services/jackin-usage-coordinator/src/join.rs` and `refresh.rs`. Policy adds a reload fence; it does not move the probe send ahead of the store error.
- An unauthorized binding returns `Unauthorized` without `refresh_binding_outcome`. M: `probe_with_scope` in `crates/jackin-usage/src/host/broker.rs`. U: same file. P: `probe_with_scope_using` in `crates/services/jackin-usage-broker/src/executor.rs` (lines 232–238). Claude collection returns `collector_not_authorized` before the snapshot when the source is not an active mapped monitor (M `probe_claude_with_scope`; P `probe_foreground_claude` lines 80–81). Policy’s collector rechecks consent before `fetch` (`experimental_claude_usage_snapshot_for_lease_with` and `fetch_claude_with_one_401_reread` in `crates/services/jackin-usage-provider-claude/src/collector.rs`).
- `is_retryable` does not include `Unauthorized` or `NeedsSecret` (M and P `policy.rs` are identical; U adds a Claude rate-limit circuit that still requires `RateLimited`).

`UsageCoordinationErrorKind::Unavailable` remains retryable after a call has already happened. That is backoff, not a bypass of the store-unavailable refusal.

## 2. Typed HTTP status authority — PASS

Credential and retry classification read `ProviderError.status()`, which is set only by `ProviderHttpError::HttpStatus`. `From<String>` and timeout, transport, and decode conversions store `http_status: None`. A message that contains 401, 403, or 429 does not become `NeedsSecret`, `Unauthorized`, or `RateLimited`.

| Tree | Function | File |
| --- | --- | --- |
| M, U | `usage_error_is_unauthorized`, `usage_error_is_rate_limited`, `ProviderError::status`, `ProviderError::rate_limit` | `crates/jackin-usage/src/usage/refresh.rs` (lines 106–123 and 190–196). U matches M. |
| M, U | `provider_probe_outcome_with_metadata` | `crates/jackin-usage/src/host/broker.rs` (M lines 951–973). Maps `ProviderErrorKind::HttpStatus` through the typed `http_status` field. |
| P | same classifiers | `crates/services/jackin-usage-provider-core/src/refresh.rs` (lines 199–205). Visibility became `pub`; the status rule is unchanged. |
| P | `provider_probe_outcome_with_metadata` | `crates/services/jackin-usage-broker/src/rediscover.rs` (lines 143–163) |

Tests that pin the rule, using synthetic messages rather than live credentials:

- M and U: `auth_and_rate_limit_classification_requires_typed_http_status` in `crates/jackin-usage/src/usage/codex/tests.rs`; `grok_status_classification_requires_typed_http_status` in `usage/grok/tests.rs`.
- P: same tests in `jackin-usage-provider-codex/src/tests/case_01.rs` and `jackin-usage-provider-grok/src/tests/case_01.rs`.

Split risk: low. The classifier moved with the provider core crate and still ignores display text.

## 3. Lock, owner, and purge guards — GAP

Main and Usage enforce all three clauses. Policy keeps coordination, repo-lock, and purge, and drops two broker-lease checks.

### What still matches

Unsafe coordination paths, symlinks, and wrong owners:

| Tree | Functions | File |
| --- | --- | --- |
| M, U | `resolve`, `validate_key`, `open_lock`, `open_directory_in_namespace`, `open_state_at` | `crates/jackin-runtime/src/runtime/coordination.rs`. U is identical to M. |
| P | same functions (`pub` instead of `pub(crate)`; tests moved out) | `crates/services/jackin-runtime-coordination/src/coordination.rs` |

`resolve` refuses a dangling symlink without creating the target. `validate_key` rejects empty keys, `..`, and non-leaf characters. Directory open uses `O_NOFOLLOW` and requires the current user, with mode `0o022` clear on the parent and `0o077` clear inside the namespace. State open requires a regular inode, current user, mode `0o077` clear, and `nlink == 1`.

Reentrant last-owner release:

| Tree | Function | File |
| --- | --- | --- |
| M, U | `RepoLock`, `acquire_repo_lock_blocking` | `crates/jackin-runtime/src/runtime/repo_cache.rs` (struct at 66, acquire at 105–117) |
| P | same | `crates/services/jackin-runtime-repo-cache/src/repo_cache.rs` (same lines) |

A same-process re-acquire clones the existing `Arc<File>` and does not take `flock` again. The descriptor lock drops when the last clone drops.

Purge cannot delete an active or uninspectable resource:

| Tree | Function | File |
| --- | --- | --- |
| M, U | `purge_container_filesystem`, `ensure_role_resources_absent_for_purge`, `ensure_container_absent_for_purge` | `crates/jackin-runtime/src/runtime/cleanup.rs` (filesystem teardown calls `ensure_backend_absent_for_purge` before `safe_remove_dir_contained`; absence check at 1126–1159) |
| P | `purge_container_filesystem` | `crates/services/jackin-runtime/src/runtime/cleanup/purge.rs` (lines 76–92) |
| P | `ensure_role_resources_absent_for_purge`, `ensure_container_absent_for_purge` | `crates/services/jackin-runtime-cleanup-absent-guards/src/absent_guards.rs` |

Running, paused, restarting, created, removing, dead, and stopped containers are refused. `InspectUnavailable` is refused. Only `NotFound` proceeds. `safe_remove.rs` differs in Policy only by the blocking-spawn wrapper; the removal checks are otherwise the same, and Usage matches Main.

### What Policy drops

| Tree | Function | File | Behavior |
| --- | --- | --- | --- |
| M | `claim_existing_lease`, `lease_path_matches_open_file` | `crates/jackin-usage/src/host/broker.rs` (claim 1254–1286, identity check 1322–1335) | Non-JSON lease bytes return `Ok(None)`. After `try_lock`, the path must still be the same regular, non-symlink inode as the descriptor. |
| U | `claim_existing_lease`, `lease_path_matches_open_file` | `crates/jackin-usage/src/host/broker.rs` (unknown-format refusal at 1459–1461; identity check at 1588; rewrite rechecks identity at 1466–1470) | Same refusal, plus a foreign-lease reclaim gate. Stricter than Main, not weaker. |
| P | `claim_leader`, `claim_existing_lease` | `crates/services/jackin-usage-broker/src/leader.rs` (claim 23–106, existing lease 108–141) | `O_NOFOLLOW` and `validate_owned_file` remain. There is no `lease_path_matches_open_file`. Non-JSON bytes that parse as a dead process id are replaceable (`kill` failure at 132–135). |

Policy cleanup still has `path_matches_file` (leader.rs 238–244) before unlink, so a swapped symlink is not deleted on the way out. The hole is acceptance and rewrite of the lease, not the later unlink.

Split risk: high for the broker lease. Low for coordination, `RepoLock`, and purge. Consolidating Policy’s `leader.rs` over Main or Usage `broker.rs` drops the unknown-format refusal and the post-lock inode check.

## 4. Diagnostic redaction — PASS

`redact_text` and `StreamRedactor` cover named secrets, bearer and basic authorization values, known token shapes, PEM private-key bodies, YAML block bodies under secret keys, and structured JSON values. Malformed or overlong streams fail closed.

| Tree | Function | File |
| --- | --- | --- |
| M, U | `redact_text`, `StreamRedactor` | `crates/jackin-diagnostics/src/redact.rs` (`redact_text` at 526). U matches M. |
| P | same | `crates/services/jackin-diagnostics/src/redact.rs`. Byte-identical to Main. |

Tests use synthetic canaries (`short-canary`, obvious dummy token shapes, PEM labels). They do not read or require a committed live credential.

- M and U: `crates/jackin-diagnostics/src/redact/tests.rs` (`redacts_named_secret_values`, `redacts_compound_credential_assignments`, `redacts_quoted_authorization_bearer_value`, `redacts_private_key_blocks`, and the structured-body cases).
- P: those tests moved to `crates/services/jackin-diagnostics/src/redact/tests/case_01.rs`. The production redactor did not move away from them.

Split risk: low. The implementation was copied whole; only the test module path changed.

## 5. Observe-only results — PASS

An observe-only monitor result does not become runnable work, a goal or policy identity, or dispatch authorization.

| Tree | Function | File | What it enforces |
| --- | --- | --- | --- |
| M | `reconcile` observe-only arm and `runnable` | `crates/jackin-usage/src/host/broker/monitor/evaluation.rs` (lines 57–128) | Clears actions, skips `append_dispatch_authorization_issues`, sets `runnable` only for `DispatchGuard`. |
| M | `status_lifecycle`, `monitor_readiness` | `monitor/status.rs` (293–308 and 429–430) | Observe-only dispatch is `NotAuthorized`. Runnable still requires `DispatchGuard`. |
| M | `prepare_start_authority`, `validate_config` | `monitor/validation.rs` (27–48 and 801–823) | Observe-only start stores `policy: None` and `goal_id: None`. A goal or policy revision is `ObservationOnly`. Experimental collection is rejected unless the purpose is observe-only. |
| U | same three | same paths | The evaluation diff only widens reset evidence inside the quota pause path. Observe-only clears actions after that path, so it does not create a pause action, a goal, or dispatch authority. |
| P | `reconcile` observe-only arm | `crates/services/jackin-usage-broker/src/monitor/evaluation.rs` (244–315) | Same clear-actions and `DispatchGuard`-only `runnable` rule. |
| P | `status_lifecycle` | `monitor/lifecycle.rs` (563–578) | Same `NotAuthorized` override. |
| P | `prepare_start_authority`, `validate_config` | `monitor.rs` (286–313 and 493–515) | Same empty goal and policy, and the same experimental-collector restriction. |

Split risk: low. The three gates moved into `monitor.rs`, `evaluation.rs`, and `lifecycle.rs` without losing the purpose check.

## Consolidation note

Invariants 1, 2, 4, and 5 are present in all three trees. Taking Policy’s crate split for those paths does not drop the guard, provided the moved functions above are the ones that land.

Invariant 3 does not fully survive that split. Keep Main or Usage `claim_existing_lease` and `lease_path_matches_open_file` when broker lease code is consolidated. Policy’s coordination open path, `RepoLock`, and `ensure_container_absent_for_purge` can move as they are.
