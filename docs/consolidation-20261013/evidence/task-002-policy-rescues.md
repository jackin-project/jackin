# Task 002 — Policy rescue and dirty tips (B041, B042, B043, B045, B046)

Read-only inspection of `/Users/donbeave/Projects/jackin-project/jackin`. No build, commit, or worktree edit.

**P:** `fba69d60c2373156dc0dcecee8e085c83776966f`

**Method.** For each tip, `git log P..TIP` is the commit list below. Parents of every unique commit except `26cf7e93` are ancestors of P. `26cf7e93`'s parent `95629842235f0d19a89b8e2d09c4e4813867fb5b` is an ancestor of P. Behavior was compared to P's tree, not by ancestry alone. Patch identity used `git patch-id --stable`. Test identity used `#[test] fn` names. Selected purge owner: `crates/services/jackin-runtime-cleanup-absent-guards`.

**Result.** No commit in these ranges has behavior that is absent from P. Do not revive `jackin-runtime-apple-container-ensure-absent`.

| Tip | `git rev-list --left-right --count P...TIP` | Commits in `P..TIP` |
| --- | --- | --- |
| B041 `ef238a7e4720fe1be8e8e295e1c502f2ba529e0b` | 362 / 1 | `ef238a7e` |
| B042 `bff77099f8273a4d480a0e430cba66afc4197543` | 169 / 2 | `bff77099`, `26cf7e93` |
| B043 `3ddbc0563ed95225b0004c393b3928fb599ee066` | 360 / 1 | `3ddbc056` |
| B045 `45e834683c8d5ca0fea1c7d3676cb136efa4acb1` | 169 / 2 | `45e83468`, `26cf7e93` |
| B046 `b72046bbc9314e93cddf4d8252570dc375935f56` | 363 / 1 | `b72046bb` |

`bff77099` and `45e83468` share tree `d1cc2456cbd53c0efb707302c1a4c8cad84760c0` and patch-id `b3877477b6982cefb93343304225fac4b1c9a48c`.

## Commits whose behavior is absent from P

None.

## B041 — `ef238a7e4720fe1be8e8e295e1c502f2ba529e0b`

**Subject:** Preserve v0.1.1 validation and test reorganization work

**Parent:** `32d78964698cdd1d97eb9fcb180e410348e46d70` (on P) — S6 test normalization: canonical external suites + case splits

**Files:** 953, all tests. 947 modified, 6 added. +3837 / −2193. Largest roots: `crates/ui/jackin-console/src` (188), `crates/services/jackin-runtime/src` (127), `crates/apps/jackin-capsule/src` (119), `crates/services/jackin-usage/src` (101). Most files are a trailing blank line.

Added files:

- `crates/services/jackin-isolation/src/state_io/tests.rs`
- `crates/services/jackin-isolation/src/state_io/tests/case_01.rs`
- `crates/services/jackin-isolation/src/state_io/tests/support.rs`
- `crates/ui/jackin-console/src/tui/view/tests/case_03.rs`
- `crates/ui/jackin-console/src/tui/view/tests/case_04.rs`
- `crates/ui/jackin-console/src/tui/view/tests/support.rs`

**Behavior on P:** present.

`#[test]` names added and not removed elsewhere are the eight state-io cases. Each function body matches P at `crates/services/jackin-isolation/src/state/state_io/tests/case_01.rs`:

- `ancestor_swap_refuses_commit_and_preserves_both_namespaces`
- `changed_manifest_after_install_rolls_back_migration`
- `competing_state_transaction_returns_actionable_busy_error`
- `dangling_state_file_alias_is_error`
- `newer_destination_after_install_is_retained_with_original_recovery`
- `old_temporary_symlink_cannot_redirect_state_write`
- `replacement_after_validation_is_atomically_restored`
- `staged_candidate_alias_refuses_install_and_preserves_source_and_canary`

`host_console_modal_states_project_visible_focus` moves from `tui/view/tests/case_02.rs` to a new `case_03.rs` (an `#[expect]` sits between `#[test]` and `fn`). P still has that test in `case_02.rs`.

Remaining hunks are test-module moves, `pub(super)` narrowing, blank lines, and import-path edits. P keeps the working paths this commit rewrites, including `crate::support::telemetry_test_guard_async` in `jackin-capsule` and the five-`../` `usage-projection-v1-current.json` include in `jackin-protocol`. The tip points those call sites at `crate::test_support` while `jackin-capsule` still declares `mod support`, and it shortens the fixture include by one `../`. Those path edits are not behavior to copy onto P.

## B042 / B045 shared commit — `26cf7e939218e9f16121d3a2fa287cdb15b27832`

**Subject:** S7 split 121: runtime-apple-container-ensure-absent extraction (M82)

**Parent:** `95629842235f0d19a89b8e2d09c4e4813867fb5b` (on P)

**Files:** 13, +141 / −18.

- `Cargo.lock`
- `Cargo.toml`
- `crates/services/jackin-runtime-apple-container-ensure-absent/Cargo.toml`
- `crates/services/jackin-runtime-apple-container-ensure-absent/README.md`
- `crates/services/jackin-runtime-apple-container-ensure-absent/src/ensure_absent.rs`
- `crates/services/jackin-runtime-apple-container-ensure-absent/src/lib.rs`
- `crates/services/jackin-runtime-apple-container-ensure-absent/tests/main.rs`
- `crates/services/jackin-runtime/Cargo.toml`
- `crates/services/jackin-runtime/src/runtime/apple_container.rs`
- `crates/tools/jackin-xtask/src/arch.rs`
- `policy/owned-packages.json`
- `policy/package-groups.json`
- `ratchet.toml`

**Behavior on P:** present. Do not revive the intermediate package.

The commit moves `ensure_absent_for_purge_with` out of `apple_container.rs` into `jackin-runtime-apple-container-ensure-absent` and re-exports it. The moved body lists containers through the injected client and bails with `cannot purge local state: apple-container `{container_name}` still exists` while that name is present. P still has that function, with that same body, in `crates/services/jackin-runtime/src/runtime/apple_container.rs`. The only signature difference is the trait path (`crate::apple_container_client::AppleContainerApi` on P, the leaf path in the extracted crate). The new crate's `tests/main.rs` is an empty layout stub.

`crates/services/jackin-runtime-cleanup-absent-guards` is on P and does not contain this function. It owns the Docker guard `ensure_role_resources_absent_for_purge` / `ensure_container_absent_for_purge` (role container and DinD sidecar, including inspect-unavailable refusal). That is a different guard, already on P. P has no `jackin-runtime-apple-container-ensure-absent` member. Wiring in this commit (workspace member, policy group, arch tier, ratchet bound) only registers the intermediate name.

## B042 — `bff77099f8273a4d480a0e430cba66afc4197543`

**Subject:** Preserve s61 rollout documentation changes

**Parent:** `26cf7e93`

**Files:** `docs/content/development/rust-policy-rollout.mdx`, +445.

**Behavior on P:** present. The added section is documentation of the move above, not a new guard.

P's blob of that file is `c6874bf41f00371e5503cdc6915b2a18bd19bbe3`, the same blob as parent `95629842`. The tip blob is `f22b8122f90ae2f4ce2aa7919ffc728b2d210718`. The added heading is `Split 121: runtime-apple-container-ensure-absent extraction (M82)`. It says the moved body is byte-identical, that semantics are unchanged, and it records pre-existing gate failures and `/tmp/jackin-s61-*` logs that are not in the repo. Those sentences are not on P. They do not add purge behavior, and they name the intermediate crate that must not be revived. Do not treat the historical failure counts as a current result.

## B043 — `3ddbc0563ed95225b0004c393b3928fb599ee066`

**Subject:** Preserve s7 usage broker extraction work

**Parent:** `d32e85563648931f80ffab141bb2650f478c2a60` (on P) — S7 split 1: jackin-instance file splits

**Files:** 22 under `crates/services/jackin-usage/src/host/broker`, +3464 / −2960. `broker.rs` becomes a module shim. New modules: `authorize.rs`, `capabilities.rs`, `client.rs`, `config.rs`, `consts.rs`, `dispatch.rs`, `ensure.rs`, `errors.rs`, `executor.rs`, `forwarding.rs`, `handle.rs`, `leader.rs`, `lease.rs`, `paths.rs`, `projection.rs`, `rediscover.rs`, `runtime.rs`, `serve.rs`, `service.rs`, `startup.rs`. `tests.rs` gains imports only (+25 / −1).

**Behavior on P:** present, in `crates/services/jackin-usage-broker`.

Normalized item compare against the parent found no new items. Body differences are `pub(crate)` visibility, `super::` to `super::super::` after the move, and name-collision artifacts in the extractor. Fail-closed authorization is the same control flow on P's `authorize.rs`: exact account, surface, source, and material fingerprint; closed provider-key aliases; mixed profile and env returns `None`; duplicated launch-key bindings with different source or material return `None`; a proof that matches other than one binding returns `None`.

These tests from the parent are still on P:

- `mixed_profile_and_env_group_fails_closed_without_changing_pure_profile`
- `grouped_broker_authorization_accepts_sibling_proofs_and_rejects_conflicts`
- `selected_routes_require_exact_source_proofs_and_same_identity`
- `launch_scope_fails_closed_on_rotation_repoint_and_mixed_agent_source`
- `launch_scope_accepts_provider_native_zhipu_alias_for_canonical_zai_binding`
- `forwarded_scope_selects_only_accounts_backed_by_forwarded_sources`

P landed the same split as `ccd6e85e1bb16edecc9c78c52358a2d209159f75` (different parent, different patch-id) and later extracted `jackin-usage-broker`. `ScopedCapability` and the `ForwardingRequirement::Env.key` field exist on the parent and on this tip; P's later history removed them (`3f3ddd7c` drops `ScopedCapability`). That is P's subsequent evolution of code already in its ancestry, not behavior this commit adds.

Coordinator check: `ScopedCapability` is present on parent `d32e8556` (`broker.rs`) and on this tip (`broker/authorize.rs`). `git log -S ScopedCapability` shows the removal in `3f3ddd7c48284b648b083305f969ee3e709b74b7` (`feat(usage): add explicit Claude usage monitoring (#1122)`). That commit is an ancestor of both main `1c3dd1be` and P `fba69d60`. Restoring the struct from B043 would undo a main change. It is not a preservation candidate from these tips. `usage_broker_capabilities` and `usage_catalog_entries` live on P in `jackin-usage-discovery`.

## B045 — `45e834683c8d5ca0fea1c7d3676cb136efa4acb1`

**Subject:** rescue: preserve local working state

**Parent:** `26cf7e93`

**Files:** same single-file patch as `bff77099`. Same tree, same patch-id.

**Behavior on P:** present. Same classification as `bff77099`. The other commit in `P..B045` is `26cf7e93`, classified above.

## B046 — `b72046bbc9314e93cddf4d8252570dc375935f56`

**Subject:** rescue: preserve local working state

**Parent:** `fb52c525d3561406f9d6d5f3a2f0b8918611d0f8` (on P) — chore(policy): re-pin rollout to rust-repository-policy v0.1.1

**Files:** 953, all tests. 225 modified, 728 added. +183259 / −179203. Same directory concentration as B041 (console, runtime, capsule, usage).

**Behavior on P:** present.

`#[test]` names: 5193 removed from old files and 5193 added in the split files. Truly new: 0. Lost: 0. This is the case-file split of suites the parent already contained. The parent is on P.

Docker connection and TLS tests that the monolithic `docker_client/tests.rs` appeared to delete are in `tests/case_01.rs` on the tip and on P, with equal bodies for `choose_connection_rejects_ssh_context` and `choose_connection_rejects_context_with_tls_material` (also equal to the parent). Names present on both trees include `choose_connection_rejects_https_context`, `choose_connection_rejects_context_with_tls_skip_verify`, `choose_connection_rejects_context_with_unknown_uri`, `choose_connection_rejects_npipe_on_unix`, `connection_choice_ssh_takes_precedence_over_tls_material`, `tls_material_present_treats_emptiness_as_absent`, and `tls_material_present_treats_populated_values_as_present`.

OTLP private-export tests are on both trees: `crash_event_exports_complete_bounded_private_shape`, `isolation_events_export_exact_private_shape`, `governed_second_line_drops_private_and_oversized_raw_records`, `facade_redacts_then_utf8_truncates_body_and_exception_fields`, `resource_matrix_has_exact_allowlist_and_ignores_secret_env_injection`, `log_second_line_rejects_type_privacy_and_size_violations`, `span_second_line_rejects_type_privacy_size_and_link_violations`. Tip bodies match the parent. Versus P, the sampled bodies differ only by `super::test_layers` versus `test_layers` after P's `use super::*` style. Assertions are the same.
