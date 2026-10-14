# Integration constraints recorded during analysis

These bind later implementation. They are not approval to start it.

## Lockfile rows on main that the policy lock drops

`mise.lock` on main contains `minisign` (2 hits) and `hyperfine` (29 hits). Policy `fba69d60` has neither. Zig minisign provenance was a keep candidate. Adopting the policy lock must account for those rows instead of treating their absence as accidental.

## Archive WAL recovery is not current main behavior

B001, B002, and B079 skip a stale WAL salt, ignore a partial frame, and retry a checkpoint between the database read and the WAL read. Main and the policy tip both return `WalError::InvalidSalt` (`wal.rs` around line 229 on each tree). That fail-closed behavior is the current target. Do not restore the permissive archive path unless a review shows the rejection itself drops committed state.

`load --model` and `--effort` trim and reject on main and on the policy CLI. B002 returns the untrimmed model string. Keep the trim.

## Tests absent from the policy tree

A source-name comparison found 34 in-scope tests on main or the usage line whose names and nearby assertions are absent from the policy tip. Broker idle, model/effort, diagnostic redaction, and `spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch` are present on all three. A direct grep confirmed these are on main and the usage line and absent from policy `fba69d60`:

- `live_lease_owner_blocks_expired_takeover_and_can_renew_after_waking`
- `expired_lease_is_reclaimed_after_dead_owner_releases_lifetime_lock`
- `revoked_consent_after_401_skips_keychain_reread_and_preserves_401`
- `future_store_schema_is_rejected`

The full dropped set is in `task-002-test-identities.md`. `task-002-dropped-test-successors.md` searched every name and one assertion needle on the policy tip: 0 exact, 0 renamed, 34 absent. Three needles appear in other tests and still do not cover the dropped assertion: `provider rate limited` is a shared message, `provider_http_status: None` in `consent_revoked_after_success_discards_the_provider_view` is one successful request, and `provider_http_status: Some(401)` in `consent_revoked_after_missing_keychain_reread_discards_first_401_view` rereads a missing Keychain entry. Port all 34. Do not call any of them superseded by a policy rename.

## Must restore from main and the usage line

Policy `crates/services/jackin-usage-broker/src/leader.rs` has `claim_existing_lease` and `claim_leader`, and the policy tree has no `lease_path_matches_open_file` symbol. Main (`crates/jackin-usage/src/host/broker.rs`) and the usage line both define that function and call it before trusting an open lease file. A coordinator grep on `fba69d60c2373156dc0dcecee8e085c83776966f` found zero matches under `jackin-usage-broker`.

When the policy broker is selected, port the main/usage rule into `claim_existing_lease` before the open lease file is trusted. `lease_path_matches_open_file` returns true only when the open descriptor has a link count other than zero, both the descriptor and `symlink_metadata` of the path are regular files, the path is not a symlink, and device and inode match. Cleanup also refuses when the current lease instance, protocol, or build id differs from the owner. Do not replace an unknown lease format just because the file text parses as a dead process id. Coordination, repo-lock, and purge guards were reported as still matching main; this lease-claim path is the exception.

## Qualification gate

`task-002-qualification-decision.md` decides **PORT**. Checked again: `require_qualified` is at `c7b8a958` `crates/jackin-xtask/src/ci_evidence/rollup.rs:151` and is absent on main and on policy. Policy `rollup.rs:128` still sets `status: "advisory"` and returns success.

Port `green_claim_qualified` and its fail-closed exit `require_qualified` onto the policy push-head ledger. A missing or cancelled first attempt, an empty expected set, a non-ledger denominator, a failed fetch, unclassified runs, or a failed binding fails the rollup command. The binding to port is the stash function at `c7b8a958` `crates/jackin-xtask/src/ci_evidence/validation.rs:1041`, which calls `validate_live_api_binding`. `14bd1471616056e07c81c59d59eafc0963b16865` has the same `rollup.rs` and the same four tests, but its `validate_qualification_binding` at `validation.rs:605` does not call `validate_live_api_binding`. Do not treat that weaker copy as the port. Do not copy the stash tree. Do not restore `RUSTC_WRAPPER` or `SCCACHE_GHA_ENABLED`. Move these tests with the predicate: `required_skipped_cohort_is_non_green`, `fixture_denominator_cannot_qualify_green`, `local_collection_cannot_qualify_green`, and `scheduled_provenance_without_current_workflow_contract_cannot_qualify_green`.

`fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` is not a stash parent. `read_limited_output` and `run_limited_output` are not ported: policy already fails a capture that would pass its byte cap, and the stored snapshot contains a conflict marker.

## CI archive behavior not selected yet

From the CI archive pass, still absent from the policy tip:

- Fail-closed `require_qualified`. Port this predicate onto the policy ledger. Policy rollup status stays advisory until that port. Do not restore global sccache.
- An always-required push-head predecessor. Policy bootstraps when none exists.
- Exact semantic cache-input keys. Also absent from main.
- Task-scoped `run_with_sccache`. Policy instead forbids installer-wide `RUSTC_WRAPPER` and `SCCACHE_GHA_ENABLED=true`. Do not restore global sccache.

Push-head, producer, and artifact binding, including visibility of missing, cancelled, in-progress, conflicting, and failed first attempts, are already on the policy tip. Empty stash commits `9dcdd232` and `0624d328` are not behavior.

## Native FFI output path must be rewritten for the deeper crate

Policy `crates/adapters/jackin-usage-ffi/boltffi.toml` keeps `output = "../../target/xcframework"` and `output = "../../native/Sources/JackinUsageBindings"`. Those relatives were correct for main's `crates/jackin-usage-ffi`. Under `crates/adapters/` they resolve to `crates/target` and `crates/native`, while `Package.swift`, `project.yml`, and the xtask check use repository-root `target/xcframework`. Integration must point boltffi at `../../../target/xcframework` and `../../../native/Sources/JackinUsageBindings`, and the native CI task must pack the XCFramework before `xcodegen`. Do not copy the archive's unregenerated pre-M1 workflow output. The in-process sidecar fallback stays deferred.

## Selected schema and wire

Checked against the trees after `task-002-schema-decision.md`. Integration keeps the policy monitor schema **4** and broker wire **v8**. Do not stamp the usage tip's schema 6 or wire v9 onto that tree.

Main writes monitor schema 4. Policy already decodes schema 4 as current and does not rewrite it. The usage tip's `migrate_v4` calls `clear_unscoped_provider_projection` (`crates/jackin-usage/src/host/broker/monitor/storage.rs`). That function is absent on main and on the policy tip. Applying it would drop quota values that main's schema-4 files still hold.

- A schema-4 store loads with no rewrite. A schema-6 store is refused and left on disk.
- A v8 socket is the current wire. A v9 socket is a protocol mismatch and is not migrated. Do not add `PREVIOUS_USAGE_BROKER_PROTOCOL_VERSION`.
- Account state stays at the policy version **3**. That version migrates the version-2 files main and the usage tip treat as current. Keep `account_state_v2_migration_persists_unresolved_attempt_provenance_atomically`. Do not add an account schema 4.
- Usage live acceptance stays unproved. This selection does not authorize a provider call.

Rollback is the policy refusal arm: monitor schemas newer than 4, and any broker wire other than v8, stay refused.

## B044 names removed after the shared ancestor

`task-002-b044-name-disposition.md` classified all 206 function names that are absent from main, the policy tip, and the usage tip. Every one is defined on ancestor `d32e85563648931f80ffab141bb2650f478c2a60` and on B044. B044's 10 commits that the policy tip lacks are file splits: 204 same-named bodies match the ancestor after whitespace stripping, and `headline` and `live_probes_enabled` differ only by visibility or a `super::` path.

The deletion is in the 360 policy commits after that ancestor, and the newest pickaxe for every one of the 206 names is the same commit: `3f3ddd7c48284b648b083305f969ee3e709b74b7` (`feat(usage): add explicit Claude usage monitoring (#1122)`). That commit is an ancestor of main. Restoring the 206 names from B044 would undo main. The two partial successors on the policy tip still differ in assertions, and those assertion differences were introduced by that same main commit. Do not cherry-pick the 10 split commits.

`task-002-b044-removal-review.md` agrees for a 15-name sample, including the first name, the last name, `catalog_cas_rejects_a_stale_rotation_after_a_newer_winner`, and `canonical_projection_alias_transition_is_atomic_idempotent_and_fail_closed`. The other 191 names were not re-checked by that review. The full 206-name pickaxe remains the earlier script.

## Accepted decisions

`task-002-decision-review.md` accepts both of these. That acceptance does not approve a branch or a source edit.

Limited output is **REJECT**. `read_limited_output` and `run_limited_output` on `fea3ade` are absent from main and from the policy tip. Policy `read_captured` already fails when the next read would pass the byte cap. The stored copies sit on an unresolved incoming side, and the incoming object `2990df17` is not in this clone. Do not port them.

The lockfile decision stands: restore minisign provenance and a hyperfine pin, and do not copy policy `mise.lock` `90d4e8356371a6eb2cc41d6479bed45369fac413`. Main records `provenance = "minisign"` twice, both on Zig 0.16.0 (`linux-x64` and `macos-arm64`). Policy Zig 0.17.0 has checksum and URL only, and the string `minisign` is absent. Policy has no hyperfine row. Restoring main's hyperfine rows as written would also copy two wrong archives: `linux-arm64-musl` points at `aarch64-unknown-linux-gnu`, and `macos-arm64` points at `x86_64-apple-darwin`. The pin must name the archive that matches the platform key. An install-time minisign check is not a lock key.

## Baseline classification

`task-003-baseline-review.md` is **PARTIAL**. Every measured `xtask ci` log ends `xtask_ci_exit:1`. A wrapper exit of 0 is not a pass. The unlocked main all-features run does not clear the locked failure. Native mise CI was not run. The usage end-to-end retry never started tests: the Linux capsule build failed in `jackin-usage-provider-claude` (`P-docker-e2e-2.log`). Shear warning names and audit advisory ids were not captured.

An integration head is not green while a candidate-specific failure remains:

- Policy and toolchain Clippy: four `clippy::assert_is_empty` sites in `attach.rs` under Rust 1.99.
- Policy nextest: `config_reference_matches_public_schema_fields`.
- Policy docs repo-links: the 12 stale `crates/services/...` paths.
- Usage nextest: `cli_provider_auth_outcomes_keep_the_selected_source_and_typed_terminal`.
- Toolchain nextest: `auto_prep_construct_triggers`.
- Policy and toolchain deny: `Unicode-3.0` on ICU 2.3.1 while exceptions still name 2.2.x, plus the stale `RUSTSEC-2026-0173` ignore.
- Toolchain deny: `CDLA-Permissive-2.0` on `webpki-root-certs` and the `rand` duplicate.

Inherited main failures stay red and are not a waiver: locked `missing_or_stale_exact_selectors_never_fall_back_to_a_sibling`, default-nextest `omp_sync_materializes_schema_and_credentials_from_wal_only_pages`, `Zlib` on zlib-rs 0.6.8, `RUSTSEC-2024-0449` `unsafe-libyaml`, and yanked `dockerfile-parser-rs`.
