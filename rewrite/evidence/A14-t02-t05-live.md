# A14 live T02/T05 — subscription imports work, launches refused below the state gate

Date: 2026-11-09. Worktree `/tmp/jackin-live-t02` (branch `wt/live-t02`, HEAD `4abb579b3`, same tip as `feature/openshell-native`). Session brief of 2026-11-09; operator order of 2026-11-08 on file. No sub-agents this session.

Scope of the worktree diff (uncommitted, per brief; `git status` shows exactly `M crates/jackin-core/src/agent.rs`, 4 lines): T02 and T05 `TupleState::BlockedVendor` -> `Supported`, reason `Some("vendor clarification pending")` -> `None`. `AuthClass` untouched. Nothing committed, nothing pushed. Build: `mise exec -- mbx +1.99.1 build -p jackin`, clean. Note: `crates/jackin-core/tests/catalog.rs` still pins T02/T05 as `BlockedVendor` at HEAD and would fail under the flipped worktree; not run, not changed (the sanctioned flip commit updates both together).

Sentinel vehicle: detached worktree `/tmp/jackin-wt-sentinel-t02` of the sentinel repo (89b2ee8), role renamed `sentinel-t02`, `[agents.claude] instructions/claude.md` and `[agents.agy] instructions/agy.md` added beside codex/omp. The validator allows it under the flipped binary: `jackin role check /tmp/jackin-wt-sentinel-t02` -> exit 0. (`[agents.agy]` needs the T05 flip: T05 is the only Antigravity tuple, `validate_agents` in `crates/jackin-core/src/manifest.rs:189` rejects agents with no Supported tuple. `[agents.claude]` also validates at HEAD via T13–T16.)

## Verdicts

- T02 (claude × Anthropic subscription): **FAIL** — import PASS, launch refused at plan time. Exact blocker below.
- T05 (agy × Google Antigravity subscription): **FAIL** — import PASS, launch refused at the image step. Exact blocker below.
- Host logins before and after: PASS on both lanes; no refresh-race exposure (no sandbox ever consumed a token).
- The ordered in-sandbox checks (placeholder-only credentials, child-of-jackin-start tree, live PONG in sandbox, no-desktop-keyring proof) are **NOT RUN**: they are contingent on a sandbox, and no sandbox could be created on either lane. The refusals are the recorded outcome.

## Environment, before (all UTC)

- 08:24:47Z `openshell -g jackin-local sandbox list` -> `No sandboxes found.` (a concurrent lane later created `jackin-rest-se-w11u` at 08:27:11Z, workspace rest-sentinel; not this lane's, left untouched).
- 08:24:55Z host `claude -p "Reply with exactly: PONG"` -> `PONG`, exit 0. 08:25:01Z host `agy -p "Reply with exactly: PONG"` -> `PONG`, exit 0.
- 08:25:18Z keychain existence-only checks: item `Claude Code-credentials` EXISTS; item service `gemini` account `antigravity` EXISTS (this is the store `ANTIGRAVITY_KEYCHAIN_ARGS` reads — the name A14 prep could not verify is now confirmed to exist). `~/.claude/.credentials.json` absent, so the claude import must and does go through the Keychain (`read_claude_host_login` falls back to the file only if it parses).
- Config snapshot taken to `/tmp/jackin-config-backup-t02.toml` (6 accounts, roles sentinel/sentinel-kimi3/sentinel-rest/the-architect, workspaces a14kimi3/arch-probe/jackin/jackin-docker-validation/rest-sentinel). No `jackin-*-claude`/`jackin-*-agy` provider existed.

## T02 claude × Anthropic subscription

- Import, 08:27:16Z: `jackin --gateway jackin-local account add claude --name claude-host --agent claude --from-host-login` -> exit 0, no secret in output, no Keychain operator prompt (the `security find-generic-password -s "Claude Code-credentials" -w` read inside `read_claude_host_login` was permitted silently). Account row `claude-host claude host-login`; gateway provider `jackin-claude-host-claude` created (profile `claude` = t02-claude.yaml, 1 credential key; brokered under `ANTHROPIC_API_KEY`, the first name of `CLAUDE_CODE_SPEC.credential_env_names` in `publish_static_login`).
- Launch, 08:27:25Z: `DOCKER_CONTEXT=default jackin --gateway jackin-local load sentinel-t02 /Users/donbeave/Projects/jackin-project/jackin-sentinel --agent claude --provider claude --account claude-host --task "…a14-t02-proof.txt…"` -> `error: tuple claude with provider claude is not supported: no anthropic-compatible base url`, exit 1. No sandbox existed immediately after.
- The refusal is past the tuple-state gate (the flip took effect; without it the error is the bare `is not supported` of A14-subscription-lanes.md) and comes from the Claude Code config renderer: `AgentTuple::claude_base_url()` (`crates/jackin-core/src/agent.rs:195`) maps only T13–T16 and returns `None` for T02, while `agent_config_file` (`crates/jackin-session/src/plan.rs:546`) routes every ClaudeCode lead through `claude_config_file`, which requires a base URL. **Blocker (exact): T02 has no `claude_base_url` entry, and the renderer has no "vendor's own endpoint, nothing to pin" path.** Fixing it needs a design decision (pin `https://api.anthropic.com`? omit the env block? how does sandbox claude authenticate a subscription OAuth token brokered as a static key — the T13–T16 lanes broker under `ANTHROPIC_AUTH_TOKEN`, T02's profile uses `ANTHROPIC_API_KEY`), not a live-proof patch; recorded, not guessed.
- Host after, 08:27:46Z: `claude -p "Reply with exactly: PONG"` -> `PONG`, exit 0; keychain item EXISTS. Refresh race: not exercised (no sandbox ran with the token); host login intact regardless.

## T05 agy × Google Antigravity subscription

- Import, 08:27:57Z: `jackin --gateway jackin-local account add antigravity --name agy-host --from-host-login` -> exit 0, no secret in output, no operator prompt. Account row `agy-host antigravity host-login`; gateway provider `jackin-agy-host-agy` created (profile `agy` = t05-antigravity.yaml, 1 credential key under `GOOGLE_API_KEY`). The import reads the `gemini`/`antigravity` Keychain item and decodes the `:`-prefixed base64 payload to `token.access_token` (`read_antigravity_host_login` + `parse_antigravity_keychain_secret`) — the previously unverified host store is now proven to exist and parse.
- Launch, 08:28:01Z: `DOCKER_CONTEXT=default jackin --gateway jackin-local load sentinel-t02 … --agent agy --provider antigravity --account agy-host --task "…a14-t05-proof.txt…"` -> `image...` then `error: launch wh533qzxquit2cs5xkxxaxyuaw failed at step image: the agent has no install target for linux/arm64`, exit 1. No sandbox created. Planning passed (role mapping, tuple state via the flip, account binding) and the failure is the image build: `ANTIGRAVITY_SPEC` pins agy 1.5.1 with an **empty install-target list** (`crates/jackin-core/src/agent.rs:763`), and `jackin-image` refuses (`MissingInstallTarget`, `crates/jackin-image/src/error.rs:17`, `key.rs:36-41`). This is the A14-prep open fact 3, now a live-confirmed blocker. A14 must add the A1 artifact targets (URL pattern `https://storage.googleapis.com/antigravity-public/antigravity-cli/{version}/linux-{target}` plus digests) before any sandbox can exist.
- No-desktop-keyring proof: NOT RUN, no sandbox by construction. The ordered env probe (no `dbus-daemon`/`secret-tool`, no `/run/dbus`, no `DBUS_SESSION_BUS_ADDRESS`) stays the deciding gate once the image builds.
- Host after, 08:28:09Z: `agy -p "Reply with exactly: PONG"` -> `PONG`, exit 0; keychain item EXISTS.

## What the live run proves about HEAD (`4abb579b3`)

1. Both host-login imports work end-to-end silently: Keychain read without operator interaction, account row, gateway provider with one credential key, no secret in any output. The T05 store name question is closed.
2. The tuple-state flip is no longer the binding constraint on either lane; both tuples now die one design layer deeper, on different steps: T02 at plan-time config rendering (needs a T02 auth/base-url design), T05 at image build (needs install targets). Neither is a planner refusal anymore.
3. The T02 credential shape is unproven by construction: the sandbox placeholder writer for `/sandbox/.claude/.credentials.json` exists (`placeholder_file` -> `{"ANTHROPIC_API_KEY": "placeholder"}`), but whether Claude Code can authenticate a subscription with the gateway-brokered `ANTHROPIC_API_KEY` token is exactly what the blocked launch cannot show.

## Cleanup and end state (08:28–08:31Z)

- `jackin account rm claude-host` / `agy-host` -> exit 0; gateway providers `jackin-claude-host-claude` and `jackin-agy-host-agy` gone (`provider list` clean of both); `jackin role rm sentinel-t02`, `jackin workspace rm a14t02` -> exit 0 each.
- Config NOT restored wholesale: a concurrent lane added accounts `kimi-native` and `omp-anthropic` after this lane's backup (diff of backup vs current shows exactly those two sections; mm-lane defect-4 precedent). This lane's additions were removed surgically; final diff vs backup is only the concurrent lane's two accounts.
- Sentinel worktree `/tmp/jackin-wt-sentinel-t02` removed (`git worktree remove --force`); sentinel repo clean, HEAD 89b2ee8 unchanged.
- Sandboxes: none created by this lane; the concurrent lane's `jackin-rest-se-w11u` still Ready and untouched. Host logins: claude PONG, agy PONG, both keychain items exist. Backup kept at `/tmp/jackin-config-backup-t02.toml`.
- `/tmp/jackin-live-t02` keeps the uncommitted 4-line flip plus this evidence file, per the brief's "flip only in this worktree, do not commit".
