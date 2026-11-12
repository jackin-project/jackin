# P11.2 sandbox process groups and launch flakes

Source: `/private/tmp/claude-501/plans/P11.2-evidence.md` (G-FAST of the Architect role fails in an OpenShell 0.1.6 sandbox). Base: `dbb35f2f7` on `wt/sandbox-pgid`.

## Root cause 1: the runner waits on a signal the sandbox denies

`TokioProcessRunner` stops a run by sending SIGKILL to the child's process group and repeating the group signal until `kill(-pgid)` answers ESRCH (`crates/jackin-session/src/host.rs`, `gone`, commit 2ef1055f). OpenShell v0.1.6 filters `kill` in the workload seccomp program: `crates/openshell-sandbox/src/linux/child_seccomp.rs` denies a negative or zero pid with `append_argument_masked_deny(SYS_kill, 0, 1 << 31)` and `append_argument_equal_deny(SYS_kill, 0, 0)`, and `process_signal.rs:24` states "Group/broadcast signaling remains deliberately denied". The loop saw EPERM, not ESRCH, and ran into its 5 s cap, which returned silently. The two tests `lifecycle::a_clean_exit_kills_a_descendant_that_closed_its_pipes` and `lifecycle::a_run_returns_only_after_no_process_of_its_group_is_left` failed in the sandbox for that reason.

Structure that allowed it: the old loop read any error of `kill_process_group` as "the group is gone" (`while kill_process_group(..).is_ok()`), so EPERM was one more silent answer, and the cap ended the wait without a result. The contract "when `run` returns, no process of the group is left" had no error to report a breach.

Per-pid signals are allowed. `mediate_process_signal` accepts a positive target, refuses only the sandbox leader's own thread group, and delivers through a retained pidfd. `pidfd_open` and `pidfd_send_signal` are denied to the workload (`child_seccomp.rs:201` returns ENOSYS), so the runner uses plain `kill(pid, SIGKILL)`. `/proc` is in the default read-only filesystem policy (`openshell-policy/src/lib.rs:981`).

### Fix

| File | Change |
|---|---|
| `crates/jackin-session/src/host/group.rs` (new) | `sweep(group)` sends SIGKILL to the group. ESRCH is `Empty`. On Linux, success and EPERM both continue with `/proc`: success asks whether any live member is left (none is `Empty`), and EPERM calls `by_member`, which sends SIGKILL to each live member. On other systems both answers are `Signalled`. Any other errno is a `GroupFault::Unreachable`. `strike(group)` is the plain group signal for `GroupGuard::fire`. |
| `crates/jackin-session/src/host/group/stat.rs` (new, Linux) | Pure parse of one `/proc/<pid>/stat` as bytes: everything after the last `)`, then state, `pgrp` and `num_threads`. |
| `crates/jackin-session/src/host.rs` | `gone` repeats `swept` (the sweep on `spawn_blocking`) every 2 ms until `Empty`, within the unchanged 5 s cap, and returns a `GroupFault`. A group fault joins `Cancelled` and `Timeout` as their `group` field (`with_group`), so the outcome that stopped the run stays visible. Any other run error, or a clean exit, becomes `ProcessError::Group`. `Drop` of the guard runs the full `sweep` synchronously. |
| `crates/jackin-core/src/boundary.rs` | `GroupFault` (`Alive { after }`, `Unreachable { message }`). `ProcessError::Timeout` and `ProcessError::Cancelled` gain `group: Option<GroupFault>`. `ProcessError::Group { program, fault }` replaces `GroupAlive` and `GroupUnreachable`. |
| `crates/jackin-openshell/src/transfer.rs` | The exhaustive `process_error` match carries `Group` into `GatewayError::Process`. `Cancelled` and `Timeout` map as before. |

Decisions:

1. A zombie is not a member, on both Linux paths. A member is gone when its state is `X`, or `Z` with `num_threads` 1 (a zombie leader with live threads still runs). A group that holds only such zombies is `Empty` whether the group signal answered success or EPERM, because the parent of an orphan zombie may never reap it (a sandbox without a reaping init) and a zombie cannot run. On macOS there is no procfs; see finding 1.
2. A fork racing the scan is caught by the next scan. Each pass kills all current members, so a new member needs a living parent. A fork loop that outruns the passes ends at the cap as `GroupFault::Alive`.
3. Reuse of a listed pid between the `/proc` read and the `kill` needs a full pid wrap in microseconds. It is not guarded (pidfds are not available to the workload).
4. The scan runs on `spawn_blocking`. `GroupGuard::fire` in the async path only sends the plain group signal. `Drop` runs the whole `sweep` on the dropping thread, because it cannot await.
5. A group fault never replaces `Cancelled` or `Timeout`. For `Spawn`, `OutputTooLarge` and `Read` the group fault takes over as `ProcessError::Group`, because no caller keys on those.

### Test seam for the EPERM path

The EPERM trigger has no hermetic source: it needs a seccomp filter, root, or Seatbelt. The effects of the EPERM branch are testable without it, because `by_member(group)` does not depend on the errno and uses real `/proc` and real signals. No fake of owned code is involved.

- A seccomp-notify rule needs `unsafe_code`, which the workspace forbids, and the OpenShell workload filter denies `seccomp(SET_MODE_FILTER)`.
- A uid mismatch (the other source of EPERM on Linux) needs root.
- Seatbelt on macOS cannot deny `kill` by argument sign, and macOS has no `/proc`.
- A trait over the signal call, or a `/proc` root path, would be a fake of owned code, and a fake `/proc` root would make the runner kill real pids.
- `lifecycle::a_run_returns_only_after_no_process_of_its_group_is_left` proves the contract on any host, and it exercises the EPERM branch when it runs in a sandbox.

The test plan for `test-reviewer` is `/private/tmp/claude-501/plans/sandbox-pgid-test-plan.md`. No test was written in this round.

## Root cause 2: the fake clock moves while real work is in flight

`recreate::a_sandbox_deleted_after_the_start_request_is_recreated_and_gets_its_own_start` ran a peer future and the launch in one `tokio::join!`. The peer wrote `run/state` through the fake gateway. That write is four `tokio::fs` round trips on the blocking pool. `FakeClock::sleep` (`crates/jackin-testkit/src/host/clock.rs`) finishes a sleep after 64 task yields, whatever else is in flight. While the peer waited for the blocking pool, the launch's supervisor wait kept yielding. One yield costs about 0.18 ms of real time here (200 ms of stall moved the clock 8.5 s, 17 sleeps), so the 20 s supervisor limit (40 sleeps) is spent by a stall of about 0.5 s. The launch then failed with `SupervisorTimeout`, the peer looped for a `run/start` that never came, and the 60 s guard ended the test with "the test hung for 60 s". A 2-vCPU VM with a busy disk gives pool stalls of that size.

Structure that allowed it: virtual time was a function of poll count, not of the completion of work in the same task. Any test peer that does real I/O while the launch sleeps can lose the race, and the loop also spun on the CPU while it waited, which starves the very blocking thread it waits for.

### Fix

`FakeClock` counts `Working` guards. `FakeGateway::link_clock` (called by `World::build`) hands it the clock, and `run_program`, `read_files`, `write_file`, `upload_tree`, `upload_file` and `download_file` hold a guard for the whole call. `FakeClock::sleep` waits on a `Notify` while any guard is held, so it neither advances time nor spins, and it restarts its yield count after a busy round. Files: `crates/jackin-testkit/src/host/clock.rs`, `gateway.rs`, `gateway/state.rs`, `world.rs`. No timeout and no assertion changed.

## Open: `adr008::a_new_launch_id_makes_a_new_sandbox_and_leaves_the_live_one_untouched`

Not reproduced, cause not found. Stated plainly:

- The test is sequential. It has no peer, so the clock race of cause 2 cannot reach it (a sleep never overlaps real I/O in one sequential launch).
- Ruled out by experiment on the host: 20 plain runs; 20 runs and 3 full launch-binary runs under 54 and then 354 CPU burners (load average 125), the test at `nice -n 20`; `WAKE_YIELDS=2` over the whole launch binary (72 tests pass); a descriptor limit of 64, 96 and 128.
- Ruled out by reading: `Labels` is a `BTreeMap`; no `tokio::spawn` in `jackin-session`; the fake supervisor writes only on a start request or `exit_codex`; the only wall-clock value in a sandbox file is `last_refresh` in `auth.json`, written before the snapshot.
- One weak spot that could produce a false difference: `SandboxHandle::collect` (`crates/jackin-testkit/src/gateway/handle.rs`) and `Supervisor::write_state` swallow I/O errors. In the sandbox the disk filled during nextest (`No space left on device`, evidence item 3), which would make a file vanish from one snapshot and not the other. This is inference, not shown.

## Red and green

Runner (host: macOS arm64):

```text
first version (EPERM read as a member listing on every OS):
  Error: "expected Timeout, got Err(GroupUnreachable { program: Git, message: \"listing its members failed: the members of a process group can be listed only through procfs\" })"
  FAIL lifecycle::a_run_returns_only_after_no_process_of_its_group_is_left
  FAIL secret::run_secret_returns_stdout_only_as_secret_bytes_and_obeys_timeout_and_cancel
final version:
  Summary [7.808s] 20 tests run: 20 passed, 0 skipped        (-E 'binary(process_runner)')
sandbox, before the change (evidence file, 2026-11-03):
  FAIL [28.888s] jackin-session::process_runner lifecycle::a_run_returns_only_after_no_process_of_its_group_is_left
  FAIL [ 1.180s] jackin-session::process_runner lifecycle::a_clean_exit_kills_a_descendant_that_closed_its_pipes
sandbox, after the change: not run (the G-FAST in a sandbox is the proof, run by the operator).
```

The first red was a real EPERM on the host: macOS answers EPERM for a group of zombies, which is what decision 1 records.

Linux code, after the review fixes: the real `host.rs` and `host/` were copied into a scratch workspace outside the repo (with the workspace lints) and checked with `clippy -D warnings` for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` and `aarch64-apple-darwin`; all three pass. The repo itself cannot cross-compile (`aws-lc-sys` needs a Linux C compiler). A deliberate type error in a Linux-only function failed the Linux check and not the macOS check, so the Linux code is compiled. `cargo zigbuild` then built a scratch binary for `aarch64-unknown-linux-gnu`, which ran on the OrbStack machine `jackin-engine` (Linux 7.2.12, no seccomp), where `kill(-pgid)` is allowed, so `by_member` was called directly:

```text
A by_member live group (3 sleeps): Empty after 2 rounds; killpg afterwards: ESRCH
B zombie-only group, killpg(0) answers Ok: by_member Empty after 1 round; sweep Empty after 1 round; killpg still Ok
C sweep of a live group, group signal allowed: Empty after 1 round; killpg afterwards: ESRCH
D leader killed and reaped, two members left: by_member Empty after 2 rounds; killpg afterwards: ESRCH
E zombie leader with a live thread (num_threads 2), a bystander whose comm is \xffx\xfe running: sweep Empty after 1 round, bystander alive, no fault
```

`stat.rs` ran against real `/proc/<pid>/stat` lines captured on that machine: the own process, a non-UTF-8 `comm`, a `comm` of `a) S 1 2 3 (b c`, a zombie leader with 2 threads (live), the same zombie after SIGKILL with 1 thread (gone), and empty, truncated and parenless lines. All verdicts matched. The EPERM trigger itself did not run on Linux.

macOS: `sweep` of the live root-owned group 90 (`logd`, EPERM for a user) answered `Signalled` in five rounds, where the first version answered `Empty`.

Clock (host), the stall is a temporary `spawn_blocking(sleep)` before the `run/state` write in `files::write_file`, removed after the run:

```text
before the fix, stall 700 ms:
  SLEEP done 500ms n=40 elapsed=20s
  Error: "the test hung for 60 s"
  FAIL [60.032s] jackin-session::launch recreate::a_sandbox_deleted_after_the_start_request_is_recreated_and_gets_its_own_start
before the fix, stall 200 ms: PASS, with 17 sleeps (8.5 s of virtual time) spent during the stall
after the fix, stall 1500 ms:
  PASS [2.262s] recreate::a_sandbox_deleted_after_the_start_request_is_recreated_and_gets_its_own_start
  PASS [0.763s] recreate::a_sandbox_deleted_after_it_was_ready_is_recreated_and_gets_its_credentials_again
after the fix, no stall: 489 tests run: 489 passed   (-p jackin-session -p jackin-testkit -p jackin-openshell)
```

CPU burners alone (54, then 354 `while :; do :; done` subshells, killed by PID) did not reproduce either flake on macOS, whose scheduler wakes pool threads fast. The stall injection reproduces the symptom of the recreate flake exactly.

## Gate

`mise exec -- mbx +1.99.1 xtask ci --fast` on `58740e2c6` (macOS arm64): `Summary [60.298s] 1187 tests run: 1187 passed, 0 skipped`, then the REUSE line `Congratulations! Your project is compliant with version 3.3 of the REUSE Specification :-)`, exit code 0. The G-FAST in a sandbox of the Architect role has not run.

## Review findings (`/private/tmp/claude-501/plans/sandbox-pgid-review.md`)

| # | Finding | Resolution |
|---|---|---|
| 1 | macOS read every EPERM as "zombies only" and returned `Empty`; a live root group also gives EPERM (`logd`, group 90). | EPERM is `Signalled` on macOS, so `gone` keeps polling and a live unsignalable group ends as `GroupFault::Alive` at the cap. The `denied` stub for macOS is gone. No better sound test exists: libproc and sysctl need `unsafe` or a new crate, and a spawned `ps` would add a tool dependency. The cost is a zombie whose parent never reaps it: on macOS it also ends as `Alive`, where launchd reaps orphans within milliseconds (review probe `f.py`). The runner reaps the leader before `gone` whenever the output pipes exist. |
| 2 | A group failure replaced `Cancelled` and `Timeout`; callers key on them (`buildx.rs`, `export.rs`, `app.rs`, `transfer.rs`). | `Cancelled` and `Timeout` carry `group: Option<GroupFault>`. A caller that matches `Cancelled { .. }` still sees the cancel with no change, and a timeout keeps `after`. Every consumer of `ProcessError` was grepped: `buildx.rs`, `export.rs`, `app.rs` use `Cancelled { .. }` and need no edit; `transfer.rs` takes the new `Group` variant; the testkit runners, `xtask/src/e2e/production.rs` and the tests construct or match the two variants and got `group: None` (list in the commit). The three unit-variant translations (`EngineError::Cancelled`, `ExportError::Cancelled`, `GatewayError::Cancelled`) and `GatewayError::Timeout` drop the group field, because those types have no field for it. The typed fault is intact at the `ProcessError` level, which is the level that holds the process group. |
| 3 | One non-UTF-8 `comm` in `/proc` aborted the sweep (`read_to_string` gives `InvalidData`). | `members` reads bytes, and `stat.rs` parses after the last `)`. Run on Linux with a bystander named `\xffx\xfe`. |
| 4 | A zombie leader with live threads was skipped as a zombie. | `Z` counts as gone only when `num_threads` is 1. Run on Linux: leader `Z`, `num_threads` 2, is a member. |
| 5 | A zombie-only group ended as `GroupAlive` after 5 s when the group signal was allowed, and as `Empty` when it gave EPERM. | One rule for both: success and EPERM both ask `/proc`, and a group of zombies is `Empty` (decision 1). Run on Linux (case B). |
| 6 | The Display table lacks the group variants; the clock guard has no test. | Test plan written; tests wait for approval. |
| 7 | The clock guard is not held by `SandboxHandle::commit_in_project` and the late-change peers. | `SandboxHandle` holds a clock handle (set by `FakeGateway::sandbox` and `sandboxes`), and `git` and `commit_in_project` hold a `Working` guard. The other gateway methods do no awaited real I/O: `create_sandbox`, `delete_sandbox` and `set_power` use `std::fs` inside one poll, `open_terminal` only spawns, and every test runtime is current-thread (no `multi_thread` in the workspace), so the clock cannot move during one poll. The peers in `tests/project/late_change.rs` are test code and were left alone: the launch they overlap waits in `lock::hold`, which polls without a limit (`lock.rs:39`), so virtual time moving during the peer's git calls cannot fail it. |
| 8 | The `/proc` scan ran on the async worker. | `gone` calls the sweep through `spawn_blocking`. `Drop` stays synchronous. The pid-reuse window is still accepted (decision 3). |

## Not proven

- The `/proc` branch against a live OpenShell kernel (the EPERM trigger). The branch body ran on Linux through `by_member`.
- `adr008` root cause.
- That a pool stall is what hit the sandbox run (the symptom and the mechanism match; the sandbox was not instrumented).
- A sandbox without a reaping init (finding 5 ran with the primitive and a plain zombie, not in a pid namespace).

## Round 3: the approved tests and three code fixes

Commits on `wt/sandbox-pgid` after `fac3877b7`: `cc8f18f8f` (fault translation), `9a197a3bb` (one group stop, unreadable stat line, the process-group tests), `5371a61c2` (clock-guard tests), and the follow-ups `8d172f92e`, `1f763613e`, `70e22a311` and `deac82e31` (test fixes found by the red runs) and `9d733b01c` (this section). Inputs: the code review, the test plan, the plan review (APPROVED WITH AMENDMENTS) and the rerun section of the P11.2 evidence. Only the amended cases were written: P1a-c, P2, U1, the parser table, H1-H4, the amended T4 rows, F1-F8 and A9-A10. A11 (`shell_exit_code_cannot_hang_the_run`) is out of scope and untouched. The Display rows of T3/A7 and the per-type Display checks of A6 were written because A6 and A7 are amendments.

### Decisions

1. Fault translation. `EngineError::Cancelled`, `ExportError::Cancelled`, `GatewayError::Cancelled` and `GatewayError::Timeout` carry `group: Option<GroupFault>`. The translations that built them from a `ProcessError` or a `GatewayError` (`transfer::process_error`, `buildx::from_process`, both `ExportError` conversions) pass the fault on. Every `Display` appends `; <fault>` through `GroupFault::note`, which goes through `plain`. `ConnectError::cancel` replaces the `is_cancelled` pattern, and the cancelled exit of the CLI now names the fault. `ProcessReply` gained `cancelled_with`, `timeout_with` and `group`. Not changed: `EngineError::Cancelled` still becomes the unit `ImageError::Cancelled` in `ensure.rs:40`, so the fault is dropped there (no review item listed it). A caller that matches `Cancelled { .. }` or `Timeout { .. }` sees no difference.
2. One group stop. The code moved from `jackin-session::host::group` to `jackin-core::process_group` as `ProcessGroup` (`of_leader`, `strike`, `sweep`, `stop`) with `Sweep`. `jackin-core` is the only crate that both `jackin-session` and `xtask` may depend on: the `layers` rows allow `xtask` to use `jackin-core` and `jackin-openshell`, and `jackin-session` has `jackin-core` as a dependency, so no row changed and the `layers` step passed. `jackin-core` already depended on `rustix` and already held `GroupFault`; its `tokio` gained the `time` feature (no new crate, no version change). `rustix` moved to the dev-dependencies of `jackin-session` and `xtask` because only their tests use it now. D14 says `jackin-core` does no I/O except config files; `ProcessGroup` (SIGKILL and `/proc` reads) is the one exception, recorded in API.md. The orchestrator owns `decisions.tsv` and should record it.
3. xtask. `RealHost::run` strikes the group, reaps the leader with `Child::kill`, then calls `ProcessGroup::stop`, the same order as `execute` in the runner; the old order waited on a leader that the sandbox never signalled. `PortableLink::drop` sweeps once. `HostError::Timeout` and `HostError::Cancelled` carry the fault and `HostError::Group` is new; `production.rs` maps them to the `ProcessError` variants. No test covers those three xtask paths (the hermetic trigger is the P1 table, and A10 allowed leaving it out if the fault was dropped; it is kept, so it is unproven).
4. Unreadable stat line. `stat::is_live_member` returns `Result<bool, Unparsable>`; `members` turns `Unparsable` into `GroupFault::Unreachable` that names the pid. ENOENT and ESRCH during the read still skip the pid. A `pgrp` beyond `i32` is unparsable too.
5. The first `cfg(test)` module in the workspace: `process_group/tests.rs`, declared `#[cfg(all(test, target_os = "linux"))]`, as A3 allowed. Reason: `by_member` runs only when the group signal answers EPERM, and no public path reaches it on a host where the group signal works. `by_member` and `members` are private; the module sees them as a child. `mod stat` was not widened. The orchestrator should record the exception.
6. H2 differs from the plan review. A `pre-commit` hook blocks inside `git commit`, and `git` holds its own guard, so deleting the guard of `commit_in_project` leaves that test green (the guard there covers the two `tokio::fs` awaits). H2 therefore blocks the write of `commit_in_project` on a FIFO, released by the test.
7. P2 and the forged name. On a host where the group signal works, the scan runs once, after the first strike, and the next poll gets ESRCH. A parser that splits at the first `)` therefore cannot fail P2 there; only the non-UTF-8 neighbour can. The forged-name neighbour is also a bystander of the U1 test (named `x) S 1 <group> (`), where `by_member` kills every parsed member, so that case is red on a Linux host. P2's forged neighbour is red only on the EPERM branch (owed).
8. P2 starts its neighbours as `sh -c 'exec "$0" 60' <symlink>` because the `containers` check rejects a computed `Command::new` program. The test waits until `comm` shows the odd name, so it cannot pass without the name.
9. Cleanup. Every spawned child is killed in a drop guard (`Held`, `Cleanup`), the git alias loops stop on a flag written on drop, the FIFO write is released on drop, and the xtask timeout test kills the background `sleep` it recorded.
10. Test environment. `git` was missing on `jackin-engine`; it was installed with `apt-get install git` (1:2.43.0-1ubuntu7.4). Linux test binaries were built on macOS with `cargo zigbuild` (target `aarch64-unknown-linux-gnu`, `CARGO_ZIGBUILD_ZIG_PATH` set) and run on `jackin-engine` through `orb -m jackin-engine <binary>` from the crate directory. Breaks ran in copies of the worktree, never in the worktree.

### Red and green

Each row: the named break (a one-place edit to production code or to the probe, reverted after the run), the red line of the first failing assertion, and where green was seen. Green: macOS rows pass in the final `xtask ci --fast` run; Linux rows pass on `jackin-engine` (aarch64, kernel 7.2.12) from the same sources. "Red (mac)" ran on macOS arm64, "Red (linux)" on `jackin-engine`.

| Test | Break | Red line |
|---|---|---|
| `a_stat_line_is_a_live_member_only_of_its_own_group_...` (linux) | `PGRP` 2 to 1 | `tests.rs:50 a sleeping member: left Ok(false), right Ok(true)` |
| same | `THREADS` 17 to 18 | `tests.rs:77 a zombie with one thread: left Ok(true), right Ok(false)` |
| same | `b'Z'` always over | `tests.rs:83 a zombie leader with a running thread: left Ok(false), right Ok(true)` |
| same | `b'Z'` never over | `tests.rs:78 a zombie with one thread: left Ok(true), right Ok(false)` |
| same | the `b'X'` arm removed | `tests.rs:87 a dead process: left Ok(true), right Ok(false)` |
| same | `rposition` to `position` | `tests.rs:93 a comm that is only parentheses: left Ok(false), right Ok(true)` |
| `a_stat_line_that_cannot_be_read_is_a_fault_...` (linux) | `ok_or(Unparsable)?` to `Ok(false)` | `tests.rs:108 cut before the group: left Ok(false), right Err(Unparsable)` |
| same | `pgrp` parsed as `u64` and cast to `i32` | `tests.rs:109 a group beyond i32: left Ok(true), right Err(Unparsable)` |
| `by_member_ends_every_live_member_...` and `..._the_orphans_...` (linux) | `by_member` sends `SIGCONT` | `the group empties: "group 14320 still had a live member after 30 s"` (both tests) |
| `by_member_ends_every_live_member_...` (linux) | `rposition` to `position` | `tests.rs:208 three members before: left 4, right 3` (the forged bystander counts as a member) |
| P1a-c (mac) | `with_group` returns `Group` for `Timeout` and `Cancelled` | `expected Cancelled, got Err(Group { program: Git, fault: Alive { after: 5s } })`; the same for `Timeout` |
| P1b-c (mac) | `execute` drops the `gone` result | `group.rs:159 left: None, right: Some(Alive { after: 5s })` |
| P1a (mac) | the clean-exit path drops the fault | `expected Some(Alive { after: 5s }) as the fault, got Ok(ProcessOutput { status: Exited(0), ... })` |
| P1a-c (linux) | `survivors` always `Signalled` | `expected None as the fault, got Err(Group { program: Git, fault: Alive { after: 5s } })`; `group.rs:159 left: Some(Alive { after: 5s }), right: None` |
| P1a-c (linux) | a zombie counts as live | `group.rs:159 left: Some(Alive { after: 5s }), right: None` (the three P1 tests fail, P2 passes) |
| P2 non-UTF-8 name (linux) | `members` reads with `read_to_string` | `Error: Group { program: Git, fault: Unreachable { message: "reading the state of process 14586 failed: stream did not contain valid UTF-8" } }` |
| P2 forged name (linux) | `rposition` to `position` | none: green on this host, see decision 7. Owed in the sandbox rerun. |
| A9 `stdin_is_dev_null_...` (mac, linux) | `Null` stdin as a pipe | `Null stdin: left "other\n", right "devnull\n"` |
| same (mac) | the probe always prints `devnull` | `Bytes stdin is not /dev/null: left "devnull\ngiven", right "other\ngiven"` |
| same (mac, linux) | none of the three paths exists | `Null stdin: left "unobservable\n", right "devnull\n"` |
| Display rows, `display.rs` and `error_text.rs` (mac) | `GroupFault::note` returns an empty string | `left "git was cancelled", right "git was cancelled; processes of its group were still alive after 5s"` |
| same | `plain` removed from `Unreachable` | `left "...x\ny\u{1b}[31m", right "...x\\ny\\u{1b}[31m"` |
| F1 `a_group_fault_rides_through_each_transfer_...` (mac) | `process_error` drops the timeout fault | `transfer.rs:958 UploadFile timeout fault: left None, right Some(Alive { after: 5s })` |
| F2 same | drops the cancel fault | `transfer.rs:972 UploadFile cancel fault: left None, right Some(Unreachable { message: "cannot list /proc: x\ny\u{1b}[0m" })` |
| F3 same | invents a fault when there is none | `UploadFile: Timeout { op: UploadFile, after: 9s, group: Some(Alive { after: 0ns }) }` |
| F3 same | a `Group` error becomes `Cancelled` | `UploadFile: Cancelled { op: UploadFile, group: None }` |
| F4 `buildx_find_reads_...` (mac) | `from_process` drops the cancel fault | `buildx.rs:702 cancelled with a fault: left None, right Some(Alive { after: 5s })` |
| F4 same | a `Timeout` becomes `Cancelled` | `Error: "timeout: got Err(Cancelled { group: None })"` |
| F4 same | a `Group` error becomes `Cancelled` | `Error: "group fault: got Err(Cancelled { group: None })"` |
| F5 `a_host_git_call_cancelled_with_a_group_fault_...` (mac) | the `ProcessError` conversion drops the fault | `abort.rs:223 index-pack: left "cancelled", right "cancelled; processes of its group were still alive after 5s"` |
| F5 same | `ExportError::Cancelled` prints without the fault | the same line |
| F6 `a_gateway_cancel_or_timeout_with_a_group_fault_...` (mac) | the `GatewayError` conversion drops the fault | `abort.rs:265 left: None, right: Some(Alive { after: 5s })` |
| F7 same | a `Timeout` passes through without its fault | `abort.rs:280 left: None, right: Some(Alive { after: 5s })` |
| F8 `a_cancel_whose_group_stop_failed_...` (mac) | `cancel()` hides a cancel that has a fault | `connect.rs:90 cannot discover gateway fake-gateway` |
| F8 same | the exit text drops the fault | `connect.rs:95 the fault is lost: cancelled while connecting to the gateway` |
| T4 `a_guard_taken_one_poll_before_a_sleep_wakes_...` (mac) | `quiet &&` removed from the completion check | `host.rs:407 woke under a guard: left Some(1), right None` |
| T4 `a_sleep_waits_for_the_last_of_two_guards`, `a_sleep_begun_under_a_guard_...` (mac) | no guard check at all | `host.rs:451 woke under two guards: left Some(65), right None` |
| T4 `a_sleep_waits_for_the_last_of_two_guards` (mac) | the counter is a flag | `host.rs:457 woke under one guard: left Some(65), right None` |
| T4 `a_sleep_blocked_by_a_guard_does_not_wake_itself_...` (mac) | `until_quiet` not awaited (busy spin) | `host.rs:481 a blocked sleep woke its own task: left 1, right 0` |
| H1 (mac) | the guard removed from `SandboxHandle::git` | `clock.rs:142 the clock moved while a git call was in flight` (H1 and H4) |
| H2 (mac) | the guard removed from `commit_in_project` | `clock.rs:210 the clock moved while the commit was in flight` |
| H1 (mac) | `sandbox()` passes no clock | `clock.rs:142 the clock moved while a git call was in flight` |
| H4 (mac) | `sandboxes()` passes no clock | the same line, in the `sandboxes` test only |
| stalled `run_program` row (mac) | the guard removed from `run_program` | `clock.rs:303 linked: true; the sleeper was held: false` |
| A10 `real_host_kills_the_whole_process_group_on_timeout` (mac) | `stop_group` does not strike or stop | `e_scratch.rs:286 the background process of the group is gone` |
| same | `stop_group` waits for the leader and never kills | `e_scratch.rs:259 the run did not return: "the test hung for 15 s"` |
| A10 `dropping_a_link_kills_the_child_and_its_group` (mac) | `PortableLink::kill` does not sweep | `f_pty.rs:188 the grandchild is gone` |

H3 (an unlinked gateway does not hold the clock) is the negative side of H1, H2 and H4: it fails if the guard is added where no clock is linked, and no separate break was run for it.

### Owed in the sandbox rerun

The group signal works on macOS and on `jackin-engine`, so these cannot show red or green here. They need the OpenShell sandbox, on the final SHA of this branch:

- P1a-c on the EPERM branch (`by_member` through the runner) and the forged-name neighbour of P2 (red only there).
- `f_pty::dropping_a_link_kills_the_child_and_its_group` and `e_scratch::real_host_kills_the_whole_process_group_on_timeout` (A10). The recorded sandbox failures are the red; the green is a sandbox run. A10 also says to change the "is it gone" oracle (`kill(pid, 0)` succeeds for a zombie) only if a sandbox run still fails and `ps` shows `Z`.
- `stdin_is_dev_null_or_the_given_bytes_followed_by_end_of_input` through `/proc/self/fd/0` (the host had `/dev/stdin`; the sandbox path was not exercised).

### Not proven, and found on the way

- The three xtask paths of decision 3 and the six-method leak check of A4 (that each guarded fake method returns its guard) have no test. The stalled `run_program` row is the representative of the six.
- `lifecycle::a_run_returns_only_after_no_process_of_its_group_is_left` fails on `jackin-engine` as a plain binary in 5 of 6 runs on the base commit `fac3877b7` too (`exit, round N`; the oracle `group_is_empty` sees a live group after `run` returned). A retry of the oracle for 1 s did not remove it (13 of 20 runs failed), and a later `ps` showed no zombie, so the cause is not found. It passed in the sandbox rerun. Not touched here.
- Run directly (not through nextest) on `jackin-engine`, `env::the_child_gets_the_spec_environment_...` fails by design ("run the test under cargo nextest") and `streaming::a_128_mib_line_does_not_grow_process_memory` reports a peak of 68144 KiB against a 48 MiB budget. Not investigated.
- `ImageError::Cancelled` still drops the fault (decision 1). Closed in round 4.

## Round 4: the group stop leaves jackin-core, and `ImageError::Cancelled` carries its fault

Commits on `wt/sandbox-pgid` after `44bdfcd40`: `ed4a8f6b1` (the move), `ee8dfb9ab`, `25305212b` (the layers fixture test) (the image fault and its test). Round 3 decision 2 put `ProcessGroup` in `jackin-core`, which breaks D14 ("No I/O except config files"; `jackin-session` owns the host process runner). Round 4 reverses it.

### Decisions

1. Layering. `ProcessGroup`, `Sweep`, the `/proc` stat parser and the Linux-only test module moved with `git mv` to `crates/jackin-session/src/process_group.rs` and `process_group/{stat,tests}.rs`. The tests are unchanged. `jackin-session` exports `ProcessGroup` and `Sweep` (`pub use process_group::*`); `host.rs` uses the local module. `GroupFault` stays in `jackin-core`: `ProcessError` holds it and it does no I/O.
2. The `layers` table. The only change is the `jackin-xtask` row: `depends` goes from `["jackin-core", "jackin-openshell"]` to `["jackin-core", "jackin-openshell", "jackin-session"]`. `xtask` is a top-level tooling binary and nothing sits above it, so the edge cannot make a cycle (`jackin-session` does not depend on `xtask`; `xtask` dev-depends on `jackin-testkit`, which depends on `jackin-session`, and that was allowed already). Every other row, the import rules, the HTTP-client rule and the `Secret::expose` allowlist are unchanged, and the `layers` step passed.
3. Dependencies. `jackin-core`: the `tokio` `time` feature is gone and the dev-dependency `tokio` features are back to `["macros"]` (both were added in round 3 for the group stop only); `rustix` stays, `fs.rs` uses it. `jackin-session`: `rustix` moved from dev-dependencies to dependencies. `jackin-xtask`: new dependency `jackin-session` (workspace entry); `Cargo.lock` gains that one line. `cargo shear --deny-warnings` runs in the `--fast` gate and passed.
4. `ImageError::Cancelled` is `Cancelled { group: Option<GroupFault> }`, with `Display` `cancelled` plus `GroupFault::note`. `from_engine` passes the fault from `EngineError::Cancelled`. The `live()` check of a cancelled token has no process, so it gives `group: None`. The exit-class match in `crates/jackin/src/error.rs` and the existing image tests use `Cancelled { .. }`.

### Audit of `Cancelled` and `Timeout` variants

Every `Cancelled` and `Timeout` construction and match in non-test code was listed with `grep -rnE "(ProcessError|GatewayError|EngineError|ExportError|ImageError|HostError)::(Cancelled|Timeout)" crates` and read. None of the conversions between the wrapped types drops `group`, except where the source has no process group:

| Site | Result |
|---|---|
| `jackin-core/src/boundary.rs` `ProcessError::{Timeout,Cancelled}` | carry `group` |
| `jackin-session/src/host.rs` `with_group` | sets `group: Some(fault)` on both; other variants become `Group` |
| `jackin-openshell/src/transfer.rs:75-76` `process_error` | passes `group` into `GatewayError::{Timeout,Cancelled}` |
| `jackin-image/src/buildx.rs:64` `from_process` | passes `group` into `EngineError::Cancelled`; `Timeout` stays `Process(Timeout{group})` |
| `jackin-image/src/ensure.rs:40` `from_engine` | was the dropped fault; now passes `group` (this round) |
| `jackin-session/src/export.rs:208, 217` | `GatewayError::Cancelled` and `ProcessError::Cancelled` pass `group`; a `Timeout` keeps the whole error in `ExportError::Gateway` or `Process` |
| `jackin/src/app.rs:52-55` `ConnectError::cancel` | reads `group` from both sources |
| `jackin/src/error.rs:129-131` `class` | classifies only; the text comes from `Display`, which carries the note |
| `xtask/src/e2e/host.rs:288-294` and `production.rs:107-112` | `HostError::{Timeout,Cancelled}` carry `group` and map to `ProcessError` with it |
| `jackin-testkit` runner, gateway, `engine.rs` | `ProcessReply` bodies carry `group`; the other constructors have no group stop |

No process, so `group: None` is correct: `GatewayError::Timeout` and `Cancelled` built from the gRPC deadline, token or status (`grpc.rs:86`, `grpc/status.rs:52`, `grpc/shell.rs:87`, `grpc/main_pty.rs:259`, `grpc/program.rs:281,284`), `attach.rs:455,503`, `project.rs:213` (`LockError::Cancelled`), `launch/run.rs:583` (an upload timer), `ExportError::Cancelled` at `export.rs:404` and `export/bundle.rs:154,456`, `EngineError::Cancelled` at `buildx.rs:340`, and the host runner's own `cancel.cancelled()` and `timer` arms (`host.rs:422,453,454`, before the group stop runs). Not wrapping a process error: `LockError::Cancelled`, `PollError::{Cancelled,Timeout}`, `UsageError::Cancelled`, `PtyError::Timeout`, `LaunchError::{ReadyTimeout,SupervisorTimeout,AttemptTimeout}`, `EngineError::LockTimeout`, `ImageError::LockTimeout`, `LaunchOutcome::Cancelled`, `CommandError::Cancelled(String)` (its text from `ConnectError` already holds the note).

### Red and green

The F9 row follows the F1-F8 pattern: a scripted `docker image inspect` reply, the public `ensure_image`, the resulting error. `a_cancel_whose_group_stop_failed_reaches_the_image_error_with_its_fault` (`crates/jackin-image/tests/image_io/ensure.rs`) has three rows: a plain cancel gives `Cancelled { group: None }` and the text `cancelled` (negative control); a cancel with `GroupFault::Alive` gives `Cancelled { group: Some(Alive) }` and the text with the note; a timeout with a fault gives `Engine(Process(Timeout { group: Some(Alive) }))`.

| Test | Break | Red line |
|---|---|---|
| F9 (mac) | `from_engine` returns `Cancelled { group: None }` | `ensure.rs:939 cancel with a fault: left None, right Some(Alive { after: 5s })` |
| F9 (mac) | `Display` without `GroupFault::note` | `ensure.rs:940 cancel with a fault: left Some("cancelled"), right Some("cancelled; processes of its group were still alive after 5s")` |
| `layers::edges_outside_the_row_are_reported` (mac) | none; the fixture named `jackin-xtask -> jackin-session` as its forbidden edge, which the new row allows | `repo_gates.rs:705 left` has no `crates/jackin-xtask/Cargo.toml` entry, `right` has one (first gate run, 1 failed of 1205) |
| same, after the fixture uses `jackin-xtask -> jackin-tui` | the `jackin-xtask` row also lists `jackin-tui` | `repo_gates.rs:705 left` has no `crates/jackin-xtask/Cargo.toml` entry, `right` has one |
| `a_stat_line_is_a_live_member_...` (linux, moved) | `b'Z' => self.threads < 1000` | `tests.rs:83 a zombie leader with a running thread: left Ok(false), right Ok(true)` |

The first layers row is the red of the old fixture against the new row; the second proves the fixture still catches a forbidden xtask edge. The break edits ran in the worktree and were restored from a copy before the commit (`git diff` of the file was empty after each). Green after the move: macOS in the gate run below; Linux on `jackin-engine` (aarch64, kernel 7.2.12), binaries built with `cargo zigbuild` as in round 3:

```text
jackin_session lib (process_group::tests): 4 passed
process_runner: 21 passed, 3 failed (the same three as round 3: env (needs nextest), lifecycle::a_run_returns_only_after_no_process_of_its_group_is_left (oracle flake, open), streaming::a_128_mib_line_does_not_grow_process_memory (68 to 80 MiB peak))
  group::* (P1a-c, P2 hostile neighbours), stdin::stdin_is_dev_null_..., lifecycle::cancel_kills_..., a_timeout_kills_...: ok
xtask e2e (--test-threads=1): e_scratch::real_host_kills_the_whole_process_group_on_timeout ok, f_pty::dropping_a_link_kills_the_child_and_its_group ok
```

The rows of round 3 stay valid: the tests moved unchanged and the Linux run above shows them live (the stat break is red at the new path).

### Gate

`mise exec -- mbx +1.99.1 xtask ci --fast` (macOS arm64, this section in the tree, code at `25305212b`): `Summary [72.237s] 1205 tests run: 1205 passed, 0 skipped`, then the REUSE line `Congratulations! Your project is compliant with version 3.3 of the REUSE Specification :-)`, exit code 0. The first run on `ee8dfb9ab` failed one test, `layers::edges_outside_the_row_are_reported` (1202 of 1203 run passed; see the layers rows above), which `25305212b` fixes. `cargo shear --deny-warnings` is a step of this gate and passed. The G-FAST in a sandbox of the Architect role has not run; the owed rows of round 3 are unchanged.

## Round 5: the test gaps of the independent red/green

Commits on `wt/sandbox-pgid` after `ca995c11f`: `ed3b2a22a` (U1), `089ae4449` (the lifecycle oracle), `3a4a19c9f` (the FakeClock reset), `ebb975956` (the exec sleepers), `cd2202524` (the xtask group fault) and `09fd55e60` (API.md); this section follows them. Inputs: the red/green report `/private/tmp/claude-501/plans/sandbox-pgid-redgreen.md` (CHANGES_REQUIRED; breaks L2, X1-X3 and M11b survived), the plan amendments A4 and A10, and the accepted oracle patch `/private/tmp/claude-501/plans/lifecycle-fix.patch`, applied with `git apply` and unchanged.

### Decisions

1. U1. `emptied` in `process_group/tests.rs` now asserts that the first `by_member` call returns `Signalled` before it polls for `Empty`. Both tests checked, just before, that the group had live members, so the first call must signal them. The poll loop still ends only on `Empty`, so a `by_member` that never returns `Empty` still fails by timeout.
2. xtask fault plumbing (A10). `ToolRunner` in `production.rs` is public, with `ToolRunner::new(host, env)`, and `e2e.rs` re-exports it. It was reachable only through `production()`, which builds `jackin` and resolves a gateway. The tests drive it with a scripted `HostProcess`, a fake at the process boundary. `production()` builds the runner through `new`. No other change in `src`.
3. xtask zombie rows. The test holds a zombie in the group, as the P1 table does (`sh -c 'exit 0'` joined to the group, not reaped, checked with `waitid(NOWAIT)`), and expects `None` on Linux and `Alive { after: 5s }` elsewhere. Three rows: the output cap, the timeout and the cancel. A clean exit of `RealHost::run` never calls `stop_group`, so unlike P1a there is no clean-exit row; the `HostError::Group` path is reached only after a read error or the output cap, and the cap row is the trigger. A fourth test checks the `ToolRunner` mapping for `Timeout`, `Cancelled` and `Group`, each with and without a fault.
4. FakeClock `yields = 0` reset (A4). A deterministic test exists, so it is pinned and not left unproven. Poll a sleep until 20 polls before it wakes, take a guard, poll 100 times (still pending), drop the guard, and expect the wake at exactly `wake_at` polls, the same count as a fresh sleep. Without the reset the sleep wakes after 20 polls. The six-method leak check of A4 is still not written (see Not proven).
5. Lifecycle oracle. The patch gives the Linux oracle its own `/proc/<pid>/stat` parser that treats `Z` and `X` as gone and keeps the signal probe elsewhere. Known cost, from the red/green report: a run that returns a moment early while the killed members are still zombies is no longer caught by this oracle (break 2 below). The strong case, a runner that never kills the group, is still caught on every run.
6. Leaked processes. `exec.rs` runs `sh -c "sleep 300"` through the fake exec backend of `exec_rpc.rs`, which kills the shell it started and nothing under it. On macOS only `collected_output_is_capped_per_stream` reproduced the leak (`head ...; head ...; sleep 300` forks the sleep, the backend kills the shell, the sleep is orphaned): two `sleep 300` per run. `program_timeout_and_cancel_stop_the_remote_process` did not leak, because `sh` execs a lone command. All three now start `exec sleep 300`, so the child the backend holds is the sleep itself, and `child.kill()` or `kill_on_drop` ends and reaps it on a pass and on a failure. A group kill in the backend would cover every `host_shell` test, but `jackin-openshell` has no `rustix` dev-dependency, and adding one was not in the brief.
7. API.md. The xtask section (line 3092) said it depends on no product crate and named an `e2e` cargo feature, `jackin-usage` and `schemars`. `crates/xtask/Cargo.toml` has no features, no `jackin-usage`, no `schemars`, and depends on `jackin-core`, `jackin-openshell` and `jackin-session`. The text now says that.
8. Test environment as in round 3: Linux binaries from `cargo-zigbuild` (target `aarch64-unknown-linux-gnu`, `CARGO_ZIGBUILD_ZIG_PATH` set, run through `mise exec` with the binary of the mise install), run on `jackin-engine` with `orb -m jackin-engine <binary>` from the crate directory. Breaks ran in the worktree through a script that restores the file from a copy and compares it (`cmp`) afterwards; the commits hold no break.
9. Forged-name fixture. The kernel cuts `comm` to 15 bytes. The forged name `x) S 1 <pgid> (` was 17 bytes with a 7-digit group id, so the string never appeared in `/proc/<pid>/stat` and the test failed on a clean tree. The trailing ` (` is dropped in `process_group/tests.rs` and in `tests/process_runner/group.rs`; nothing else changes. With a 7-digit pgid the name is exactly 15 bytes. Swapping `rposition` for `position` in `stat.rs` still turns the unit rows red (see the table). The integration row `group::hostile_neighbours::...` stayed green under that swap in one Linux run, so the swap is caught by the unit rows only.
10. Positive control for the lifecycle oracle. `cancel_once_the_group_is_known` in `lifecycle.rs` asserts `!group_is_empty(scratch)?` once the script has written `pgid` and before it cancels, so the oracle must see the live group. A wrong field index in the oracle now fails the test at that line instead of passing silently, where before it could pass without ever seeing a live member.
11. API.md row 13. The seam of `xtask ci` that faked the repository's own tools was named `ToolRunner`. That name now belongs to the public `ToolRunner` of `crates/xtask/src/e2e/production.rs`, the `ProcessRunner` of the e2e runner built with `ToolRunner::new(host, env)` over `HostProcess`, which is a different type. The row says so.

### Red and green

Red lines are the first failing assertion of the run. Green: macOS rows pass in the gate run below; Linux rows were run on `jackin-engine`.

| Test | Break | Red line |
|---|---|---|
| `by_member_ends_every_live_member_...` and `..._the_orphans_...` (linux) (L2) | `by_member` returns `Sweep::Empty` after the kills | `tests.rs:177 the first sweep, while the group has live members, reports that it signalled them: left Ok(Empty), right Ok(Signalled)` (both tests) |
| `a_failed_group_stop_after_too_much_output_is_a_group_error_and_never_swallowed` (mac) (X1) | `RealHost::run` drops the fault of a failed group stop and returns the read error | `e_scratch.rs:473 expected Some(Alive { after: 5s }) as the fault, got Err(OutputTooLarge { program: "/bin/sh", stream: "stdout", limit: 1048576 })` |
| `a_timeout_that_leaves_only_a_zombie_...` (mac) (X2a) | `HostError::Timeout` built with `group: None` | `e_scratch.rs:488 left: None, right: Some(Alive { after: 5s })` |
| `a_cancel_that_leaves_only_a_zombie_...` (mac) (X2b) | `HostError::Cancelled` built with `group: None` | `e_scratch.rs:500 left: None, right: Some(Alive { after: 5s })` |
| `a_tool_run_hands_the_group_fault_of_the_host_to_the_process_error` (mac) (X3a) | `production.rs` maps `Timeout` with `group: None` | `e_scratch.rs:563 a timeout keeps its fault: left None, right Some(Alive { after: 5s })` |
| same (X3b) | `production.rs` maps `Cancelled` with `group: None` | `e_scratch.rs:587 a cancel keeps its fault: left None, right Some(Unreachable { message: "cannot list /proc" })` |
| same (X3c) | the `HostError::Group` arm is removed | `e_scratch.rs:612 expected Group, got Err(Spawn { program: Git, message: "git: processes of its group were still alive after 5s" })` |
| the three zombie rows (linux) | `b'Z' => self.threads > 1000` in `stat.rs` (a zombie counts as live) | `expected None as the fault, got Err(Group { program: "/bin/sh", fault: Alive { after: 5s } })`; the timeout and cancel rows `left: Some(Alive { after: 5s }), right: None` |
| `a_sleep_interrupted_by_a_guard_counts_its_quiet_yields_again_from_zero` (mac) (M11b) | drop `if !quiet { yields = 0 }` from `FakeClock::sleep` | `host.rs:487 the quiet yields before the guard still counted: left Some(20), right Some(65)`; the other four guard tests stay green |
| `lifecycle::a_run_returns_only_after_no_process_of_its_group_is_left` (linux), oracle | the unpatched oracle (`git apply -R`) | 27 of 40 runs red: `lifecycle.rs:193:5 exit, round 6`; with the patch 40 of 40 pass |
| same, runner (linux) | `strike` does nothing and `stop` returns `Ok(())` at once | `lifecycle.rs:239:5 exit, round 0` (patched oracle) |
| `a_stat_line_is_a_live_member_only_of_its_own_group_...` and `by_member_ends_every_live_member_of_a_group_it_does_not_leave_...` (linux), forged name | `rposition` replaced by `position` in `process_group/stat.rs` | `tests.rs:94 a comm that is only parentheses: left Ok(false), right Ok(true)`; `tests.rs:213 three members before: left 4, right 3` |
| `lifecycle::a_run_returns_only_after_no_process_of_its_group_is_left` (linux), positive control | the oracle reads the wrong field (`.get(2)` to `.get(3)` in `group_is_empty`) | `lifecycle.rs:250:5 the oracle sees no member of a group that is still running` |
| `collected_output_is_capped_per_stream` (mac), leak | the base file (`sleep 300` without `exec`) | two `sleep 300` left per run, parent 1 (pids 90855 and 91192 in the two-test run, 54332 and 54344 in the single-test run, all killed by pid); `ps` after the change: none, over 8 runs of both tests |
| same, failure path | the assertion `limit == CAP` made `CAP + 1`, with the change in place | the test fails 3 of 3 and `ps` shows no new `sleep 300` |

Linux greens after the three small fixes (binaries from `cargo-zigbuild`, run on `jackin-engine`): `process_group::tests` 4 of 4; `process_runner group::` 4 of 4; `process_runner lifecycle::` 4 of 4. Earlier greens: `process_group::tests` 4 of 4; `e_scratch::` and `f_pty::` 16 of 16 (`--test-threads=1`); `lifecycle` 40 of 40. `ps` on `jackin-engine` after each run showed no test process; the 8 loop subshells of the broken-runner row were killed by their group id (846851).

### Owed in the sandbox rerun

Unchanged from round 3, plus the new xtask rows and the patched lifecycle oracle (`lifecycle::a_run_returns_only_after_no_process_of_its_group_is_left`). On the EPERM branch a zombie is not live, so the zombie rows expect the Linux answer there.

### Not proven

- The six-method leak check of A4: that each guarded fake method returns its guard. The stalled `run_program` row stands for the six.
- The leak fix has no assertion. A leaked sleeper shows only in `ps`; no test fails on it.
- Two `sleep 300` processes (pids 42691 and 42987, parent 1, started 17:08:47) were on the host before my runs of the exec tests and are not mine; they were left alone.
- The `streaming::a_128_mib_line_does_not_grow_process_memory` failure of a plain Linux run is explained in the red/green report: `resident_kib()` reads the RSS of the whole test binary, which holds every test as a thread when the binary runs directly; it fails the same way on the base commit and passes under nextest.
- The Linux-only code of this round (the oracle, `by_member` test) was compiled and run on `jackin-engine`; clippy ran on macOS only, because the Linux target needs a C cross compiler for `aws-lc-sys`.

### Gate

`mise exec -- mbx +1.99.1 xtask ci --fast` (macOS arm64, this section in the tree, code at `cd2202524`): `Summary [76.987s] 1210 tests run: 1210 passed, 0 skipped`, then the REUSE line `Congratulations! Your project is compliant with version 3.3 of the REUSE Specification :-)`, exit code 0. `ps` after the run: no `sleep` that began after the gate started. Two `sleep 300` (pids 96790 and 97297, parent 1, started 17:22:46, cwd in a `jackin-openshell` test `TempDir`) began 85 s before the gate and are not from my runs (inferred: another session running the base `exec.rs`); they were left alone. The G-FAST in a sandbox has not run; the owed rows are listed above.

With the three small fixes (decisions 9 to 11) in the tree: `Summary [57.380s] 1210 tests run: 1210 passed, 0 skipped`, then the REUSE compliance line, exit code 0.

## Round 6: the forged-name fixture without `/bin/sleep`

Branch `wt/uutils-fixture` from `79c974eee`. CI runs 41622876122 and 41624781035 (`ubuntu-26.04`) failed `process_group::tests::by_member_ends_every_live_member_of_a_group_it_does_not_leave_to_the_group_signal` at `tests.rs:226`: `/bin/sleep` there is a uutils multicall binary, which refuses to run under a link name that is not a utility name, so the bystander exited and `try_wait` returned `Some`.

### Decisions

1. Fixture. The link `x) S 1 <pgid> ` and the non-UTF-8 link now point to `/bin/sh`, and the bystander runs `exec "$0" -c 'read _'`. The kernel takes `comm` from the name passed to `execve`, so the shell runs under the forged name. A shell does not look at its own name, so the coreutils in use make no difference, and the bystander does not exec another program. No file is written, so there is no ETXTBSY window (the link target is the system's `/bin/sh`). I considered a `#!/bin/sh` script file; it needs a written, executable file and gives the same `comm`, so the symlink to the interpreter is the smaller change.
2. The bystander blocks on its stdin, which is now a pipe the test holds. It ends by itself when the test drops the pipe, so a test that dies leaves no `sleep 60` behind.
3. `Held::drop` in `tests/process_runner/group.rs` closes the stdin pipe before `kill` and `wait`. Reason, from a live OpenShell 0.1.6 sandbox: `hostile_neighbours::a_run_ends_ok_and_spares_neighbours_whose_names_try_to_confuse_the_scan` always took 60.0 s. The non-UTF-8 neighbour cannot be killed there: `kill(pid, SIGKILL)` on a process whose `comm` is not valid UTF-8 answers EPERM (`kill -9` from a shell: `Operation not permitted`; a valid non-ASCII name, `é`, and the forged ASCII name are killed normally). The test thread sat in `do_wait` inside `Held::drop` until `sleep 60` ended; `SigPnd` of the neighbour was 0. The cause is inferred, not read from a trace: `mediate_process_signal` (`openshell-sandbox/src/linux/process_signal.rs`) resolves the target with `std::fs::read_to_string("/proc/<tid>/status")`, which fails on a `Name:` line with invalid UTF-8. The fault is in OpenShell, not in the runner or the fixture. The runner's sweep did not wait on the neighbour (the forged neighbour was killed and reaped by the test within 2 s).
4. Product note, not fixed here: in that sandbox, `by_member` on a group that holds a member whose `comm` is not valid UTF-8 gets EPERM from `kill_process` and returns `GroupFault::Unreachable`. That is the honest answer for a process the sandbox does not let the runner signal.

### Red and green

Linux binaries from `cargo zigbuild` (`aarch64-unknown-linux-gnu`), run on `jackin-engine` (Ubuntu 24.04) and in a live OpenShell 0.1.6 sandbox (Ubuntu 24.04). uutils: `rust-coreutils` 0.0.24 from apt, with `/usr/bin/coreutils` bind-mounted over `/usr/bin/sleep` (unmounted afterwards; `/bin/sleep` is the 67944-byte GNU file again). The 0.0.24 multicall binary answers `function/utility not found` for the forged name, where the CI image's answers `Security violation`; both exit at once.

| Run | Old fixture | New fixture |
|---|---|---|
| `jackin-engine`, GNU | `process_group` 4 of 4, `group::` 4 of 4 | `process_group` 4 of 4, `group::` 4 of 4 |
| `jackin-engine`, GNU, `kill(-pgid)` answers EPERM (seccomp filter through `prctl`) | hostile row ok, 0.97 s | hostile row ok, 0.97 s; `process_group` 4 of 4 |
| `jackin-engine`, uutils as `/bin/sleep` | `tests.rs:226:5 a process of another group is untouched` (3 of 4); `group.rs:233:17 the non-UTF-8 name neighbour was killed` | `process_group` 4 of 4, `group::` 4 of 4 |
| live OpenShell sandbox, GNU | hostile row ok after 60.04 s | `group::` 4 of 4 in 3.93 s; `process_group` 4 of 4 |

`rposition` to `position` in `stat.rs`, new fixture, GNU: `tests.rs:94:5 a comm that is only parentheses: left Ok(false), right Ok(true)` and `tests.rs:214:5 three members before: left 4, right 3`. The integration row stays green under that break, as decision 9 of round 5 says. The file was restored from a copy and compared with `cmp`; the commit holds no break.

The `by_member` unit rows ran in a live sandbox for the first time and passed.

### Not proven

- The CI image `ubuntu-26.04` itself. The red is reproduced with uutils 0.0.24 on Ubuntu 24.04, whose refusal text differs.
- The OpenShell cause in decision 3 is inferred from the symptom matrix and the source, not from a trace of the supervisor.
