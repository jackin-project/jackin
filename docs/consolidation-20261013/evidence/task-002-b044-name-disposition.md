# B044 name disposition

## Name count

206 names, one function name per line, no blanks and no duplicates.

## Re-checked history

- Ancestor `d32e85563648931f80ffab141bb2650f478c2a60` is an ancestor of P `fba69d60c2373156dc0dcecee8e085c83776966f` and of B044 `40823a76a3e20b901d5bc410979cee0931fd5c48`.
- Commits after the ancestor on P: 360.
- Commits reachable from B044 and not from P: 10.
- Those 10 commits, newest first:
  - 40823a76 rescue: preserve local working state
  - 459507ea S7 split: jackin-usage file splits (coordinator)
  - 123bd437 S7 split: jackin-usage file splits (snapshot_store, projection, publish)
  - 4191a567 S7 split: jackin-usage file splits (token_monitor, state, accounts)
  - 46a24b4f S7 split: jackin-usage file splits (usage)
  - 9158cf03 S7 split: jackin-usage file splits (format, view)
  - 40dc9004 S7 split: jackin-usage file splits (claude, codex)
  - b950a152 S7 split: jackin-usage file splits (cursor, grok)
  - 39b4d39e S7 split: jackin-usage file splits (minimax, kimi)
  - b5b66a7f S7 split: jackin-usage file splits (zai, openrouter, amp, antigravity)
- Nine subjects say `S7 split`. The tip subject is `rescue: preserve local working state`. Its diff is the same shape as the others: `broker.rs` shrinks and new files appear under `host/broker/`. All 10 unique commits are file splits of the ancestor tree, by diff shape.
- The first three names in the list were checked again with `git grep -E` for `fn NAME`. Each definition is on the ancestor and on B044, and the exact name is absent from P.

- `activate_broker`: ancestor hits 1, B044 hits 1, P hits 0
  - ancestor `crates/adapters/jackin-usage-ffi/src/bridge.rs:551` `fn activate_broker(`
  - B044 `crates/adapters/jackin-usage-ffi/src/bridge.rs:551` `fn activate_broker(`
- `activation_scope`: ancestor hits 1, B044 hits 1, P hits 0
  - ancestor `crates/services/jackin-usage/src/host/broker/tests/support.rs:292` `pub(super) fn activation_scope(temp: &tempfile::TempDir) -> UsageDiscoveryScope {`
  - B044 `crates/services/jackin-usage/src/host/broker/tests/support.rs:292` `pub(super) fn activation_scope(temp: &tempfile::TempDir) -> UsageDiscoveryScope {`
- `apply_broker_generation`: ancestor hits 1, B044 hits 1, P hits 0
  - ancestor `crates/services/jackin-usage/src/host/broker.rs:67` `pub fn apply_broker_generation(&mut self, state: UsageGenerationView) -> Result<(), String> {`
  - B044 `crates/services/jackin-usage/src/host/broker/runtime.rs:39` `pub fn apply_broker_generation(&mut self, state: UsageGenerationView) -> Result<(), String> {`

Name absence on P is not proof that the behavior is gone. P has 360 commits after this ancestor, so a test can move under a new function name.

## Method

Local git only, from a Python script that calls `git -C /Users/donbeave/Projects/jackin-project/jackin`. No build, commit, push, branch, or edit in the jackin repo. The coverage matrix was not edited.

Definition search uses POSIX ERE, one alternation of every name, on `*.rs`:

```
fn (NAME|...)[[:space:]]*($|[(<])
```

The pattern is `fn NAME` plus a terminator so `fn foo` does not match `fn foo_bar`. `[[:space:]]` is the POSIX class. `\b` and `\s` are not used. The same search was run on the ancestor, B044, M, P, and U.

Presence classes from that grep:

- `ON_ANCESTOR_ABSENT_TIP`: a definition is on the ancestor and on B044, and the exact name is absent from M, P, and U.
- `ONLY_ON_B044`: a definition is on B044 and not on the ancestor, M, P, or U.
- `ABSENT_EVERYWHERE`: the name is in the list and `fn NAME` is not on B044.
- `EXACT_NAME_ON_M_P_OR_U`: the exact name is on M, P, or U. This bucket is only for a failed absence check.

Function bodies were taken from `git cat-file --batch` and brace-matched with strings, raw strings, comments, char literals, and lifetimes skipped. A body is under 80 lines when the signature line through the closing brace is fewer than 80 lines.

Rename attempts ran only for `ON_ANCESTOR_ABSENT_TIP` names whose chosen ancestor body is under 80 lines and contains an assertion needle. File order, `#[test]` preferred when one of the short bodies is a test. Stop at 40 attempts. Each attempt is one name.

An assertion needle is a string literal inside `assert!`, `assert_eq!`, `assert_ne!`, `assert_matches!`, or `debug_assert*!`. Literals must be at least 8 characters, or at least 4 when they contain a digit or `%`. A single assertion source line of 24–180 characters that contains `::` or a call is also a needle. Weak one-word literals such as `tempdir` and `expected` were skipped. At most 6 literals and 4 code lines were searched per name, longer literals first.

A hit counts only when the needle is token-bounded: the character before and after it is not an ASCII letter, digit, or `_`. That rejects `2.1.7` inside `2.1.79`. The needle must also be confined to the ancestor function. Similar function words were not used.

One incidental shared line is not a rename. The attempt is proved only when that ancestor-unique string sits in exactly one other P function, and either the string is an assertion message of at least 16 characters that contains a space, or at least two extracted needles from the ancestor body occur in that P function, or the ancestor body had only one extracted needle and that needle occurs there. If that test fails, the attempt is disproved and the name stays `UNMATCHED`. `RENAMED_ON_P` means the cited string was found under the new name. A cluster ratio below the full needle list means other assertions in the ancestor body were not found there.

Names on the ancestor with no proved rename stay `UNMATCHED`, including names that were not attempted. `UNMATCHED` names stay preservation leads.

## Class counts

### Git grep presence

| Class | Count |
| --- | ---: |
| ON_ANCESTOR_ABSENT_TIP | 206 |
| ONLY_ON_B044 | 0 |
| ABSENT_EVERYWHERE | 0 |
| EXACT_NAME_ON_M_P_OR_U | 0 |
| Sum | 206 |

### After rename attempts

`ON_ANCESTOR_ABSENT_TIP` is split into `RENAMED_ON_P` and `UNMATCHED`. The other presence classes stay as they are. These exclusive classes cover every name:

| Class | Count |
| --- | ---: |
| RENAMED_ON_P | 2 |
| UNMATCHED | 204 |
| ONLY_ON_B044 | 0 |
| ABSENT_EVERYWHERE | 0 |
| EXACT_NAME_ON_M_P_OR_U | 0 |
| Sum | 206 |

Rename attempts used: 40 of the 40 cap. Proved: 2. Disproved: 38.

Names on both the ancestor and B044: 206. Of those, ancestor bodies that match a same-named B044 body after stripping blank lines and surrounding whitespace: 204. The two that differ are `headline` and `live_probes_enabled`. `headline` is the same struct literal with `pub(crate)` and a wrapped signature. `live_probes_enabled` is the same probe-policy comparison, with the path rewritten from `super::HostProbePolicy` to `super::super::HostProbePolicy` after the module moved. That pattern fits the unique B044 commits being file splits.

Parser misses (grep hit with no brace-matched body):

- None.

### UNMATCHED breakdown

204 names remain UNMATCHED.

- attempted and disproved: 38
- under 80 lines, no assertion needle: 91
- body at or over 80 lines: 10
- eligible, past the 40-attempt cap: 65

UNMATCHED names stay preservation leads. They are on the ancestor, the exact name is absent from P, and this pass did not prove that the same assertion cluster lives on P under another function name. Keeping the exact name off M, P, and U does not by itself show lost behavior.

## Forty attempted matches

| # | Name | Ancestor body | Needle searched to a verdict | Verdict | P function |
| ---: | --- | --- | --- | --- | --- |
| 1 | `bounded_events_and_refresh_floor` | 8 lines `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_02.rs:122` | `assert_eq!(bridge.refresh_floor_secs().expect("floor"), 120)` (1 P hits); `assert!(batch.events.len() <= 256)` (0 P hits); `assert!(batch.next_cursor >= 1)` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 2 | `broker_catalog_admits_current_identity_and_rejects_stale_identity` | 44 lines `crates/services/jackin-usage/src/host/broker/tests/case_04.rs:55` | `assert_eq!(client.current(current).unwrap().generation, 0)` (0 P hits); `UsageCoordinationErrorKind::CatalogRevoked` (16 P hits); `client.current(stale).unwrap_err().kind,` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 3 | `broker_client_refresh_returns_immediately_and_joins_one_generation` | 58 lines `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_01.rs:168` | `broker@example.test` (0 P hits); `assert_eq!(snapshot.buckets[0].remaining_percent, Some(64))` (1 P hits); `assert_eq!(snapshot.account_label, "broker@example.test")` (0 P hits); `assert_eq!(executor.calls.load(Ordering::SeqCst), 1)` (33 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 4 | `broker_failure_for_anonymous_source_stays_surface_scoped` | 44 lines `crates/services/jackin-usage/src/host/broker/tests/case_02.rs:87` | `Gemini auth not available to Capsule` (2 P hits); `assert_eq!(view.status, UsageSnapshotStatus::NeedsSecret)` (3 P hits); `assert!(runtime.list_accounts(None).unwrap().is_empty())` (0 P hits); `assert!(validated.bindings[0].identity.is_none())` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 5 | `broker_failure_without_snapshot_surfaces_honest_gap_in_snapshot` | 78 lines `crates/services/jackin-usage/src/host/broker/tests/case_02.rs:7` | `Grok billing requires an authenticated profile` (5 P hits); `codex-key` (5 P hits); `assert_eq!(view.status, UsageSnapshotStatus::Unavailable)` (4 P hits); `Some("Grok billing requires an authenticated profile")` (1 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 6 | `broker_policy_retry_jitter_is_stable_bounded_and_provider_wins` | 15 lines `crates/services/jackin-usage/src/coordinator/policy/tests.rs:28` | `assert!((1000..=1060).contains(&first))` (0 P hits) | UNMATCHED | no assertion needle from this body occurs on P |
| 7 | `canon_amp_presence_does_not_promote_durable_history` | 29 lines `crates/services/jackin-usage/src/host/tests/case_05.rs:36` | `history must not be selected implicitly` (0 P hits); `amp@example.com` (4 P hits); `historical` (23 P hits); `assert!(!rows[0].selected, "history must not be selected implicitly")` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 8 | `canon_desktop_inventory_is_grouped_and_complete` | 52 lines `crates/services/jackin-usage/src/host/tests/case_05.rs:151` | `57% left` (5 P hits); `opencode` (515 P hits); `assert_eq!(account.plan_or_status_label, "—")` (0 P hits); `.all(\|group\| group.surface_id != "opencode")` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 9 | `canon_each_account_retains_its_own_status_limit_and_error` | 42 lines `crates/services/jackin-usage/src/host/tests/case_05.rs:67` | `history unavailable` (0 P hits); `assert_eq!(history.last_error.as_deref(), Some("history unavailable"))` (0 P hits); `assert_eq!(current.remaining_percent, Some(88))` (0 P hits); `assert_eq!(history.remaining_percent, Some(11))` (0 P hits) | UNMATCHED | no assertion needle from this body occurs on P |
| 10 | `canon_open_rejects_unknown_surface_and_resets_changed_profile` | 28 lines `crates/services/jackin-usage/src/host/tests/case_05.rs:121` | `second profile` (0 P hits); `assert!(runtime.open(invalid).is_err())` (0 P hits); `.list_accounts(Some("codex"))` (0 P hits); `.expect("second profile")` (0 P hits) | UNMATCHED | no assertion needle from this body occurs on P |
| 11 | `canon_openai_account_never_appears_under_routed_providers` | 34 lines `crates/services/jackin-usage/src/host/tests/case_04.rs:234` | `minimax accounts` (0 P hits); `codex accounts` (1 P hits); `zai accounts` (0 P hits); `.list_accounts(Some("minimax"))` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 12 | `canon_presence_only_state_is_not_an_account` | 33 lines `crates/services/jackin-usage/src/host/tests/case_04.rs:305` | `amp accounts` (0 P hits); `unavailable` (557 P hits); `assert_eq!(amp.plan_or_status_label, "unavailable")` (0 P hits); `.map(\|state\| state.status_word.as_str()),` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 13 | `canon_same_account_label_on_two_providers_remains_two_accounts` | 33 lines `crates/services/jackin-usage/src/host/tests/case_04.rs:270` | `rows.iter().filter(\|row\| row.account_label == email).count(),` (0 P hits) | UNMATCHED | no assertion needle from this body occurs on P |
| 14 | `canon_sel_rejects_unknown_and_cross_surface_keys` | 23 lines `crates/services/jackin-usage/src/host/tests/case_04.rs:340` | `sha256:unknown` (3 P hits); `assert_eq!(rows.iter().filter(\|row\| row.selected).count(), 1)` (0 P hits); `assert!(runtime.set_selected_account("zai", &key).is_err())` (0 P hits); `.set_selected_account("codex", "sha256:unknown")` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 15 | `canon_sel_stale_persisted_key_remains_explicitly_unavailable` | 29 lines `crates/services/jackin-usage/src/host/tests/case_04.rs:365` | `sha256:unknown` (3 P hits); `assert_eq!(persisted.get("codex"), Some(&"sha256:unknown".to_owned()))` (0 P hits); `assert_eq!(snapshot.status, UsageSnapshotStatus::Unavailable)` (0 P hits); `assert_eq!(rows[0].account_key, key)` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 16 | `canon_sel_valid_historical_choice_survives_reopen` | 27 lines `crates/services/jackin-usage/src/host/tests/case_05.rs:7` | `historical` (23 P hits); `assert_eq!(rows[0].lifecycle, "historical")` (0 P hits); `assert!(rows[0].selected)` (0 P hits); `assert_eq!(rows.len(), 1)` (8 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 17 | `canonical_projection_alias_transition_is_atomic_idempotent_and_fail_closed` | 32 lines `crates/services/jackin-usage/src/host/tests/case_01.rs:142` | `failed transaction preserves alias` (0 P hits); `canonical account alias collision` (1 P hits); `conflicting alias` (0 P hits); `alias replay` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 18 | `canonical_projection_keeps_unresolved_capability_out_of_account_rows` | 31 lines `crates/services/jackin-usage/src/host/tests/case_01.rs:85` | `assert_eq!(projection.unresolved[0].provider_id, "openai")` (0 P hits); `assert!(projection.providers[0].accounts.is_empty())` (0 P hits); `assert_eq!(projection.unresolved.len(), 1)` (0 P hits); `assert_eq!(projection.providers.len(), 1)` (1 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 19 | `canonical_projection_provider_order_is_settled_and_not_agent_named` | 22 lines `crates/services/jackin-usage/src/host/tests/case_01.rs:118` | `OpenRouter` (122 P hits); `Anthropic` (256 P hits); `OpenCode` (216 P hits); `.map(\|surface\| surface.label())` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 20 | `canonical_projection_uses_current_membership_provider_names_and_rust_ranks` | 76 lines `crates/services/jackin-usage/src/host/tests/case_01.rs:7` | `Selected account is no longer available.` (2 P hits); `reads must not republish generations` (0 P hits); `Alpha@example.test` (0 P hits); `zulu@example.test` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 21 | `canonical_runtime_preserves_action_required_failure_without_snapshot` | 30 lines `crates/services/jackin-usage/src/host/projection/tests/case_06.rs:65` | `Approve credential access` (0 P hits); `needs_secret` (10 P hits); `assert_eq!(account.freshness.phase, UsageFreshnessPhaseV1::Failed)` (0 P hits); `assert_eq!(account.issues[0].message, "Approve credential access")` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 22 | `canonical_runtime_preserves_broker_failure_retry_and_recovery` | 56 lines `crates/services/jackin-usage/src/host/projection/tests/case_06.rs:7` | `Provider request quota exceeded` (0 P hits); `rate_limited` (11 P hits); `assert_eq!(account.freshness.last_good_at_epoch, Some(1_800_000_000))` (0 P hits); `assert_eq!(account.freshness.phase, UsageFreshnessPhaseV1::Current)` (1 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 23 | `canonical_runtime_projects_discovery_diagnostics_without_account_rows` | 36 lines `crates/services/jackin-usage/src/host/projection/tests/case_06.rs:111` | `keychain_consent_required` (4 P hits); `config_invalid` (3 P hits); `assert_eq!(projection.issues[0].scope, UsageIssueScopeV1::Projection)` (0 P hits); `assert_eq!(projection.issues[0].code, "config_invalid")` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 24 | `catalog_cas_rejects_a_stale_rotation_after_a_newer_winner` | 36 lines `crates/services/jackin-usage/src/host/broker/tests/case_04.rs:264` | `assert_eq!(client.current_projection().unwrap(), winner)` (0 P hits); `UsageCoordinationErrorKind::CatalogRevisionConflict` (13 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 25 | `catalog_conflict_fails_closed_after_bounded_retries` | 57 lines `crates/services/jackin-usage/src/host/broker/tests/case_07.rs:7` | `every attempt must run its own post-lease discovery scan` (0 P hits); `"every attempt must run its own post-lease discovery scan"` (0 P hits); `UsageCoordinationErrorKind::CatalogRevisionConflict` (13 P hits); `reconciles.load(Ordering::SeqCst),` (2 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 26 | `catalog_conflict_retries_with_rediscovery_then_succeeds` | 64 lines `crates/services/jackin-usage/src/host/broker/tests/case_06.rs:325` | `every conflict retry must re-discover, not reuse the losing scan` (0 P hits); `generation-fresh` (0 P hits); `assert_eq!(final_projection.discovery_revision, "generation-fresh")` (0 P hits); `"every conflict retry must re-discover, not reuse the losing scan"` (0 P hits) | UNMATCHED | no assertion needle from this body occurs on P |
| 27 | `catalog_revision_change_purges_old_state_and_allows_only_new_revision` | 41 lines `crates/services/jackin-usage/src/coordinator/tests/case_02.rs:228` | `join_ok(&coordinator, &account, first.generation, 1_001).phase,` (cluster 3/4) | RENAMED_ON_P | `catalog_revision_change_stores_cooldown_marker_and_allows_new_revision` `crates/services/jackin-usage-coordinator/src/tests/case_02.rs:1714` |
| 28 | `catalog_revocation_retains_materialized_last_good_but_fences_late_result` | 77 lines `crates/services/jackin-usage/src/coordinator/tests/case_02.rs:149` | `revocation must clear active ownership` (cluster 2/5) | RENAMED_ON_P | `catalog_revocation_clears_materialized_data_and_fences_late_result` `crates/services/jackin-usage-coordinator/src/tests/case_02.rs:1115` |
| 29 | `claude_cli_usage_output_maps_current_windows` | 36 lines `crates/services/jackin-usage/src/usage/tests/case_08.rs:7` | `Session 100% · Weekly 54%` (0 P hits); `assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session))` (8 P hits); `assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly))` (8 P hits); `assert_eq!(buckets[0].remaining_percent, Some(100))` (2 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 30 | `claude_cli_usage_output_maps_scoped_weekly_fable` | 34 lines `crates/services/jackin-usage/src/usage/tests/case_08.rs:45` | `Session 91% · Weekly 72%` (0 P hits); `assert!((usage.scoped_weekly[0].1 - 35.0).abs() < f64::EPSILON)` (0 P hits); `assert_eq!(fable.remaining_percent, Some(65))` (1 P hits); `assert_eq!(usage.scoped_weekly[0].0, "Fable")` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 31 | `claude_code_user_agent_parses_cli_version` | 21 lines `crates/services/jackin-usage/src/usage/tests/case_09.rs:291` | `Claude Code 2.1.7\n` (0 P hits); `Claude Code 2.2.0` (0 P hits); `claude-code/2.2.0` (0 P hits); `--version` (115 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 32 | `claude_scope_restriction_error_is_explicit` | 46 lines `crates/services/jackin-usage/src/usage/tests/case_14.rs:233` | `Claude token lacks usage scope (inference-only); quota unavailable` (1 P hits); `Claude OAuth usage request failed: connection reset` (0 P hits); `Claude OAuth usage HTTP 401 Unauthorized` (0 P hits); `HTTP 403 insufficient_scope` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 33 | `claude_usage_diagnostic_invokes_explicit_usage_command` | 19 lines `crates/services/jackin-usage/src/usage/tests/case_07.rs:335` | `usage output` (4 P hits); `assert_eq!(diagnostic.args, vec!["-p", "/usage"])` (0 P hits); `assert_eq!(diagnostic.stdout, "usage output")` (0 P hits); `assert_eq!(diagnostic.command, "claude")` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 34 | `claude_usage_diagnostic_preserves_cli_failure_output` | 15 lines `crates/services/jackin-usage/src/usage/tests/case_07.rs:356` | `not logged in` (0 P hits); `assert_eq!(diagnostic.stderr, "not logged in")` (0 P hits); `assert_eq!(diagnostic.exit_code, Some(1))` (0 P hits); `assert!(!diagnostic.success)` (0 P hits) | UNMATCHED | no assertion needle from this body occurs on P |
| 35 | `cli_fallback_last_error_uses_normalized_scope_message` | 36 lines `crates/services/jackin-usage/src/usage/claude/tests.rs:7` | `Claude token lacks usage scope (inference-only); quota unavailable` (1 P hits); `Claude provider usage unavailable; cached quota is stale` (1 P hits); `oauth boom` (0 P hits); `claude_resolved_last_error(UsageSnapshotStatus::Fresh, Some(normalized), true...` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 36 | `compact_depleted_with_and_without_resets_at` | 60 lines `crates/services/jackin-usage/src/host/tests/case_03.rs:7` | `expected depleted countdown form, got {label}` (0 P hits); `depleted used style` (0 P hits); `depleted no reset` (0 P hits); `Cl resets` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 37 | `compact_status_bar_label_empty_when_unavailable_or_disabled` | 24 lines `crates/services/jackin-usage/src/host/tests/case_02.rs:86` | `unavailable without remaining_percent must not invent %` (0 P hits); `all-disabled must yield empty compact label` (0 P hits); `"unavailable without remaining_percent must not invent %"` (0 P hits); `runtime.compact_status_bar_label().expect("compact"),` (0 P hits) | UNMATCHED | no assertion needle from this body occurs on P |
| 38 | `compact_status_bar_label_for_pinned_known_and_disabled` | 24 lines `crates/services/jackin-usage/src/host/tests/case_02.rs:231` | `disabled` (155 P hits); `Cl 37%` (1 P hits); `.compact_status_bar_label_for("claude")` (0 P hits); `.compact_status_bar_label_for("codex")` (0 P hits) | UNMATCHED | assertion text on P is not an ancestor-unique cluster in one different function |
| 39 | `compact_status_bar_label_picks_lowest_remaining_percent` | 27 lines `crates/services/jackin-usage/src/host/tests/case_02.rs:40` | `compact used` (0 P hits); `Cx 18%` (0 P hits); `Cx 82%` (0 P hits); `runtime.compact_status_bar_label().expect("compact used"),` (0 P hits) | UNMATCHED | no assertion needle from this body occurs on P |
| 40 | `compact_status_bar_label_tie_keeps_all_order` | 15 lines `crates/services/jackin-usage/src/host/tests/case_02.rs:69` | `Cx 40%` (0 P hits); `runtime.compact_status_bar_label().expect("compact"),` (0 P hits) | UNMATCHED | no assertion needle from this body occurs on P |

The two proved matches share a cited assertion string and part of the surrounding cluster. On P, `catalog_revision_change_stores_cooldown_marker_and_allows_new_revision` asserts the purge list is empty and stores a cooldown marker. The ancestor function asserts the account is present in the purge list. On P, `catalog_revocation_clears_materialized_data_and_fences_late_result` asserts `revoked.snapshot == None`. The ancestor function asserts `remaining_percent == Some(80)`. The cited strings are the rename proof. The assertions that differ on those two tests stay preservation leads.

## Every name

| Name | Final class | Detail |
| --- | --- | --- |
| `activate_broker` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/bridge.rs:551` |
| `activation_scope` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker/tests/support.rs:292` |
| `apply_broker_generation` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker.rs:67` |
| `bounded_events_and_refresh_floor` | UNMATCHED | attempted, rename not proved; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_02.rs:122` |
| `broker_catalog_admits_current_identity_and_rejects_stale_identity` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_04.rs:55` |
| `broker_client_refresh_returns_immediately_and_joins_one_generation` | UNMATCHED | attempted, rename not proved; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_01.rs:168` |
| `broker_failure_for_anonymous_source_stays_surface_scoped` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_02.rs:87` |
| `broker_failure_without_snapshot_surfaces_honest_gap_in_snapshot` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_02.rs:7` |
| `broker_lock` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/bridge.rs:579` |
| `broker_policy_retry_jitter_is_stable_bounded_and_provider_wins` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/coordinator/policy/tests.rs:28` |
| `broker_refresh_in_progress` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker.rs:54` |
| `canon_amp_presence_does_not_promote_durable_history` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:36` |
| `canon_desktop_inventory_is_grouped_and_complete` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:151` |
| `canon_each_account_retains_its_own_status_limit_and_error` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:67` |
| `canon_open_rejects_unknown_surface_and_resets_changed_profile` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:121` |
| `canon_openai_account_never_appears_under_routed_providers` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_04.rs:234` |
| `canon_presence_only_state_is_not_an_account` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_04.rs:305` |
| `canon_projection_ignores_removed_legacy_shared_snapshot` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:111` |
| `canon_same_account_label_on_two_providers_remains_two_accounts` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_04.rs:270` |
| `canon_sel_rejects_unknown_and_cross_surface_keys` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_04.rs:340` |
| `canon_sel_stale_persisted_key_remains_explicitly_unavailable` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_04.rs:365` |
| `canon_sel_valid_historical_choice_survives_reopen` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:7` |
| `canonical_capabilities_for_launch` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-runtime/src/usage_relay.rs:625` |
| `canonical_discovered_account` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/tests/support.rs:65` |
| `canonical_instance_id` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:82` |
| `canonical_projection` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/projection.rs:96` |
| `canonical_projection_alias_transition_is_atomic_idempotent_and_fail_closed` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_01.rs:142` |
| `canonical_projection_keeps_unresolved_capability_out_of_account_rows` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_01.rs:85` |
| `canonical_projection_provider_order_is_settled_and_not_agent_named` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_01.rs:118` |
| `canonical_projection_uses_current_membership_provider_names_and_rust_ranks` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_01.rs:7` |
| `canonical_runtime_preserves_action_required_failure_without_snapshot` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/projection/tests/case_06.rs:65` |
| `canonical_runtime_preserves_broker_failure_retry_and_recovery` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/projection/tests/case_06.rs:7` |
| `canonical_runtime_projects_coordination_failure_and_retains_last_good` | UNMATCHED | ancestor body 102 lines, at or over 80; ancestor `crates/services/jackin-usage/src/host/projection/tests/case_06.rs:149` |
| `canonical_runtime_projects_discovery_diagnostics_without_account_rows` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/projection/tests/case_06.rs:111` |
| `capabilities_for_forwarded_scope` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker.rs:481` |
| `catalog_cas_rejects_a_stale_rotation_after_a_newer_winner` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_04.rs:264` |
| `catalog_conflict_fails_closed_after_bounded_retries` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_07.rs:7` |
| `catalog_conflict_retries_with_rediscovery_then_succeeds` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_06.rs:325` |
| `catalog_revision_change_purges_old_state_and_allows_only_new_revision` | RENAMED_ON_P | catalog_revision_change_stores_cooldown_marker_and_allows_new_revision at crates/services/jackin-usage-coordinator/src/tests/case_02.rs:1714; needle `join_ok(&coordinator, &account, first.generation, 1_001).phase,` |
| `catalog_revocation_retains_materialized_last_good_but_fences_late_result` | RENAMED_ON_P | catalog_revocation_clears_materialized_data_and_fences_late_result at crates/services/jackin-usage-coordinator/src/tests/case_02.rs:1115; needle `revocation must clear active ownership` |
| `claude_cli_usage_output_maps_current_windows` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/usage/tests/case_08.rs:7` |
| `claude_cli_usage_output_maps_scoped_weekly_fable` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/usage/tests/case_08.rs:45` |
| `claude_code_user_agent` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/usage/claude.rs:1322` |
| `claude_code_user_agent_parses_cli_version` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/usage/tests/case_09.rs:291` |
| `claude_code_user_agent_with` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/usage/claude.rs:1337` |
| `claude_code_version_from_text` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/usage/claude.rs:1349` |
| `claude_scope_restriction_error_is_explicit` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/usage/tests/case_14.rs:233` |
| `claude_usage_diagnostic_invokes_explicit_usage_command` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/usage/tests/case_07.rs:335` |
| `claude_usage_diagnostic_preserves_cli_failure_output` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/usage/tests/case_07.rs:356` |
| `cli_fallback_last_error_uses_normalized_scope_message` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/usage/claude/tests.rs:7` |
| `commit_staged_discovery` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/discovery.rs:2397` |
| `compact_depleted_with_and_without_resets_at` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_03.rs:7` |
| `compact_status_bar_label_empty_when_unavailable_or_disabled` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:86` |
| `compact_status_bar_label_for_pinned_known_and_disabled` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:231` |
| `compact_status_bar_label_picks_lowest_remaining_percent` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:40` |
| `compact_status_bar_label_tie_keeps_all_order` | UNMATCHED | attempted, rename not proved; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:69` |
| `compact_status_bar_strip_all_eight_host_surfaces` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_03.rs:143` |
| `compact_status_bar_strip_hard_cap_three_and_hides_zero` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:279` |
| `compact_status_bar_strip_soonest_reset_beats_higher_remaining` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:309` |
| `compact_status_bar_strip_soonest_then_remaining_cap_and_separator` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:257` |
| `concurrent_catalog_rotations_publish_one_complete_revision` | UNMATCHED | ancestor body 85 lines, at or over 80; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_04.rs:177` |
| `confidence_label` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:786` |
| `confirmed_empty_scan_still_revokes_live_catalog` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_07.rs:129` |
| `coordinator_post_terminal_manual_refresh_starts_later_generation` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/coordinator/tests/case_01.rs:73` |
| `counting_broker` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker/tests/support.rs:299` |
| `desktop_inventory_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:391` |
| `desktop_inventory_for_catalog` | UNMATCHED | ancestor body 88 lines, at or over 80; ancestor `crates/services/jackin-usage/src/host.rs:1422` |
| `desktop_projection_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:401` |
| `desktop_projection_keeps_missing_selected_route_without_sibling_fallback` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/selected_account_route.rs:11` |
| `desktop_provider_group_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:486` |
| `desktop_provider_projection_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:431` |
| `desktop_provider_state_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:503` |
| `detail_presentation_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:571` |
| `detail_presentation_rides_the_snapshot_dto` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_02.rs:226` |
| `detail_row_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:661` |
| `diagnostic_view_for_surface` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:980` |
| `disable_surface_removes_from_list_and_blocks_snapshot` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:7` |
| `disabled_probe_policy_skips_dispatch_and_is_never_due` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_06.rs:192` |
| `disc_dedup_adapter_reuses_handle_for_same_provider_secret` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/discovery/tests.rs:43` |
| `disc_dedup_legacy_shared_snapshot_never_creates_active_row` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/discovery/tests/case_03.rs:162` |
| `disc_diagnostics_export_only_sanitized_scope_and_copy` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_01.rs:228` |
| `disc_source_adapter_caches_identical_declaration_before_second_read` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/discovery/tests.rs:13` |
| `disc_source_manual_retry_evicts_only_failed_resolution` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/discovery/tests.rs:76` |
| `disc_unresolved_same_labels_do_not_overwrite_discovered_views` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/discovery/tests/case_02.rs:142` |
| `discovered_account_keys` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:1883` |
| `discovery_diagnostic_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:55` |
| `driving_bucket_for` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:1628` |
| `dual_bucket_snapshot_exposes_session_and_weekly_remainings` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:361` |
| `enabled_surface_ids` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:1861` |
| `ensure_discovered_account` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/discovery.rs:2486` |
| `ensure_usage_broker` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker.rs:1746` |
| `ensure_usage_broker_publishes_fresh_discovery_not_stale_caller_input` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_07.rs:174` |
| `ensure_usage_broker_with_hooks` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker.rs:1773` |
| `estimate_caption_for` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:1623` |
| `event_batch_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:554` |
| `event_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:562` |
| `events_cursor_advances_and_bounds` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:149` |
| `existing_broker_reconcile_revokes_without_returning_stale_projection` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_04.rs:318` |
| `failed_generation` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker/tests/support.rs:176` |
| `failed_production_rotation_retains_last_good_discovery_and_broker` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_01.rs:70` |
| `fetch_claude_cli_usage` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/usage/claude.rs:1375` |
| `fixture_snapshot_matches_capsule_view_fields` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_01.rs:193` |
| `fixture_snapshot_round_trip_via_bridge` | UNMATCHED | ancestor body 89 lines, at or over 80; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_01.rs:266` |
| `format_compact_entry` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:1639` |
| `format_prefs` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:1169` |
| `glance_daily_bucket` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/tests/support.rs:215` |
| `glance_view` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/tests/support.rs:224` |
| `glance_weekly_bucket` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/tests/support.rs:198` |
| `headline` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/usage/claude.rs:961` |
| `hermetic_layout_never_starts_host_usage_discovery` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-runtime/src/usage_relay/tests/case_02.rs:302` |
| `host_accounts_path` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:440` |
| `host_paths_under_data_dir` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:218` |
| `host_snapshot_store_path` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:434` |
| `identity_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:644` |
| `inject_dual_remaining` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/tests/support.rs:126` |
| `inject_remaining` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/tests/support.rs:83` |
| `inject_remaining_at` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/tests/support.rs:87` |
| `inject_snapshot` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:868` |
| `isolated_cli_missing_sibling_fails_with_installation_diagnostic` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/apps/jackin/tests/broker_installation.rs:37` |
| `launch_discovery_relay_uses_distinct_canonical_ids_for_same_surface` | UNMATCHED | ancestor body 121 lines, at or over 80; ancestor `crates/services/jackin-runtime/src/usage_relay/tests/case_01.rs:278` |
| `live_probes_enabled` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker.rs:48` |
| `lock_activation` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker.rs:1718` |
| `merged_bar_skips_disabled_surfaces` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:22` |
| `metadata_detail_row_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:690` |
| `missing_broker_override_preserves_failed_path` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/apps/jackin/tests/broker_installation.rs:64` |
| `money_bucket_preserved_in_host_snapshot` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:112` |
| `money_dto_preserves_full_signed_domain_currency_and_exponent` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/dto/tests.rs:51` |
| `multi_account_list_select_and_snapshot` | UNMATCHED | ancestor body 157 lines, at or over 80; ancestor `crates/services/jackin-usage/src/host/tests/case_03.rs:174` |
| `next_events_resync_flag_not_error` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:201` |
| `next_refresh_label_due_and_countdown` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_03.rs:69` |
| `open_bridge_with_live` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/support.rs:35` |
| `open_prepared` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host.rs:752` |
| `open_runtime_activation_failure_is_error_and_keeps_runtime_closed` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_01.rs:39` |
| `open_runtime_publishes_catalog_before_broker_admission` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_01.rs:7` |
| `open_with_discovery` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:729` |
| `open_with_validated_discovery` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:744` |
| `overview_row_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:705` |
| `overview_rows_and_format_prefs_round_trip` | UNMATCHED | ancestor body 113 lines, at or over 80; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_02.rs:7` |
| `overview_rows_numeric_and_status_word` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_03.rs:83` |
| `panic_probe_is_contained` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_01.rs:257` |
| `parse_claude_usage_output` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/usage.rs:1623` |
| `print_bare_limit` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/apps/jackin/src/cli/usage.rs:285` |
| `production_projection_runtime` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/projection/tests/support_04.rs:18` |
| `production_rotation_revokes_in_flight_join_and_clears_bridge_phase` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_01.rs:122` |
| `provider_glance_row_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:181` |
| `provider_glance_rows_accept_affirmative_origin_when_unsupported` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_06.rs:7` |
| `provider_glance_rows_do_not_fallback_to_unrelated_slots` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_06.rs:30` |
| `provider_glance_rows_empty_without_credentials` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:356` |
| `provider_glance_rows_icon_keys_match_closed_desktop_domain` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_06.rs:77` |
| `provider_glance_rows_marks_canonical_placeholder_refreshing` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_06.rs:122` |
| `provider_glance_rows_never_include_opencode` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_06.rs:58` |
| `provider_glance_rows_preserve_dimmed_last_known` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_06.rs:101` |
| `provider_glance_rows_redetect_new_credentials` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_06.rs:167` |
| `provider_glance_rows_reject_negative_credential_placeholders` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:364` |
| `provider_glance_rows_select_amp_daily` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:278` |
| `provider_glance_rows_show_dash_before_first_success` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:332` |
| `provider_glance_rows_show_dash_for_paid_only_amp` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:302` |
| `provider_glance_rows_show_three_weekly_labels` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:230` |
| `provider_glance_rows_use_exact_seven_provider_order` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_05.rs:205` |
| `provider_glance_rows_via_bridge_project_rust_rows` | UNMATCHED | ancestor body 92 lines, at or over 80; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/case_02.rs:132` |
| `reconcile_broker_catalog` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/bridge.rs:519` |
| `reconcile_discovery` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/discovery.rs:2437` |
| `reconcile_selected_accounts` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:1750` |
| `record_broker_error` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker.rs:190` |
| `record_broker_error_view` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker.rs:138` |
| `record_discovered_snapshot` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/discovery.rs:2447` |
| `refresh_floor_tracks_completed_broker_refresh` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:176` |
| `registered_api_account_resolves_without_importing_ambient_credentials` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/discovery/tests.rs:101` |
| `removed_last_selected_account_keeps_unavailable_provider_and_restores_exact_key` | UNMATCHED | ancestor body 122 lines, at or over 80; ancestor `crates/services/jackin-usage/src/host/tests/case_04.rs:7` |
| `require_open` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:1839` |
| `rotated_catalog_revision_rejects_in_flight_broker_result` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_04.rs:7` |
| `run_claude_usage_diagnostic_with` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/usage.rs:1604` |
| `run_host` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/apps/jackin/src/cli/usage.rs:298` |
| `run_host_snapshot` | UNMATCHED | ancestor body 104 lines, at or over 80; ancestor `crates/apps/jackin/src/cli/usage.rs:313` |
| `schedule_join` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/bridge.rs:593` |
| `scoped` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/usage/claude.rs:948` |
| `scripted_discovery` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker/tests/support.rs:262` |
| `seeded_personal_account` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/tests/support.rs:172` |
| `selected_account_missing` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:1778` |
| `selected_account_route_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:458` |
| `selected_account_route_dto_preserves_each_typed_state` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/adapters/jackin-usage-ffi/src/dto/tests.rs:7` |
| `selected_account_route_is_unselected_without_persisted_choice` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_01.rs:288` |
| `selected_account_route_resolves_during_cold_placeholder_without_sibling_fallback` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_01.rs:302` |
| `selected_account_routes_remain_scoped_to_their_provider` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_01.rs:328` |
| `selected_route_and_view_for_catalog` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:1796` |
| `selected_view_for_catalog` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:1788` |
| `sequential_reconcile_after_fresh_read_still_accepts_last_writer` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_07.rs:220` |
| `slow_activator_stale_caller_catalog_never_wins` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_06.rs:246` |
| `snapshot_surfaces_discovery_diagnostic_instead_of_refreshing` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_01.rs:246` |
| `source_label` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:782` |
| `stage_discovery` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/discovery.rs:2365` |
| `status_bar_provider_glance_rows_sb3_sb17_sb19` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_02.rs:329` |
| `surface_dto` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:544` |
| `surface_enabled` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:819` |
| `surface_refresh_in_progress` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker.rs:60` |
| `to_host_config` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:791` |
| `transient_empty_scan_does_not_wipe_live_catalog` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/broker/tests/case_07.rs:66` |
| `unavailable_and_refreshing_never_invent_percent` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-usage/src/host/tests/case_01.rs:217` |
| `usage_broker_handle_for` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host/broker.rs:1845` |
| `usage_relay_dispatch_denies_projection_for_surface` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/services/jackin-runtime/src/usage_relay/tests/case_03.rs:89` |
| `usage_relay_rejects_host_only_catalog_reconciliation` | UNMATCHED | eligible, past the 40-attempt cap; ancestor `crates/apps/jackin-capsule/src/usage_relay_proxy/tests/case_01.rs:216` |
| `validated_discovery` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:813` |
| `view_dto_with_context` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/dto.rs:615` |
| `with_discovery_diagnostic` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/services/jackin-usage/src/host.rs:958` |
| `write_account_config` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/support.rs:56` |
| `write_isolated_config` | UNMATCHED | ancestor body under 80 lines and has no assertion needle; ancestor `crates/adapters/jackin-usage-ffi/src/bridge/tests/support.rs:5` |

## Preservation

204 UNMATCHED names stay preservation leads. This pass did not show that their behavior was deleted, and it did not show that P still has that behavior under a new name, except for the 2 names classed RENAMED_ON_P. The 0 ONLY_ON_B044 names are definitions that this search did not find on the ancestor. The 0 ABSENT_EVERYWHERE names have no `fn NAME` on B044. Exact name absence on M, P, and U is not proof of lost behavior.
