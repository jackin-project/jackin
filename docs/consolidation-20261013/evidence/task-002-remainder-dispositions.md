# TASK-002 remainder dispositions

Synthesis of REM-001 through REM-079 from `goal-references/02-change-coverage-matrix.md`. Closing authority is the listed task-002 reports and `integration-constraints.md`. The matrix was not edited.

The 2026-09-21 snapshot matches ancestor `fce94cea` and has no remainder row. `task-002-snapshot-20260921.md` records no snapshot-only behavior to preserve.

## Counts

| Disposition | Rows |
| --- | ---: |
| ALREADY_ON_P | 22 |
| ALREADY_ON_M | 3 |
| REJECT_DEBRIS | 15 |
| REJECT | 1 |
| PRESERVE_CANDIDATE | 7 |
| ALIAS | 28 |
| OPEN | 2 |
| HISTORICAL_NOT_CURRENT | 1 |
| Total | 79 |

## Rows

| ID | Branch | Tip SHA | Disposition | Evidence file | Reason |
| --- | --- | --- | --- | --- | --- |
| REM-001 | B001 | `3cb98f2a634c1e660e501489d7383329569db7a3` | ALREADY_ON_P | task-002-docs-model.md | Trimmed `load --model` / `--effort` and fail-closed `WalError::InvalidSalt` are on `fba69d60`, which integration-constraints records as the target for this archive's stale-salt skip. |
| REM-002 | B002 | `b1e08b880223594cceccf3ea44322d44e3cdf984` | ALREADY_ON_P | task-002-docs-model.md | `fba69d60` stores the trimmed model id and rejects a stale WAL salt with `InvalidSalt`, which is the recorded target over this tip's padded `parse_model` return and permissive WAL skip. |
| REM-003 | B003 | `fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` | REJECT | task-002-decision-review.md | `read_limited_output` and `run_limited_output` repeat the byte cap already enforced by policy `read_captured`. The stored copies sit on an unresolved conflict side. Do not port them. |
| REM-004 | B004 | `0a898cd246fbbf62f3b463f0097ffd6a368b1e07` | REJECT_DEBRIS | task-002-probes.md | The inspected tip change is the `_hook_proof` / `hook_proof_dirty_unstaged` marker on `ae993f77`, and that hk base stays on main as `799deeef`. |
| REM-005 | B005 | `1c42a33b2fe22aae4b40cd6ec6fd391cfd367c7d` | ALREADY_ON_P | task-002-ci-archives.md | This schema-4 ledger's push-head visibility, cancellation, conflict, and artifact-binding rules are on `fba69d60` at schema 7. |
| REM-006 | B006 | `1d97cf23f96e5a234d2a6a51d53f3e46c8d7825d` | REJECT_DEBRIS | task-002-probes.md | The unique delta is a docs-only PROBE-A comment in `plans/cicd-reliability/FAILURE_LEDGER.md` on parent `ebf5d5cd`, which is on main. |
| REM-007 | B007 | `5a7d4f19ed2c1061c4e5be518274ffa8779aeb5e` | REJECT_DEBRIS | task-002-probes.md | The tip is a comment-only brand probe plus an empty commit whose tree equals that comment, and the parent below the probe is on main. |
| REM-008 | B008 | `5a8264746286a981debb099646a6354d9b10e0cb` | REJECT_DEBRIS | task-002-probes.md | The only blob is malformed `crates/jackin-brand/src/untracked_bad.rs`, and the subject name `e57f46b` is not an object in this repository. |
| REM-009 | B009 | `5c4aa5eba5aaea2f2bfa2339f91066567a8e9979` | REJECT_DEBRIS | task-002-probes.md | The tip adds `let _hook_proof = 41 + 1` on `ae993f77`, whose landed tests remain on main. |
| REM-010 | B010 | `6d65e649855f98f133f58f605727b098b5d4eabb` | ALREADY_ON_M | task-002-ci-archives.md | `clippy_affected.rs` and its tests on this tip are byte-identical to main, including the staged-only lock and manifest union. |
| REM-011 | B011 | `07beb1c705ee8f813d2c8f5a438291cc031c9d1a` | REJECT_DEBRIS | task-002-probes.md | The unique delta is the one-line PROBE b8 brand comment on parent `799f5774`, which is on main. |
| REM-012 | B012 | `14bd1471616056e07c81c59d59eafc0963b16865` | PRESERVE_CANDIDATE | task-002-qualification-decision.md | Same `rollup.rs` and the four qualification tests as the stash tip. Its `validate_qualification_binding` does not call `validate_live_api_binding`. Port the stricter stash binding, not this copy. The always-required push-head predecessor is still absent; policy bootstraps when none exists. |
| REM-013 | B013 | `256b88d1a7e85b4855d1e64399e2af4482026e17` | REJECT_DEBRIS | task-002-probes.md | The commit result is unstaged `hook_proof_dirty_unstaged` on `ae993f77`, and the index tree equals that base. |
| REM-014 | B014 | `631d734b99ee9b413230ddb210698a5c80f41d70` | REJECT_DEBRIS | task-002-probes.md | The tip adds `hook_proof_clean` and `hook_proof_dirty_unstaged` on `ae993f77`, with no product behavior. |
| REM-015 | B015 | `7646c61bd4261a5908693bdaf0a6ecd9e9643448` | PRESERVE_CANDIDATE | task-002-native.md | On `fba69d60`, `crates/adapters/jackin-usage-ffi/boltffi.toml` still uses `../../target/xcframework` and `../../native/Sources/JackinUsageBindings`, and native CI runs xcodegen before the XCFramework is packed. |
| REM-016 | B016 | `66227a0033a6f8b917e684de1776753e33978590` | REJECT_DEBRIS | task-002-probes.md | The tip is an empty commit on the same brand-comment patch as the other PROBE b8 tips, and base `82ff0593` is on main. |
| REM-017 | B017 | `77508d454bfe9a903043586db2c8c5faa1f9e960` | REJECT_DEBRIS | task-002-probes.md | The merge result equals B011 and adds only the PROBE b8 cache-marker comment onto `799f5774`, which is on main. |
| REM-018 | B018 | `a28ab0a27eccc892decf8a4df55244b70096f631` | REJECT_DEBRIS | task-002-probes.md | The tip reformats `main` and adds `hook_proof_badly_formatted`, `hook_proof_clean`, and `hook_proof_dirty_unstaged` on `ae993f77`. |
| REM-019 | B019 | `b930e3658c671ca21ab7c25ea1a40349bb69284a` | REJECT_DEBRIS | task-002-probes.md | The merge result equals B007 and adds only the PROBE b8 brand comment onto `c90bf147`, which is on main. |
| REM-020 | B020 | `c7b8a95856cce6f5e8442f0e6fafcf025a089132` | PRESERVE_CANDIDATE | task-002-qualification-decision.md | Port `green_claim_qualified`, `require_qualified`, and the stash binding at `validation.rs:1041` that calls `validate_live_api_binding`, plus the four qualification tests. Do not copy this stash tree and do not restore global sccache. |
| REM-021 | B021 | `d0bdbc021350dbd7b9f5f011b8e96d2fa3858f78` | HISTORICAL_NOT_CURRENT | task-002-disposition-review.md | One commit ahead of main. Of the 18 nonempty added lines, 12 are already on main and policy via `ee1da0c6`. The other 6 are historical matrix rows. Main's current `120s` row already records FAIL. Do not import this commit as a baseline. |
| REM-022 | B022 | `df80aa7360da0eb6702e8cbe3eafd80c69d4b3bb` | REJECT_DEBRIS | task-002-probes.md | The merge adds the PROBE b8 brand comment and a trailing blank line on `Cargo.lock` onto `87521a95`, which is on main. |
| REM-023 | B023 | `e978dda9643079998f342380ccd123562139858a` | REJECT_DEBRIS | task-002-probes.md | This commit has the same parent, tree, and patch-id as B006 and adds only the PROBE-A docs comment. |
| REM-024 | B024 | `ffe6d6767bf2be30f54df7d245574b5980c05eac` | REJECT_DEBRIS | task-002-probes.md | The inspected `main.rs` payload defines invalid duplicate `hook_proof_clean` on `ae993f77`, and that base stays on main. |
| REM-025 | B025 | `0437497d7f22fbdb7c8aad986be932704d707fe9` | ALREADY_ON_P | task-002-native.md | `desktop-release` and deployment floor `26.0` are on `fba69d60` in xtask, `project.yml`, and `Package.swift`, and this tip's workflows are still parent `c52e912b`. |
| REM-026 | B026 | `2d97bba390864fbdc79aa6566e3394df938408af` | ALREADY_ON_P | task-002-src3.md | The tip tree equals its parent, and the ancestral product behaviors on this line are on `fba69d60`, including verbatim local version embedding for workspace `0.6.4`. |
| REM-027 | B027 | `502e0627d3940e2cd68ae0078910f21452d9ccc1` | ALREADY_ON_P | task-002-src3.md | The per-request 0/1/2 wire-log assertion matches `fba69d60` `crates/services/jackin-diagnostics/tests/wire_failure_support/mod.rs`. |
| REM-028 | B028 | `6ce20ce13cb87c5ab89b44d72698fa0d68aacf39` | ALREADY_ON_P | task-002-src3.md | Typed HTTP 401/403/429 classification and header `retry_after_seconds` are on `fba69d60`, which keeps structured retry metadata. |
| REM-029 | B029 | `8fb49688a6408627e22118f92d24f47785564979` | ALREADY_ON_M | task-002-native.md | The parent of this knowingly unregenerated pre-M1 input is already on main, and the inspected tip delta is the rejected unmatched generator state. |
| REM-030 | B030 | `12dea6c5fd0f9d2c594d5ad3159e6493d03352f4` | ALREADY_ON_P | task-002-src3.md | `fba69d60` denominators come from the push-head ledger, which is the selected obligation set for this tip's first-parent-history collector. |
| REM-031 | B031 | `432e0108ce90b64b428c69c27d4a2ac23e559379` | ALREADY_ON_P | task-002-native.md | `desktop-release` and deployment floor `26.0` are hardcoded on `fba69d60` in xtask, `project.yml`, and `Package.swift`. |
| REM-032 | B032 | `b1dc105acffb68ba71fff86f9c18dc5ff77de924` | ALREADY_ON_P | task-002-src3.md | `fba69d60` forbids `RUSTC_WRAPPER` and `SCCACHE_GHA_ENABLED=true` in generated workflow and installer environment, which is the selected sccache policy for this installer-wide wrapper. |
| REM-033 | B033 | `d16cd795d4a539b84ba5f44290663b2363d49350` | ALREADY_ON_P | task-002-native.md | `desktop-release` and deployment floor `26.0` are on `fba69d60` in xtask, `project.yml`, and `Package.swift`, while this commit only updates the generator input digest. |
| REM-034 | B034 | `33d88b1cf764f614d3935c11c48decd53386083e` | ALREADY_ON_P | task-002-src3.md | `fba69d60` ships sibling `jackin-usage-broker` and returns `Unavailable` when that executable is missing, which is the selected activation path for this tip. |
| REM-035 | B035 | `9ef617d014df15c6d967501bcbbdcb3689ed453c` | ALREADY_ON_P | task-002-usage-and-toolchain.md | `fba69d60` already has the shortened mise surface and the Rust `1.99` floor, with later mise and mbx pins than this tip. |
| REM-036 | B036 | `fba69d60c2373156dc0dcecee8e085c83776966f` | OPEN | task-002-ci-archives.md | B036 is the architecture candidate, not approval to merge its 366 commits. |
| REM-037 | B037 | `2e4a62da4bc4406764162a267163389a5fb513a7` | PRESERVE_CANDIDATE | task-002-dropped-test-successors.md | Schema 6 and wire v9 are not candidates: integration keeps monitor schema 4 and wire v8. Still absent from `fba69d60`: `lease_path_matches_open_file` and all 34 dropped tests. None of those 34 has a policy rename. |
| REM-038 | B038 | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` | PRESERVE_CANDIDATE | integration-constraints.md | `lease_path_matches_open_file`, the dropped in-scope tests named there, and the `mise.lock` minisign and hyperfine rows are on main and absent from `fba69d60`. |
| REM-039 | B039 | `41dd148e514d1bf3f84401f0ecbf8e87b58bb261` | PRESERVE_CANDIDATE | task-002-ci-archives.md | Task-scoped `run_with_sccache` is absent from `fba69d60`, which forbids installer-wide wrapper environment, so the provision-before-use rule remains to be carried or decided. |
| REM-040 | B040 | `2abed4ad67d8438ab9eb124b564be6ad5b545847` | ALREADY_ON_P | task-002-usage-rescues.md | Every function body in the 17 commits is on `fba69d60`, including the revoked-account result-free marker tests. |
| REM-041 | B041 | `ef238a7e4720fe1be8e8e295e1c502f2ba529e0b` | ALREADY_ON_P | task-002-policy-rescues.md | The eight added state-io tests match `fba69d60`, and the remaining hunks are test-module moves and blank lines. |
| REM-042 | B042 | `bff77099f8273a4d480a0e430cba66afc4197543` | ALREADY_ON_P | task-002-policy-rescues.md | `ensure_absent_for_purge_with` remains in `fba69d60` `apple_container.rs`, and the rollout doc names an intermediate crate that stays unrevived. |
| REM-043 | B043 | `3ddbc0563ed95225b0004c393b3928fb599ee066` | ALREADY_ON_P | task-002-policy-rescues.md | The broker split's fail-closed checks are on `fba69d60`. `ScopedCapability` was removed by `3f3ddd7c`, which is on main, so restoring it would undo a main change. |
| REM-044 | B044 | `40823a76a3e20b901d5bc410979cee0931fd5c48` | ALREADY_ON_M | task-002-b044-name-disposition.md | All 206 names exist on ancestor `d32e8556` and are absent from the tips. The newest pickaxe for every name is `3f3ddd7c` (`feat(usage): add explicit Claude usage monitoring (#1122)`), which is on main. The 10 unique commits are file splits. Do not restore the names. |
| REM-045 | B045 | `45e834683c8d5ca0fea1c7d3676cb136efa4acb1` | ALREADY_ON_P | task-002-policy-rescues.md | This tip shares tree `d1cc2456` and patch-id `b3877477` with B042, and that purge body remains in `fba69d60` `apple_container.rs`. |
| REM-046 | B046 | `b72046bbc9314e93cddf4d8252570dc375935f56` | ALREADY_ON_P | task-002-policy-rescues.md | The 5193 moved test names are a case-file split of the parent already on `fba69d60`, with zero names added or lost. |
| REM-047 | B047 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALREADY_ON_P | task-002-stash-challenge.md | The coordination sources, tests, shim, and crate scaffolding on this stash match `fba69d60` byte for byte. |
| REM-048 | B048 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-049 | B049 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALREADY_ON_P | task-002-stash-challenge.md | The repo-cache sources, tests, shim, and crate scaffolding on this stash match `fba69d60` byte for byte. |
| REM-050 | B050 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-051 | B051 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-052 | B052 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-053 | B053 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-054 | B054 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-055 | B055 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-056 | B056 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-057 | B057 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-058 | B058 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-059 | B059 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-060 | B060 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-061 | B061 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-062 | B062 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-063 | B063 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-064 | B064 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-065 | B065 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-066 | B066 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-067 | B067 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-068 | B068 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-069 | B069 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-070 | B070 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-071 | B071 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-072 | B072 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-073 | B073 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-074 | B074 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-075 | B075 | `80ab98038e945ec48c82dad241ebf33aa0efa82d` | ALIAS | task-002-stash-parents.md | This ref is the same object as B047 `80ab98038e945ec48c82dad241ebf33aa0efa82d`. |
| REM-076 | B076 | `b1b90781739333ba7c8829c092a845f6ea82b11c` | ALIAS | task-002-stash-parents.md | This ref is the same object as B049 `b1b90781739333ba7c8829c092a845f6ea82b11c`. |
| REM-077 | B077 | `fba69d60c2373156dc0dcecee8e085c83776966f` | OPEN | task-002-ci-archives.md | B077 is the same architecture candidate as B036, not approval to merge its 366 commits. |
| REM-078 | B078 | `ef40fea11a54ade7911de7622e6e011707241dd4` | PRESERVE_CANDIDATE | task-002-docs-model.md | Hyperfine and zig `provenance = "minisign"` are on this `mise.lock` tip and on main, and both are absent from `fba69d60`. |
| REM-079 | B079 | `d04467f05f1183008322047b1828c201e551e947` | ALREADY_ON_P | task-002-docs-model.md | Trimmed `load --model` / `--effort`, the config journal, and fail-closed `WalError::InvalidSalt` are on `fba69d60`, which is the recorded target for this tip's permissive WAL skip. |
