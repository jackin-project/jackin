# Flake: `gateway contract::shell_exit_code_cannot_hang_the_run`

Source: `/private/tmp/claude-501/p112/live4-gfast.log` (one failure in the live G-FAST run in an 8-vCPU OpenShell sandbox, `crates/jackin-openshell/tests/gateway/contract.rs:295`, "the shell_exit_code check did not end") and `/private/tmp/claude-501/plans/P11.2-evidence.md` (two later runs passed). Base: `e13cb4283` on `wt/flake-shell-exit`.

## Is it the `FakeClock` cause of `wt/sandbox-pgid`?

No. The test does not use `jackin-testkit`, `FakeClock` or the fake gateway of `jackin-testkit`. `jackin-openshell` has no dependency on `jackin-testkit`. The test drives a real TLS gRPC connection to the in-process tonic server of `tests/gateway/server.rs` on tokio's own paused clock. The `Working` guard of commits `58740e2c6` and `161ff30e7` therefore cannot cover it, and the two branches touch disjoint files (`wt/sandbox-pgid` changes `src/transfer.rs` and `tests/gateway/{discovery,transfer}.rs` in this crate, nothing in `contract.rs`, `src/contract.rs` or `harness.rs`). The structure is the same: virtual time advances at a rate that is not tied to the completion of the real work in flight.

## Root cause

`drive_paused` (`tests/gateway/harness.rs`, removed by this fix) paused tokio's clock and advanced it by 2 s after every 20 ms of real time (`SETTLE`), whatever the runtime was doing. That is 100 virtual seconds per real second, fixed, for a test whose remaining real work (cleanup RPC, five more checks, each a gRPC round trip) needs CPU on the one runtime thread. The test passes only if all of that work fits in the budget that is left after the 60 s `PROGRAM_TIMEOUT`: 120 s limit minus about 62 s is 28 steps, about 560 ms of real time. Measured on a quiet host, the work needs about 10 ms, so the margin is 50 times. The sandbox log shows the host was about 20 to 50 times slower than that at the time (neighbour tests that take 0.1 s took 3 to 9 s), which spends the margin. When the budget is spent, `drive_paused` aborts the task and returns `None`, which is the panic at `contract.rs:295`.

The same fixed rate also drives the earlier flake of this test (commit `203c3413f`, P5.3): the SDK's h2 keepalive pings at +10 s and drops the connection at +30 s of virtual time, and a pong that is late in real time loses the same race. That fix (the 20 ms settle) narrowed the window; it did not remove the coupling of virtual time to wall time.

### Evidence

Production setting, `SETTLE` 20 ms, original code:

| Load | Runs | Failures |
|---|---|---|
| quiet host, 1 process | 100 (nextest `--stress-count`) | 0 |
| 36 busy `yes` processes, 1 process at a time | 100 | 0 |
| 32 parallel processes plus 40 `yes` | 160 | 0 |
| 120 parallel processes plus 120 `yes` | 360 | 0 |
| 64 parallel processes at `nice -n 20` plus 100 busy perl | 384 | 0 |
| 12 whole `gateway` binary iterations, 40 burners | 12 | 0 |
| 24 parallel processes in background QoS (`taskpolicy -b`) plus 24 busy perl, runs about 15 times slower | 96 | 0 |

The flake did not reproduce on this host at the production setting. The slowest observed pass used 74 of 120 virtual seconds (`DRIVE elapsed=74s`; a quiet run uses 62 to 64).

The mechanism reproduces when the ratio of real work to real settle time is scaled the way a slow host scales it. I added temporary prints (removed again) and changed only `SETTLE`, then ran `nextest run ... --stress-count 20 --retries 0`:

| `SETTLE` | Failures in 20 | Virtual time used in passes |
|---|---|---|
| 20 ms (production) | 0 | 62 to 64 s |
| 2 ms | 0 | 66 to 74 s |
| 250 us | 0 | 94 to 98 s |
| 50 us | 14 | n/a |

All 14 failures had the exact panic of the live log, at `contract.rs:295:10`, with `DRIVE elapsed=120s`. In them the `shell_exit_code` check ended at 64 to 66 virtual seconds as expected, and each of the five following checks then spent 14 to 16 virtual seconds on 2 ms of real work. That is the budget being spent by wall-clock slowness alone.

Not proven: that the live failure was the budget and not the keepalive (both end in the same panic); the live log does not hold the per-check times. Both follow from the one cause, and the fix removes both.

## Why the structure allowed it

A test that needs a 60 s timeout to fire had no way to shorten it, because `PROGRAM_TIMEOUT` was a private constant of the contract. The test therefore borrowed tokio's clock, which also drives every other timer of the stack under test (h2 keepalive, `call_timeout`, the SSH lease window). Any rate chosen for the virtual clock is wrong on some host.

Related sites:

- `tests/gateway/exec.rs:149` and `tests/gateway/status.rs:364` call `tokio::time::pause()` and rely on the clock's auto-advance with 5 s timeouts, which are shorter than the first h2 ping (10 s). They share the coupling but not the exposure: no work of theirs must finish after virtual time has been spent. Not changed.
- `jackin-testkit` `FakeClock`: the cause of `wt/sandbox-pgid`, already fixed there.

## Fix

The contract takes the wait as an input and the test no longer touches the clock.

| File | Change |
|---|---|
| `crates/jackin-openshell/src/contract.rs` | `ContractFixture` gains `shell_end_timeout`. `SHELL_END_TIMEOUT` (60 s, the old value) is exported. `shell_exit_code` waits `ctx.fixture.shell_end_timeout` instead of `PROGRAM_TIMEOUT`. |
| `crates/jackin-openshell/tests/gateway/contract.rs` | The test sets `shell_end_timeout` to 1 s, uses the shared `connect`, and awaits the run under `guard` (10 s). No `pause`, no `call_timeout` override. |
| `crates/jackin-openshell/tests/gateway/harness.rs` | `drive_paused` is deleted; it had no other caller. |
| `crates/jackin-session/examples/export_live.rs`, `upload_project.rs`, `crates/jackin-testkit/tests/gateway/support.rs`, `tests/gateway/contract.rs` | Struct literals take `shell_end_timeout: SHELL_END_TIMEOUT`. The live contract run keeps its 60 s. |

The test now runs in real time. The only exit from the hung shell is the 1 s timeout, so a slow host cannot change the outcome; it can only make the test take longer, up to the 10 s guard.

After the fix, 556 direct runs of the test binary with 64 and 100 parallel copies at `nice -n 20` against 100 and 120 busy perl processes: 0 failures (256 and 300 runs). A quiet nextest run passes in about 1.1 s.

Red proof: with the fixture timeout set to 3600 s (the same as a check that has no timeout), the test fails after 10 s with "the operation hung" (`harness.rs:32`). With 1 s it passes in about 1.1 s.

## For `test-reviewer`

The test changed (setup only, same behavior and same assertion). The repo rule asks an independent reviewer to see it fail on broken code and pass on fixed code. The red/green proof above is mine, not an independent one. No new test is needed, so there is no plan file.
