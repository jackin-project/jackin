# A3M Unit 4 live proof — round 4 (final contract)

Date: 2026-11-06. Gateway: jackin-local. Branch: feature/openshell-native, HEAD 9a4e239dc (fix: non-lead instances run on attach, restart returns to ready).
Sandbox: jackin-a3m4-shnl (deleted after evidence). Workspace: a3m4 (/tmp/a3m-project4, git main).
Instances: codex/codex/codex-local/gpt-5.1-codex (lead); omp/zai/omp-zai/glm-5.3, branch agent/omp.
Task file: /tmp/a3m-task4.md (Part A codex, Part B omp). Sentinel role at /tmp/jackin-sentinel. No credential printed.
Contract: a non-lead instance has no always-on interactive process; its process runs while a host holds its PTY or while driven through exec; `jackin agent restart` stops only that instance process and returns it to ready.

## Step verdicts

1. Build + launch: PASS. `mise exec -- mbx +1.99.1 build -p jackin` PASS. Launch `--agent codex --agent omp --provider-for omp=zai --account-for codex=codex-local --account-for omp=omp-zai --task-file /tmp/a3m-task4.md` PASS, sandbox ready. agents.json lists both instances. Task status dir `/sandbox/.jackin/task/status/` EXISTS at launch. Both homes `/sandbox/.jackin/homes/{codex,omp}` exist; omp worktree `/sandbox/work/omp/project` on branch `agent/omp`. At rest (no attach, no exec): only jackin-start PID 372 and lead codex PID 740 — no omp process. Contract holds at launch.
2. Parallel exec drive: PASS. Codex exec (workdir /sandbox/project) and `omp -p` (workdir /sandbox/work/omp/project) launched in parallel. omp PID 950 running at 08:17 while codex exec completed commit d79fc27 on main and status/codex.md = `codex done`; omp still running afterwards (sleep 120 instruction). Both agent processes were live in the same window (omp 950 concurrent with the codex drive; lead codex 740 also present). Note: the sampled `ps` at +45s caught omp 950 and lead codex 740; codex's short exec child had already exited after finishing Part A.
3. Status + commits: PASS. status/codex.md `codex done`, part-a.txt, commit d79fc27 on main. After step 4 re-drive: status/omp.md `omp done`, part-b.txt, commit 26d7f77 on agent/omp.
4. Restart omp only during long exec: PASS. While omp PID 950 was in its long exec run, `jackin agent restart jackin-a3m4-shnl omp` exit 0, printed `omp ready`. After: omp process gone; jackin-start PID 372 and codex PID 740 unchanged; no new always-on omp process (returned to ready). Then omp driven again through exec and finished Part B (26d7f77).
5. Export: PASS. Host refs jackin/jackin-a3m4-shnl/main and jackin/jackin-a3m4-shnl/agent/omp arrived (d79fc27 / 26d7f77; main later ffa8df8 after integrate + re-export).
6. Integrate: PASS. `merged agent/omp`, no conflict. Merge commit ffa8df8 on exported main contains both d79fc27 and 26d7f77.

Cleanup: sandbox deleted (`deleted sandbox jackin-a3m4-shnl`; `jackin ls` shows no sandboxes).

## Defects

None against the final contract. Round-3 defect (non-lead PTY `script` failure / always-on expectation) is superseded by the amended contract and the 9a4e239dc fix: non-lead instances have no always-on process, run under exec/PTY, and restart returns them to ready — all verified live above.

Overall: PASS. All steps pass under the final contract.
