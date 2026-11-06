# A3M Unit 4 live proof — round 3

Date: 2026-11-06. Gateway: jackin-local. Branch: feature/openshell-native, HEAD cf32585a0 (fix: start non-lead instances under a sandbox PTY).
Sandbox: jackin-a3m3-ljxs (deleted after evidence). Workspace: a3m3 (/tmp/a3m-project3, git main).
Instances: codex/codex/codex-local/gpt-5.1-codex (lead); omp/zai/omp-zai/glm-5.3, branch agent/omp.
Task file: /tmp/a3m-task3.md (Part A codex, Part B omp). No credential printed.

## Step verdicts

1. Build + launch: PARTIAL PASS. `mise exec -- mbx +1.99.1 build -p jackin` PASS. Launch with --agent codex --agent omp --provider-for omp=zai --account-for codex=codex-local --account-for omp=omp-zai --task-file PASS, sandbox ready. agents.json lists both. Task status dir EXISTS at launch without manual mkdir (round-2 defect 2 FIXED). /sandbox/.jackin/run/omp/output exists, FIFO input exists.
2. Supervised omp stays alive: FAIL. Checked ~2 min after launch, no attach: no script/omp process. Only jackin-start (372) and lead codex (744). run/omp/output (60 bytes): `script: failed to create pseudo-terminal: Permission denied`. The PTY start itself fails in the sandbox; omp never starts.
3. Status + commits per branch (exec-driven parallel): PASS. Codex exec: status/codex.md `codex done`, part-a.txt, commit 1a4243b on main. omp -p: status/omp.md `omp done`, part-b.txt, commit 3c829fa on agent/omp. Both ran concurrently via parallel sandbox exec.
4. Restart omp only: PARTIAL PASS / FAIL on alive. `jackin agent restart jackin-a3m3-ljxs omp` exit 0. Codex PID 744 and jackin-start 372 unchanged before/after. But restarted omp again dies instantly: run/omp/output still `script: failed to create pseudo-terminal: Permission denied`; no omp process after restart.
5. Export: PASS. Host refs jackin/jackin-a3m3-ljxs/agent/omp at 3c829fa and jackin/jackin-a3m3-ljxs/main at 1a4243b.
6. Integrate: PASS. `merged agent/omp`, no conflict.

Cleanup: sandbox deleted (see below).

## Defects

1. `script -qec` cannot allocate a PTY inside the OpenShell sandbox: `script: failed to create pseudo-terminal: Permission denied`. Non-lead instances therefore still exit at once (now at the script layer, not the omp TTY check). Start command built at crates/jackin-session/src/multi_session.rs:268 (`setsid script -qec {command} /dev/null <> {run_dir}/input > {run_dir}/output 2>&1 &`), reused by restart. Likely sandbox seccomp/landlock or missing devpts access for the exec session; needs a PTY allocation method permitted in-sandbox (e.g. python pty, socat PTY, or openshell PTY exec) or script run under jackin-start's existing PTY context.
2. (Fixed this round, verified) Task status dir now created at launch: crates/jackin-session/src/multi_session.rs:84-86.

Overall: FAIL. Steps 3, 5, 6 pass; step 1 launch passes; steps 2 and 4 fail — supervised omp does not stay alive.
