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
