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
