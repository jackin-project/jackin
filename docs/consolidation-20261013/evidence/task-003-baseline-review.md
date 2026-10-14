# TASK-003 independent baseline review

Reviewer did not run these baselines, did not run cargo, and did not edit or commit the Jackin clone. Sources are the logs and result notes under `baselines/`. Result-note claims were re-checked against the logs. A wrapper shell exit of 0 is not a product pass when `xtask_ci_exit` is 1. An unlocked pass does not clear a locked failure.

**Verdict: PARTIAL.** Required baseline commands are not all recorded, and several failing steps do not have a retained reason that matches M. This review does not approve an integration branch and does not approve source edits.

Host line present only in `M.log`: macOS 27.0.1 `26A434` arm64. Usage e2e on OrbStack was **NOT_RUN**. Native `mise -C native run ci` (and the native merge, scheduled, and desktop test commands) was **NOT_RUN**.

## How a failure is classed

| Class | Rule used |
| --- | --- |
| `INHERITED_FROM_M` | The same check fails on M for the same retained reason. |
| `CANDIDATE_SPECIFIC` | The retained reason is absent from M's recorded failure for that check. |
| `SHARED_SUPPLY_CHAIN` | Third-party license or advisory rejection recorded on more than one candidate and absent, as that reason, from M. |
| `ENVIRONMENT` | Tool or host refusal before a product result. |
| `UNCERTAIN` | The step failed, and the log does not retain enough to match or separate it from M. |

A step that contains both an inherited reason and a different reason is split. The step is not labeled inherited as a whole.

`cargo xtask ci` prints `error: N ci step(s) failed` and then the output of those steps. Planned commands that are absent from that list have no retained stdout here. This review does not mark them passed.

Fail-fast cancelled the rest of each nextest run. Tests that were not run are not passes.

## Recorded commands

| Head | SHA | Toolchain | Recorded result |
| --- | --- | --- | --- |
| M | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` | Rust 1.97.1 | `M.log` default nextest `nextest_run_exit:100`. `M-all-features.log` `nextest_all_features_exit:0`, 7426 passed. `M-xtask-ci-fast.log` `xtask_ci_fast_exit:1`. `M-xtask-ci.log` `xtask_ci_exit:1`. |
| P | `fba69d60c2373156dc0dcecee8e085c83776966f` | Rust 1.99.0 | `P-xtask-ci.log` is the Mise 2026.10.4 refusal. Product result is `P-xtask-ci-2.log`, Mise 2026.10.8, `xtask_ci_exit:1`. |
| U | `2e4a62da4bc4406764162a267163389a5fb513a7` | Rust 1.97.1 | `U-xtask-ci.log` `xtask_ci_exit:1`. |
| T | `9ef617d014df15c6d967501bcbbdcb3689ed453c` | Rust 1.99.0, Mise 2026.10.8 | `T-xtask-ci.log` `xtask_ci_exit:1`. |

`M-all-features.log` contains no `--locked` flag. Its nextest profile line is `default`. The locked command is the one printed in `M-xtask-ci.log`: `cargo nextest run --workspace --all-features --locked`. That locked run failed. The unlocked 7426 pass stands beside it and does not replace it.

## Environment records that are not product results

`P-xtask-ci.log` (2026-10-12T22:41:11Z to 22:41:12Z). Mise 2026.10.4 refused `min_version` 2026.10.7. `rustc_exit:1`, `metadata_exit:1`, `list_exit:1`, `xtask_ci_exit:1`. Class: **ENVIRONMENT**. The product run is `P-xtask-ci-2.log`.

`T-xtask-ci.log` install prefix. `boltffi_cli` 0.31.0 had no prebuilt binary. `cargo-binstall` reported fallback to cargo-install disabled. Mise then ran `cargo install`, and the CI steps ran. Class of the install path: **ENVIRONMENT**. The five CI step failures below are product results on that completed install.

## M, the main baseline

`M-xtask-ci.log` started 2026-10-12T22:24:10Z, ended 2026-10-12T22:40:02Z. `xtask_ci_exit:1`. Five steps failed.

| Step | Retained reason | Class |
| --- | --- | --- |
| `cargo xtask lint --strict` | `container-paths` 7, `headers` 3, `telemetry-registry`, `ratchet` 110 | Main baseline |
| `cargo nextest run --workspace --all-features --locked` | 5075/7426 run: 5074 passed (1 leaky), 1 failed, 5 skipped, 2351 not run. Fail: `jackin-omp-store capture::tests::missing_or_stale_exact_selectors_never_fall_back_to_a_sibling`. Panic at `crates/jackin-omp-store/src/capture.rs:1227`: `unwrap()` on `Err(Unavailable)` | Main baseline. Locked failure stands |
| `cargo audit` | 1307 advisories loaded. 807 crate dependencies. `5 vulnerabilities found` and `2 denied warnings found`. Advisory ids are not in the log | Main baseline summary. Id list **UNCERTAIN** |
| `cargo deny check advisories bans licenses sources` | Exit 7. `Zlib` on zlib-rs 0.6.8. `error[workspace-duplicate]` rustix 1.1.4. `RUSTSEC-2024-0449` unmaintained `unsafe-libyaml` 0.2.11. Yanked `dockerfile-parser-rs` 3.3.0. Three `license-exception-not-encountered` warnings | Main baseline |
| `cargo shear --deny-warnings` | Exit 1. No warning names in the captured step body | **UNCERTAIN** |

`M-xtask-ci-fast.log` (22:15:14Z to 22:23:50Z), `xtask_ci_fast_exit:1`, reports the same five step names. Audit, deny, shear, and the lint step match the full run's retained reasons. The nextest step does not. It failed `jackin-config accounts::stores::omp::tests::missing_store_yields_no_accounts` (`crates/jackin-config/src/accounts/stores/omp/tests.rs:68`, `unwrap()` on `Err(Malformed)`), 1869/7426 run. The later full locked log **passed** that test at 1852/7426. Class of this fast-only test: **UNCERTAIN**. It is not a second stable inherited failure, and it does not clear the sibling-selector failure.

`M.log` default `cargo nextest run`, `nextest_run_exit:100`. 4467/7414 run: 4466 passed (2 leaky), 1 failed, 5 skipped. Fail: `jackin-instance auth::tests::omp_sync_materializes_schema_and_credentials_from_wal_only_pages` (`crates/jackin-instance/src/auth/tests.rs:376`, `unwrap()` on `Err(Malformed)`). The locked all-features log and the unlocked all-features log both **passed** this test. It remains a recorded default-nextest failure on M. P, U, and T have no separate default nextest log.

`M-all-features.log` (22:11:06Z to 22:14:51Z): 7426 tests run, 7426 passed (1 slow, 1 leaky), 5 skipped, `nextest_all_features_exit:0`. This includes a pass of `missing_or_stale_exact_selectors_never_fall_back_to_a_sibling` and of `spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch`. The locked CI log does not contain `spawned_ticker`. That lease test was among the 2351 not run after the locked fail-fast. Unlocked execution is not a locked pass.

Clippy is not one of M's five failed steps. On this 1.97.1 run the runner did not report `clippy::assert_is_empty`.

## P product run

`P-xtask-ci-2.log`, 2026-10-12T22:42:24Z to 22:59:14Z. Mise 2026.10.8. `rustc 1.99.0`. `rustc_exit:0`, `metadata_exit:0`, `list_exit:0`, `xtask_ci_exit:1`. Seven steps failed.

| Step | Retained reason | Class |
| --- | --- | --- |
| `cargo clippy ... --locked -- -D warnings` | Exit 101. `jackin-protocol` did not compile. Four `clippy::assert_is_empty` errors in `crates/core/jackin-protocol/src/attach.rs` at lines 884, 886, 903, and 1091. `could not compile jackin-protocol (lib) due to 4 previous errors` | **CANDIDATE_SPECIFIC** |
| `cargo xtask lint --strict` | `container-paths` 24, `headers` 4, `ratchet` 99, `arch` 3. Untiered: `jackin-instance-agents`, `jackin-instance-credentials`, `jackin-instance-roles`. No `telemetry-registry` gate in this failure block | **CANDIDATE_SPECIFIC**. Container-path, header, and ratchet gates also fail on M; this retained set does not match M |
| `cargo nextest run --workspace --all-features --locked` | 598/7455 run: 597 passed, 1 failed, 5 skipped. Fail: `jackin::docs_commands config_reference_matches_public_schema_fields`. Stderr: `Error: No such file or directory (os error 2)` | **CANDIDATE_SPECIFIC**. M's locked all-features log passed this test |
| `cargo audit` | 1307 advisories. **927** crate dependencies. `5 vulnerabilities found` and `2 denied warnings found`. Ids not retained | **UNCERTAIN**. Same count text as M, different dependency count |
| `cargo deny ...` | Exit 5 | Split below |
| `cargo shear --deny-warnings` | Exit 1. No warning names in the captured step body | **UNCERTAIN** |
| `cargo xtask docs repo-links` | Exit 1. 35 `RepoFile` paths missing, 12 unique, all under `crates/services/...` | **CANDIDATE_SPECIFIC** |

P deny split:

| Reason | Class |
| --- | --- |
| `Zlib` on zlib-rs 0.6.8 | **INHERITED_FROM_M** |
| `RUSTSEC-2024-0449` `unsafe-libyaml` | **INHERITED_FROM_M** |
| Yanked `dockerfile-parser-rs` | **INHERITED_FROM_M** |
| 18 `Unicode-3.0` rejections, including `icu_collator` 2.3.1. The not-encountered exceptions still name `icu_collator@2.2.1` and the other 2.2.x ICU crates | **SHARED_SUPPLY_CHAIN** with T. M's deny license error is `Zlib` only |
| `warning[advisory-not-detected]` for `RUSTSEC-2026-0173` in `deny.toml` | **CANDIDATE_SPECIFIC**. Absent from M's deny output. Also on T |

P deny does not report M's `rustix` 1.1.4 workspace-duplicate. That absence does not make the deny step a pass.

Docs paths that do not exist, unique:

- `crates/services/jackin-instance/src/auth.rs`
- `crates/services/jackin-usage/src/logging.rs`
- `crates/services/jackin-usage/src/token_monitor.rs`
- `crates/services/jackin-usage/src/token_monitor/claude.rs`
- `crates/services/jackin-usage/src/token_monitor/kimi.rs`
- `crates/services/jackin-usage/src/usage/amp.rs`
- `crates/services/jackin-usage/src/usage/claude.rs`
- `crates/services/jackin-usage/src/usage/codex.rs`
- `crates/services/jackin-usage/src/usage/format.rs`
- `crates/services/jackin-usage/src/usage/grok.rs`
- `crates/services/jackin-usage/src/usage/zai.rs`
- `crates/services/jackin-usage/src/usage_snapshot_store.rs`

## U

`U-xtask-ci.log`, 2026-10-12T22:59:52Z to 23:08:13Z. Rust 1.97.1. `rustc_exit:0`, `metadata_exit:0`, `list_exit:0`, `xtask_ci_exit:1`. Five steps failed. Clippy is not among them.

| Step | Retained reason | Class |
| --- | --- | --- |
| `cargo xtask lint --strict` | Same four gates as M: `container-paths` 7, `headers` 3, `telemetry-registry`. `ratchet` 120 versus M's 110 | Split below |
| `cargo nextest run --workspace --all-features --locked` | 715/7480 run: 714 passed (2 leaky), 1 failed, 5 skipped. Fail: `jackin::usage_provider_flow auth_outcomes::cli_provider_auth_outcomes_keep_the_selected_source_and_typed_terminal`. Stderr: fixture clock rejected `wall_epoch=1700000001`, `monotonic_millis=2000` | **CANDIDATE_SPECIFIC**. That test name is absent from M's unlocked 7426-pass log. It is not the OMP sibling failure |
| `cargo audit` | 1307 advisories. 807 crate dependencies. `5 vulnerabilities found` and `2 denied warnings found`. Ids not retained | **INHERITED_FROM_M** at the retained summary. The five ids are not listed |
| `cargo deny ...` | Exit 7. Same error set as M: `Zlib` / zlib-rs 0.6.8, rustix 1.1.4 workspace-duplicate, `RUSTSEC-2024-0449`, yanked `dockerfile-parser-rs` 3.3.0 | **INHERITED_FROM_M** |
| `cargo shear --deny-warnings` | Exit 1. No warning names | **UNCERTAIN** |

U lint split: the matching gate counts (`container-paths` 7, `headers` 3, `telemetry-registry`) are **INHERITED_FROM_M**. U-only ratchet rows are **CANDIDATE_SPECIFIC**, including `clippy::clone_on_ref_ptr` on `jackin-usage` at 72 versus M's 64, `jackin-usage` complexity 61 versus M's 60, file-size rows for `host/broker.rs`, `usage_provider_flow.rs`, and broker test-layout rows that are not in M's lint block. The lint step is therefore not wholly inherited.

## T

`T-xtask-ci.log`, 2026-10-12T23:08:48Z to 23:25:09Z. `rustc_exit:0`, `metadata_exit:0`, `list_exit:0`, `xtask_ci_exit:1`. Five steps failed.

| Step | Retained reason | Class |
| --- | --- | --- |
| `cargo clippy ... --locked -- -D warnings` | Exit 101. Same four `clippy::assert_is_empty` errors, `crates/jackin-protocol/src/attach.rs` lines 884, 886, 903, 1091. `could not compile jackin-protocol (lib) due to 4 previous errors` | **CANDIDATE_SPECIFIC**. Same lint class as P. Path prefix differs (`crates/jackin-protocol` here, `crates/core/jackin-protocol` on P) |
| `cargo xtask lint --strict` | Same four gate names as M. `headers` 2 versus M's 3. `ratchet` 107 versus M's 110. Protocol container-path literal is line 146 versus M's line 147 | Split. Overlapping gate family is the M class. Rows present only on T, including usage-ffi test-layout and the public-surface 7-to-8 growth, are **CANDIDATE_SPECIFIC**. The step is not wholly inherited |
| `cargo nextest run --workspace --all-features --locked` | 3825/7246 run: 3824 passed, 1 failed, 5 skipped. Fail: `jackin-dev::bin/jackin-dev tests::auto_prep_construct_triggers`. Panic at `crates/jackin-dev/src/tests.rs:111`: `mise.toml should trigger a construct build` | **CANDIDATE_SPECIFIC**. M's locked all-features log passed this test |
| `cargo audit` | 1307 advisories. **821** crate dependencies. The captured text is `2 denied warnings found`. There is no `5 vulnerabilities found` line | **UNCERTAIN**. This is not M's five-vulnerability summary, and the warning ids are absent |
| `cargo deny ...` | Exit 7 | Split below |

T has no `docs repo-links` failure in the five-step list. That is not a retained pass of the docs gate.

T deny split:

| Reason | Class |
| --- | --- |
| `Zlib` on zlib-rs 0.6.8 | **INHERITED_FROM_M** |
| `RUSTSEC-2024-0449` `unsafe-libyaml` | **INHERITED_FROM_M** |
| Yanked `dockerfile-parser-rs` | **INHERITED_FROM_M** |
| `Unicode-3.0`, including `icu_collator` 2.3.1, with 2.2.x exceptions not encountered | **SHARED_SUPPLY_CHAIN** with P |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | **SHARED_SUPPLY_CHAIN** |
| `CDLA-Permissive-2.0` on `webpki-root-certs` | **CANDIDATE_SPECIFIC** supply-chain license. Not in M's or P's rejected-license set |
| `error[duplicate]` crate `rand`, plus `unmatched-skip` for `rand = =0.8.7` | **CANDIDATE_SPECIFIC** |
| `error[workspace-duplicate]` rustix **1.1.5** | **UNCERTAIN**. M's duplicate is rustix **1.1.4**. Same ban kind, different locked version |
| `warning[advisory-not-detected]` `RUSTSEC-2026-0173` | **CANDIDATE_SPECIFIC**. Also on P. Absent from M |

## What must be fixed before an integration head can be called green

These are recorded failures whose reason is not M's. M being red does not excuse them. An integration head that still fails any of these is not green.

- P and T clippy: four `clippy::assert_is_empty` sites in `jackin-protocol` `attach.rs` (884, 886, 903, 1091) under Rust 1.99.0 `-D warnings`.
- P nextest: `config_reference_matches_public_schema_fields` (`os error 2`).
- P docs repo-links: the 12 stale `crates/services/...` paths listed above.
- P lint: arch tier gaps for the three `jackin-instance-*` crates, and the container-path / header / ratchet set that does not match M's counts.
- U nextest: `cli_provider_auth_outcomes_keep_the_selected_source_and_typed_terminal` fixture-clock rejection.
- U lint: the usage ratchet and test-layout rows that are not in M's lint block.
- T nextest: `auto_prep_construct_triggers` (`mise.toml should trigger a construct build`).
- T lint: rows that are not in M's lint block.
- P and T deny: `Unicode-3.0` on the resolved 2.3.1 ICU crates, including `icu_collator`, while exceptions still name 2.2.x.
- T deny: `CDLA-Permissive-2.0` on `webpki-root-certs`, the extra compound Unicode license, and the `rand` duplicate / stale `rand = =0.8.7` skip.
- P and T deny: stale `RUSTSEC-2026-0173` ignore (`advisory-not-detected`).

## Inherited main baseline

These already fail on M for the retained reason. They are not new candidate defects. They are also not a waiver. `xtask ci` stays red while they fail.

- M locked all-features nextest: `missing_or_stale_exact_selectors_never_fall_back_to_a_sibling` (`Unavailable` at `capture.rs:1227`). The unlocked 7426 pass does not clear it.
- M default nextest: `omp_sync_materializes_schema_and_credentials_from_wal_only_pages` (`Malformed`). No candidate default-nextest log exists.
- M and U lint gates with matching counts: `container-paths` 7, `headers` 3, `telemetry-registry`, plus the ratchet rows that are in both blocks.
- M and U deny, and the same three errors inside P and T deny: `Zlib` on zlib-rs 0.6.8, `RUSTSEC-2024-0449` `unsafe-libyaml` 0.2.11, yanked `dockerfile-parser-rs` 3.3.0.
- M and U deny: rustix 1.1.4 workspace-duplicate.
- M and U `cargo audit` summary: 5 vulnerabilities and 2 denied warnings, 807 dependencies, 1307 advisories. The advisory ids were not captured.

## Uncertain, so neither waived nor charged as a new named defect

- `cargo shear --deny-warnings` on M, P, U, and T: exit 1, no named warnings in the step body.
- P `cargo audit`: same "5 vulnerabilities and 2 denied warnings" text as M, on 927 dependencies, ids absent.
- T `cargo audit`: only "2 denied warnings", 821 dependencies, ids absent. This is not a recorded copy of M's five-vulnerability line.
- M `xtask ci --fast` nextest: `missing_store_yields_no_accounts` failed, and the later full locked run passed that test.
- T deny rustix workspace-duplicate at 1.1.5 versus M's 1.1.4.
- M audit advisory ids, on every head.

## Not recorded, so not passed

Usage e2e on OrbStack: **NOT_RUN**.

Native mise CI: **NOT_RUN**. That includes `mise -C native run ci`, `mise -C native run merge`, `mise -C native run scheduled`, and `mise exec -- mbx +1.99.0 xtask desktop test`.

Also absent as retained command results:

- P `xtask health --format json`, P `xtask ci --fast`, P default nextest, and a P all-features nextest outside the failed locked CI step.
- P `cargo hack` feature-powerset is in the CI plan list and is not in the seven-step failure list. Its stdout is not retained. Not marked passed.
- U and T `xtask ci --fast`, and U and T default nextest.
- A separate `mise install` result for each head.
- Stdout for CI plan steps that the failure summary omits (fmt, actionlint, check, doctests, schema-check, roadmap, research, and the capsule/console nextest step). Omission is not a retained pass.
- The foreground lease test `spawned_ticker_reconciles_one_sleep_jump_and_wakes_monitor_watch` passed in M's unlocked all-features log and was not executed in the locked fail-fast logs.

## Decision

**PARTIAL.** Every head has a real `xtask ci` log with `xtask_ci_exit:1`, and the named product failures above are in those logs. The command set required for a baseline is not complete: e2e and native mise CI were not run, several per-head commands have no log, shear and audit ids are unnamed, and fail-fast left most tests unrun. Inherited failures and candidate failures are separable for the named nextest, clippy, docs, and deny reasons. They are not separable for shear, for the audit id lists, or for the fast-only `missing_store` result.

No integration branch is approved. No source edit is approved.
