> Research input dated 2026-11-01. It is evidence, not authority. If it disagrees with [DESIGN.md](../DESIGN.md), DESIGN.md is correct. Grok text is out of scope.

# Grok Build and Codex CLI for jackin (research, accessed 2026-11-01)

Source shorthand (all accessed 2026-11-01):
- GB = github.com/xai-org/grok-build @ SOURCE_REV 559751f (pushed 2026-09-29), docs at crates/codegen/xai-grok-pager/docs/user-guide/*.md
- CX = github.com/openai/codex @ tag rust-v0.166.0 (published 2026-10-29T19:47Z)
- OS = github.com/NVIDIA/OpenShell @ main; latest release v0.1.6 (2026-10-27)
- CXD = learn.chatgpt.com/docs/* (developers.openai.com/codex/* returns 308 to it)

## 0. Is "Grok Build" real?
Yes, released. It is not ambiguous.
- Official name: Grok Build ("SpaceXAI's terminal-based AI coding agent"). Binary: `grok`. Repo: https://github.com/xai-org/grok-build (README; Apache-2.0 first-party code; no external contributions).
- Docs: https://docs.x.ai/build/overview (the .md variant 404s). Changelog: x.ai/build/changelog (403 to curl, not read).
- Third-party blogs say announced 2026-05-25 as beta (mer.vin, lead only). Stable channel is now 1.0.x.
- Recommendation: target the `grok` binary installed from https://x.ai/cli/install.sh (or the npm package below). Do not target the `ingebyd/grok-build` mirror or any third-party "grok CLI".
- jackin already has a Grok adapter (crates/core/jackin-core/src/agent/adapters/grok.rs: install from https://x.ai/cli/install.sh, XAI_API_KEY, ~/.grok/auth.json) and a Codex adapter (adapters/codex.rs: install from https://chatgpt.com/codex/install.sh, OPENAI_API_KEY, ~/.codex/auth.json, CODEX_HOME).

## 1. Names, distribution, versions, Linux arch

### Grok Build
- Latest stable: 1.0.70. Evidence: `curl https://x.ai/cli/stable` returned `1.0.70` (last-modified 2026-10-24); `npm view @xai-official/grok` dist-tags `latest 1.0.70`, `alpha 1.0.73`, modified 2026-10-28.
- Install: `curl -fsSL https://x.ai/cli/install.sh | bash` or `... | bash -s <version>` (GB 01-getting-started.md). Env: GROK_CHANNEL (stable|alpha|enterprise), GROK_BIN_DIR (default ~/.grok/bin) (install.sh lines 6, 378, 202).
- npm package `@xai-official/grok` exists ("Bring Grok into your terminal"). npm is a secondary path; install.sh is the documented one.
- Artifacts: `https://x.ai/cli/grok-<version>-<os>-<arch>` with fallback https://storage.googleapis.com/grok-build-public-artifacts/cli (install.sh lines 200-201, 377, 423).
- Linux x86_64 and aarch64 both supported (install.sh lines 175-183: `Linux->linux`, `arm64|aarch64->aarch64`, `x86_64|amd64->x86_64`).
- Self-update: `grok update`; `auto_update = true` by default in config; `--no-auto-update` flag (GB 05-configuration.md:46, 14-headless-mode.md).
- Pin the version at image build time and set `auto_update = false`.

### Codex CLI
- Name: Codex CLI (OpenAI), binary `codex`. Latest: 0.166.0. Evidence: `gh release view` rust-v0.166.0 (2026-10-29); `npm view @openai/codex` dist-tags latest 0.166.0, alpha 0.166.0-alpha.6.
- Install options (CX README.md): `curl -fsSL https://chatgpt.com/codex/install.sh | sh`; `npm install -g @openai/codex`; `brew install --cask codex`; GitHub release tarballs. Installer downloads from releases.openai.com/codex, falls back to GitHub Releases; `CODEX_INSTALLER_USE_RELEASES_OPENAI_COM=false` forces GitHub.
- Linux: x86_64 and aarch64 musl static builds in the release (`codex-x86_64-unknown-linux-musl.tar.gz`, `codex-aarch64-unknown-linux-musl.tar.gz`); npm optionalDependencies include `@openai/codex-linux-x64` and `-linux-arm64`.
- Release also ships sibling binaries `codex-code-mode-host`, `codex-exec-server`, `bwrap-*`. jackin's adapter already copies `codex-code-mode-host` next to `codex`.
- Tarball and Linux bwrap helper are in the same release, so a pinned download by tag is straightforward.

## 2. Auth modes

### Grok Build (GB 02-authentication.md, 01-getting-started.md)
- Browser OAuth to auth.x.ai (default; `grok login --oauth`). Tokens stored at `~/.grok/auth.json` (0600); MCP OAuth tokens in `~/.grok/mcp_credentials.json`. `GROK_HOME` relocates the directory.
- Device code: `grok login --device-auth` (alias `--device-code`). Docs say it is for SSH, Docker and remote VMs. Source references https://accounts.x.ai/device and https://accounts.x.ai/oauth2/device. Works in a remote sandbox if egress to those hosts is allowed. I did not run it.
- API key: `XAI_API_KEY` (also accepts legacy `GROK_CODE_XAI_API_KEY`, 11-custom-models.md:114). Used only as fallback if no session token in auth.json. Per-model `api_key`/`env_key` in config.toml outranks both.
- Enterprise: OIDC (`GROK_OIDC_ISSUER`, `GROK_OIDC_CLIENT_ID`), and an external auth provider command (`GROK_AUTH_PROVIDER_COMMAND`, stdout is the token, JSON with expires_in allowed). The external provider is the documented hook for "sandboxed VMs, CI runners". It is the most natural point to integrate an OpenShell-brokered token, but the token still ends up in `~/.grok/auth.json` (hot-reloaded).
- `GROK_DEPLOYMENT_KEY` is used by install.sh for enterprise channel (already in jackin env_model.rs:41).
- Subscription gating: browser login ties to a grok.com account/subscription. The beta was for SuperGrok/X Premium+ per a third-party article (unverified for 1.0).

### Codex (CXD /docs/auth; CX login source)
- ChatGPT sign-in (subscription), API key, and "Codex access tokens" for enterprise automation.
- Env/stdin: `OPENAI_API_KEY` via `codex login --with-api-key`; `CODEX_ACCESS_TOKEN` via `codex login --with-access-token`. `CODEX_API_KEY` is read by `codex exec` for a single run (CXD non-interactive page; constant `CODEX_API_KEY_ENV_VAR` in login/src/auth.rs).
- Storage: `~/.codex/auth.json` (plaintext) or OS keyring; `cli_auth_credentials_store = file|keyring|auto|ephemeral`. `CODEX_HOME` relocates it.
- Device code: `codex login --device-auth`, described as preferred for headless/remote; marked beta; must be enabled in ChatGPT security or workspace settings.
- Other headless options: copy `auth.json`; forward callback port 1455.
- Tokens refresh automatically during use (auth.openai.com/oauth/token).

## 3. Config, instructions, agent-level sandbox

### Grok
- Config: `~/.grok/config.toml` (user), `.grok/config.toml` (project; only MCP servers, plugins, permission rules load from it), `~/.grok/pager.toml` (TUI), `~/.grok/sandbox.toml`, `managed_config.toml`, `requirements.toml` (05-configuration.md:40, 323, 839-841).
- Instruction files: AGENTS.md at every level root-to-cwd (deeper wins); also loads CLAUDE.md/Claude.md/CLAUDE.local.md and `~/.claude/`, `~/.cursor/` by default (Claude compatibility on); `.grok/rules/*.md`, `$GROK_HOME/rules/`. Startup loading requires folder trust (`--trust`) (12-project-rules.md:3, 19-26, 30-42).
- Permission modes: default prompt, `auto`, `acceptEdits`, `dontAsk`, `plan`, `bypassPermissions` ("always-approve"). CLI `--yolo` / `--always-approve` / `--permission-mode bypassPermissions`; `--allow` / `--deny` rules (22-permissions-and-safety.md:40, 71, 300; 14-headless-mode.md).
- Own OS sandbox is off by default (`--sandbox off|workspace|devbox|read-only|strict`, 18-sandbox.md). Uses Landlock and seccomp on Linux, plus bubblewrap for some profiles. Profile `devbox` is described as "disposable dev VMs".
- Interaction with OpenShell: OpenShell requires Landlock ABI 3 and seccomp user-notification itself (OS docs/about/support-matrix.mdx, "Kernel Requirements"). Leave Grok's `--sandbox` at `off` (default) inside OpenShell and use `--yolo`/`bypassPermissions` for non-interactive runs. Whether Grok's Landlock or bwrap can nest inside OpenShell was NOT tested. Do not enable it.
- Folder trust prompt must be pre-granted (`grok --trust`) or the project rules are skipped.

### Codex
- Config: `~/.codex/config.toml` (user; `$CODEX_HOME`), project `.codex/config.toml` (subject to trust), `requirements.toml`; docs are split across learn.chatgpt.com/docs/config-basic (404 to my fetch), config-advanced, config-reference. I did not read those pages.
- AGENTS.md: `AGENTS.override.md` is preferred over `AGENTS.md` (core/src/agents_md.rs:44-45); user doc and project docs are concatenated with a separator; skipped if project is untrusted (agents_md.rs:56-58). `project_doc_max_bytes` caps size.
- Sandbox modes: `read-only`, `workspace-write`, `danger-full-access`. Approval policies: `on-request`, `never`, `granular` (`untrusted` retired). Full bypass: `--dangerously-bypass-approvals-and-sandbox` (alias `--yolo`) (CXD /docs/agent-approvals-security).
- Linux sandbox uses `bwrap` + seccomp. CXD says containers may block the namespaces and the fix is "run Codex with `--sandbox danger-full-access`" after configuring the outer container (same page).
- `codex exec` defaults to a read-only sandbox (CXD non-interactive page). `--full-auto` is removed.
- Recommendation for OpenShell: `codex exec --sandbox danger-full-access` (set `approval_policy = "never"`, which exec already implies). NVIDIA's own harness does exactly this: `codex exec --skip-git-repo-check --sandbox danger-full-access --ephemeral [--ignore-user-config] [--ignore-rules]` (OS scripts/agents/runtime/harnesses/codex/exec.sh).
- Tension: `--ignore-user-config` skips `$CODEX_HOME/config.toml`, which defeats a jackin-written config. Use `-c key=value` overrides instead.

## 4. Non-interactive mode and output

### Grok (14-headless-mode.md)
- `grok -p "<prompt>"` (alias `--single`); also `--prompt-file`, `--prompt-json`.
- `--output-format plain|json|streaming-json|streaming-messages-json`; `--include-partial-messages`.
- Other flags: `-m`, `--cwd`, `-r/--resume`, `-c/--continue`, `-s/--session-id`, `--max-turns`, `--tools`, `--disallowed-tools`, `--yolo`, `--allow/--deny`, `--effort`, `--no-auto-update`.
- `json` result carries text, stopReason, sessionId, usage, modelUsage, total_cost_usd (cost only for API-key traffic today).
- Headless sets `RUST_LOG=off` by default, so only the answer prints.
- Interactive: plain `grok` runs the full-screen TUI (needs a PTY). ACP mode: `grok agent stdio` (05-configuration.md:25).
- I did not read the event schema for `streaming-json`. It is in lines 200+ of 14-headless-mode.md.

### Codex (CXD non-interactive page)
- `codex exec "<prompt>"`, `codex exec -` reads stdin; piped stdin plus an argument is treated as context.
- Progress goes to stderr, final message to stdout. `--json` gives JSONL events (`thread.started`, `turn.started`, `turn.completed`, `turn.failed`, `item.*`, `error`). `-o/--output-last-message <file>`, `--output-schema <file>`, `--ephemeral`, `--skip-git-repo-check`, `codex exec resume --last|<id>`.
- Interactive: `codex` TUI (PTY). There is also `codex --remote ws://...` plus `codex-app-server` (used by NVIDIA's example).

## 5. Egress allow-list

### Grok (hosts from install.sh and GB source strings; usage per host partly inferred)
- Install and update: `x.ai` (install.sh, `/cli/<channel>`, artifacts) and `storage.googleapis.com` (fallback bucket `grok-build-public-artifacts`). After image build, only needed for `grok update` or auto-update.
- Auth: `auth.x.ai` (`/authorize`, `/token`), `accounts.x.ai` (device flow: `/device`, `/oauth2/device`, sign-in). Needed only for OAuth or device login.
- Inference with a session login: `cli-chat-proxy.grok.com` (`/v1/responses`, `/v1/models`, `/v1/chat/completions`, `/v1/messages`, `/v1/deployment/config`). Config key `endpoints.cli_chat_proxy_base_url`, env `GROK_CLI_CHAT_PROXY_BASE_URL`.
- Inference with `XAI_API_KEY`: `api.x.ai` (`/v1`, `/v1/models`, `/v1/api-key`, `/v1/responses`). Config `endpoints.xai_api_base_url` / `GROK_XAI_API_BASE_URL`. Inferred: API-key sessions use api.x.ai; I did not confirm which host the default model entry uses with a key.
- Other hosts in source: `proxy.grok.com`, `server.grok.com`, `code.grok.com`, `assets.grok.com`, `api.mixpanel.com` (product analytics, inferred from the string; gated by `[features] telemetry`, `GROK_TELEMETRY_ENABLED`). Session trace upload goes through the proxy or a configured bucket (`trace_upload` follows telemetry).
- Telemetry is a documented master switch; set `GROK_TELEMETRY_ENABLED=0` and `trace_upload = false`. The default value is unclear: 05-configuration.md:93 shows `telemetry = false` under an example block, so I could not confirm the shipped default.
- Web search/fetch tools and MCP servers add arbitrary hosts. Remove with `--disallowed-tools web_search,web_fetch` if not wanted.

### Codex (OS providers/codex.yaml header; CX source strings)
- NVIDIA's profile allows: `api.openai.com:443`, `auth.openai.com:443`, `chatgpt.com:443`, `ab.chatgpt.com:443` (the last for experiment lookups). Also in source: `files.openai.com`, `chat.openai.com` (legacy), `gov.chatgpt.com`/`gov.api.openai.com` (gov accounts only).
- ChatGPT-login inference: `https://chatgpt.com/backend-api/codex/responses` and `/models`. API-key inference: `https://api.openai.com/v1/responses`.
- Token refresh: `https://auth.openai.com/oauth/token`. Browser login: `auth.openai.com/oauth/authorize`, local callback on port 1455.
- Update check (interactive TUI only): `registry.npmjs.org/@openai%2fcodex` (npm installs), `formulae.brew.sh/api/cask/codex.json` (brew), plus a GitHub releases check (`api.github.com/repos/openai/codex/releases/latest`) (tui/src/npm_registry.rs:5, updates.rs:61). Config key `check_for_update_on_startup` (core/src/config/mod.rs:1189). Disable it in a baked image.
- Telemetry: metrics exporter defaults to `statsig` and follows `analytics.enabled` (config/src/types.rs:660). Host not confirmed. `[otel]` is off by default per docs. The `ab.chatgpt.com` host appears only in NVIDIA's profile, not in Rust source I grepped.
- Web search default is `cached` (no live egress); `live` needs open egress.

## 6. NVIDIA OpenShell support

- OpenShell v0.1.6 (2026-10-27). Docs: https://docs.nvidia.com/openshell/latest/ ; source docs in OS/docs.
- Codex: yes, as an example, not built in.
  - `providers/codex.yaml` is a reviewable example. It must be imported explicitly (`openshell provider profile import -f providers/codex.yaml --global`). `providers/README.md`: "OpenShell does not load it".
  - Declares credentials `CODEX_AUTH_ACCESS_TOKEN`, `CODEX_AUTH_REFRESH_TOKEN`, `CODEX_AUTH_ACCOUNT_ID`, `CODEX_AUTH_ID_TOKEN`. Endpoints as listed in section 5. Its `binaries` allow-list is `/usr/bin/codex`, `/usr/local/bin/codex`, `/usr/lib/node_modules/@openai/**`. For a `~/.local/bin/codex` install (jackin's layout) the list must be edited, otherwise credentials are not injected and traffic is denied (providers/README.md "Read the header before importing").
  - `examples/codex-app-server/` (Dockerfile: ubuntu:24.04 + nodejs/npm + `npm install -g @openai/codex@latest`; README shows provider creation from `~/.codex/auth.json`, `openshell provider refresh configure ... --strategy oauth2-refresh-token`, and `openshell sandbox create --provider codex ... -- <cmd>`). In that flow the sandbox gets opaque handles for access token and account id and never receives the refresh token.
  - `scripts/agents/runtime/harnesses/codex/{install-codex.sh,exec.sh}` show NVIDIA's own pattern: `npm install -g @openai/codex@<ver>`, then write `$HOME/.codex/auth.json` with `auth_mode: "chatgpt"` and the env values (which are placeholders, `openshell:resolve:*`) so the CLI sees a well-formed file. Account id must be a literal, not a placeholder.
- API-key path: `providers/openai.yaml` exists (not read in detail). Credential injection works by env placeholders that the sandbox proxy rewrites on the wire, only for hosts, ports and paths declared by the profile (docs/how-it-works/providers/profiles.mdx lines 28, 47, 206). So real keys are not written into the sandbox.
- Grok: no. `gh api search/code` for "grok" in repo found only an unrelated hit (docs/kubernetes/access-control.mdx); no `providers/grok.yaml` or `xai.yaml`; `providers/` lists anthropic, aws*, claude-code, codex, copilot, cursor, deepinfra, github, google*, nvidia, oci-genai, openai, openrouter, pypi. A custom profile is needed for Grok (id e.g. `grok`, credential `XAI_API_KEY`, endpoints `api.x.ai`, optionally `cli-chat-proxy.grok.com`, binaries `[/home/agent/.grok/bin/grok]` or the resolved path).
- Docs: docs/about/overview.mdx line 53 lists "Claude Code, OpenCode, Codex, or GitHub Copilot CLI" as supported patterns. docs/about/run-your-first-agent.mdx: "OpenShell can run any agent that is available in a sandbox image". Default workload image is `nvcr.io/nvidia/base/ubuntu:24.04` for linux/amd64 and linux/arm64 and "does not bundle agent CLIs" (support-matrix.mdx). Use `openshell sandbox create --from <image> --provider <name> -- <cmd>`.
- Interactive PTY: `openshell sandbox create ... -- <cmd>` is attachable; the codex example uses `--no-tty` explicitly, so TTY is the default. Detailed PTY semantics not read.

## 7. Credential-never-in-sandbox: what is feasible
- API-key mode works cleanly for both: provider stores the key; sandbox env holds a placeholder; proxy rewrites the Authorization header for declared hosts. Unverified that Grok's `XAI_API_KEY` placeholder passes Grok's `/v1/api-key` probe (api_key_probe.rs exists in xai-grok-login) rather than being rejected locally for shape (`xai-...`). Needs a live test.
- Codex ChatGPT-login mode works with the NVIDIA pattern: the access token and account id are placeholders; refresh token stays on the gateway. A placeholder-bearing `auth.json` is still written into the sandbox, so "never written" is only true for the real secret values.
- Grok OAuth mode has no equivalent. Grok owns refresh against `auth.x.ai` using the refresh token stored in `~/.grok/auth.json`. OpenShell has no Grok refresh strategy. The `oauth2-refresh-token` strategy needs a known client_id; Grok's is not documented. Real tokens would land in the sandbox. Prefer `XAI_API_KEY` for Grok.

## Version notes
- Grok: stable 1.0.70; alpha 1.0.73. GB docs examples still show 0.1.42 and 0.2.x numbers (stale doc examples). Docs reference `grok-4.6` model.
- Codex: 0.166.0 stable; `--full-auto` removed, `untrusted` approval retired, `--approve-for-me` referenced in docs (not verified in source).
- OpenShell example uses `@openai/codex@latest`; pin instead.

## Not verified
- Live behavior of `grok login --device-auth` inside an OpenShell sandbox; Grok's Landlock/bwrap sandbox nesting in OpenShell (kept off).
- Whether Grok accepts an OpenShell placeholder as `XAI_API_KEY`; which host a default-model request uses with a key.
- Grok telemetry default and full list of telemetry hosts (mixpanel is a string match only). x.ai/build/changelog (403).
- Codex statsig/analytics host; Codex config-basic/advanced/reference pages and `project_doc_fallback_filenames`.
- OpenShell docs for PTY attach and for symlink resolution of `binaries` paths (Grok's installer places a symlink in ~/.grok/bin to ~/.grok/downloads).
- `providers/openai.yaml` and `claude-code.yaml` content.
