# A4 live proof T09 (omp with Z.AI), round 5

Verdict: PASS

## Versions
- jackin 0.1.0 built from HEAD 7494322bb "fix: no system config for macro-generated agents (A4)" in temp worktree /tmp/jackin-a4-r5 (removed after proof). Proof-only change there: T09 TupleState::Unavailable -> Supported in crates/jackin-core/src/agent.rs. No change in main checkout.
- OpenShell 0.1.7, gateway jackin-local Connected (mTLS, https://localhost:17670).
- Tuple T09: omp v18.16.0, provider zai, model glm-5.3, credential env ZAI_API_KEY, baseUrl https://api.z.ai/api/coding/paas/v4, config in sandbox showed apiKey: placeholder (no real key in sandbox).

## Account/provider
- Reused account omp-zai (jackin account ls: omp-zai zai omp-host). Provider jackin-omp-zai-omp pre-existing; launch reported "provider: skipped, the provider exists".
- Sentinel role /tmp/jackin-sentinel branch feature/openshell-native HEAD 82ff544. Temp workspace a4-sentinel registered for launch, removed after (see cleanup defect note).

## Launch and vehicle
- Interactive: DOCKER_CONTEXT=default jackin load sentinel /tmp/jackin-sentinel --agent omp --provider zai --account omp-zai --gateway jackin-local --task "Answer exactly: PONG. Then use one tool to list files in the current directory."
- Image build passed (no-system-config fix works; scan no longer passes a /sandbox system config). Sandbox jackin-a4-sent-jo7l created, ready.
- Vehicle: interactive launch returned sandbox id without attaching in non-TTY shell, so agent prompt was driven via openshell sandbox exec -g jackin-local jackin-a4-sent-jo7l -- omp -p --no-session (omp print mode in sandbox; jackin has no exec subcommand, vehicle recorded here).
- Text run output: "PONG." plus directory listing of project root.
- JSON run (omp -p --mode json): exactly 1 tool_execution_start/end, toolName bash, arguments command "ls -1", toolResult listing AGENTS.md, CLAUDE.md, Dockerfile, etc., assistant final text starting "PONG". Provider zai, model glm-5.3 in message metadata.

## Billing (percentages only, key held in memory only, never written)
Surface: GET https://api.z.ai/api/monitor/usage/quota/limit, plan level max.
- Before: TIME_LIMIT 0%, TOKENS_LIMIT 5h 0%, TOKENS_LIMIT weekly 37%.
- After: TIME_LIMIT 0%, TOKENS_LIMIT 5h 1%, TOKENS_LIMIT weekly 37%.
Plan usage moved (5h 0% -> 1%); no pay-as-you-go/credits surface.

## Cleanup
- Sandbox deleted: jackin purge jackin-a4-sent-jo7l --force -> deleted; jackin ls: no sandboxes.
- Temp worktree /tmp/jackin-a4-r5 removed after evidence written.
- Minor defect (cleanup only, not T09): `jackin workspace rm a4-sentinel` (also with --gateway jackin-local at top/workspace level) fails with "cannot reach gateway jackin-docker: gateway https://localhost:17671 is unreachable" because it probes the unrelated jackin-docker gateway; the a4-sentinel entry was therefore removed from ~/.config/jackin/config.toml directly. No file/line pinned (not investigated; unrelated gateway jackin-docker was down).

## Defects
- None blocking T09. Prior round-4 scan defect is fixed at HEAD.

## A9 re-proof (2026-11-06)

Verdict: PASS

- Binary: jackin 0.1.0 built from HEAD of feature/openshell-native (`mise exec -- mbx +1.99.1 build -p jackin`). Role: /Users/donbeave/Projects/jackin-project/jackin-sentinel (feature/openshell-native, declares [agents.omp] model glm-5.3). Workspace add a9-sentinel was rejected because a9-codex already shares that path; the launch used the path directly. Provider jackin-omp-zai-omp pre-existing ("provider: skipped, the provider exists"); account omp-zai.
- Launch: `DOCKER_CONTEXT=default ./target/debug/jackin load sentinel /Users/donbeave/Projects/jackin-project/jackin-sentinel --agent omp --provider zai --account omp-zai --gateway jackin-local --task "In the working directory, create a file named a9-proof.txt containing exactly the line: a9-omp-ok. Then stop."` Image built; sandbox jackin-a9-code-jctg created and ready.
- Vehicle note: as in A4, the non-TTY launch returned the sandbox id without driving the interactive omp (pid 674 running, no file after 4 min of polling). The task was then driven via `openshell sandbox exec -g jackin-local jackin-a9-code-jctg -- sh -c 'cd /sandbox/project && omp -p --no-session "<same task>"'`, which reported creating the file.
- File proof: `cat /sandbox/project/a9-proof.txt` -> `a9-omp-ok`; `od -c` shows exactly `a 9 - o m p - o k \n` (10 bytes).
- Usage before (host `~/.omp/agent/agent.db` usage_history, provider zai, latest poll recorded_at 1793659502044): 5h TOKENS_LIMIT 1.0%, weekly 37.0%. Usage after (fresh host `omp usage` poll, recorded_at 1793973118322, plan max): 5h 1.0%, weekly 37.0% — the 5h percent did not move at percent granularity. Tokens recorded instead: the sandbox omp log for the print-mode run (pid 893, session 01a1b11e-bab9-77fb-8993-46a1f6b1885f) records assistantUsageContextTokens 9056 / storedContextTokens 8136 at agent end, and a post-run usage fetch for provider zai resolved 3 limits (13:49:37Z). Sandbox usage_history held only the startup poll (5h 0.0% in its own fresh window).
- Hygiene (no key read or grepped): sandbox `/sandbox/.omp/config.toml` (153 bytes) and `/sandbox/.omp/auth.json` (50 bytes) each contain the word `placeholder` for the zai credential and no other key material was inspected; config names provider zai, model glm-5.3, api.z.ai. Processes: `jackin-start omp` and `/usr/local/bin/omp` running as user sandbox with HOME=/sandbox (workspace-local `/sandbox/.omp` config).
- Cleanup: `jackin purge jackin-a9-code-jctg --force` -> deleted; `openshell sandbox list -g jackin-local` -> no sandboxes. No a9-sentinel workspace entry was created (add rejected). Host `~/.config/jackin/config.toml` was rewritten by jackin during launch (section reordering only, jackin-docker entries preserved); restored byte-identical from the pre-launch backup. No commit or push made.
