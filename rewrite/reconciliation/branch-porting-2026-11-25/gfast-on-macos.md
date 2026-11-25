# demo-gate "no exit line" investigation

Date: 2026-11-25. Checked on a clean worktree of origin/main b4ee3acd7.

## Conclusion: (a) environment artifact, not a bug in demo-gate

demo-gate does not hang, wait on stdin, need a TTY, or start a TUI, VHS or the gateway.

What `xtask ci --fast` does (crates/xtask/src/ci.rs, checks/demo_gate.rs, repo.rs):
- Steps run in order: fmt clippy nextest deny shear comments layers pins containers errors baseline screen demo-gate reuse. (docs is skipped by --fast.)
- Before each step xtask prints `xtask: <step>` to stderr. It prints NOTHING when a step passes and NOTHING when the whole run passes. Exit status is the only success signal. xtask never prints an exit line.
- demo-gate runs, all with stdin=/dev/null and stdout/stderr piped (captured, so silent while running):
  `cargo tree -p jackin`/`-p jackin-tui` (3 calls), `cargo build -p jackin-tui --message-format=json`,
  then `nm -C` and `strings -a` on the jackin_console rlib. Pure static checks, no network, no PTY.
- It spawns the real rustup cargo (the CARGO env var), dev profile, default features: a different
  build than nextest's --all-features, so on a cold worktree it compiles a lot, with zero output.
- reuse runs `reuse lint` and ends with the "Congratulations ... compliant" summary.

Evidence (clean worktree wt/demo-gate-main, detached, stdin /dev/null, no TTY):
- run1 (cold target dir, machine loaded with ~10 other agents' cargo/mbx builds):
  output was only `xtask: demo-gate`, then exit 0 after 1127 s (18.8 min; the xtask build and the dev build of jackin-tui, aws-lc-sys etc.).
  The process tree showed cargo -> rustc / aws-lc-sys build script the whole time. Not hung, just slow and silent.
- run2 (warm): `xtask: demo-gate`, exit 0 in 19 s.
- run3 (warm, stdin closed with `<&-`, `--only demo-gate,reuse`): `xtask: demo-gate`, `xtask: reuse`, reuse summary, exit 0 in 25 s.
- `setsid` does not exist on macOS (`setsid: command not found`, exit 1). A launch such as `nohup setsid ...`
  never starts xtask at all, so there is no output and no exit line.

So the missing exit line comes from how the runs were launched and watched:
1. `setsid` launches never started (macOS has no setsid).
2. Broad `pkill -f` patterns by other agents killed other agents' xtask/cargo processes mid-step. A killed
   run leaves exactly `xtask: demo-gate` as its last line.
3. demo-gate is silent while it builds, and a cold worktree under load took ~19 min, which exceeds typical
   tool/parent timeouts (the run is killed with no exit line).
4. On success xtask itself prints no final line, so a wrapper has to print one.

No PR: there is no demo-gate defect to fix. Optional hardening (not done): have `xtask ci` print a final
`xtask: ok` line and a heartbeat during the demo-gate build.

## Reliable G-FAST recipe (macOS)

Run from the repository root (or worktree root). Launch detached with your own PID, print the exit line yourself:

    L=/path/to/scratch/gfast.log
    CARGO_BUILD_JOBS=3 NEXTEST_TEST_THREADS=3 nohup bash -c \
      'T0=$(date +%s); mise exec -- mbx +1.99.1 xtask ci --fast; rc=$?; echo "G-FAST EXIT=$rc elapsed=$(( $(date +%s)-T0 ))s"' \
      < /dev/null > "$L" 2>&1 &
    echo $! > /path/to/scratch/gfast.pid

Rules:
- Do not use `setsid` (absent on macOS) and never use `pkill`/`pkill -f`. Kill only the PID you wrote.
- Wait on your PID in chunks shorter than your tool timeout (or use Monitor / run_in_background):
      while kill -0 $(cat gfast.pid) 2>/dev/null; do sleep 5; done; tail -20 gfast.log
- Report G-FAST only from the `G-FAST EXIT=<rc>` line. EXIT=0 is a pass. The last `xtask: <step>` line before it is the
  failing step if rc != 0 (violations are printed above it). No `G-FAST EXIT=` line means the run was killed or is still running.
- Expect the last xtask lines on success: `xtask: demo-gate`, `xtask: reuse`, the reuse summary, then your EXIT line.
- Budget time: a cold worktree under load can take 20+ minutes (nextest, then demo-gate's silent dev build).
  Do not treat silence after `xtask: demo-gate` as a hang until the PID is gone; check
  `ps -axo pid,ppid,etime,command | awk -v p=<xtask pid> '$2==p'` for cargo/rustc children.
- To check demo-gate alone: `mise exec -- mbx +1.99.1 xtask ci --only demo-gate` with the same wrapper.
