# A14 prep — live proof scripts for T02, T05, T03, T09, T01

Date: 2026-11-08. Prepared in worktree `/tmp/jackin-wt-live` (branch `wt/live-t02t05`, HEAD `94fefa015`, same head as `feature/openshell-native`).
Scope: read-only prep. No code edited, no sandbox launched, no commit, no secret read or printed. One live host check per blocked pair, dated below.
Operator live-test orders for T02 and T05: `rewrite/evidence/operator-order-2026-11-08.md`. Class O/S step texts: `rewrite/GOAL-MULTI-AGENT.md` tasks A6/A7 and the Gates table. Prior CLI syntax: `rewrite/evidence/A4-omp-zai.md`, `A6-grok.md`, `A6-muse.md`, `A9-codex.md`, `A3M-live.md`.

## Host snapshot (2026-11-08T13:05–13:07Z, all UTC)

- Host CLIs: `grok` 1.0.70 (f50904eaee6c), `claude` 2.1.319 (Claude Code), `agy` 1.5.3, `omp` 18.18.1, `codex` 0.167.0 (codex-cli). `jackin` is not on PATH; every script builds it.
- Gateway: `jackin-local` (https://localhost:17670, mTLS) connected and current; `jackin-docker` (17671) registered. `openshell -g jackin-local sandbox list` -> `No sandboxes found.`
- Baseline pin (root `Cargo.toml:66-67`): `terminal-components-baseline` rev `1d7f63dbb13ecb2bf3e3295e7a296dc2080a62b1` (newer than the A8-recorded `f539d9a`). This is the "pin bump" behind the T09 re-proof.
- Sandbox spec pins (crates/jackin-core/src/agent.rs): claude 2.1.311, grok 1.0.77, agy 1.5.1 (no install targets yet), omp 18.16.0, codex 0.166.0. Host versions differ; record both in each evidence file.
- Host jackin accounts (`~/.config/jackin/config.toml` sections): `codex-local`, `grok-live`, `muse-host`, `muse-live`, `omp-zai`, `jackin-docker-validation`. No claude or antigravity account exists yet.
- omp usage snapshot (13:07:06Z, percentages only): Zai plan `max`; 5h 5.0% used, weekly 38.0% used, Zread monthly 0.0%. Room for a re-proof run.
- Providers on jackin-local (13:10:34Z, `openshell -g jackin-local provider list`): `jackin-omp-zai-omp` (omp-zai, 1 key), `jackin-codex-local-codex` (codex, 2 keys), `jackin-muse-live-muse` (muse, 1 key), `jackin-grok-live-grok` (grok, 1 key), `jackin-jackin-docker-validation-codex`. No claude or antigravity provider exists yet.

## Live host checks, dated (no secrets, no sandbox)

### T03 Grok — reply attempt: still quota-blocked

2026-11-08T13:05:49Z, host `grok -p "Reply with exactly: PONG" --no-auto-update --output-format json` in `/tmp`:

```json
{"type":"error","message":"Internal error: {\n  \"message\": \"API error (status 402 Payment Required): Grok Build usage balance exhausted\",\n  \"http_status\": 402\n}","cwd":"/private/tmp/jackin-wt-live"}
```

exit 1. Login intact: 13:05:58Z `grok models` -> `You are logged in with grok.com.` / `Default model: grok-4.7`, exit 0. `~/.grok/auth.json` mode `-rw-------` (0600), 1830 bytes, mtime 2026-11-08 14:30:38 +0700 (host auto-refresh ran today; metadata only). Verdict: the 2026-11-06 blocker condition persists. A14 action now is the dated blocker update in the T03 script below, unless the quota recovers before the run.

### T02 Claude — host reply: works; login lives in the macOS Keychain

2026-11-08T13:06:03Z, host `claude -p "Reply with exactly: PONG"` in `/tmp` -> `PONG`, exit 0 (default model). Login storage: Keychain item `Claude Code-credentials` EXISTS (existence check only); `~/.claude/.credentials.json` does not exist. So the T02 host import must read the Keychain entry (GOAL A7 step 2), and the host/sandbox refresh race test is mandatory before any flip.

### T05 Antigravity — host reply: works

2026-11-08T13:06:17Z, host `agy -p "Reply with exactly: PONG"` in `/tmp` -> `PONG`, exit 0. State dir `~/.gemini/antigravity-cli/` active (brain/, conversations/, cli.log). No Keychain service found under the guessed names `Antigravity`, `antigravity`, `agy`, `Antigravity CLI` (fact; the real host store name is unverified — do not read values to find it). A1 research: on Linux, agy auth storage is the OS keyring via secret-service over D-Bus — exactly what an OpenShell sandbox does not have. That is the fact T05 must prove or refute.

## Shared conventions for every script (from A4/A6/A9 evidence)

1. Build: from the jackin repo root, `mise exec -- mbx +1.99.1 build -p jackin`. Run cargo only through `mise exec -- mbx`. No `cargo test`.
2. Temp-worktree pattern for still-blocked tuples: `git worktree add /tmp/jackin-a14-<id> HEAD`, flip only that tuple row in `crates/jackin-core/src/agent.rs` (state, reason), build there, prove, `git worktree remove`. No change in the main checkout until the gate passes; then flip in-repo together with the evidence file in one signed commit.
3. Role prerequisite: the sentinel role declares only `[agents.codex]` and `[agents.omp]`. A launch of claude/grok/agy fails today with `RoleError::NoInstructionMapping` ("role sentinel has no instructions for agent ..."). First add `[agents.<agent>]` with `instructions = "instructions/<agent>.md"` plus that file in the sentinel role repo (branch `feature/openshell-native`).
4. Launch vehicle: non-TTY `jackin load` returns the sandbox id without attaching. Drive agents with `openshell sandbox exec -g jackin-local <sandbox> -- sh -c '...'` (A4/A9 recorded pattern).
5. Hygiene: never `cat` a host credential file; keychain checks are existence-only (`security find-generic-password -s <name> >/dev/null 2>&1 && echo EXISTS`); in-sandbox scans print only `stat` modes/sizes and `grep -c placeholder`. No secret in any file, log, or evidence.
6. Cleanup per run: `./target/debug/jackin purge <sandbox> --force`; `openshell -g jackin-local sandbox list` -> none; back up `~/.config/jackin/config.toml` before launch and restore after (jackin rewrites section order, A9 note); remove temp workspace entries (direct config edit if `jackin workspace rm` still probes the down `jackin-docker` gateway, A4 cleanup defect).
7. Full G-E2E gate: `mise exec -- mbx +1.99.1 xtask e2e --gateway jackin-local --role ../jackin-sentinel --builder orbstack --platform linux/arm64 --engine-port 2376 --engine-certs ~/.local/share/jackin-e2e-engine/client`. `xtask e2e` has no `--agent`/`--provider` flags (`crates/xtask/src/e2e/args.rs`), so per-tuple live gates are the manual launches below, as in A4/A6/A9.

## T02 Claude Code subscription (class O; operator order on file)

Code prerequisites before any live step: (a) implement `account add claude --from-host-login` reading only the `Claude Code-credentials` Keychain entry for the selected `CLAUDE_CONFIG_DIR` (host has no `.credentials.json` file); store provider keys as profile env var names (the 7c30d78c4 lesson); (b) sandbox placeholder writer for `/sandbox/.claude/.credentials.json` mode 0600; (c) flip T02 `BlockedVendor -> Supported` in the temp worktree only; (d) sentinel `[agents.claude]`.

1. Host login before: `date -u +%Y-%m-%dT%H:%M:%SZ; cd /tmp && claude -p "Reply with exactly: PONG"` -> expect `PONG`, exit 0 (proven 2026-11-08T13:06:03Z). `security find-generic-password -s "Claude Code-credentials" >/dev/null 2>&1 && echo EXISTS` -> `EXISTS`.
2. Account add: `./target/debug/jackin --gateway jackin-local account add claude --name claude-host --from-host-login` -> expect exit 0, no secret in output; `openshell -g jackin-local provider list` -> provider `jackin-claude-host-claude` present.
3. Launch: `DOCKER_CONTEXT=default ./target/debug/jackin load sentinel /Users/donbeave/Projects/jackin-project/jackin-sentinel --agent claude --provider claude --account claude-host --gateway jackin-local --task "In the working directory, create a file named a14-t02-proof.txt containing exactly the line: a14-t02-ok. Then stop."` -> image built (claude 2.1.311), sandbox ready; record sandbox id.
4. Sandbox fixed prompt: `openshell sandbox exec -g jackin-local <sbx> -- sh -c 'cd /sandbox/project && claude -p "Answer exactly: PONG"'` -> expect `PONG`. Then drive the task text the same way; `openshell sandbox exec -g jackin-local <sbx> -- od -c /sandbox/project/a14-t02-proof.txt` -> `a 1 4 - t 0 2 - o k \n` (12 bytes).
5. Placeholder scan: `openshell sandbox exec -g jackin-local <sbx> -- sh -c 'stat -c "%a %s" /sandbox/.claude/.credentials.json; grep -c placeholder /sandbox/.claude/.credentials.json; grep -c "sk-ant-" /sandbox/.claude/.credentials.json'` -> expect `600 <n>`, `grep -c placeholder` >= 1, `grep -c sk-ant-` = 0.
6. Refresh race (A7 step 3; blocks the flip): with the sandbox up, run the host prompt (`claude -p "Reply with exactly: HOST-PONG"` -> `HOST-PONG`) in the same window as a sandbox run past access-token expiry (or a forced refresh), then re-run the sandbox prompt. Both must answer; record both timestamps. If the host logs out, stop: T02 -> `unavailable`, refresh-race blocker, no ship.
7. Host login after: repeat step 1 -> `PONG`, exit 0; keychain item still `EXISTS`.
8. Second-sandbox isolation (class O step 6): one live negative check — launch a second sandbox that tries the same credential path; record the typed refusal. State the one-check scope in the evidence.
9. Cleanup (conventions above), then flip T02 in-repo `Supported` with `rewrite/evidence/A14-claude.md` in one commit; close `rewrite/blockers/T02-claude.md` with the dated result.

## T05 agy no-keyring proof (class O-shaped; operator order on file)

Code prerequisites: (a) find the agy host credential store (research, no values printed) and implement `account add antigravity --name agy-host --from-host-login`; (b) define sandbox credential delivery — the spec has no separate auth file (`auth_file` = settings.json), so the placeholder must ride an env var the profile declares; this is the make-or-break of the proof; (c) flip T05 in the temp worktree only; (d) sentinel `[agents.agy]`. Note the spec pins agy 1.5.1 with an empty install-target list; A14 must add the A1 artifact targets first.

1. Host before: `cd /tmp && agy -p "Reply with exactly: PONG"` -> `PONG`, exit 0 (proven 2026-11-08T13:06:17Z).
2. Account add: `./target/debug/jackin --gateway jackin-local account add antigravity --name agy-host --from-host-login` -> exit 0; provider `jackin-agy-host-agy` listed.
3. Launch: `DOCKER_CONTEXT=default ./target/debug/jackin load sentinel /Users/donbeave/Projects/jackin-project/jackin-sentinel --agent agy --provider antigravity --account agy-host --gateway jackin-local --task "In the working directory, create a file named a14-t05-proof.txt containing exactly the line: a14-t05-ok. Then stop."` -> sandbox ready.
4. No-keyring proof (the T05 gate): `openshell sandbox exec -g jackin-local <sbx> -- sh -c 'command -v dbus-daemon busctl secret-tool; echo tools-exit=$?; ls /run/dbus 2>&1; env | grep -c DBUS_SESSION_BUS_ADDRESS'` -> expect no keyring tools, no `/run/dbus`, `0`. Then `openshell sandbox exec -g jackin-local <sbx> -- sh -c 'cd /sandbox/project && agy -p "Answer exactly: PONG"'` -> `PONG` proves agy works with no desktop keyring. If agy fails with a keyring/D-Bus error, record it verbatim; T05 -> `unavailable: needs a desktop keyring in the sandbox`, dated blocker update — that is the ordered outcome, not a failed A14.
5. File proof: drive the task; `od -c /sandbox/project/a14-t05-proof.txt` -> `a 1 4 - t 0 5 - o k \n` (12 bytes).
6. Placeholder scan: `stat -c "%a %s"` and `grep -c placeholder` on `/sandbox/.gemini/antigravity-cli/settings.json` (and the credential env path chosen in (b)); no real token strings.
7. Host after: repeat step 1 -> `PONG`, exit 0.
8. Cleanup; flip T05 in-repo `Supported` with `rewrite/evidence/A14-antigravity.md`, or keep blocked with the recorded reason.

## T03 Grok quota re-check (class O when quota recovers)

1. Quota re-check first (cheap, no build): `date -u +%Y-%m-%dT%H:%M:%SZ; grok -p "Reply with exactly: PONG" --no-auto-update --output-format json`.
   - Still 402 (today's exact output above, 2026-11-08T13:05:49Z): append the dated result to `rewrite/blockers/T03-grok.md` ("quota still exhausted, HTTP 402, host login intact"), T03 stays `unavailable`, stop.
   - A reply arrives: continue with the full class O steps.
2. Host login state: `grok models` -> `You are logged in with grok.com. Default model: grok-4.7` (login intact, quota live).
3. Account add: `./target/debug/jackin --gateway jackin-local account add grok --name grok-live --from-host-login` -> exit 0 (defect fixed at 7c30d78c4; provider `jackin-grok-live-grok` already exists on jackin-local from 2026-11-06 — expect `provider: skipped, the provider exists`).
4. Flip T03 `Unavailable -> Supported` in the temp worktree; sentinel `[agents.grok]`. Stale-defect note: the A6 defect list ("install targets empty", "hosts only x.ai") no longer matches HEAD — `GROK_BUILD_SPEC` now pins 1.0.77 with both Linux targets and hosts `x.ai`, `cli-chat-proxy.grok.com`, `auth.x.ai`. Re-check `crates/jackin-core/src/assets/t03-grok.yaml` endpoints against that host list before launch.
5. Launch: `DOCKER_CONTEXT=default ./target/debug/jackin load sentinel /Users/donbeave/Projects/jackin-project/jackin-sentinel --agent grok --provider grok --account grok-live --gateway jackin-local --task "In the working directory, create a file named a14-t03-proof.txt containing exactly the line: a14-t03-ok. Then stop."`
6. Sandbox fixed prompt: `openshell sandbox exec -g jackin-local <sbx> -- sh -c 'cd /sandbox/project && grok -p "Answer exactly: PONG" --no-auto-update'` -> `PONG`; then the task; `od -c /sandbox/project/a14-t03-proof.txt` -> 12 bytes `a 1 4 - t 0 3 - o k \n`.
7. Placeholder scan: `/sandbox/.grok/auth.json` -> mode `600`, `grep -c placeholder` >= 1, no real session key.
8. Refresh + concurrency (class O steps 4-5): sandbox auth.json mtime/content unchanged across a gateway refresh (record `stat` before/after); while the sandbox is up, host `grok -p "Reply with exactly: HOST-PONG" --no-auto-update` -> `HOST-PONG` (host and sandbox logged in at the same time).
9. Subscription billing basis: the reply must come from the subscription, not credits — before the run confirm Auto Top-up off and prepaid balance zero (research/agent-provider.md "Grok exhausted-limit test precondition"); record the check.
10. Second-sandbox isolation as T02 step 8; cleanup; flip in-repo with `rewrite/evidence/A14-grok.md` or dated blocker update.

## T09 omp Z.AI re-proof after the pin bump

Purpose: T09 must stay `Supported` after the baseline pin bump (`f539d9a` -> `1d7f63dbb13ecb2bf3e3295e7a296dc2080a62b1`) and the A10-A13 catalog work. No tuple flip needed; run from the main checkout at HEAD.

1. Build: `mise exec -- mbx +1.99.1 build -p jackin`.
2. Usage before: `cd /tmp && omp usage` -> record percentages only (2026-11-08T13:07Z: 5h 5.0%, weekly 38.0%, plan max).
3. Launch: `DOCKER_CONTEXT=default ./target/debug/jackin load sentinel /Users/donbeave/Projects/jackin-project/jackin-sentinel --agent omp --provider zai --account omp-zai --gateway jackin-local --task "In the working directory, create a file named a14-t09-proof.txt containing exactly the line: a14-t09-ok. Then stop."` -> `provider: skipped, the provider exists`, image built, sandbox ready.
4. Drive: `openshell sandbox exec -g jackin-local <sbx> -- sh -c 'cd /sandbox/project && omp -p --no-session "In the working directory, create a file named a14-t09-proof.txt containing exactly the line: a14-t09-ok. Then stop."'`
5. File proof: `openshell sandbox exec -g jackin-local <sbx> -- od -c /sandbox/project/a14-t09-proof.txt` -> `a 1 4 - t 0 9 - o k \n` (12 bytes).
6. Hygiene: `grep -c placeholder /sandbox/.omp/config.toml /sandbox/.omp/auth.json` -> >= 1 each; read nothing else.
7. Usage after: `omp usage` -> the 5h or weekly percent moves, or record sandbox-run token counts (A9 pattern); no pay-as-you-go surface.
8. Cleanup: purge, sandbox list empty, config backup/restore. Append the dated re-proof to `rewrite/evidence/A4-omp-zai.md`.

## T01 Codex regression

1. Build at HEAD as above.
2. Host auth metadata before: `stat -f "mode=%Sp size=%z mtime=%Sm" ~/.codex/auth.json` (2026-11-08: `-rw-------`, 4250 bytes, `Oct 29 22:50:55 2026` — unchanged since A9).
3. Launch: `DOCKER_CONTEXT=default ./target/debug/jackin load sentinel /Users/donbeave/Projects/jackin-project/jackin-sentinel --agent codex --account codex-local --gateway jackin-local --task "In the working directory, create a file named a14-t01-proof.txt containing exactly the line: a14-t01-ok. Then stop."` -> sandbox ready; `agents.json` lists the codex instance.
4. Drive (A9 pattern; do not pin `gpt-5.1-codex` — rejected on ChatGPT accounts, A9 finding; roles already dropped the pin): `openshell sandbox exec -g jackin-local <sbx> -- sh -c 'cd /sandbox/project && env HOME=/sandbox/.jackin/homes/codex codex exec --sandbox danger-full-access -c check_for_update_on_startup=false "In the working directory, create a file named a14-t01-proof.txt containing exactly the line: a14-t01-ok. Then stop."'` -> created-file report, exit 0.
5. File proof: `od -c /sandbox/project/a14-t01-proof.txt` -> `a 1 4 - t 0 1 - o k \n` (12 bytes).
6. Placeholder scan: `/sandbox/.jackin/homes/codex/.codex/auth.json` -> mode 600, `openshell:resolve:...` placeholder reference, `OPENAI_API_KEY` null, no real token.
7. Host unchanged: repeat step 2 -> identical size and mtime.
8. Cleanup: purge; sandbox list empty; config restore.
9. Full G-E2E gate (conventions item 7) -> expect `RESULT PASS=19 FAIL=0 SKIP=0 ... a1_host_login=unchanged` (A9 baseline). Record in `rewrite/evidence/A14-codex.md` or append to `A9-e2e.md`.

## Open facts for A14 (stated, not guessed)

1. `account add --from-host-login` accepts `grok` and `muse` only (`crates/jackin/src/commands/account.rs`: "--from-host-login needs the grok or muse provider"). Claude and antigravity import code does not exist yet; both scripts above start with that implementation.
2. Catalog states at HEAD: T01/T09 `Supported`; T02/T05 `BlockedVendor` ("vendor clarification pending"); T03 `Unavailable` ("not yet proven on a real gateway"). Flips happen only after each live gate passes (stop rules in GOAL-MULTI-AGENT.md).
3. agy host credential store name is unverified and sandbox credential delivery is undefined (spec `auth_file` is settings.json) — the no-keyring proof decides T05; record the mechanism chosen.
4. Claude sandbox pin 2.1.311 vs host 2.1.319: expected skew; record both, do not "fix" by pinning host.
5. `xtask e2e` has no per-agent flags; per-tuple gates stay manual launches; G-E2E covers the sentinel journeys.
6. The A4 `jackin workspace rm` gateway-probe defect (probes down `jackin-docker`) may still apply to temp-workspace cleanup; verify or use the recorded direct-config-edit fallback.
