# A14 live proof T14 (Claude Code with Z.AI) and T18 (Codex with Z.AI)

Verdicts: T14 PASS, T18 PASS (vehicle notes and defects below).

## Versions

- jackin 0.1.0 built in temp worktree /tmp/jackin-live-zai2 (detached HEAD df7674dd4 "test(session): use T14 for tuple support ordering after T10 flip"). Proof-only diff there (no commit, no push, main checkout untouched):
  1. T14 and T18 `TupleState::Unavailable` -> `Supported`, reason -> `None` (crates/jackin-core/src/agent.rs, the sanctioned flip pattern of edf0517ab).
  2. Claude Code install triples `linux-arm64`/`linux-x64` -> `arm64`/`x64` (crates/jackin-core/src/agent.rs). The template already carries `linux-`; the doubled URL `.../2.1.311/linux-linux-arm64/claude` failed the checksum ADD (defect 3). The pinned digests are correct for the single-`linux` URLs (arm64 2e2496c9..., amd64 86dc1fcc... both re-hashed from the vendor URLs at proof time).
  3. `sync_login` static-key publish: gate `agent != Agent::Codex` -> `|| static_publish` (tuple auth class StaticKey|ApiKey), and credential env names resolved per tuple — omp -> `omp_credential_env`, claude -> `claude_auth_env`, codex -> `codex_credential_env` (crates/jackin-session/src/accounts.rs). Without this, T14 account add failed `no credential is available` (Claude spec `credential_env_names` is empty) and T18 account add silently returned HostOnly without publishing (defect 1).
  4. Profile asset t14-claude-zai.yaml `env_vars: [ZAI_API_KEY]` -> `[ANTHROPIC_AUTH_TOKEN]` (crates/jackin-core/src/assets/). The gateway rejects publish keys not declared by the profile and brokers the token under the profile env name; ANTHROPIC_AUTH_TOKEN is the name Claude Code reads (defect 2).
- Build `mise exec -- mbx +1.99.1 build -p jackin` with the touched sources (cache bypass habit, A14 defect 3); every rebuild picked up the edits.
- OpenShell 0.1.7, gateway jackin-local (mTLS, https://localhost:17670). Claude Code 2.1.311 (downloads.claude.ai, checksum-pinned ADD in the role image build), Codex v0.166.0.
- Credential: Z.AI plan key from the host omp store (`omp --profile zai token zai`) into the environment only (49 chars, matches the omp-lane T09 reproof), never printed, never written to a file, never passed as exec argv.

## Common launch shape

- Role: temp worktree /tmp/jackin-wt-sentinel3 (detached 89b2ee8) with `[agents.claude] model = "glm-5.3" instructions = "instructions/claude.md"` added beside the existing `[agents.codex]`/`[agents.omp]` (instructions/claude.md follows the codex/omp pattern; the concurrent sentinel2 worktree of the native worker was left untouched). The role manifest renames the role to `sentinel-zai` because the registered git source `sentinel` (pinned by the operator workspace `jackin`) cannot be repointed; registered with `jackin role add sentinel-zai /tmp/jackin-wt-sentinel3`.
- Project: clean main checkout /Users/donbeave/Projects/jackin-project/jackin-sentinel, workspace `a14zai`, gateway jackin-local.
- Accounts: `jackin account add zai --name claude-zai --agent claude --credential env:ZAI_API_KEY` and `jackin account add zai --name codex-zai --agent codex --credential env:ZAI_API_KEY`, both exit 0, rows `claude-zai zai env`, `codex-zai zai env`. Gateway providers `jackin-claude-zai-claude` (t14-claude-zai) and `jackin-codex-zai-codex` (t18-codex-zai) published at resource version 1.
- Launch: `DOCKER_CONTEXT=default jackin load sentinel-zai <project> --agent <a> --provider zai --account <acct> --gateway jackin-local --task "Answer exactly: PONG. Then use one tool to list files in the current directory."` — T14 sandbox `jackin-a14zai-p8ca` (first build pulled claude 2.1.311 into the role image), T18 sandbox `jackin-a14zai-2qbi` (layers cached). Both Ready; agent driven through `openshell sandbox exec` (known A4 non-TTY behavior).

## T14 — Claude Code with Z.AI

- settings.json `/sandbox/.claude/settings.json` exactly `{"env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic","ANTHROPIC_AUTH_TOKEN":"placeholder"}}`, mode 0600, placeholder count 1; `/sandbox/.claude/.credentials.json` = `{}`; no other credential files.
- Credential delivery: sandbox env `ANTHROPIC_AUTH_TOKEN` is a 63-char gateway token (raw key 49) from the brokered provider attach; `ZAI_API_KEY` unset; launch env file holds only the role env (`export SENTINEL_FAIL_PREFLIGHT`).
- Sanctioned-config run FAILED (defect 4): with the published settings.json in effect, `claude -p ... --model glm-5.3` ends `Failed to authenticate. API Error: 401 token expired or incorrect` — the settings.json `env.ANTHROPIC_AUTH_TOKEN: "placeholder"` overrides the brokered process-env token.
- Vehicle run (sanctioned credential, minimal config): `CLAUDE_CONFIG_DIR=/tmp/cc` with `{"env":{"ANTHROPIC_BASE_URL":"https://api.z.ai/api/anthropic"}}` (no auth entry) so the brokered env token flows: strict run answers exactly `PONG` (exit 0, 1.9 s); tool run lists /sandbox/project (`config`, `hooks`, `instructions`, `probe`, `tests`, `jackin.role.toml`, `policy.yaml`, `Dockerfile`, `AGENTS.md`/`CLAUDE.md`, `README.md`, ...), exit 0. Model warning `[claude-code:unrecognized_model] glm-5.3` (assumes 200k window) is cosmetic.
- Hygiene: uid 1000(sandbox); `sudo: not found` rc 127; `claude` PID 677 child of `/bin/sh /usr/local/bin/jackin-start claude` PID 372 child of 1, both user sandbox, state `running 1`; raw-key sha256 absent from every 40+ char token found under /sandbox/.claude and /sandbox/.jackin (5 candidates hashed) and from env (63 != 49); /proc/677/environ unreadable cross-session (hardening).
- Export gate: sandbox commit `ba223e2 docs: t14 export proof marker` on feature/openshell-native; guarded `jackin purge jackin-a14zai-p8ca` refused `error: the work in sandbox jackin-a14zai-p8ca is not exported (branch feature/openshell-native not exported); run jackin export jackin-a14zai-p8ca or pass --force`, exit 3.

## T18 — Codex with Z.AI

- config.toml `/sandbox/.codex/config.toml`: `model = "glm-5.3"`, `model_provider = "zai"`, `[model_providers.zai] base_url = "https://api.z.ai/api/v1"`, `env_key = "ZAI_API_KEY"`, `name = "Z.AI"`, `wire_api = "responses"`; placeholder count 0; `auth.json` ABSENT.
- Credential delivery: sandbox env `ZAI_API_KEY` is a 55-char gateway token (raw 49); `ANTHROPIC_AUTH_TOKEN` unset (no cross-lane leakage); codex PID 667 child of `jackin-start codex --sandbox danger-full-access -c check_for_update_on_startup=false` PID 372.
- Answer: `codex exec --skip-git-repo-check "Answer exactly: PONG. ..."` → `provider: zai`, `model: glm-5.3`, final text `PONG.`, `tokens used 2,198`, exit 0 (live traffic through api.z.ai/api/v1 with the brokered token). Tool run: exactly one shell command listing /sandbox/project (full sentinel listing: `.github`, `AGENTS.md`, `Dockerfile`, `jackin.role.toml`, `mise.toml`, `policy.yaml`, `probe`, `tests`, ...), exit 0.
- Tool sandbox note (defect 5): codex's own bwrap sandbox panics in the sentinel image (`bubblewrap is unavailable: no system bwrap ... no bundled codex-resources/bwrap`), so a default-sandbox `codex exec` tool call fails; the sanctioned launch shape already runs `--sandbox danger-full-access` (the OpenShell sandbox is the fence) and the tool call then works.
- Hygiene: uid 1000(sandbox); sudo not found; raw-key sha256 absent from every 40+ char token under /sandbox/.codex and /sandbox/.jackin (7 candidates hashed); brokered token only.
- Export gate (full cycle): sandbox commit `5dff653 docs: t18 export proof marker`; guarded purge refused with the same unexported-work error, exit 3; `jackin export jackin-a14zai-2qbi` published `refs/heads/jackin/jackin-a14zai-2qbi/feature/openshell-native -> 5dff653b` (host HEAD 89b2ee8 and working tree unchanged, 0 modified); guarded purge after export: `deleted sandbox jackin-a14zai-2qbi`, exit 0.

## Cleanup

- Sandbox jackin-a14zai-p8ca purged with --force after the refusal proof; jackin-a14zai-2qbi deleted by the guarded post-export purge. `openshell sandbox list -g jackin-local` -> no sandboxes.
- Exported ref deleted from the host role repo (`git update-ref -d`); host role repo clean (0 modified, 0 jackin/* refs).
- Accounts claude-zai and codex-zai removed; workspace a14zai removed; role source sentinel-zai removed; `~/.config/jackin/config.toml` byte-identical to the pre-proof backup (1807 bytes).
- Gateway providers `jackin-claude-zai-claude` (t14-claude-zai) and `jackin-codex-zai-codex` (t18-codex-zai) left in place (published through the sanctioned account path; left for the operator, as A14 left minimax/mimo).
- Temp worktrees /tmp/jackin-live-zai2 and /tmp/jackin-wt-sentinel3 kept (hold this evidence and the proof diff); no commit or push anywhere.

## Defects (main tree)

1. `sync_login` cannot publish static-key accounts for the claude and codex lanes: the publish block is gated `agent != Agent::Codex` (so codex+zai returns HostOnly and never publishes) and non-omp env names come from `agent.spec().credential_env_names.first()`, which is empty for Claude Code, so `account add` fails `no credential is available`. Both paths were unreachable while T13-T20 were Unavailable. Fix shape proven here: gate on the tuple auth class and resolve the env name per tuple (`claude_auth_env` / `codex_credential_env`).
2. Claude-lane profile assets broker the token under the provider key name (`ZAI_API_KEY`, likewise `KIMI_API_KEY`/`MINIMAX_CODE_API_KEY`/`XIAOMI_TOKEN_PLAN_SGP_API_KEY` in t13/t15/t16) instead of the env name Claude Code reads (`ANTHROPIC_AUTH_TOKEN` for T14/T15/T16, `ANTHROPIC_API_KEY` for T13); the gateway also rejects publish keys not declared by the profile, so the account cannot be published under the right name either. Fixed for t14 in the worktree; t13/t15/t16 need the same change on main.
3. Claude Code install triples double the template's `linux-` prefix (`linux-linux-arm64` in the ADD URL) and the build then fails the checksum. The pinned digests are correct for the single-`linux` URLs; only the triples are wrong.
4. `render_claude_config` pins `ANTHROPIC_AUTH_TOKEN: "placeholder"` into settings.json; Claude Code applies settings env over the process environment, so the published config clobbers the brokered token and every sanctioned-config call dies 401. The lane works when settings.json carries only ANTHROPIC_BASE_URL and the brokered env token flows. Main fix needed (omit the auth entry, or document the gateway swap).
5. The sentinel construct lacks bubblewrap, so codex tool calls under codex's default inner sandbox panic; jackin's launch args (`--sandbox danger-full-access`) avoid it, but any user-driven default `codex exec` tool call fails. Consider shipping bwrap or codex-resources in the construct.
