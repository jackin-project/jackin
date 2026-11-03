# Flake: `f_pty::markers_defeat_the_terminal_echo`

Source: `/private/tmp/claude-501/plans/gfast-pgid.log` around line 1147, the fast gate on merge `83cbc346f`. One failure; the test passed 6 of 6 when run alone. Base: `494be498e` on `wt/flake-pty-echo`.

```
FAIL [   0.099s] (1121/1210) jackin-xtask::e2e f_pty::markers_defeat_the_terminal_echo
thread 'f_pty::markers_defeat_the_terminal_echo' panicked at crates/xtask/tests/e2e/f_pty.rs:113:5:
assertion `left == right` failed
  left: ("sh: child setpgid (70882 to 70882): Operation not permitted\nok", 0)
 right: ("ok", 0)
```

The test was quarantined first (`d042d20d7`) and is unquarantined by the fix.

## Did the merge cause it?

No. The merge exposed an older flake. The test binary was run directly (the `--exact` filter, 16 parallel workers, 24 busy `yes` processes started and killed by PID), on the merge head and on its first parent `f48d7bc70`:

| Commit | Runs | Failures |
|---|---|---|
| `f48d7bc70` (before the merge) | 1280 | 12 |
| `494be498e` (merge head) | 640 + 1280 | 4 + 7 |

All 23 failures have the exact `setpgid` text of the gate log, in the first `run_shell` ("ok") or the last one ("second"). The change of 83cbc346f to `PortableLink::kill` runs when the link drops, after the assertions, and cannot put text into the shell output. Under nextest with 36 busy processes, 200 stress iterations of the test alone passed (the rate is about 0.7 percent per run and nextest runs the iterations one at a time, so the reproduction needs parallel runs).

## Root cause

`/bin/sh` on macOS is bash 3.2. The test starts it on a PTY with no arguments (`crates/xtask/tests/e2e/f_pty.rs:109`). Bash treats a shell whose stdin is a terminal as interactive and turns job control on. `PtySession::run_shell` sends `( {command} )` (`crates/xtask/src/e2e/pty.rs:380`). With job control on, bash forks a subshell for the parentheses, and both the parent and the child call `setpgid(child, child)` to put it in its own group (inferred from the message text "child setpgid (X to X)" and from bash's usual fork protocol; I did not read the bash source). On macOS the two calls appear to race, and the child's call occasionally fails with EPERM. Bash prints the error to the terminal, ahead of the command output. The reader collects it as part of the body, and the equality assertion fails. This is the first wrong state: the shell reported its own job-control race on the PTY.

### Evidence

- No jackin code is involved. A 12-line Python script (`pty.fork`, `/bin/sh`, send the same marker line, read) printed the same error: 2 of 300 on a quiet host, 26 of 4000 with 16 parallel scripts and 24 busy processes.
- Job control is the trigger. The same script with `set +m` sent first: 0 of 4000, against 20 of 4000 in the paired run without it.
- Command-line flags cannot turn it off. In `/bin/sh` (bash 3.2) an interactive shell ignores `+m`, `+i` and `-s`: `set -o` still shows `monitor on` and `$-` is `himBH` (checked with `+m`, `+i`, `-s`, `+o monitor`). An in-stream `set +m` works because it is a builtin that runs before the next fork.

## Why the structure allowed it

The test needs a terminal, for the echo, but not a job-controlled shell. Starting a shell on a PTY turns job control on without anyone asking for it, and `run_shell` makes the shell fork on every call. Each call is a chance for the race, so the failure rate rises with the number of calls (4 in this test) and with host load, and the output the race writes is indistinguishable from command output.

Related sites:

- `PtySession::run_shell` is also used by `crates/xtask/src/e2e/steps/j12.rs:62` and `j15.rs:153,192` against the shell of the sandbox. That shell is on the OpenShell side, not macOS bash 3.2, and no failure was seen there. Not changed.
- The other `f_pty` tests use `/bin/sh -c script`, which is not interactive and has no job control. They are not exposed.

## Fix

`crates/xtask/tests/e2e/f_pty.rs`: the `shell` closure of `markers_defeat_the_terminal_echo` sends `set +m` right after opening the PTY, and the `#[ignore]` is removed. The shell reads its input in order, so the setting is in force before the first `run_shell` line, and the echo of `set +m` precedes the start marker, which `run_shell` discards. The assertions are unchanged, so the test still checks that the echoed command line is not part of the answer.

## Proof

Same surface: the test binary, run directly, 16 parallel workers, 24 busy `yes` processes started and killed by PID.

| State | Runs | Failures |
|---|---|---|
| before the fix, `494be498e` | 640 + 1280 | 4 + 7 |
| after the fix | 3200 | 0 |

At the observed rate (about 0.7 percent), 3200 runs would give about 22 failures. Probability of none by chance is below 1e-9. Nextest with 36 busy processes: 100 stress iterations of all six `f_pty` tests passed, with no `setpgid` line in the log.

## For `test-reviewer`

No new test is needed. The existing test is the regression test: it failed at about 0.7 percent per run on the unfixed code under load and passes without a failure in 3200 runs on the fixed code. The red proof is probabilistic, not deterministic, because the race is inside bash and the kernel.

## Not verified

- The exact kernel step that returns EPERM to the child. The cause is shown by the Python reproduction and by the `set +m` control, not by tracing XNU.
- Whether the sandbox shell of `j12` and `j15` has the race.
