# P11.2 sandbox process groups and launch flakes

Source: `/private/tmp/claude-501/plans/P11.2-evidence.md` (G-FAST of the Architect role fails in an OpenShell 0.1.6 sandbox). Base: `dbb35f2f7` on `wt/sandbox-pgid`.

## Root cause 1: the runner waits on a signal the sandbox denies

`TokioProcessRunner` stops a run by sending SIGKILL to the child's process group and repeating the group signal until `kill(-pgid)` answers ESRCH (`crates/jackin-session/src/host.rs`, `gone`, commit 2ef1055f). OpenShell v0.1.6 filters `kill` in the workload seccomp program: `crates/openshell-sandbox/src/linux/child_seccomp.rs` denies a negative or zero pid with `append_argument_masked_deny(SYS_kill, 0, 1 << 31)` and `append_argument_equal_deny(SYS_kill, 0, 0)`, and `process_signal.rs:24` states "Group/broadcast signaling remains deliberately denied". The loop saw EPERM, not ESRCH, and ran into its 5 s cap, which returned silently. The two tests `lifecycle::a_clean_exit_kills_a_descendant_that_closed_its_pipes` and `lifecycle::a_run_returns_only_after_no_process_of_its_group_is_left` failed in the sandbox for that reason.

Structure that allowed it: the old loop read any error of `kill_process_group` as "the group is gone" (`while kill_process_group(..).is_ok()`), so EPERM was one more silent answer, and the cap ended the wait without a result. The contract "when `run` returns, no process of the group is left" had no error to report a breach.

Per-pid signals are allowed. `mediate_process_signal` accepts a positive target, refuses only the sandbox leader's own thread group, and delivers through a retained pidfd. `pidfd_open` and `pidfd_send_signal` are denied to the workload (`child_seccomp.rs:201` returns ENOSYS), so the runner uses plain `kill(pid, SIGKILL)`. `/proc` is in the default read-only filesystem policy (`openshell-policy/src/lib.rs:981`).

### Fix

| File | Change |
|---|---|
| `crates/jackin-session/src/host/group.rs` (new) | `sweep(group)` sends SIGKILL to the group. ESRCH is `Empty`. EPERM on Linux lists the members from `/proc/<pid>/stat` (field 5, `pgrp`, read after the last `)` because the command name may hold spaces and parentheses; zombies `Z` and `X` are skipped) and sends SIGKILL to each pid. EPERM on macOS is `Empty`. Any other errno is a fault. |
| `crates/jackin-session/src/host.rs` | `gone` repeats `sweep` every 2 ms until `Empty`, within the unchanged 5 s cap. `GroupGuard::fire` and `Drop` use the same `sweep`, so a dropped run also stops the group by member. `gone` is awaited with `?` on the success path, the error path and the missing-pipes path, so its error replaces `Timeout` or `Cancelled`. |
| `crates/jackin-core/src/boundary.rs` | `ProcessError::GroupAlive { program, after }` for a cap that expired with members alive, and `ProcessError::GroupUnreachable { program, message }` for a group that cannot be signalled or listed. |
| `crates/jackin-openshell/src/transfer.rs` | The exhaustive `process_error` match carries both variants into `GatewayError::Process`. |

Decisions:

1. Zombies do not count as members in the `/proc` path. They cannot run, and an orphan zombie waits for its new parent to reap it, which a sandbox without a reaping init may never do. This matches macOS, where `kill(-pgid)` on a group of zombies returns EPERM (measured: `os.killpg` gives EPERM for signals 0 and 9 on a zombie-only group and ESRCH after the reap). The Linux host path keeps the old rule (a zombie answers success until reaped).
2. A fork racing the scan is caught by the next scan. Each pass kills all current members, so a new member needs a living parent. A fork loop that outruns the passes ends at the cap as `GroupAlive`.
3. Reuse of a listed pid between the `/proc` read and the `kill` needs a full pid wrap in microseconds. It is not guarded (pidfds are not available to the workload).
4. The scan runs on the async thread, as the old `kill` did. It runs only after EPERM.

### Test seam for the EPERM path

No hermetic seam exists without a fake of code the repo owns or a new dependency, so the live proof is the G-FAST in a sandbox.

- The denial is a seccomp-notify rule. A test would have to install a seccomp filter in the test process. `unsafe_code` is forbidden in the workspace, so that needs a crate such as `seccompiler`, and the OpenShell workload filter denies `seccomp(SET_MODE_FILTER)`. The test would pass on a Linux CI host and fail in the sandbox, which is the surface this task serves.
- A uid mismatch (the other source of EPERM on Linux) needs root.
- Seatbelt on macOS cannot deny `kill` by argument sign, and macOS has no `/proc`.
- A trait over the signal call, or a `/proc` root path, would be a fake of owned code, and a fake `/proc` root would make the runner kill real pids.
- The existing test `a_run_returns_only_after_no_process_of_its_group_is_left` proves the contract on any host, and it is the test that exercises the EPERM branch when it runs in a sandbox. On the host it keeps proving the `killpg` branch.

Test plan for `test-reviewer` (not written): behavior "the member sweep stops every live process of a group and skips zombies". Positive case: a Linux-only unit test starts a process group of eight loops, calls the Linux branch of `sweep` directly, and asserts that `/proc` lists no member of the group afterwards. Negative case: a group whose only members are zombies lists none. No other test covers the `/proc` parse, because on a Linux host the group signal succeeds and the branch is never taken. The test would need `pub(crate)` access, so it would live in `group.rs`.

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

Linux code: `clippy --target aarch64-unknown-linux-gnu -W clippy::pedantic -D warnings` passes on a scratch crate that includes `group.rs` (the workspace cannot cross-compile, because `aws-lc-sys` needs `aarch64-linux-gnu-gcc`). The `/proc/<pid>/stat` parse was run on six sample lines (command name with `)` and spaces, zombie, other group, garbage). Neither runs the Linux branch against a live kernel.

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

## Not proven

- The `/proc` branch against a live OpenShell kernel.
- `adr008` root cause.
- That a pool stall is what hit the sandbox run (the symptom and the mechanism match; the sandbox was not instrumented).
