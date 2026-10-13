# TASK-002 — model, effort, OMP durability, mise lock

Read-only inspection of `/Users/donbeave/Projects/jackin-project/jackin`. Commands were `git log`, `git rev-list`, `git merge-base`, `git show`, `git grep`, and `git diff` only. No checkout, commit, build, or source edit.

| Pin | SHA | Subject |
| --- | --- | --- |
| M | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` | fix(usage): retain foreground broker for auth lease (#1124) |
| P | `fba69d60c2373156dc0dcecee8e085c83776966f` | docs: record Velnor d35 lock audit |
| B001 | `3cb98f2a634c1e660e501489d7383329569db7a3` | WIP: preserve c72 record changes |
| B002 | `b1e08b880223594cceccf3ea44322d44e3cdf984` | WIP: preserve current evidence record edits |
| B078 | `ef40fea11a54ade7911de7622e6e011707241dd4` | WIP recovery snapshot: jackin main working-tree mise.lock |
| B079 | `d04467f05f1183008322047b1828c201e551e947` | Add model and effort overrides to load |

`git rev-list --count P..TIP` matches the brief: B001 32, B002 35, B078 1, B079 43. None of these tips is an ancestor of M or of P. Merge-base of each tip with P is `0aa821a088e1bacf3d4d85a4c9faaa67faa85132`. `M...TIP` is 5 behind and N ahead for each tip. The five commits on M and not on these tips are `9b6e1d92` (transition squash-merge), `868ce535`, `3f3ddd7c`, `835df807`, and `1c3dd1be`. A commit missing from P is not treated as unique when the same outcome exists in the M or P tree under a later path.

B001 and B002 share the model, restore, and OMP commits. B001's only commit that B002 does not have is `3cb98f2a`, which edits `plans/jackin-build-split/` and does not change product code. B079 is a different line that adds the CLI override and an earlier OMP snapshot implementation. B078 is one `mise.lock` commit.

## Behaviors absent from both M and P

These three outcomes succeed on the source tips and fail closed on both M and P. M and P rewrote the snapshot into `OmpSnapshot` / `validate_wal`. That rewrite is not the same success path.

### 1. A stale WAL generation suffix is skipped and the committed image is synced

Source `apply_committed_omp_wal` stops at the first frame whose salts do not match the current header, after at least one valid current-generation frame, and materializes the last commit. A salt mismatch whose checksum still follows the current chain fails closed. A stale suffix whose checksum does not follow the chain is ignored.

| Tree | Path |
| --- | --- |
| B001 and B002 | `crates/jackin-instance/src/auth.rs:3622` `apply_committed_omp_wal` |
| B001 and B002 tests | `crates/jackin-instance/src/auth/tests.rs:344` `omp_sync_ignores_stale_old_generation_frames_after_current_commit`; `:376` `omp_sync_ignores_stale_suffix_after_uncommitted_current_generation_frames` |
| B079 | `crates/jackin-instance/src/auth.rs:4720` `apply_committed_omp_wal` |
| B079 tests | `crates/jackin-instance/src/auth/tests.rs:380` `omp_sync_ignores_stale_old_generation_frames_after_current_commit`; `:410` `omp_sync_ignores_new_uncommitted_frames_before_stale_generation_suffix` |

Those tests sync and keep `fixture-reused-current-token` or `fixture-main-selected-token`. They drop the stale or uncommitted marker.

M and P return `WalError::InvalidSalt` on the first salt mismatch and never materialize the prefix.

| Tree | Path |
| --- | --- |
| M | `crates/jackin-omp-store/src/wal.rs:229` `return Err(WalError::InvalidSalt)` |
| M tests | `crates/jackin-instance/src/auth/tests.rs:387` `omp_sync_rejects_reused_wal_generations_instead_of_falling_back_to_old_state` |
| P | `crates/adapters/jackin-omp-store/src/wal.rs:226` same `InvalidSalt` return inside `validate_wal` |
| P tests | `crates/adapters/jackin-omp-store/src/wal/tests/case_01.rs:20` `rejects_a_stale_generation_suffix_instead_of_falling_back`; `crates/services/jackin-instance-roles/src/tests/auth_case_01.rs:183` `omp_sync_rejects_reused_wal_generations_instead_of_falling_back_to_old_state` |

`git grep omp_sync_ignores_stale` is absent on M and on P.

### 2. A partial WAL frame is ignored and the committed image is still synced

B001/B002 `omp_sync_ignores_partial_and_valid_uncommitted_suffixes` (`crates/jackin-instance/src/auth/tests.rs:424`) and B079 `omp_sync_ignores_partial_and_valid_uncommitted_wal_suffixes` (`crates/jackin-instance/src/auth/tests.rs:458`) append either a 19-byte partial frame or a complete uncommitted frame and still sync `fixture-wal-current-token`.

M and P accept a complete same-generation uncommitted tail and reject a partial frame. That partial-frame success path is absent.

| Tree | Path |
| --- | --- |
| M | `crates/jackin-instance/src/auth/tests.rs:409` `omp_sync_accepts_a_complete_uncommitted_tail_but_rejects_a_partial_frame`; `crates/jackin-omp-store/src/wal.rs` `validate_wal` returns `WalError::InvalidLength` unless the body length is a whole number of frames |
| P | `crates/services/jackin-instance-roles/src/tests/auth_case_01.rs:205` same accept-complete / reject-partial test; `crates/adapters/jackin-omp-store/src/wal.rs:189` `InvalidLength` when `body_len` is not a multiple of the frame size; `crates/adapters/jackin-omp-store/src/wal/tests/case_01.rs:79` `rejects_partial_frames_and_present_empty_wal` |

The complete uncommitted tail is present on M and P. `a_stable_uncommitted_wal_spill_exports_the_last_committed_selected_row` exports the last committed row and excludes the spill bytes. P: `crates/adapters/jackin-omp-store/src/capture/tests/case_01.rs`. M: `crates/jackin-omp-store/src/capture.rs`.

### 3. A checkpoint between the database read and the WAL read is retried and the checkpointed image is synced

B001/B002 `omp_sync_retries_across_real_sqlite_checkpoint_reset_interleaving` (`crates/jackin-instance/src/auth/tests.rs:447`) and B079 `omp_snapshot_retries_when_sqlite_checkpoints_between_database_and_wal_reads` (`crates/jackin-instance/src/auth/tests.rs:301`) replace `agent.db` / `agent.db-wal` after the database read with the checkpointed image and an empty WAL. Provision returns synced and the bytes contain `fixture-wal-current-token`.

M and P reject a writer that commits or checkpoints between capture passes. The result is `Err(OmpError::Unavailable)`.

| Tree | Path |
| --- | --- |
| M | `crates/jackin-omp-store/src/capture.rs:1152` `source_writers_that_commit_or_checkpoint_between_capture_passes_are_rejected` |
| P | `crates/adapters/jackin-omp-store/src/capture/tests/case_01.rs:35` same test |

## Already on M and P

### `load --model` and `--effort`

Trim-and-reject, the four-value effort enum, dry-run null keys, launch, and restore are on M and on P. B079's CLI commit `d04467f0` and B001/B002's `42c6f8ed` plus `8dac3d6e` are the source of that contract. P moved the same functions into the split crates.

| Behavior | Source tip | P path compared |
| --- | --- | --- |
| Trim and reject an empty model | B079 `crates/jackin/src/cli/role.rs:99` `parse_model_id` trims, rejects with `model ID must not be empty`, returns the trimmed id. M `crates/jackin/src/cli/role.rs:99` `parse_model` trims and rejects with `model ID cannot be empty`. | P `crates/apps/jackin/src/cli/role.rs:99` `parse_model` matches M, including the error string |
| Effort enum `low` / `medium` / `high` / `max`, default `Medium`, trim and case-fold on parse | B002 and B079 `crates/jackin-core/src/reasoning_effort.rs` | P `crates/core/jackin-core/src/reasoning_effort.rs`. The enum and `FromStr` body match |
| Dry-run keeps existing keys and writes null `model_override` and `effort` when unset | B002 `crates/jackin/src/app/load_cmd.rs` `apply_dry_run_load_overrides_json` matches M. B079 `dry_run_json_reports_explicit_model_and_effort_overrides` matches M | P `crates/apps/jackin/src/app/load_cmd.rs:833` `apply_dry_run_load_overrides_json`. Test `crates/apps/jackin/src/app/load_cmd/tests/case_02.rs` `dry_run_json_reports_explicit_model_and_effort_overrides` differs from M only by `super::` versus the split module path |
| Launch carries model and effort; empty model fails validation | B002 `resolve_dry_run_model_projection`, `role_model_defaults`, `resolved_instance_models_from_role_models`, `launch_model_and_effort_overrides_fan_out_to_every_codex_slot`, `an_empty_model_override_is_a_validation_failure` match M after whitespace normalization | P `crates/services/jackin-runtime-launch-dry-run/src/dry_run.rs`, `crates/services/jackin-runtime-launch-capsule-setup/src/capsule_setup.rs`, `crates/services/jackin-runtime/src/runtime/launch/programmatic/tests/case_01.rs`. Call sites use the split crate names. The projection tests match |
| Restore rejects model and effort overrides and keeps the stored agent | B002 `current_role_reuse_is_compatible` and `validate_explicit_restore_options` still require `opts.model` and `opts.effort` to be absent. M and P fold `account` and `configuration` into `opts.selection` and keep the model and effort checks. Stored-agent tests match byte for byte | P `crates/services/jackin-runtime/src/runtime/launch/launch_pipeline.rs:1709` and `:1731`. Tests `crates/services/jackin-runtime/src/runtime/launch/launch_pipeline/tests/restore_reuse_intent.rs`. Stored agent: `crates/services/jackin-runtime/src/runtime/launch/tests/case_16.rs:227` `explicit_restore_rejects_agent_different_from_stored_instance` and `:260` `load_agent_attaches_explicit_restore_container_with_stored_agent_before_role_repo` |

B002 `parse_model` (`crates/jackin/src/cli/role.rs:99`) rejects a whitespace-only model and then returns `Ok(s.to_owned())`, so a padded non-empty id keeps its padding. That return is not on M or P. M and P store the trimmed id. Restoring the B002 return would undo the trim that is already on both trees. B079 already returns the trimmed id. The B079 error text `must not be empty` is not the M/P text `cannot be empty`. Both reject the empty value.

B079 help text asserts `` low`, `medium`, `high`, or `max ``. M and P assert `(low, medium, high, or max)`. Both document the same four values.

### Config journal, role overrides, and restore durability

B079 `crates/jackin-config/src/persist.rs` has 33 functions. All 33 match M `crates/jackin-config/src/persist.rs` and P `crates/core/jackin-config/src/persist.rs` after dropping `pub(crate)` and `super::`. The extra journal tests from `1ea072fb` are on M and on P (`crates/core/jackin-config/src/persist/tests/case_02.rs` and `case_03.rs`), including `abort_journal_installation_failure_retains_forward_generation_after_normal_drop` and `staged_parent_sync_precedes_both_forward_and_restore_journals`. B002 does not have those later journal tests. B079 does, and M and P already have them.

`WorkspaceRoleOverride::is_default` (`self == &Self::default()`, so an explicit empty launch list is not default) is the same function at B079 and M `crates/jackin-config/src/schema.rs:134` and P `crates/core/jackin-config/src/schema.rs:134`. The editor tests `editor_delete_env_var_preserves_explicit_empty_github_override` and `editor_delete_env_var_preserves_explicit_empty_default_launch` are on B079 and M `crates/jackin-console/src/tui/screens/editor/model/tests.rs` and on P `crates/ui/jackin-console/src/tui/screens/editor/model/tests/case_03.rs:314` and `:364`.

### mise musl rows and zig minisign

B078 changes only `mise.lock` versus parent `0aa821a0`. The added rows are:

- `[tools.hyperfine."platforms.linux-arm64-musl"]` points at `hyperfine-v1.20.0-aarch64-unknown-linux-gnu.tar.gz` (same checksum as the GNU arm64 row).
- `[tools.xcbeautify."platforms.linux-x64-musl"]` and `[tools.xcbeautify."platforms.linux-x64-musl-baseline"]` point at `xcbeautify-3.2.1-x86_64-unknown-linux-gnu.tar.xz`.
- `[tools.zig."platforms.linux-x64"]` gains `provenance = "minisign"`.

Those rows are on M `mise.lock`. They are not absent from both trees.

P `mise.lock` compared with B078:

- xcbeautify `linux-x64-musl` and `linux-x64-musl-baseline` still point at the same x86_64 GNU archive. Present on P.
- hyperfine is absent from P, so the arm64-musl alias is absent from P and present on M.
- zig on P is `0.17.0` (`zig-x86_64-linux-0.17.0.tar.xz`) with no `provenance` line. `minisign` occurs zero times in P `mise.lock`. M and B078 still have `provenance = "minisign"` on zig `0.16.0` `linux-x64`. That field is on M, so it is not absent from both.

## Not claimed

No tests were executed. Fixture bytes were not replayed. The three absent OMP outcomes are taken from the source assertions versus the M and P assertions and the `validate_wal` salt and length checks. Docs-only commits in the `P..TIP` ranges, including B001 `3cb98f2a`, are not product behavior.
