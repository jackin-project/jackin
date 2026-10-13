# TASK-002 independent review

Reviewer: independent of the report authors. The prefetch reports were treated as leads, then re-checked with `git -C /Users/donbeave/Projects/jackin-project/jackin`. The Jackin clone was not edited. No commit, push, fetch, reset, or `cargo` run.

Repository: `/Users/donbeave/Projects/jackin-project/jackin`
Acceptance source: `/Users/donbeave/Projects/merge/jackin/GOAL.md` TASK-002
Reviewed: 2026-10-13

Pins used below:

| Name | SHA |
| --- | --- |
| M `main` | `1c3dd1be4a3b1ed518133ec3899c084c792370ff` |
| P | `fba69d60c2373156dc0dcecee8e085c83776966f` |
| U | `2e4a62da4bc4406764162a267163389a5fb513a7` |
| T | `9ef617d014df15c6d967501bcbbdcb3689ed453c` |

## Verdict

**PARTIAL.**

Checks 1–8 pass. TASK-002 acceptance is still incomplete. Implementation approval stays blocked. This review does not approve an integration branch and does not approve source edits.

## Check 1 — P is 366 ahead and 0 behind M; merge base is M

**PASS**

`git rev-list --left-right --count 1c3dd1be4a3b1ed518133ec3899c084c792370ff...fba69d60c2373156dc0dcecee8e085c83776966f`

```text
0	366
```

Left is commits reachable from M and not from P (0 behind). Right is commits reachable from P and not from M (366 ahead).

`git merge-base` of those two SHAs is `1c3dd1be4a3b1ed518133ec3899c084c792370ff`, equal to `git rev-parse` of M.

## Check 2 — U is 25 ahead and 0 behind M; merge base is M

**PASS**

`git rev-list --left-right --count 1c3dd1be4a3b1ed518133ec3899c084c792370ff...2e4a62da4bc4406764162a267163389a5fb513a7`

```text
0	25
```

`git merge-base` is `1c3dd1be4a3b1ed518133ec3899c084c792370ff`.

## Check 3 — `should_exit_idle` on P

**PASS**

File on P: `crates/services/jackin-usage-broker/src/serve.rs`. The only definition is at line 518. The body is one expression:

```rust
fn should_exit_idle(
    now_monotonic: Duration,
    last_activity_monotonic: Duration,
    idle_exit: Duration,
    coordinator_idle: bool,
    has_active_monitor: bool,
    has_authenticated_foreground_guard: bool,
) -> bool {
    now_monotonic.saturating_sub(last_activity_monotonic) >= idle_exit
        && coordinator_idle
        && !has_active_monitor
        && !has_authenticated_foreground_guard
}
```

True requires all four conjuncts: idle elapsed (`saturating_sub` of the monotonic stamps is at least `idle_exit`), `coordinator_idle`, no active monitor, and no authenticated foreground guard. There is no other return path.

## Check 4 — `lease_path_matches_open_file`

**PASS**

`git grep lease_path_matches_open_file fba69d60 -- '*.rs'` exits 1. The symbol is absent from P.

On M it is defined at `crates/jackin-usage/src/host/broker.rs:1322` and called from that same file (lines 1187, 1196, 1234, 1238, 1293, 1304, 1313, 1344, 1358, 1363).

## Check 5 — `require_qualified` and `green_claim_qualified` on `c7b8a958`

**PASS**

Resolved commit: `c7b8a95856cce6f5e8442f0e6fafcf025a089132`.

Present there:

- `green_claim_qualified` field, `crates/jackin-xtask/src/ci_evidence.rs:358`
- assignment, `crates/jackin-xtask/src/ci_evidence/rollup.rs:119`
- `fn require_qualified`, `crates/jackin-xtask/src/ci_evidence/rollup.rs:151`

`git grep` of both symbols on M and on P exits 1.

## Check 6 — `fea3ade` is not an ancestor of stash tip `0a898cd2`; `read_limited_output` exists on `fea3ade`

**PASS**

`git merge-base --is-ancestor fea3ade6830912017f2ad80c0c5a9d1acfbde0b7 0a898cd246fbbf62f3b463f0097ffd6a368b1e07` exits 1.

On `fea3ade`, `async fn read_limited_output` is at `crates/jackin-process/src/lib.rs:854`, with call sites at lines 740 and 741.

## Check 7 — B026 `2d97bba3` has an empty diff against its first parent

**PASS**

`2d97bba390864fbdc79aa6566e3394df938408af` has one parent, `a43df8f41168a432982a8d363c3a1a6b8299306e`.

`git diff --numstat` and `git diff --raw` from `^1` to the commit are empty (0 lines, 0 bytes). Both trees are `3c01a03b8cb2aae87c284438aab1a66284485959`.

## Check 8 — `ScopedCapability` on `3ddbc056`, removed by `3f3ddd7c`, and that commit is in M and P

**PASS**

Resolved:

- `3ddbc0563ed95225b0004c393b3928fb599ee066` defines `pub(crate) struct ScopedCapability` at `crates/services/jackin-usage/src/host/broker/authorize.rs:21`, with further uses under `crates/services/jackin-usage`.
- `3f3ddd7c48284b648b083305f969ee3e709b74b7` has parent `868ce53519234f879d26a8bd2dffaa30fae2d729`. That parent contains `ScopedCapability` (`crates/jackin-usage/src/host/broker.rs`, 4 hits). The `3f3ddd7c` diff deletes `struct ScopedCapability` and its `scoped_capabilities` uses. `git grep ScopedCapability 3f3ddd7c` exits 1.
- `git merge-base --is-ancestor 3f3ddd7c` succeeds for both M and P.
- `git grep ScopedCapability` on the M tip and the P tip also exits 1.

`3ddbc056` itself is not an ancestor of `3f3ddd7c` (`merge-base --is-ancestor` exits 1). The stated claim still holds: the symbol exists on `3ddbc056`, commit `3f3ddd7c` removes it from its own parent, and that removal commit is an ancestor of M and of P.

## Missing evidence

These gaps keep TASK-002 acceptance incomplete. Passing inventory checks do not close them.

### Unclosed REM rows

`/Users/donbeave/Projects/merge/jackin/goal-references/02-change-coverage-matrix.md`, section "Mandatory unexamined-remainder ledger": **REM-001 through REM-079** (79 rows, no missing numbers, no duplicates). Every row is `OPEN / DEFER pending full source audit` with `independent reviewer NOT_RUN`.

The prefetch reports do not replace those rows. A search of `/Users/donbeave/Projects/merge/jackin-evidence-prefetch-20261013` for `REM-` in the task reports returns no remainder-ledger closure.

GOAL.md TASK-002 acceptance requires no unexamined in-scope remainder before implementation approval, or else the execution stays blocked. These 79 rows are still open.

### Unrun P, U, and T baselines

`/Users/donbeave/Projects/merge/jackin-evidence-prefetch-20261013/baselines/` contains only M artifacts:

- `M.log`
- `M-all-features.log`
- `M-metadata.json`
- `M-nextest-list.json`
- `M-xtask-ci-fast.log`
- `M-xtask-ci.log`

No `P-*`, `U-*`, or `T-*` baseline file is in that directory. T is `9ef617d014df15c6d967501bcbbdcb3689ed453c`. This review did not run `cargo`.

### Schema decision

Constants re-read from the trees:

| Constant | M | U | P |
| --- | --- | --- | --- |
| `USAGE_MONITOR_SCHEMA_VERSION` | 4 (`crates/jackin-protocol/src/usage_monitor.rs:11`) | 6 (same path) | 4 (`crates/core/jackin-protocol/src/usage_monitor.rs:11`) |
| `USAGE_BROKER_PROTOCOL_VERSION` | `v8` | `v9` | `v8` |
| `ACCOUNT_STATE_SCHEMA_VERSION` | 2 | 2 | 3 (`crates/services/jackin-usage-coordinator/src/state/envelope.rs:14`) |

U `crates/jackin-usage/src/host/broker/monitor/storage.rs` has `4 => migrate_v4`, `5 => migrate_v5`, and the current arm at schema 6. `git grep fn migrate_v5` on P exits 1. No reviewed selection chooses monitor schema 4 versus 6, broker wire v8 versus v9, or account schema 2 versus 3. `task-002-usage-rescues.md` leaves schema 6 and wire v9 unselected.

### 206 B044 names not yet classified

`/Users/donbeave/Projects/merge/jackin-evidence-prefetch-20261013/b044-fn-names-absent-from-M-P-U.txt` has 206 non-blank lines and 206 unique names. The file has no trailing newline (`wc -l` reports 205).

`task-002-usage-rescues.md` calls this list a deletion question inside P's later history, not a per-name disposition. This review did not classify them. Spot check only: `fn activate_broker` and `fn write_isolated_config` each occur once on B044 `40823a76a3e20b901d5bc410979cee0931fd5c48` and zero times on M, P, and U.

## Approval boundary

Checks 1–8 may stand as inventory facts. They do not close REM-001–REM-079, do not supply P/U/T baseline results, do not choose a schema, and do not classify the 206 B044 names.

No integration branch is approved. No source edit is approved.
