# TASK-005 apply list

Future integration only. The branch, when it is created, descends from main `1c3dd1be4a3b1ed518133ec3899c084c792370ff`. It does not descend from policy `fba69d60c2373156dc0dcecee8e085c83776966f`. Do not merge the 366 policy commits wholesale. B036 and B077 stay the layout source (`task-002-remainder-dispositions.md` REM-036 and REM-077, both OPEN). Whole cherry-picks are allowed only for a commit whose hunks are all approved. None of the 366 is that commit.

TASK-004 (`task-004-architecture-review.md`) accepted the seven policy roots and the symbol destinations checked there. That acceptance is the carry set below. It did not approve a branch, a source edit, or a freeze of the schema numbers.

`BLOCKED: yes` means the row is not an approved edit. It stays blocked until a later decision. `BLOCKED: no` means the row is a foundation extract already accepted as a destination. It still does not authorize this file's reader to create the branch or edit a tree.

## 1. Workspace membership and ownership graph

| Field | Value |
| --- | --- |
| Source SHA or path | `fba69d60c2373156dc0dcecee8e085c83776966f` `Cargo.toml` `[workspace].members` (150 paths), `policy/owned-packages.json`, `policy/package-groups.json`, `ratchet.toml`. Exclude `crates/tools/jackin-lints`. `[patch.crates-io] arrayref` stays `vendor/arrayref`. |
| Destination path | Same paths on the policy layout: `Cargo.toml`, `Cargo.lock` only as far as the layout resolves, `policy/owned-packages.json`, `policy/package-groups.json`, `ratchet.toml`, and the seven roots `crates/adapters`, `crates/apps`, `crates/core`, `crates/services`, `crates/testing`, `crates/tools`, `crates/ui`. |
| Behavior that must survive | One coherent graph. Counts: adapters 5, apps 2, core 9, services 126, testing 2, tools 3, ui 3. No package outside those roots. No parallel flat `crates/<package>` copy of a moved member. `jackin-diagnostics`, `jackin-telemetry`, and `jackin-usage` stay services. `jackin-omp-store` and `jackin-usage-ffi` stay adapters. |
| BLOCKED | no |

Do not copy policy `mise.lock` blob `90d4e8356371a6eb2cc41d6479bed45369fac413` as the lock. `task-002-decision-review.md` accepts restoring minisign provenance and a hyperfine pin. Main's `linux-arm64-musl` row points at the GNU archive and its `macos-arm64` row points at the x86_64 Darwin archive; those two URLs are not restored as written.

## 2. Core types, protocol, and process

| Field | Value |
| --- | --- |
| Source SHA or path | `fba69d60c2373156dc0dcecee8e085c83776966f` under `crates/core/`. Checked symbols: `UsageSnapshotStatus` at `crates/core/jackin-protocol/src/control.rs`; `MonitorFieldEvidence` and `MonitorQuotaWindowStatus` at `crates/core/jackin-protocol/src/usage_monitor.rs`; `redact` is a service (unit 6). Process byte-cap lives at `crates/core/jackin-process/src/lib.rs`. |
| Destination path | `crates/core/jackin-brand`, `jackin-build-meta`, `jackin-config`, `jackin-core`, `jackin-env`, `jackin-manifest`, `jackin-process`, `jackin-process-directory`, `jackin-protocol`. |
| Behavior that must survive | One owner for protocol and config. Typed status is `UsageSnapshotStatus`, `provider_outcome`, `ProviderErrorKind`, and `ProviderError::status`. Quota window used-percent and reset evidence stay independent (`quota_window_status_keeps_used_and_reset_evidence_independent`). Process capture keeps the existing byte cap and failure on overflow. |
| BLOCKED | no |

The monitor, wire, and account numbers in these files are unit 12. Carrying the crate paths does not approve those numbers.

## 3. Runtime coordination, repo-cache, and purge

| Field | Value |
| --- | --- |
| Source SHA or path | `fba69d60c2373156dc0dcecee8e085c83776966f`. `universe_dir`, `open_lock`, `open_in_namespace` in `crates/services/jackin-runtime-coordination/src/coordination.rs`. Purge member `crates/services/jackin-runtime-cleanup-absent-guards` (`ensure_role_resources_absent_for_purge`). Backend `ensure_absent_for_purge` stays in `crates/services/jackin-runtime`. Stash `80ab98038e945ec48c82dad241ebf33aa0efa82d` matches the policy coordination tree. Stash `b1b90781739333ba7c8829c092a845f6ea82b11c` matches the policy repo-cache tree. |
| Destination path | `crates/services/jackin-runtime-coordination/`, `crates/services/jackin-runtime-repo-cache/`, `crates/services/jackin-runtime-cleanup-absent-guards/`, `crates/services/jackin-runtime/src/runtime/apple_container.rs`. |
| Behavior that must survive | Pathname hash for the universe directory, flock as the only lock ownership, lock inodes created once and not truncated or removed. Tests `concurrent_first_lock_opens_all_share_one_inode` and `three_process_contenders_preserve_lock_inode_until_and_after_owner_exit`. Coordination, repo-lock, and purge guards were reported as still matching main. |
| BLOCKED | no |

Do not revive `jackin-runtime-apple-container-ensure-absent` from `26cf7e939218e9f16121d3a2fa287cdb15b27832` (B042 `bff77099f8273a4d480a0e430cba66afc4197543`, B045 `45e834683c8d5ca0fea1c7d3676cb136efa4acb1`). That intermediate crate is not a policy member.

## 4. Usage-broker idle exit

| Field | Value |
| --- | --- |
| Source SHA or path | `fba69d60c2373156dc0dcecee8e085c83776966f` `crates/services/jackin-usage-broker/src/serve.rs` `should_exit_idle`. Main counterpart `1c3dd1be4a3b1ed518133ec3899c084c792370ff` `crates/jackin-usage/src/host/broker/serve_loop.rs` uses the sixth name `has_foreground_liveness`. |
| Destination path | `crates/services/jackin-usage-broker/src/serve.rs`. |
| Behavior that must survive | Exit only when the idle window elapsed, the coordinator is idle, no monitor is active, and the sixth flag is false. Active monitors keep the service alive. Passive idle expires. Foreground liveness blocks idle exit through `has_authenticated_foreground_guard`. Regression `spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch`. |
| BLOCKED | no |

Main's `ClaudeCollectorLiveness` object is not this unit. Shutdown wiring outside `should_exit_idle` stays for TASK-007.

## 5. Fail-closed broker authorization already on the policy broker

| Field | Value |
| --- | --- |
| Source SHA or path | Policy broker extracted from the same split as `3ddbc0563ed95225b0004c393b3928fb599ee066`, landed under `fba69d60c2373156dc0dcecee8e085c83776966f` `crates/services/jackin-usage-broker`. |
| Destination path | `crates/services/jackin-usage-broker/` authorize path, plus `usage_broker_capabilities` and `usage_catalog_entries` in `crates/services/jackin-usage-discovery/`. |
| Behavior that must survive | Exact account, surface, source, and material fingerprint. Closed provider-key aliases. Mixed profile and env returns no authorization. Duplicated launch-key bindings with a different source or material return no authorization. A proof that matches other than one binding returns no authorization. Tests: `mixed_profile_and_env_group_fails_closed_without_changing_pure_profile`, `grouped_broker_authorization_accepts_sibling_proofs_and_rejects_conflicts`, `selected_routes_require_exact_source_proofs_and_same_identity`, `launch_scope_fails_closed_on_rotation_repoint_and_mixed_agent_source`, `launch_scope_accepts_provider_native_zhipu_alias_for_canonical_zai_binding`, `forwarded_scope_selects_only_accounts_backed_by_forwarded_sources`. |
| BLOCKED | no |

Restoring `ScopedCapability` is unit 14, not part of this extract.

## 6. Diagnostic redaction

| Field | Value |
| --- | --- |
| Source SHA or path | `fba69d60c2373156dc0dcecee8e085c83776966f` `crates/services/jackin-diagnostics/src/redact.rs` `redact_text` and `redact_and_cap`. |
| Destination path | `crates/services/jackin-diagnostics/src/redact.rs`. |
| Behavior that must survive | Named secret values are redacted (`redacts_named_secret_values`). OTLP private-export negatives stay discoverable after the B046 case split (unit 9). |
| BLOCKED | no |

## 7. Trimmed `load --model` and `--effort`

| Field | Value |
| --- | --- |
| Source SHA or path | `fba69d60c2373156dc0dcecee8e085c83776966f` `crates/apps/jackin/src/cli/role.rs` `LoadArgs.model`, `LoadArgs.effort`, `parse_model`, `parse_reasoning_effort`. Same parsers on main `1c3dd1be4a3b1ed518133ec3899c084c792370ff` `crates/jackin/src/cli/role.rs`. |
| Destination path | `crates/apps/jackin/src/cli/role.rs`. |
| Behavior that must survive | Model and effort are trimmed and rejected when empty or unknown. Tests `load_args_parses_task_scoped_model_and_effort`, `load_args_parses_model_and_effort_overrides`, `load_args_rejects_empty_model_and_unknown_effort`, `load_help_documents_model_and_effort_overrides`. |
| BLOCKED | no |

Restoring an untrimmed model string is unit 16.

## 8. Adapter, app, testing, tools, and UI membership

| Field | Value |
| --- | --- |
| Source SHA or path | `fba69d60c2373156dc0dcecee8e085c83776966f` member list in `task-004-source-unit-map.md`. |
| Destination path | `crates/adapters/jackin-docker`, `jackin-image`, `jackin-omp-store`, `jackin-oppicker`, `jackin-usage-ffi`; `crates/apps/jackin`, `jackin-capsule`; `crates/testing/jackin-otlp-testbed`, `jackin-test-support`; `crates/tools/jackin-dev`, `jackin-pr-trailers`, `jackin-xtask`; `crates/ui/jackin-console`, `jackin-launch`, `jackin-tui`. |
| Behavior that must survive | Thin app entrypoints, packaged sidecar binary `jackin-usage-broker` beside the app, OMP store at the adapter root, FFI crate at the adapter root. Missing sidecar stays fail-closed `Unavailable` in `crates/services/jackin-usage-broker/src/ensure.rs`. Typed provider outcome stays at `crates/services/jackin-usage-provider-core/src/outcome.rs` and `ProviderErrorKind` at `crates/services/jackin-usage-provider-core/src/refresh.rs`. |
| BLOCKED | no |

The boltffi relative outputs in `crates/adapters/jackin-usage-ffi/boltffi.toml` are unit 13. They are not approved as written (`../../target`, `../../native`).

## 9. Test-module splits already present on the policy tree

| Field | Value |
| --- | --- |
| Source SHA or path | `ef238a7e4720fe1be8e8e295e1c502f2ba529e0b` (eight state-io tests; remaining hunks are moves and blank lines). `b72046bbc9314e93cddf4d8252570dc375935f56` (5193 names moved, zero names added or lost). Both behaviors are already on `fba69d60c2373156dc0dcecee8e085c83776966f`. |
| Destination path | State-io: `crates/services/jackin-isolation/src/state/state_io/tests/case_01.rs`. Docker TLS and connection negatives: `jackin-docker` `tests/case_01.rs` on the policy tree. OTLP private-export tests stay on the policy telemetry test paths. Do not copy the tip's shortened fixture include or `crate::test_support` edits from `ef238a7e`. |
| Behavior that must survive | The eight state-io names (`ancestor_swap_refuses_commit_and_preserves_both_namespaces` through `staged_candidate_alias_refuses_install_and_preserves_source_and_canary`). TLS feature guards and connection rejection (`choose_connection_rejects_ssh_context`, `choose_connection_rejects_context_with_tls_material`, and the sibling connection negatives named in `task-002-policy-rescues.md`). Telemetry redaction negatives (`facade_redacts_then_utf8_truncates_body_and_exception_fields` and the other private-export names in that report). |
| BLOCKED | no |

## 10. Minimum Rust floor required to build the layout

| Field | Value |
| --- | --- |
| Source SHA or path | `fba69d60c2373156dc0dcecee8e085c83776966f` `rust-toolchain.toml` channel `1.99.0`, `Cargo.toml` `rust-version = "1.99"`, `mise.toml` `rust = { version = "1.99.0", mr_boxington = true }`. |
| Destination path | `rust-toolchain.toml`, `Cargo.toml` `[workspace.package].rust-version`, `mise.toml` rust pin. |
| Behavior that must survive | The selected layout is built with Rust 1.99. TASK-006 owns the rest of toolchain, cache, and lock reconciliation. |
| BLOCKED | no |

## 11. In-scope tests absent from the policy tree

| Field | Value |
| --- | --- |
| Source SHA or path | Main `1c3dd1be4a3b1ed518133ec3899c084c792370ff` and usage `2e4a62da4bc4406764162a267163389a5fb513a7`. Full dropped set: `task-002-test-identities.md` (34 names). Named in `integration-constraints.md`: `live_lease_owner_blocks_expired_takeover_and_can_renew_after_waking`, `expired_lease_is_reclaimed_after_dead_owner_releases_lifetime_lock`, `revoked_consent_after_401_skips_keychain_reread_and_preserves_401`, `future_store_schema_is_rejected`. |
| Destination path | Policy equivalents under `crates/services/jackin-usage-broker/`, `crates/services/jackin-usage-provider-claude/`, `crates/services/jackin-usage-coordinator/`, and `crates/core/jackin-protocol/`. Exact child files are not approved. |
| Behavior that must survive | All 34 names are absent on the policy tip. `task-002-dropped-test-successors.md` found no exact name and no renamed test with the same assertion. Port each one. The three nearby hits (`provider rate limited`, `provider_http_status: None`, `provider_http_status: Some(401)`) assert different outcomes and are not successors. |
| BLOCKED | yes |

## 12. Monitor schema 4, broker wire v8, account state 3

| Field | Value |
| --- | --- |
| Source SHA or path | Policy constants on `fba69d60c2373156dc0dcecee8e085c83776966f`: `USAGE_MONITOR_SCHEMA_VERSION` `4` at `crates/core/jackin-protocol/src/usage_monitor.rs`, `USAGE_BROKER_PROTOCOL_VERSION` `"v8"` at `crates/core/jackin-protocol/src/usage_broker.rs`, `ACCOUNT_STATE_SCHEMA_VERSION` `3` at `crates/services/jackin-usage-coordinator/src/state/envelope.rs`. `task-002-schema-decision.md` selects those three numbers. TASK-004 had left the migration open; that selection closes the number choice and does not approve an edit. |
| Destination path | Those three policy paths. Load arms: `crates/services/jackin-usage-broker/src/monitor/storage.rs` and `crates/services/jackin-usage-coordinator/src/state/stores.rs`. |
| Behavior that must survive | A schema-4 monitor store loads with no rewrite. A schema newer than 4, including 6, is refused and left on disk. A v8 socket is the current wire. Any other broker wire, including v9, is a protocol mismatch and is not migrated. Account state 3 migrates version-2 files. Keep `account_state_v2_migration_persists_unresolved_attempt_provenance_atomically`. Do not add an account schema 4. Usage live acceptance stays unproved. |
| BLOCKED | yes |

## 13. boltffi outputs for the deeper crate

| Field | Value |
| --- | --- |
| Source SHA or path | `fba69d60c2373156dc0dcecee8e085c83776966f` `crates/adapters/jackin-usage-ffi/boltffi.toml` still has `output = "../../target/xcframework"` and `output = "../../native/Sources/JackinUsageBindings"`. Those relatives were correct for main `crates/jackin-usage-ffi`. Preservation lead: B015 `7646c61bd4261a5908693bdaf0a6ecd9e9643448` (`task-002-native.md`). |
| Destination path | `crates/adapters/jackin-usage-ffi/boltffi.toml`, rewritten to `../../../target/xcframework` and `../../../native/Sources/JackinUsageBindings`. Native CI packs the XCFramework before `xcodegen`. Consumers stay `native/Package.swift` and `native/project.yml` at repository-root `target/xcframework`. |
| Behavior that must survive | FFI output lands on the repository-root XCFramework that SwiftPM, XcodeGen, and the xtask check already use. Do not copy the archive's unregenerated pre-M1 workflow output (B029 `8fb49688a6408627e22118f92d24f47785564979`). |
| BLOCKED | yes |

## 14. Do not restore `ScopedCapability`

| Field | Value |
| --- | --- |
| Source SHA or path | Present on `3ddbc0563ed95225b0004c393b3928fb599ee066` `broker/authorize.rs` and on ancestor `d32e85563648931f80ffab141bb2650f478c2a60`. Removed by `3f3ddd7c48284b648b083305f969ee3e709b74b7`, which is on main `1c3dd1be4a3b1ed518133ec3899c084c792370ff` and on policy `fba69d60c2373156dc0dcecee8e085c83776966f`. |
| Destination path | Do not add the struct under `crates/services/jackin-usage-broker/`. |
| Behavior that must survive | Fail-closed authorization in unit 5. Restoring `ScopedCapability` would undo main. |
| BLOCKED | yes |

## 15. Do not stamp schema 6 or wire v9

| Field | Value |
| --- | --- |
| Source SHA or path | Usage tip `2e4a62da4bc4406764162a267163389a5fb513a7`: `USAGE_MONITOR_SCHEMA_VERSION` `6`, `USAGE_BROKER_PROTOCOL_VERSION` `"v9"`, `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION` `"v8"` at `crates/jackin-usage/src/host/broker.rs`, `migrate_v4` / `clear_unscoped_provider_projection` in `crates/jackin-usage/src/host/broker/monitor/storage.rs`. |
| Destination path | Do not write those values onto `crates/core/jackin-protocol/src/usage_monitor.rs`, `crates/core/jackin-protocol/src/usage_broker.rs`, or `crates/services/jackin-usage-broker/src/monitor/storage.rs`. |
| Behavior that must survive | Policy refusal of monitor schemas newer than 4 and of every broker wire other than v8. `clear_unscoped_provider_projection` is absent on main and on policy. Applying it would drop quota values that main's schema-4 files still hold. Do not add `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION`. |
| BLOCKED | yes |

## 16. Do not restore permissive WAL or untrimmed `load --model`

| Field | Value |
| --- | --- |
| Source SHA or path | B001 `3cb98f2a634c1e660e501489d7383329569db7a3`, B002 `b1e08b880223594cceccf3ea44322d44e3cdf984`, B079 `d04467f05f1183008322047b1828c201e551e947`. B002 returns the untrimmed model string. Those tips skip a stale WAL salt. |
| Destination path | Do not change policy or main `wal.rs` (fail-closed `WalError::InvalidSalt` around line 229 on each tree) or `crates/apps/jackin/src/cli/role.rs` `parse_model`. |
| Behavior that must survive | Stale WAL salt is `WalError::InvalidSalt`. `load --model` and `--effort` stay trimmed and rejected, as on main and on the policy CLI. Do not restore the permissive archive path unless a review shows the rejection itself drops committed state. |
| BLOCKED | yes |

## 17. Do not restore global sccache

| Field | Value |
| --- | --- |
| Source SHA or path | B032 `b1dc105acffb68ba71fff86f9c18dc5ff77de924` sets installer-wide `RUSTC_WRAPPER=sccache` and `SCCACHE_GHA_ENABLED=true`. |
| Destination path | Do not write that environment into `.github/workflows/`, generated workflow fixtures, or the installer prefix. Policy `fba69d60c2373156dc0dcecee8e085c83776966f` forbids both names at that scope. |
| Behavior that must survive | Installer-wide wrapper and GHA sccache stay off. Task-scoped `run_with_sccache` from `41dd148e514d1bf3f84401f0ecbf8e87b58bb261` is not selected here. It remains a TASK-006 candidate, not an approved edit. |
| BLOCKED | yes |

## 18. In-process sidecar fallback stays deferred

| Field | Value |
| --- | --- |
| Source SHA or path | B034 `33d88b1cf764f614d3935c11c48decd53386083e` `crates/jackin-usage/src/host/broker.rs` `ensure_usage_broker` else-branch and `ensure_usage_broker_falls_back_to_in_process_when_sidecar_missing`. Same patch-id as `0268e45490b0d3dcb5519e6218b2bd81dcb5107a`. |
| Destination path | Do not add that branch to `crates/services/jackin-usage-broker/src/ensure.rs`. |
| Behavior that must survive | A missing or unusable `jackin-usage-broker` executable returns `Unavailable`. Lease, CAS, and capability safety for an in-process fallback are not proved. Availability of a handle is not that proof. |
| BLOCKED | yes |

## 19. Qualification gate

| Field | Value |
| --- | --- |
| Source SHA or path | Stash tip `c7b8a95856cce6f5e8442f0e6fafcf025a089132` `require_qualified` at `crates/jackin-xtask/src/ci_evidence/rollup.rs:151`. Related schema-5 tip `14bd1471616056e07c81c59d59eafc0963b16865`. `task-002-qualification-decision.md` decides PORT. |
| Destination path | `crates/tools/jackin-xtask/src/ci_evidence/` on the policy layout. Policy `rollup.rs:128` still sets status `advisory` and returns success. The four symbols are absent from main `1c3dd1be4a3b1ed518133ec3899c084c792370ff` and from policy `fba69d60c2373156dc0dcecee8e085c83776966f`. |
| Behavior that must survive | Port `green_claim_qualified` and the fail-closed exit `require_qualified`. The binding to port is stash `c7b8a958` `validation.rs:1041`, which calls `validate_live_api_binding`. `14bd147` has the same `rollup.rs` and the same four tests, but its `validate_qualification_binding` does not call the live binding. Do not port that weaker copy. Move `required_skipped_cohort_is_non_green`, `fixture_denominator_cannot_qualify_green`, `local_collection_cannot_qualify_green`, and `scheduled_provenance_without_current_workflow_contract_cannot_qualify_green`. Do not copy the stash tree. Do not restore `RUSTC_WRAPPER` or `SCCACHE_GHA_ENABLED`. Push-head, producer, and artifact binding already on the policy tip stay. |
| BLOCKED | yes |

## 20. `lease_path_matches_open_file` port

| Field | Value |
| --- | --- |
| Source SHA or path | Main `1c3dd1be4a3b1ed518133ec3899c084c792370ff` and the usage line, both `crates/jackin-usage/src/host/broker.rs`. Policy `fba69d60c2373156dc0dcecee8e085c83776966f` has `claim_existing_lease` and `claim_leader` and has no `lease_path_matches_open_file` symbol under `jackin-usage-broker`. |
| Destination path | `crates/services/jackin-usage-broker/src/leader.rs`, inside `claim_existing_lease`, before the open lease file is trusted. |
| Behavior that must survive | The function returns true only when the open descriptor has a link count other than zero, both the descriptor and `symlink_metadata` of the path are regular files, the path is not a symlink, and device and inode match. Cleanup also refuses when the current lease instance, protocol, or build id differs from the owner. Do not replace an unknown lease format just because the file text parses as a dead process id. |
| BLOCKED | yes |

## 21. Native CI

| Field | Value |
| --- | --- |
| Source SHA or path | `fba69d60c2373156dc0dcecee8e085c83776966f` `native/mise.toml`. `tasks.format-check` sets `DEVELOPER_DIR=/Applications/Xcode_26.6.app/Contents/Developer`. Host check: `baselines/native-host.md`. Installed Xcode is 27.0. Xcode 26.6 is absent. |
| Destination path | `native/mise.toml` tasks `ci`, `merge`, and `scheduled`. |
| Behavior that must survive | The pinned Xcode 26.6 gate. A run on Xcode 27.0 is not that gate. Native `ci`, `merge`, and `scheduled` were not started. |
| BLOCKED | yes |

## 22. docker-e2e

| Field | Value |
| --- | --- |
| Source SHA or path | Policy `fba69d60c2373156dc0dcecee8e085c83776966f`. First attempt: `baselines/P-docker-e2e-result.md` (`e2e_exit:100`, log `P-docker-e2e.log`). Retry: `P-docker-e2e-2.log`, `capsule_build_exit:1`. |
| Destination path | `mise exec -- mbx +1.99.0 nextest run -p jackin --features e2e --profile docker-e2e` against `crates/apps/jackin`. JUnit would land under `target/nextest/docker-e2e/`. |
| Behavior that must survive | `usage_broker_e2e` must actually run. The first attempt failed in `crates/apps/jackin/tests/dind_e2e/common.rs` because `JACKIN_CAPSULE_BIN` was unset. 24 tests selected, 1 ran, 0 passed. The retry compiled the Linux capsule into a disposable home and failed in `jackin-usage-provider-claude` (nine unused-code and `let_underscore_drop` errors). No capsule binary was produced and no usage scenario ran. That is not a pass. |
| BLOCKED | yes |

## Not carried

These rows are decided refusals or out of this wave. They are not approved edits and they are not open ports.

- `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` `read_limited_output` and `run_limited_output`: REJECT, accepted by `task-002-decision-review.md`. Policy `read_captured` already enforces the byte cap. Do not port the conflicted copies.
- B044 `40823a76a3e20b901d5bc410979cee0931fd5c48`: do not cherry-pick its 10 split commits and do not restore the 206 function names. The newest pickaxe for every name is `3f3ddd7c48284b648b083305f969ee3e709b74b7`, which is on main. An independent sample agreed for 15 names and left the other 191 on that script (`task-002-b044-removal-review.md`). Two catalog assertion pairs differ on the policy tip and are not restored from B044.
- Probe debris, empty stash commits, and the knowingly unregenerated pre-M1 native generator output stay out.

this file authorizes no branch and no source edit.
