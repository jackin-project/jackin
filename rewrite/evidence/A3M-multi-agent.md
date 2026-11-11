# A3M Unit 0 live proof

- Gateway: jackin-local (OpenShell 0.1.7, mTLS, status Connected — verified via `openshell -g jackin-local --workspace default status`)
- Workspace: default (passed explicitly on every call)
- Sandbox: a3m-unit0-prover
- Image: nvcr.io/nvidia/base/ubuntu:24.04 (upstream default pulled by `openshell sandbox create` with no --from/--template)
- Start: 2026-11-06T04:50:00Z; kill/restart: 2026-11-06T04:50:22Z-04:50:31Z
- All calls: `openshell -g jackin-local --workspace default ... < /dev/null` with `timeout`

## Create
Command: `timeout 120 openshell -g jackin-local --workspace default sandbox create --name a3m-unit0-prover --detach -- sleep infinity < /dev/null`
Output (trimmed): `Created sandbox: a3m-unit0-prover` / `Pulling image nvcr.io/nvidia/base/ubuntu:24.04` / create_exit=0
`sandbox get`: Phase: Ready, Main process instance present, Restart policy: never.

## Route 1 (supervisor route) — NOT PROVABLE on stock image
PID 1 is `openshell-sandbox launch-capability-free ...`, main child is `sleep infinity` (PID 369).
Probe: `ls -la /sandbox/.jackin/run` -> `No such file or directory`; `command -v supervisorctl supervisord jackin-start tmux screen` -> (none); `/sandbox` and `/usr/local/bin` empty.
Finding: stock image has no jackin-start / supervisor / per-instance control directory. Route 1 needs the A3M jackin-start change and cannot be proven on the stock image. Not faked.

## Route 2 (exec route) — SELECTED
Separate exec calls started two loops:
- `sandbox exec ... -- sh -c 'nohup sh -c "while true; do echo A-tick; sleep 2; done" > /tmp/a.log 2>&1 & echo A_PID=$!'` -> A_PID=387
- same for B -> B_PID=391
Simultaneous ps (04:50): both 387 and 391 present with child `sleep 2` each.
Separate streams: /tmp/a.log contained only A-tick x3; /tmp/b.log only B-tick x3.
Kill/restart: `kill 387` -> ps showed only 391; B lines grew 6 -> 7 while A dead; Phase stayed Ready (`sandbox get` grep). Restarted A as A2_PID=425 writing /tmp/a2.log (A2-tick x3) while B reached 9 lines; final ps showed 391 and 425 concurrently; Phase: Ready.

## Four facts (Route 2)
1. Two processes run at same time: PASS
2. Each output stream stays separate: PASS (separate exec calls, separate files, no mixing)
3. One restarts, other does not stop: PASS (B PID 391 unchanged, log kept growing)
4. Sandbox phase stays Ready after one process exits: PASS

Note: processes were detached with nohup inside exec calls, so they survived exec return. A pure ExecSandboxInteractive PTY detach test was not performed; host-PTY detach survival remains to be stated/proven in Unit 3 design.

## Delete
2026-11-06T04:50:37Z
✓ Deleted sandbox a3m-unit0-prover
delete_exit=0
No sandboxes found.
2026-11-06T04:50:38Z
--- raw trimmed log at /tmp/a3m-evidence-raw.log (host only) ---
