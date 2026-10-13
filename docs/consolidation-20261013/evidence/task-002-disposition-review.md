# TASK-002 disposition review

Reviewer: independent of `task-002-remainder-dispositions.md`. That file was read as a lead and the rows below were re-checked with `git -C /Users/donbeave/Projects/jackin-project/jackin`. The Jackin clone was not edited. The coverage matrix was not edited. No commit, push, fetch, reset, or `cargo` run.

Repository: `/Users/donbeave/Projects/jackin-project/jackin`
Reviewed file: `/Users/donbeave/Projects/merge/jackin-evidence-prefetch-20261013/task-002-remainder-dispositions.md`
Matrix left unchanged: `/Users/donbeave/Projects/merge/jackin/goal-references/02-change-coverage-matrix.md`
Reviewed: 2026-10-13

Pins used below:

| Name | SHA |
| --- | --- |
| M `main` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| P policy | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| U usage | `2e4a62da4bc4406764162a267163389a5fb513a7` |

## Verdict

**PARTIAL.**

The examined rows can be accepted as evidence, with one correction on REM-021: twelve of the nonempty lines added by `d0bdbc02` are already in `plans/cicd-reliability/EXECUTION.md` on main and on policy. The coverage matrix was not updated. All 79 remainder rows there still read `OPEN / DEFER pending full source audit`. Implementation stays blocked. This review does not approve an integration branch and does not approve source edits. `fba69d60` stays an architecture candidate. Its 366 commits are not approved.

Rows outside this list were not re-checked.

## REM-003 — `read_limited_output` on `fea3ade`

**PASS**

`fea3ade6830912017f2ad80c0c5a9d1acfbde0b7` defines both helpers in `crates/jackin-process/src/lib.rs`: `run_limited_output` at line 711 and `read_limited_output` at line 854.

`git grep` for `read_limited_output` on main `1c3dd1be` and on policy `fba69d60` exits 1 for the whole tree. The same exit holds for `run_limited_output` in that file on both tips.

`git log --all -S 'fn read_limited_output'` returns only `fea3ade6`.

## REM-012 and REM-020 — `require_qualified`

**PASS**

`require_qualified` is absent from main `1c3dd1be` and from policy `fba69d60` (`git grep` exit 1 on each).

It is present on `c7b8a95856cce6f5e8442f0e6fafcf025a089132` at `crates/jackin-xtask/src/ci_evidence/rollup.rs:151`, and also on the REM-012 tip `14bd1471616056e07c81c59d59eafc0963b16865` at the same relative path.

On `c7b8a958`, `green_claim_qualified`, `validate_qualification_binding`, and `validate_live_api_binding` are present. `git grep` for those three symbols on main and on policy exits 1.

## REM-015 — boltffi output paths on policy

**PASS**

On policy `fba69d60`, `crates/adapters/jackin-usage-ffi/boltffi.toml` contains:

- line 27: `output = "../../target/xcframework"`
- line 36: `output = "../../native/Sources/JackinUsageBindings"`

`git grep` for `../../../` in that file exits 1.

## REM-021 — B021 `d0bdbc02`

**PASS on the commit shape. The absence sentence is corrected.**

`git rev-list --left-right --count 1c3dd1be...d0bdbc021350dbd7b9f5f011b8e96d2fa3858f78` prints `64 1`. B021 is one commit ahead of main and 64 behind it. The only parent is `fd18e95c43c65031af56aa50e281b91efd08afb9`, and that parent is the merge base with main. `git show --numstat` is one file: `20 0 plans/cicd-reliability/EXECUTION.md`.

`d0bdbc02` is not an ancestor of main and not an ancestor of policy. The heading text that is on main arrived in `ee1da0c64eff098a71d5badf181ae2560feb0f55` (`docs(ci): post-merge verification evidence and goal matrix (#1021)`), a different commit.

Of the 20 added lines, two are blank. Of the other 18, the same 12 lines are present verbatim on main and on policy, and the same 6 are absent from both. The absent lines are the evidence-matrix rows for the failure inventory, phased steps, shared-change pin, `120s across scenarios`, reviews, and independent verification. The current `120s` row on main and policy is a later measurement (`Measured @44243ed4`) and still records `FAIL`. This commit does not prove a one-minute CI result. Do not import `d0bdbc02` as the current baseline.

## REM-037 — schema 6 and wire v9

**PASS**

Schema 6 and wire v9 are the usage tip's constants, and they are not the policy tip's constants.

| Constant | M | U | P |
| --- | --- | --- | --- |
| `USAGE_MONITOR_SCHEMA_VERSION` | `4` in `crates/jackin-protocol/src/usage_monitor.rs:11` | `6` at the same path | `4` in `crates/core/jackin-protocol/src/usage_monitor.rs:11` |
| `USAGE_BROKER_PROTOCOL_VERSION` | `"v8"` in `crates/jackin-protocol/src/usage_broker.rs:14` | `"v9"` at the same path | `"v8"` in `crates/core/jackin-protocol/src/usage_broker.rs:14` |

`fn migrate_v4` and `fn clear_unscoped_provider_projection` are on U in `crates/jackin-usage/src/host/broker/monitor/storage.rs`. `git grep` for those functions on main and on policy exits 1.

`lease_path_matches_open_file` is absent on policy (`git grep` exit 1). It is defined on main at `crates/jackin-usage/src/host/broker.rs:1322` and on U at `crates/jackin-usage/src/host/broker.rs:1588`.

## REM-043 — `ScopedCapability` removed by `3f3ddd7c`

**PASS**

`git merge-base --is-ancestor 3f3ddd7c48284b648b083305f969ee3e709b74b7` succeeds for main and for policy. `git log -1 -S ScopedCapability 3f3ddd7c` names that same commit.

`git grep ScopedCapability` hits `crates/jackin-usage/src/host/broker.rs` on the parent and exits 1 on the commit tree, on main, and on policy. The REM-043 tip `3ddbc056` still contains the symbol under `crates/services/jackin-usage/src/host/broker.rs`.

## REM-044 — 206 names stay preservation leads

**PASS**

`b044-fn-names-absent-from-M-P-U.txt` has 206 names and 206 unique names. A POSIX `fn NAME` search on `*.rs` defines all 206 on ancestor `d32e85563648931f80ffab141bb2650f478c2a60` and on B044 `40823a76a3e20b901d5bc410979cee0931fd5c48`. The same search exits 1 on main, policy, and usage.

`d32e8556` is an ancestor of policy and of B044. Policy has 360 commits after that ancestor. B044 has 10 commits that policy does not contain. Those 10 are `40823a76` plus nine `S7 split` commits (`459507ea`, `123bd437`, `4191a567`, `46a24b4f`, `9158cf03`, `40dc9004`, `b950a152`, `39b4d39e`, `b5b66a7f`). They are not the fix for the 206 names. The names are already on the shared ancestor and are absent after the 360 later policy commits.

Policy does define two different successors: `catalog_revision_change_stores_cooldown_marker_and_allows_new_revision` and `catalog_revocation_clears_materialized_data_and_fences_late_result`. The original names `catalog_revision_change_purges_old_state_and_allows_only_new_revision` and `catalog_revocation_retains_materialized_last_good_but_fences_late_result` are absent on policy. All 206 names stay preservation leads.

## REM-036 and REM-077 — policy stays open

**PASS**

Both rows name `fba69d60c2373156dc0dcecee8e085c83776966f`. `git rev-list --count 1c3dd1be..fba69d60` is 366. `git rev-list --count fba69d60..1c3dd1be` is 0. The merge base is main. The matrix rows for REM-036 and REM-077 still say `OPEN / DEFER pending full source audit`. That tip is the architecture candidate. It is not approval of those 366 commits.

## Matrix and implementation

The disposition file says the matrix was not edited. This review re-read the matrix and left it unchanged. Remainder rows REM-001 through REM-079 remain `OPEN / DEFER pending full source audit` with `independent reviewer NOT_RUN`.

No integration branch is approved. No source edit is approved. Implementation stays blocked.
