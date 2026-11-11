> Research input dated 2026-11-01. It is evidence, not authority. If it disagrees with [DESIGN.md](../DESIGN.md), DESIGN.md is correct. Grok text is out of scope.

# Role repos and OpenShell-native role design

Date: 2026-11-01. Scope: read-only analysis of jackin-sentinel, jackin-agent-smith, jackin-the-architect (all `main`), plus the jackin role-manifest model, role loading/build code and construct image (`/Users/donbeave/Projects/jackin-project/jackin`, HEAD 470fc5d6).

Base path for all repos: `/Users/donbeave/Projects/jackin-project/`. Paths below are relative to the named repo.

Evidence labels: "read" = I read it in this session. "inferred" = my reading of code or docs. "not verified" = could not confirm.

## 0. Read first: a recorded decision pack already exists

`jackin/docs/openshell-v2/` (README, SPEC, CONTRACTS, PLAN, ACCEPTANCE, task cards, dated 2026-10-25) already records an OpenShell-only rewrite of jackin. It is a plan only (every task is `NOT_STARTED`; `jackin/docs/openshell-v2/README.md` "Delivery status"). The jackin tree on `main` is still v1 (manifest `v1alpha7`, `jackin/crates/core/jackin-core/src/constants.rs:10`). Nothing in v1 code has been rewritten yet.

The design below is built to fit that pack. It contradicts none of it. These are the points where the brief narrows or sharpens the pack, which the owner should confirm:

| # | Pack position | Brief position | Treatment in this design |
|---|---|---|---|
| D1 | Docker/Buildx stay as trusted image builders; no Docker execution (`SPEC.md` "Decision and product scope"; `CONTRACTS.md` C07). | "No direct Docker execution." | Consistent. Role images are still built with Buildx from the role `Dockerfile`. OpenShell does not build Dockerfiles (verified in OpenShell docs, see section 4.1). |
| D2 | Agent-neutral. Pack never names Claude, Grok or Codex except `the-architect --agent codex` in examples (`SPEC.md:391`, `:457`). | Codex is the only agent (Grok dropped 2026-11-02). | Narrowing, not a conflict. No agent enum, no `--agent` flag. Pack text that assumes selectable agents (SPEC "selectable agents", `tasks/T14.md` Amp pass condition, `SPEC.md:391`) needs edits. |
| D3 | Role ABI: user `sandbox`, `/sandbox/project`, `/sandbox/.jackin`, no sudo, no chown at start (`CONTRACTS.md` C08). | Same direction. | All three roles and the construct base violate C08 today (section 1.4). Rebase onto a new construct. |
| D4 | "Role authors declare requirements; authorized operators own the final grant. Use native OpenShell policy documents. Do not add a second policy language." (`SPEC.md` "Policy ownership"; `CONTRACTS.md` C03, C13). | Role includes an OpenShell policy. | The role ships a native `policy.yaml` verbatim. The manifest only points at it. No policy DSL in `jackin.role.toml`. |
| D5 | "Build the complete declared selectable agent set into each role image. Reject unknown or missing versions" and "Do not retain implicit fallback installers" (`CONTRACTS.md` C07; `tasks/T14.md` Removal). | Codex only. | The manifest requires an exact version pin for Codex. The `curl chatgpt.com/codex/install.sh \| bash` fallback installer in v1 is deleted. |
| D6 | `jackin.role.toml` keeps its name; `schema_version = 2`; strict unknown-field rejection; no old-schema fallback (`CONTRACTS.md` C03). | Breaking, no migration. | Delete `jackin-manifest` migrations and `jackin role migrate`. |
| D7 | Crate graph has no `jackin-manifest`; role parsing belongs to `crates/core/jackin-config`, resolution to `crates/services/jackin-roles` (`CONTRACTS.md` C02; `tasks/T14.md` write scope). | n/a | The new schema lives in `jackin-config`; `jackin-manifest` and `jackin-core/src/manifest.rs` are deleted. |

Nothing here needs an owner stop. Open questions are in section 7.

`jackin/CONTEXT.md` is a v1 glossary. These terms become wrong or need a v2 redefinition when the rewrite lands: Instance ("one agent container"), Reconnect/hardline, Durable home, Default-home, First-seed, and Role hook (`setup-once`/`source`/`preflight`, "hard-fails the start", container start). Not in the brief's scope to edit; flagged for the docs-writer.

## 1. File inventory

Legend: **CL** Claude-specific. **DK** Docker/container-runtime-specific. **CX** Codex-specific. **GK** Grok-specific. **OA** other-agent (Amp, OpenCode, Kimi) specific. **VN** Velnor CI generator output. **GEN** neutral. "dies" = removed by the rewrite. "reshape" = stays in changed form.

### 1.1 jackin-sentinel (135 commits; `git ls-files` = 34 files)

| File | What it does | Tags | Fate |
|---|---|---|---|
| `jackin.role.toml` (89 lines) | Manifest `version = "v1alpha4"`, `dockerfile`, `published_image`, `agents = [claude, codex, amp, opencode, kimi, grok]`, one table per agent with a model, `[hooks]` (all three), 12 `[env.*]` declarations covering every env feature (static default, literal `${other.VALUE}` template, free text, required text, select, select with default, skippable, `depends_on` chains, derived skippable). Lines 4-7, 12-28, 30-33, 35-39 (Claude env), 41-89. | CL (`[claude]`, `CLAUDE_CODE_*` env), OA (`[amp]`, `[kimi]`, `[opencode]`), CX, GK, DK (`published_image` on docker.io) | reshape |
| `Dockerfile` (8 lines) | `FROM projectjackin/construct:0.37-trixie@sha256:67fc...`, `SHELL`, `USER agent`. No `RUN`. | DK, GEN | reshape |
| `hooks/setup-once.sh` | Runs once per durable home. Writes `/jackin/state/jackin-sentinel/setup-once.env`. Generates `~/.local/bin/jackin-sentinel-report` (the report script) via heredoc. Report prints `JACKIN_SENTINEL_REPORT_BEGIN/END` plus env values, `JACKIN_DIND_HOSTNAME`, `TESTCONTAINERS_HOST_OVERRIDE`, hook markers, `docker ps`. | DK (DIND vars, `docker ps`, `/jackin/state`) | dies; report moves to a build-time probe |
| `hooks/source.sh` | Exports `JACKIN_SENTINEL_SOURCE_HOOK=1`, state dir, `PATH`. Sourced by every shell. | DK (`/jackin/state`) | dies (Dockerfile `ENV` covers it) |
| `hooks/preflight.sh` | Fails with exit 42 if the source hook did not run; appends `preflight=1`, agent, select/branch values to `preflight.log`. | DK (`/jackin/state`) | reshape |
| `scripts/marketplace-audit.sh` | Pre-commit check that `"x@y"` strings in `jackin.role.toml` use an allow-listed marketplace. | CL (Claude plugin marketplaces) | dies |
| `.pre-commit-config.yaml` | `gitleaks` and the marketplace audit. | GEN + CL | reshape (drop audit) |
| `.hadolint.yaml` | Ignores DL3004 ("construct exposes sudo"). No runner invokes hadolint (not in `mise.toml`, `.pre-commit-config.yaml`, or `.github/workflows/ci.yml`; read). | DK | dies (no sudo in v2) |
| `.dockerignore`, `.gitignore`, `.err` (empty tracked file) | Build-context and ignore rules; `.err` is junk. | DK / GEN | `.dockerignore` stays (it is part of the cache key, `CONTRACTS.md` C07); `.err` delete |
| `mise.toml`, `mise.lock` | Tools: actionlint, gitleaks, prek, python, uv, `pipx:reuse`. Task `test` = `prek run --all-files`. `reuse` is installed but no prek hook calls it. | GEN | stays |
| `renovate.json` | Tracks `projectjackin/construct` tag via regex versioning `^\d+\.\d+-trixie$`. | DK (construct tag scheme) | reshape (new construct tag scheme) |
| `REUSE.toml`, `LICENSE`, `LICENSES/`, `NOTICE` | SPDX/REUSE licensing. | GEN | stays |
| `README.md` | Describes the role, the manual checklist, and test tiers (PTY, functional, Docker E2E). Load command `jackin load jackin-project/sentinel . --rebuild --debug` (line 15). Line 28 and 41 claim a "Docker build output from a real `RUN` layer" and a `jackin-sentinel build layer` line. The Dockerfile has no `RUN` today, so checklist step 3 cannot pass now (read). | DK, CL | rewrite |
| `AGENTS.md` / `CLAUDE.md` (symlink) | Shared agent policy, identical to the other two repos (`diff` shows no difference with the architect copy). | GEN | stays |
| `.github/AGENTS.md`, `.github/CLAUDE.md` | Velnor "do not hand-edit generated workflows" rules. | VN | stays |
| `.github/workflows/ci.yml`, `.github/actions/*`, `.github/scripts/velnor-tools-cache-identity.sh`, `.github/actionlint.yaml` | Velnor-generated CI. Jobs: `actionlint`, `plan`, `publish-baseline`, `required` (job names read from `ci.yml`). No job builds the image, runs `jackin role validate`, or runs prek. | VN | stays (generated; do not hand-edit) |
| `.velnor/config.toml`, `.velnor/release-manifest.json`, `config/fleet/velnor-host.env` | Generator inputs and fleet host env. | VN | stays |

### 1.2 jackin-agent-smith (`git ls-files` = 29 files)

Same Velnor/legal/mise/hadolint/dockerignore set as sentinel. Differences:

| File | What it does | Tags | Fate |
|---|---|---|---|
| `jackin.role.toml` (23 lines) | `version = "v1alpha4"`, `agents = ["claude"]`, `[identity] name = "Agent Smith"`, `[claude] model = "claude-sonnet-4-6"`, plugins `code-review@claude-plugins-official`, `feature-dev@claude-plugins-official`, env `CLAUDE_CODE_NO_FLICKER=1`, `CLAUDE_CODE_MAX_OUTPUT_TOKENS=64000`. | CL (entire file except `dockerfile`, `published_image`, identity) | rewrite |
| `Dockerfile` (20 lines) | construct 0.37 digest, `USER agent`, `ENV MISE_TRUSTED_CONFIG_PATHS=/workspace`, `ARG NODE_VERSION=24.21.0`, mise installs Node with a BuildKit secret `github_token` (`uid=1000`) and a mise cache mount at `/home/agent/.cache/mise`. | DK (`/workspace`, `agent` user paths, uid 1000) | reshape |
| `scripts/marketplace-audit.sh` | Allow-list `claude-plugins-official\|jackin-marketplace`. | CL | dies |
| `renovate.json` | Construct tag rule plus a regex manager for `ARG NODE_VERSION`. | GEN + DK | stays/reshape |
| `README.md` | "Plugins ... under `[claude].plugins`" (line 28); "Trust rationale: see AGENTS.md § Threat model" (lines 16, 30). `AGENTS.md` has no Threat model section, so the link is already dangling (read). | CL | rewrite |
| `mise.toml` | Same as sentinel but `uv = 0.12.15` (sentinel pins 0.12.17). | GEN | stays |

No hooks, no `config/`-level role config, no `AGENTS.md.d`.

### 1.3 jackin-the-architect (`git ls-files` = 31 files)

| File | What it does | Tags | Fate |
|---|---|---|---|
| `jackin.role.toml` (68 lines) | `version = "v1alpha7"`, six agents, `[claude]` with 13 plugins and 4 `[[claude.marketplaces]]`, `[codex]`, `[amp]`, `[opencode] model = "zai/glm-5.2"`, `[kimi]`, `[grok] model = "grok-build"`, `[hooks] preflight`, env `CLAUDE_CODE_*`, `CONTEXT7_API_KEY` (default empty), `CAVEMAN_DEFAULT_MODE=ultra`. | CL, OA, CX, GK | rewrite |
| `Dockerfile` (162 lines) | Rust/jackin dev environment. Build args: cargo-binstall, OpenTofu, Caveman, ctx7, skills, headroom, uv, RTK versions. Installs apt build tools as root; copies `jackin-toolchain/` to `/tmp/jackin-mise`; installs Rust and the jackin mise tools; cargo-binstall `cargo-watch lychee`; OpenTofu; npm `ctx7` and `skills`; Caveman for opencode (installer) and codex/amp (skills CLI); `jackin-dev` skills for codex and amp; `shadcn/improve` skill for claude-code, codex, amp, opencode, kimi-code-cli; builds a combined `/home/agent/AGENTS.md` from `AGENTS.md.d/*.md` and symlinks it into `.claude`, `.codex`, `.config/amp`, `.kimi-code`, `.grok`; headroom via `uv tool`; RTK via mise and `rtk init -g --opencode`. | DK (agent user, `/home/agent`, BuildKit secrets uid 1000), CL (`~/.claude/CLAUDE.md`, claude-code skills), CX, GK (`~/.grok/AGENTS.md`), OA (amp, opencode, kimi) | reshape heavily |
| `AGENTS.md.d/caveman.md` | Terse-reply style prompt with `/caveman lite\|full\|ultra\|wenyan` switch. Baked into the in-image agent instructions (not the repo's AGENTS.md). | CL (slash command is the Claude plugin), CX | reshape |
| `AGENTS.md.d/token-tools.md` | "Shell output: RTK (prefix for codex/amp/etc.). Other: headroom_compress." | OA, CX | reshape |
| `caveman-config.json` | `{"defaultMode":"ultra"}`, copied to `~/.config/caveman/config.json` mode 440. | CL/CX (Caveman) | reshape |
| `hooks/preflight.sh` (184 lines) | Per-agent setup after bootstrap. `case JACKIN_AGENT` handles claude (caveman flag, statusline, RTK hook, ctx7, headroom), codex (verify caveman skill, ctx7 `--codex`, headroom), opencode, amp\|kimi\|grok (native Context7 MCP wiring, `grok mcp add ... --header "Authorization: Bearer ${CONTEXT7_API_KEY}"`, lines 77-85), headroom MCP registration. | CL, CX, GK, OA | reshape to codex + grok only |
| `docs/pr-workflow.md` | A table of Claude Code slash commands (`/review`, `/pr-review-toolkit:*`, `/simplify`, `/autofix-pr`, `/commit-commands:*`, `/jackin-dev:*`) and plugin troubleshooting. | CL (whole file) | dies (or rewrite for codex) |
| `jackin-toolchain/mise.toml` | Generated copy of jackin `mise.toml` `[tools]` and `[tool_alias]` (includes `aqua:apple/container` macOS-only, `cargo-zigbuild`, `syft`, `cosign`, etc.). | DK (`aqua:apple/container`) | stays, regenerated |
| `jackin-toolchain/rust-toolchain.toml` | Copy of jackin's `rust-toolchain.toml` (channel 1.98.1). | GEN | stays |
| `scripts/update-jackin-toolchain.rs` (196 lines) | `rust-script` that downloads upstream `mise.toml` and `rust-toolchain.toml` from `jackin-project/jackin` `main` and writes the two files above. | GEN | stays |
| `scripts/marketplace-audit.sh`, `scripts/marketplace-audit.test.sh` | Claude marketplace allow-list and its regression tests (prefix spoof, double `@`, unknown marketplace). | CL | die |
| `.pre-commit-config.yaml` | gitleaks plus both marketplace scripts. | GEN + CL | reshape |
| `.hadolint.yaml` | Ignores DL3004 (sudo) and DL3003 (cd in RUN for the caveman installer). No runner. | DK | dies |
| `renovate.json` | Construct tag rule plus 7 regex managers for Dockerfile `ARG`s (cargo-binstall, opentofu, caveman, ctx7, headroom, uv, rtk). | GEN + DK | stays; drop managers for removed args |
| `logo.txt` | jackin ASCII banner. Not referenced by any file I read. Not verified whether jackin reads it. | GEN | unclear; check with `grep` before deleting |
| `README.md` | Claims a scheduled `Jackin Toolchain` workflow opens PRs when upstream pins change (line 23). `.github/workflows/` contains only `ci.yml`, so the claim is stale. Also mentions `jackin load the-architect --agent codex` (line 10), Claude plugin docs (lines 38-46), "AGENTS.md § Threat model" (line 19, anchor does not exist). | CL, OA, DK | rewrite |
| Velnor/legal/mise files | As in sentinel, plus `.github/workflows/ci.yml` has an extra "Prepare Rust components" step (rust 1.98.1). | VN | stays |

### 1.4 What in the three repos violates the v2 role ABI today (`CONTRACTS.md` C08)

Read from the Dockerfiles and from `jackin/docker/construct/Dockerfile`:

- User `agent`, UID 1000, group 0, home `/home/agent` (`docker/construct/Dockerfile:156-158`, `:193`). C08 requires named user `sandbox`, `/sandbox` paths.
- `sudo`, `iptables`, `ipset` installed in the base (`docker/construct/Dockerfile:50-52`). C08 forbids sudo, setuid and firewall setup at start. OpenShell owns enforcement.
- `docker-ce-cli`, `docker-buildx-plugin`, `docker-compose-plugin` in the base (`:129-137`). No agent gets a Docker socket in v2; the managed engine is an unshipped upstream capability (`SPEC.md` "Optional Docker capability").
- `/workspace` work directory (architect and agent-smith `MISE_TRUSTED_CONFIG_PATHS`). C08 uses `/sandbox/project`.
- Hooks live at `/jackin/runtime/hooks/*` and state at `/jackin/state` (derived image, `jackin/crates/adapters/jackin-image/src/derived_image.rs:98-135`). v2 has no `/jackin` tree; state goes under `/sandbox/.jackin`.
- The construct base is pinned at `0.37-trixie` in all three roles and in `repo_contract.rs:25`; `docker/construct/VERSION` already says `0.38-trixie` (read).

## 2. Current role manifest schema (as jackin parses it)

All pointers are in `/Users/donbeave/Projects/jackin-project/jackin`.

### 2.1 Pipeline

1. `validate_role_repo(repo_dir)`: `crates/core/jackin-manifest/src/repo.rs:89-121`.
   - requires `jackin.role.toml` (`:90-94`), calls `load_role_manifest` (`:96`), resolves and boundary-checks `dockerfile` (`:97`), runs the Dockerfile rule (`:98`), checks each declared hook path exists and is non-empty (`:100-110`), then `validate_role_manifest` and prints warnings (`:112-115`).
2. `load_role_manifest`: `crates/core/jackin-manifest/src/manifest.rs:25-41`. Parses with `toml_edit` for the version, then `toml::from_str::<RoleManifest>` (`:35`), then feature-gated version checks (`:37`), then `validate_agent_consistency` (`:39`).
3. Version gate: `migrations.rs:76-87` rejects a manifest newer than `CURRENT_MANIFEST_VERSION = "v1alpha7"` (`jackin-core/src/constants.rs:10`). Migration steps `migrations.rs:11-47` are all no-ops except the last, which rejects removed `[claude|codex|opencode].providers` (`:49-62`).
4. Feature gates by version: `manifest.rs:56-92` (opencode needs v1alpha3, kimi v1alpha4, `[docker]` v1alpha6).

### 2.2 Fields (`crates/core/jackin-core/src/manifest.rs`)

`RoleManifest` (`:12-42`, `deny_unknown_fields`):

| Field | Type | Notes |
|---|---|---|
| `version` | string, default current | `:15`. Not a `u32`; string `v1alphaN`. |
| `dockerfile` | string, required | Relative path inside the repo (`repo.rs:123-155`). |
| `published_image` | `Option<String>` | Used for image freshness decisions (`jackin-image/src/image_decision.rs:138`) and `jackin role published-image*` helpers (`apps/jackin/src/role_authoring.rs:41-76`). |
| `identity` | `Option<{name}>` | Display name only (`:191-195`). |
| `agents` | `Option<Vec<Agent>>` | Default `[claude]` when absent (`:64-66`). Must not be empty (`validate.rs:19-23`). |
| `claude` / `codex` / `amp` / `kimi` / `opencode` / `grok` | `Option<...Config>` | `ClaudeConfig {model, marketplaces[], plugins[]}` (`:205-214`); `ClaudeMarketplaceConfig {source, sparse[]}` (`:197-203`); Codex, Kimi, Opencode, Grok are `{model}` (`:103-133`); Amp is empty (`:112`). |
| `hooks` | `Option<HooksConfig>` | `{setup_once, source, preflight}`, each an optional relative path (`:151-160`). `entries()` fixes the in-image file names `setup-once.sh`, `source.sh`, `preflight.sh` (`:169-189`). |
| `env` | `BTreeMap<String, EnvVarDecl>` | `EnvVarDecl {default, interactive, skippable, prompt, options[], depends_on[]}` (`:135-149`). |
| `docker` | `Option<ManifestDockerConfig>` | `{min_profile, dind, allowed_hosts[], capabilities_add[]}` (`:44-55`). |

The `Agent` enum has 12 variants including Antigravity, Gemini, Cursor, Muse, Omp and Hermes, which have no manifest table (`has_agent_config`, `:68-83`, returns `false`).

### 2.3 Validation rules

- Agent consistency (`validate.rs:14-68`): every agent in `supported_agents()` must have its `[agent]` table (`:25-30`). A table whose agent is not listed is only a warning (`:34-65`), and the check covers only codex, claude, amp, kimi and opencode (grok is missing from the warn list). Two representations of one fact (list plus tables) allow that mismatch. Deleted in v2: there is one required `[codex]` table.
- Env rules (`validate.rs:70-119`): valid identifier; not a reserved runtime name (`RESERVED_RUNTIME_ENV_VARS`, `jackin-core/src/env_model.rs:154-180`: JACKIN_*, DOCKER_*, TESTCONTAINERS_HOST_OVERRIDE, firewall and sudo vars, CODEX_LANE_*, CLAUDE_*); non-interactive needs `default`; `options` needs `interactive`; warnings for dead `prompt` and `skippable`.
- Interpolation (`validate.rs:139-194`): `${env.NAME}` allowed in `prompt` and `default` only when listed in `depends_on` (entries must start `env.`, no self, no duplicates, must exist); options are static. `topological_env_order` rejects cycles (`:116`).
- Dockerfile rule (`repo_contract.rs:62-116`): last `FROM` must be exactly `projectjackin/construct:<version>-trixie` with an optional `@sha256`; no `--platform`; a floating `trixie` tag is rejected. Constants `:14-25`.
- Paths (`repo.rs:123-155`): must be relative, no `..`, file must exist, canonical path must stay under the repo.

### 2.4 How the fields are consumed at build and run time

- Build: `render_derived_dockerfile` (`jackin-image/src/derived_image.rs:256-340`) appends to the role Dockerfile: agent install blocks (prefetched binary or `fallback_install_command` curl-pipe), a Claude plugin RUN (`:213-250`, runs `claude plugin marketplace add` / `plugin install`), a `/jackin/default-home` snapshot (`:137-165`), hook COPY lines owned `agent:0` into `/jackin/runtime/hooks/` (`:98-135`), the entrypoint and agent-status assets (`:11-56`), the capsule binary, a runtime-home chown loop (`:167-190`), and `USER agent` with `ENTRYPOINT /jackin/runtime/jackin-capsule` (`:325-327`).
- Run: `docker/runtime/entrypoint.sh:102-137` runs `setup-once.sh` once (marker `setup-once.done`), sources `source.sh` into the shell, then runs `preflight.sh` every start. Hooks hard-fail the start (`CONTEXT.md` "Role hook").
- Agent facts that matter for v2: Codex installs with `https://chatgpt.com/codex/install.sh`, API-key mode uses `OPENAI_API_KEY`, credential file `.codex/auth.json`, `CODEX_HOME` (`jackin-core/src/agent/adapters/codex.rs:11-12,63-88`). Auth modes today: `sync`, `api_key`, `ignore` (plus `oauth_token` for Claude) (`jackin-core/src/auth.rs:7-17`). The Grok adapter (`adapters/grok.rs`) is deleted with the Grok scope.
- Scaffold: `jackin role create` writes a manifest with `[claude] plugins = []` and a CI file using `jackin-project/jackin-role-action` (`apps/jackin/src/role_authoring.rs:188-267`). The three role repos replaced that CI with Velnor output, so none runs `jackin role validate`.

### 2.5 Claude-, Docker- and multi-agent-specific parts of the model

Deleted by the rewrite: `ClaudeConfig`, `ClaudeMarketplaceConfig`, Amp/Kimi/Opencode configs, `ManifestDockerConfig` and `DindGrant`, `published_image` helpers, all of `migrations.rs`, the feature-version gates, `agents` list default of Claude, `HooksConfig.setup_once` and `.source`, the derived-image hook staging and zshenv shim, default-home snapshot and `fallback_install_command` installers.

## 3. What jackin-sentinel verifies today and how a person uses it

### 3.1 Intent

Sentinel is the "manifest surface area in one place" role (`README.md:18-28`). It does not test agents. It tests that jackin parses and executes every manifest feature and that the launch UI behaves.

### 3.2 What it covers (read from the files)

| Area | Where | What a run proves |
|---|---|---|
| Manifest parse for all six agent tables | `jackin.role.toml:7-28` | jackin accepts the full agent surface; `agents` list matches tables. |
| Static env | `STATIC_DEFAULT`, `LITERAL_TEMPLATE = "preserve-${other.VALUE}"` (`:41-45`) | Defaults reach the container; a `${other.X}` that is not `${env.X}` stays literal. |
| Interactive prompts | `FREE_TEXT`, `FREE_TEXT_REQUIRED`, `SELECT_PROJECT`, `SELECT_MODE`, `OPTIONAL_API_KEY` (`:47-70`) | Text, required text, select, select with default and skippable dialogs in the launch cockpit. |
| Dependency order and interpolation | `BRANCH`, `COMBINED_LABEL`, `OPTIONAL_DERIVED` (`:72-89`) | `depends_on` ordering, `${env.*}` in prompt and default, skip cascade (OPTIONAL_DERIVED is skipped when OPTIONAL_API_KEY is skipped). |
| `setup_once` | `hooks/setup-once.sh` | Runs once; writes `setup-once.env` containing agent, static default, literal template. Report prints it as `SETUP_ONCE_MARKER_*`. |
| `source` | `hooks/source.sh` | Sets `JACKIN_SENTINEL_SOURCE_HOOK=1` and `PATH` so the report command is found. |
| `preflight` | `hooks/preflight.sh` | Exits 42 if the source hook did not run (ordering proof: source before preflight); appends one line per start. Report prints `JACKIN_SENTINEL_PREFLIGHT_COUNT`. |
| Runtime variables | `jackin-sentinel-report` in `setup-once.sh:18-56` | Prints `JACKIN`, `JACKIN_AGENT`, `JACKIN_INSTANCE_ID`, `JACKIN_DIND_HOSTNAME`, `TESTCONTAINERS_HOST_OVERRIDE`, all env values, `DOCKER_CLI=available/missing`, `docker ps`. |
| Build UI | README step 3 | Docker build output stays in the cockpit log. Currently unprovable because the Dockerfile has no `RUN`. |
| Dirty worktree exit | README step 6 | Cleanup is a rich dialog. Manual only. |

### 3.3 How a person uses it (README.md:12-47)

1. From any workspace: `jackin load jackin-project/sentinel . --rebuild --debug`.
2. Walk the six-step manual checklist: cockpit owns the screen, complete every env dialog, check the build log, launch an agent and run `jackin-sentinel-report`, compare the report to what was entered, exit with a dirty isolated worktree.
3. The report is bounded by `JACKIN_SENTINEL_REPORT_BEGIN` and `JACKIN_SENTINEL_REPORT_END` so future tests parse it instead of agent UI text. README names three planned tiers (PTY, functional, Docker E2E). I found no automated test that consumes sentinel in the jackin tree (`grep` for `jackin-sentinel` over `jackin/` returns only an unrelated research log).

### 3.4 Gaps in the current sentinel

- No automation. Every check is human-driven.
- No negative checks at all: no check that something is denied.
- The report script is generated at runtime by a hook, so it exists only after `setup_once` ran.
- It reports Docker availability as a positive (`DOCKER_CLI=available`). In v2 that becomes a defect.
- No credentials, network or filesystem checks, because v1 had no enforcement surface outside the container.
- Claude env vars, five agent tables and DinD variables are noise for v2.
- The report is produced by code running inside the sandbox as the agent UID. In v2 such output is untrusted for security claims (`SPEC.md` "Protocol and authority"; `CONTRACTS.md` C09), so security assertions must come from an observer outside the sandbox.

## 4. OpenShell-native role design

### 4.1 Facts about OpenShell that constrain the design

Sources: OpenShell `main` at 42671c2d (2026-10-31, `gh api repos/NVIDIA/OpenShell/commits/main`) and the pinned docs the pack cites at c4688aa8. Fetched 2026-11-01.

- Image input. `openshell sandbox create --from <OCI ref>`. Rootfs `.tar` works only on a VM-driver local gateway. "`--from` does not expand catalog aliases and does not build local Dockerfiles or directories." Build with the local engine, then pass the tag; a remote gateway needs a registry push (docs `how-it-works/sandboxes/overview.mdx` at c4688aa8). So a role must be a Dockerfile built by Jackin plus Buildx. I found no community-sandbox or catalog concept in that doc ("does not mention them"). The default image is `nvcr.io/nvidia/base/ubuntu:24.04`.
- Templates. `openshell sandbox template create` stores image, env, resources; `--template` cannot combine with `--from`, `--env`, etc. (`sandboxes/templates.mdx`). The pack treats templates as operator-managed shapes, not a replacement for the role recipe (`SPEC.md` "Portable role ABI").
- Transfer and exec. `sandbox upload|download`, `--upload src:dest` at create, `sandbox exec -n <name> [--tty] -- cmd`, `sandbox connect`, `forward start`, `stop`/`start`/`delete`. Uploads respect `.gitignore` in Git repos; symlinks are preserved (same doc).
- Policy. One YAML, `version: 1`, top-level `filesystem_policy`, `landlock`, `process`, `network_policies`, `network_middlewares`. Unknown fields and duplicate keys are rejected. Filesystem, Landlock and process are startup-only; `network_policies` are live. `process.run_as_user` may be `sandbox` or a numeric UID; root is rejected. Network rules are keyed by name and bind **endpoints to binaries** (`binaries: [{path: ...}]`; an empty list matches no binary). Endpoints can use `protocol: rest`, `access`/`rules`, and `enforcement: enforce`. Network policy never authorizes loopback, link-local or the metadata address `169.254.169.254`. Rule names starting `_provider_` are reserved for rules providers contribute (`policies/schema.mdx` on `main`; applied with `--policy ./policy.yaml` at create; `/etc/openshell/policy.yaml` in the image applies only when no saved policy exists, `manage-policies.mdx`).
- Providers. A provider has a profile `id`. The profile declares credentials (env var names, auth style), endpoints and `binaries`. The agent receives opaque placeholders `openshell:resolve:env:KEY`; the proxy substitutes real values only for the bound host, port and path, and returns 403 `credential_endpoint_mismatch` otherwise (`providers/overview.mdx`). "The gateway starts with an empty provider-profile catalog" and a profile must be imported (`examples/codex-app-server/README.md`). Providers attach with `--provider` at create or `sandbox provider attach`.
- Provider profiles shipped as examples: anthropic, claude-code, codex, copilot, cursor, openai, openrouter, github and others under `providers/` on `main`. The `codex` and `openai` profiles cover this design; Jackin imports an adapted copy because `binaries` must name jackin's install path.
- Codex credentials. The upstream `codex` profile exposes `CODEX_AUTH_ACCESS_TOKEN`, `CODEX_AUTH_ACCOUNT_ID` and optional refresh material, with gateway-side OAuth refresh (`examples/codex-app-server/codex.yaml`). **The Codex CLI does not read those env vars itself.** The upstream example runs a launcher that writes `~/.codex/auth.json` with `auth_mode: "chatgptAuthTokens"`, the placeholder handles as tokens, and an empty `refresh_token` (`examples/codex-app-server/start-codex-app-server`). So for ChatGPT login, jackin's agent adapter must perform that write at session start. A role author must not own this.
- Grok is out of scope (user decision, 2026-11-02). No Grok profile, install, endpoint or credential work is needed. Note for the record: OpenShell ships no Grok or xAI profile.

### 4.2 Role = what

A role is a versioned recipe with four parts. Each part has one owner.

| Part | Owner | Form |
|---|---|---|
| Image recipe | role author | `Dockerfile` (tools, language runtimes, repo-specific setup) built by Buildx from the construct v2 base. |
| Agent set | role author declares, jackin installs | `[codex]` with an exact version pin. Jackin appends the install layer (root-owned `/opt/jackin/agents/codex/`, Capsule at `/opt/jackin/bin/jackin-capsule`). Role Dockerfiles never install Codex. |
| Requested policy | role author declares, operator grants | native `policy.yaml` passed verbatim to `openshell sandbox create --policy`. |
| Launch inputs and hooks | role author | `[env.*]` (non-secret values only) and one `preflight` hook. |

Not part of a role: credentials, accounts, providers, gateway, target, project. Those are jackin user and project config (`CONTRACTS.md` C03). The agent-to-provider mapping lives in `crates/adapters/jackin-agents` (embedded provider profiles; install paths; credential file formats), because the profile's `binaries` list must name the exact install paths jackin chose. A role that installed its own agent could silently break credential injection (OpenShell example profiles warn that a profile whose `binaries` do not match "matches nothing: ... the credential is never injected and the traffic is denied", `providers/README.md`).

### 4.3 How a role image is built and delivered

1. `jackin load <role>` resolves the role repo and validates `jackin.role.toml` (pure parse) and the filesystem rules (adapter).
2. Buildx builds the role `Dockerfile` (final `FROM` must be the digest-pinned construct v2 base), then jackin's derived layer adds agent installs and Capsule. Cache key per `CONTRACTS.md` C07.
3. The artifact is delivered by same-engine fast path or registry digest to the target gateway.
4. `openshell sandbox create --from <immutable ref> --policy <role policy.yaml> --provider <account provider(s)>`; then project materialization (`/sandbox/project`); then Capsule `serve`; then `StartSession` over `jackin-capsule relay --stdio` through `sandbox exec` (`CONTRACTS.md` C09).

The role never gets a Docker socket and never runs `docker`. The only Docker use is the Buildx builder the pack already allows.

### 4.4 New minimal `jackin.role.toml` (schema_version 2, Codex only)

```toml
schema_version = 2

[role]
name = "jackin' Sentinel"
dockerfile = "Dockerfile"

[codex]
version = "<exact codex release>"
model = "<optional default model>"

[policy]
file = "policy.yaml"

[hooks]
preflight = "hooks/preflight.sh"

[env.STATIC_DEFAULT]
default = "static-value"

[env.SELECT_MODE]
interactive = true
options = ["fast", "thorough", "diagnostic"]
prompt = "Select sentinel mode:"
default = "diagnostic"
```

Rust shape (pure, `crates/core/jackin-config`; unknown fields rejected):

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleRecipe {
    pub schema_version: SchemaVersion2,          // literal 2, anything else is an error
    pub role: RoleHeader,                        // { name: DisplayName, dockerfile: RelPath }
    pub codex: CodexSpec,                        // required: a role without Codex is not a role
    #[serde(default)] pub policy: Option<PolicyRef>,   // { file: RelPath }
    #[serde(default)] pub hooks: Hooks,          // { preflight: Option<RelPath> }
    #[serde(default)] pub env: BTreeMap<EnvName, EnvDecl>,
}
pub struct CodexSpec { pub version: PinnedVersion, pub model: Option<ModelId> }
```

Design points and what they remove:

- One required `[codex]` table. The v1 `agents = [...]` list plus per-agent tables could disagree; `validate_agent_consistency` (`validate.rs:14-68`) existed only to police that and is deleted. There is no `AgentId` enum, no `--agent` flag on `jackin load`, and `JACKIN_AGENT` is a constant. Parse once at the boundary into `RoleRecipe`, then trust it.
- `PinnedVersion` is an exact version newtype. It rejects `latest`, ranges and empty. The version feeds the C07 cache key and the "validate every advertised executable" check.
- `RelPath` validates once at parse (relative, no `..`, no root). Existence and symlink-escape checks stay in an adapter in `jackin-roles` (they touch the filesystem). This replaces `repo.rs:123-155`.
- `[hooks]` keeps only `preflight` (runs before every agent session; non-zero exit blocks the session). `setup_once` and `source` are dropped: build-time work belongs in the Dockerfile; shell environment belongs in Dockerfile `ENV` and `[env]`; the v1 `setup_once` marker lived in `/jackin/state` and the zshenv shim is Docker-image plumbing. If a role needs per-domain first-run work, add a `setup` hook later (open question 3).
- `[env.*]` keeps the existing `EnvVarDecl` shape (`default`, `interactive`, `skippable`, `prompt`, `options`, `depends_on`, `${env.*}`), because it is host-side prompt DX and independent of the runtime. New rule: env values are non-secret by contract. A secret goes through a provider, not a prompt. The reserved-name list (`env_model.rs:154-180`) is cut to the v2 runtime variables; DOCKER_*, TESTCONTAINERS, firewall, sudo, CLAUDE_* names go.
- Dropped outright: `published_image` (delivery is target config, `SPEC.md` "Roles and reproducible image construction"), `[identity]` (merged into `[role]`), `[docker]`, the `agents` list, every non-Codex table, `version = "v1alphaN"` and every migration.
- `policy.yaml` is not parsed into a Jackin type beyond: file exists, size <= 4 MiB (OpenShell limit), `version: 1`, only the five known top-level keys, `process.run_as_user` is `sandbox` or numeric non-zero. Deeper validation is OpenShell's. Jackin records the requested policy digest and later the observed loaded revision (`CONTRACTS.md` C06, C13).

Required rejection tests: unknown top-level key, `schema_version = 1`, missing `[codex]`, a `[claude]` or `[grok]` table, `version = "latest"`, absolute or `..` path, empty hook file. A secret typed into an env prompt cannot be detected, so the rule is documented rather than enforced.
- `[env.*]` keeps the existing `EnvVarDecl` shape (`default`, `interactive`, `skippable`, `prompt`, `options`, `depends_on`, `${env.*}`), because it is host-side prompt DX and independent of the runtime. New rule: env values are non-secret by contract. A secret goes through a provider, not a prompt. The reserved-name list (`env_model.rs:154-180`) is cut to the v2 runtime variables; DOCKER_*, TESTCONTAINERS, firewall, sudo, CLAUDE_* names go.
- Dropped outright: `published_image` (delivery is target config, `SPEC.md` "Roles and reproducible image construction"), `[identity]` (merged into `[role]`), `[docker]`, the `agents` list, every non-Codex table, `version = "v1alphaN"` and every migration.
- `policy.yaml` is not parsed into a Jackin type beyond: file exists, size ≤ 4 MiB (OpenShell limit), `version: 1`, only the five known top-level keys, `process.run_as_user` is `sandbox` or numeric non-zero. Deeper validation is OpenShell's. Jackin records the requested policy digest and later the observed loaded revision (`CONTRACTS.md` C06, C13).

Equivalent TOML rejections that must exist as tests: unknown top-level key, `schema_version = 1`, zero agents, an agent id like `claude`, `version = "latest"`, absolute or `..` path, empty hook file, secret-looking env prompt is not detectable so document the rule rather than enforce it.

### 4.5 Policy shape (role-owned `policy.yaml`)

Sentinel example (restrictive on purpose, because the point is to prove enforcement):

```yaml
version: 1
filesystem_policy:
  include_workdir: false
  read_only: [/usr, /lib, /etc, /opt]
  read_write: [/sandbox, /tmp]
landlock:
  compatibility: hard_requirement
process:
  run_as_user: sandbox
  run_as_group: sandbox
network_policies:
  sentinel_probe_allowed:
    endpoints:
      - host: api.github.com
        port: 443
        protocol: rest
        access: read-only
        enforcement: enforce
    binaries:
      - path: /usr/bin/curl
```

Agent egress (api.openai.com, auth.openai.com, chatgpt.com, ab.chatgpt.com) is not written here. It arrives through the attached provider as `_provider_` rules (hosts read from the upstream `codex` profile), so the role policy carries only what the role itself needs. Paths and field names above come from the schema page (read); `hard_requirement` is chosen so a kernel without Landlock fails startup instead of silently running unconfined (`best_effort` "runs without the filesystem rules").

### 4.6 Provider for Codex

Owner: `jackin-agents` embeds the profile; the OpenShell adapter imports it into the target gateway workspace (`openshell provider profile import`) before the first sandbox create. The account store only holds references (`SPEC.md` "Credentials, account state and policy").

| Mode | Status in first release | Provider | Binaries scoped | Agent-side step |
|---|---|---|---|---|
| API key | supported | `openai`-style profile adapted to name the Codex binary instead of `curl` | `/opt/jackin/agents/codex/**` | none; `OPENAI_API_KEY` is a placeholder in env |
| ChatGPT login | supported, gateway-side refresh | `codex` profile (upstream example) with refresh material configured by `openshell provider refresh configure` | same | session launcher writes `~/.codex/auth.json` with `auth_mode: chatgptAuthTokens` and placeholder tokens (copy upstream behavior; verify against the pinned Codex version) |

One domain, one account, one provider set. Two Codex accounts are two OpenShell sandboxes (`SPEC.md` "Domains and account isolation"). The default project shell is a third, credential-free domain.

### 4.7 Agent config

- Per-agent config lives in the domain-private `/sandbox/.jackin` and the Codex home under `/sandbox` (C08). Jackin owns writing Codex `auth.json`. The role owns only `model` and the pinned `version`.
- The Architect's baked `AGENTS.md` (from `AGENTS.md.d`) becomes `~/.codex/AGENTS.md` with `$HOME=/sandbox`. A Dockerfile step builds it, as today, minus the Claude, Amp, Kimi and Grok links.
- Plugins and marketplaces disappear as manifest concepts (they were `[claude]`-only). Codex extensions are installed in the Dockerfile with pinned versions (for example `skills add ... -a codex`), so the C07 build identity covers them.

### 4.8 Construct base (jackin repo, prerequisite for every role)

`docker/construct/Dockerfile` needs a v2 variant: user `sandbox` (create with home `/sandbox`, group matching OpenShell identity rules), no `sudo`, `iptables`, `ipset`, Docker CLI/Buildx/Compose; `.claude`, `.codex` pre-created dirs go; keep git, gh, mise, zsh, fish, starship, tirith, shellfirm, ripgrep, fd, fzf, jq. Tag scheme and renovate regex in all three roles change with it. Unresolved: OpenShell's requirements on image contents (I did not verify what the Docker driver needs inside an image; `docs/security/verify-image-contents.mdx` exists on `main` and was not read). Owner: openshell-specialist.

### 4.9 Alternatives rejected

| Option | Why not |
|---|---|
| Role = OpenShell sandbox template (`sandbox template create`) | Templates cannot be combined with `--from`/`--env`; they need workspace-admin rights; the pack says they do not replace RoleRecipe (`SPEC.md` "Portable role ABI"). Keep as an operator tool. |
| Let OpenShell build the Dockerfile | It does not (docs, quoted in 4.1). |
| Role installs Codex in its Dockerfile | The provider profile `binaries` list must match install paths; version provenance and the C07 executable inventory need jackin to own the install. |
| Credentials in `jackin.role.toml` (`[codex.auth]`, provider names) | Role authors would grant credentials; the pack gives that to the operator (`SPEC.md` "Policy ownership"). |
| A jackin policy DSL (`[network] allow = [...]`) | Pack forbids a second policy language (`CONTRACTS.md` C03). |
| Keep `setup_once` and `source` | Both exist to work around a persistent mutable `/home/agent` and a shared shell; v2 images are built, and the Dockerfile `ENV` covers `source`. Add back only on demand. |
| Keep `agents = [...]` plus tables, or an `[agents.codex]` map | With one agent the list is a constant and the map is a one-variant enum. The required `[codex]` table is the smallest form. A second agent later is a deliberate `schema_version` bump. |
| Ship the Codex provider profile in each role | Duplicates and drifts; the profile depends on jackin's install layout, so jackin owns it. |

## 5. What sentinel must check in the new world

### 5.1 Principle

Sentinel proves the end-to-end path jackin to OpenShell to sandbox to agent. A check run only by code inside the sandbox cannot prove a security property, because the agent UID can tamper with it (`SPEC.md` "Protocol and authority"). So every check has an observer:

- **H** host observer: jackin JSON output (`jackin inspect <id> --json` is a proposed command in `SPEC.md` "Proposed command examples", not yet existing) and the OpenShell CLI/API (`openshell sandbox get`, `openshell policy get`, `openshell provider ...`).
- **P** in-sandbox probe: a script baked into the image and run through `openshell sandbox exec`. It supplies evidence of what the workload sees. It cannot supply proof of enforcement alone; each enforcement claim pairs the probe result with an H observation of the loaded policy and the gateway's denial record.

A check without credentials reports `SKIPPED (no credential)`, never PASS (`GOAL.md` "Report unavailable infrastructure as BLOCKED, not PASS").

### 5.2 Checks

| ID | Property | Positive scenario | Negative scenario | Observer |
|---|---|---|---|---|
| S01 | Manifest strictness | `jackin role validate .` accepts sentinel. | Table-driven mutations fail: unknown key, `schema_version = 1`, missing `[codex]`, a `[grok]` or `[claude]` table, `version = "latest"`, `..` path, missing policy file, empty preflight. Each names the field. | H |
| S02 | Image build and identity | Build succeeds; build log shows the sentinel `RUN` marker line in the cockpit log, not the main screen; resolved image digest equals the digest the sandbox runs (OpenShell observation). | Change one agent version, `.dockerignore`, or the Dockerfile: cache key changes. Unpinned version: build rejected before Docker starts. | H |
| S03 | Sandbox created | Sandbox exists in the explicit gateway and workspace; state ready; immutable sandbox ID recorded in the activation receipt. | Run with the wrong workspace id: refused with an authority error, no sandbox created. | H |
| S04 | Role ABI | P: `id -un` is `sandbox`, uid != 0, `$HOME=/sandbox`, `/sandbox/project`, `/sandbox/.jackin`, `/sandbox/cache` exist; `jackin-capsule` at `/opt/jackin/bin`. | P: `sudo`, `docker`, `iptables` absent; `docker.sock` absent; writing `/usr/local/bin` and `/etc` fails. | P + H (`WorkloadIdentity` condition) |
| S05 | Workspace materialized | Dirty tracked file and an untracked file present in `/sandbox/project` with digests matching the materialization manifest. | `.env` and `.git/config` credentials excluded; host checkout unchanged after the agent edits files; a returned patch applies only on explicit action; a host change after launch makes apply report a conflict instead of overwriting. | H + P |
| S06 | Codex launches | Pinned `codex --version` equals the manifest pin; a Capsule session starts `codex` and shows its prompt; with a disposable key, `codex exec 'reply with OK'` returns. | Start with no credential: session starts, agent reports auth missing, jackin shows the domain as unauthenticated, not crashed. | H + P |
| S07 | Codex session resume after reconnect | Start a Codex session, exit it so the Capsule session ends, start a new session in the same domain: Codex state in `/sandbox/.codex` survives (retained state, not a live process). | Recreate the sandbox: state is gone unless a checkpoint was taken; jackin shows it as a new domain. | H + P |
| S08 | Credential injection by provider | P: agent env contains the placeholder form `openshell:resolve:env:...`, never a real key (compare against the key's first 6 characters from the host test environment). A real model call by the agent succeeds. | Same placeholder via `curl` (a binary not in the profile) to `api.openai.com` is denied; the agent binary sending the placeholder to `example.com` returns 403 `credential_endpoint_mismatch`. | P + H (gateway log) |
| S09 | Domain and account isolation | Two Codex accounts launch two sandboxes; each env holds its own placeholder handles and the handles differ; the default project shell domain has no provider attached and no `OPENAI_*`/`CODEX_AUTH_*` variable. | Attaching an account's provider to another account's domain requires an explicit grant; the second sandbox cannot use the first sandbox's placeholder (403). | P + H (`provider list` per sandbox) |
| S10 | Egress policy enforced | P: `curl https://api.github.com/zen` works (allowed read-only GET). H: effective policy revision equals the requested policy digest. | P: `curl https://example.com` denied; `POST` to `api.github.com` denied by `access: read-only`; `169.254.169.254` denied; after `openshell policy update --remove-rule` the former success fails and open connections close (live reload). | P + H |
| S11 | Filesystem policy enforced | P: write to `/sandbox/project` and `/tmp` works. | P: write to `/usr`, `/etc`, `/opt` fails; with `hard_requirement`, a gateway without Landlock ABI v3 refuses to start the sandbox (run only on a qualified target). | P + H |
| S12 | Hook contract | `preflight.sh` ran before each session; counter in `/sandbox/.jackin/sentinel/preflight.log` equals sessions started (2 sessions, count 2); the report shows the env it received. | A role whose preflight exits 42 never gets a session; jackin shows the hook failure and creates no PTY. | P + H |
| S13 | Env resolution | Static default, literal template, free text, required text, select, select with default, skippable, derived and cascade-skipped values show in the report exactly as entered. | Required text left empty is refused; skipped `OPTIONAL_KEY` skips `OPTIONAL_DERIVED`; a prompt that tries `${env.UNLISTED}` fails validation (S01). | H + P |
| S14 | Attach, detach, reconnect | Start a session, print a marker, detach the client; PTY keeps running (a `sleep`-and-append loop keeps writing). Reconnect creates a new relay with a new exec request ID, replay or snapshot contains the marker, loop continues. | Kill the jackin client process mid-session: session survives. Stop and start the sandbox: old PTYs are gone, Capsule boot ID changes, jackin requires a new activation epoch and does not replay the old command. | H + P |
| S15 | Lifecycle and cleanup | `eject` stops the domains and keeps project data; `purge` removes only owned sandbox and providers it created. | Sentinel seeds an unrelated sandbox in the same workspace; it survives `purge`. Uncertain delete leaves the instance visible as pending. | H |
| S16 | Dirty exit dialog | Exit with dirty isolated worktree shows the rich cleanup dialog (PTY test in jackin repo, fed by sentinel's fixture). | n/a | jackin PTY harness |

Checks S06 to S09 need disposable Codex accounts (two for S09) supplied by the operator environment (names for env vars to be chosen when writing the driver). Checks S01, S02 (build), S04, S10, S11, S12 need no secrets.

### 5.3 Sentinel repo shape after the rewrite

```text
jackin.role.toml      v2 schema, [codex], policy, preflight, env prompts
Dockerfile            FROM construct v2 digest; COPY probe to /opt/sentinel (root-owned); one RUN with a build marker
policy.yaml           as 4.5
hooks/preflight.sh    counter + env snapshot, exit 42 test mode via SENTINEL_FAIL_PREFLIGHT
probe/sentinel-probe  baked read-only report: SENTINEL_REPORT_BEGIN ... SENTINEL_REPORT_END (key=value, schema line first)
verify/               host driver and expected values (per-check scripts for S01-S15)
README.md             human checklist generated from the table in 5.2
```

Change from v1: the report becomes a build-time artifact (the same code on every start) so a missing report is itself a failure signal. It prints only facts; every verdict is decided by `verify/`.

CI: today's Velnor CI runs none of this. Adding `jackin role validate .` and a Buildx build means changing the generator inputs (`.velnor/config.toml`) or upstream `velnor-new`, never `.github/` by hand (`.github/AGENTS.md`). The end-to-end half needs a runner with Docker and an OpenShell gateway, which the current `ubuntu-26.04` label and "docker unit only" profile comment (`mise.toml` line 17-18) do not provide. Owner: cicd-architect.

## 6. Notes on the other two roles in the new world

### 6.1 agent-smith (code review)

Everything it declares is Claude (`code-review`, `feature-dev` plugins). With Claude gone, the identity of the role is a read-only reviewer. The OpenShell-native version expresses that as policy rather than plugins:

- `[codex]` only, no plugins.
- `policy.yaml`: `filesystem_policy.read_only: [/sandbox/project, ...]`, `read_write: [/sandbox/.jackin, /tmp]`, `include_workdir: false`, network limited to the agent providers plus `api.github.com` read-only. Startup-only fields, so a read-only project means the domain cannot edit source at all (`schema.mdx`: "Filesystem ... Startup").
- Dockerfile keeps Node via mise; paths move off `/workspace` and `agent`; BuildKit secret `uid=1000` becomes the `sandbox` UID.
- Review instructions: a `~/.codex/AGENTS.md` review prompt baked in the image.

Open: whether Agent Smith keeps its "code review" product role or needs an owner decision (open question 6).

### 6.2 the-architect (jackin development)

- Drop: `[claude]` plugins and marketplaces, Claude env, caveman statusline and flag, `register_rtk_hook` (Claude), amp/opencode/kimi installs and MCP wiring, `docs/pr-workflow.md`, both `marketplace-audit*` scripts and their pre-commit entries.
- Keep: Rust toolchain via `jackin-toolchain/` and `update-jackin-toolchain.rs`; ctx7; headroom; RTK; `skills add` for codex; OpenTofu; lychee; cargo-watch.
- Change: `preflight.sh` shrinks to the `codex` case (`verify_codex_caveman_skills`, `setup_context7 codex --codex --mcp`, `register_headroom_mcp`; v1 lines 169-173). Today it passes `CONTEXT7_API_KEY` to `ctx7 setup ... --api-key` (line 42), which writes the key into agent config. In v2 that secret should come through a provider for `mcp.context7.com` as a placeholder, and the policy must allow that host. Whether a placeholder in a stored header survives is **not verified**; owner: openshell-specialist.
- Hard blocker: jackin's own build and test flows need a Docker engine (the construct is built with `docker-bake.hcl`). The agent cannot reach one inside the sandbox until the managed engine companion exists (`SPEC.md` "Optional Docker capability": "proposed upstream capability, not a shipped OpenShell feature"). The Architect can edit and unit-test Rust but not rebuild the construct image.
- The toolchain sync copies jackin's `mise.toml`. When jackin drops `aqua:apple/container` and Docker-bound tools, regenerate; no script change.
- README claim of a scheduled `Jackin Toolchain` workflow is stale (only `ci.yml` exists).

## 7. Risks and open questions

| # | Question | Resolver |
|---|---|---|
| 1 | Codex: confirm the installed binary path is the process that opens sockets (a Node wrapper would appear as `node` in the binary-scoped rules), and that the install layout in `/opt/jackin/agents/codex` matches the profile `binaries`. | openshell-specialist, with a live run |
| 2 | Codex ChatGPT login: confirm the `auth.json` placeholder format works with the pinned Codex release and that the gateway refresh works for a second sandbox. | openshell-specialist |
| 3 | Hooks: is `preflight` alone enough, or does a per-domain `setup` hook have a real user? Sentinel was the only user of `setup_once`. | owner |
| 4 | Env prompts: keep the full `depends_on`/`${env.*}` graph (sentinel is its only user) or cut it for a smaller schema. This design keeps it because it is runtime-independent and cheap. | owner |
| 5 | `published_image`: dropping it removes prebuilt-image fast paths in v1 (`image_decision.rs:138`). Confirm registry delivery config replaces it. | owner, agent-sandbox-engineer |
| 6 | Agent Smith's purpose without Claude plugins (read-only reviewer policy proposed). | owner |
| 7 | Construct v2: image content requirements of the OpenShell Docker driver, user creation rules for `sandbox`, supervisor needs (`iproute2`, etc.). Not read. | openshell-specialist, agent-sandbox-engineer |
| 8 | Policy validation depth in `jackin role validate`: static subset (proposed) versus calling OpenShell offline. I did not find an offline lint command for policies (only `provider profile lint`). | openshell-specialist |
| 9 | Sentinel E2E needs a runner with Docker, a gateway and disposable accounts. Velnor-generated CI cannot host it as is. | cicd-architect |
| 10 | The proposed `jackin inspect --json` and `jackin shell` commands exist only in the pack; S03, S05, S12 and S14 depend on them. | rust-cli-tui-engineer |
| 11 | Pack text (T14 pass condition) mentions Amp; acceptance cases that assume several agents need an edit once Codex is the only agent. | docs-writer |
| 12 | `logo.txt` in the architect: unreferenced in what I read; confirm before deleting. | locator |

Test plan note: the sentinel checks above are behavior tests through public interfaces (CLI output, OpenShell state, in-sandbox observations). The repo's rule is that tests need a reviewed plan before they are written (`jackin/AGENTS.md`), so the plan in 5.2 goes to `test-reviewer` before any code in jackin or the role repos is written.

## 8. Sequencing (each unit ends in a verifiable state)

Order is forced by dependencies: roles cannot change until jackin parses the new schema.

| Unit | Repo | Change | Gate |
|---|---|---|---|
| R1 | jackin | Construct v2 image: `sandbox` user, no sudo/firewall/Docker CLI. | The construct validation flow in `docker/construct/README.md` (Buildx via `docker-bake.hcl`); plus an image-content check that no `sudo` binary exists. Exact command per the repo's docs. Not run. |
| R2 | jackin | `RoleRecipe` strict parse in `jackin-config`; delete `jackin-manifest`, `jackin-core/src/manifest.rs`, migrations, `role migrate`. | `cargo nextest run -p jackin-config` and clippy per `COMMITS.md`. Not run. |
| R3 | jackin | `jackin-roles` resolution: boundary checks, Dockerfile rule (construct v2 digest), cache key, agent install layer with `PinnedVersion`, no fallback installers, no hook staging. | `cargo nextest run -p jackin-roles`. |
| R4 | jackin | `jackin-agents`: Codex provider profile (API key and ChatGPT login), Codex `auth.json` writer, binary path constants shared with the install layer. | nextest for the crate; live import into a local gateway (openshell-specialist). |
| R5 | jackin-sentinel | Rewrite per 5.3. | `jackin role validate .`; Buildx build; `verify/` run on a qualified target. |
| R6 | jackin-the-architect | Per 6.2. | Same as R5, plus `scripts/update-jackin-toolchain.rs` still produces the same two files. |
| R7 | jackin-agent-smith | Per 6.1. | Same as R5. |

Gate commands for the role repos' own repo-local checks exist today as `mise run test` (= `prek run --all-files`, `mise.toml`), which covers gitleaks and the marketplace audits only.

## 9. AGENTS.md and related text that becomes wrong

### 9.1 The AGENTS.md files themselves

`AGENTS.md` is identical in all three role repos (`diff` against the architect shows no difference, 15 lines, 2334 bytes). It is the shared working agreement ("No legacy code", "research project", "delegate first", PR-feedback rules). It names no agent, no Docker, no hook, no plugin. **Nothing in these three `AGENTS.md` files becomes wrong after the rewrite.** `CLAUDE.md` is a symlink to it. It remains valid for the developers (and agents) editing the role repos, whatever agents the role ships.

One dangling reference already exists and is not caused by the rewrite: `README.md` in agent-smith (lines 16, 30) and the architect (lines 19, 47) link to "AGENTS.md § Threat model", which does not exist.

### 9.2 Text that does become wrong (in the files that play the agent-instructions role)

| File | Text | Why wrong after the rewrite |
|---|---|---|
| `jackin-the-architect/AGENTS.md.d/caveman.md` | `Switch level: /caveman lite\|full\|ultra\|wenyan` | `/caveman` is the Claude plugin command. Codex gets Caveman as a skill (`README.md:13`), so the slash form is not available. |
| `jackin-the-architect/AGENTS.md.d/token-tools.md` | "RTK (prefix for codex/amp/etc.)" | Amp and "etc." leave; only Codex remains. |
| `jackin-the-architect/Dockerfile:124-135` | Links `AGENTS.md` into `/home/agent/{.claude,.codex,.config/amp,.kimi-code,.grok}` | Home is `/sandbox`; only `.codex` remains. |
| `jackin-sentinel/README.md` | Lines 12-16 load command; 18-28 "every supported agent table", "runtime hooks setup_once, source, preflight", "Docker build output", "Docker E2E"; 30-33 report installed by hooks; 35-47 checklist items 3 and 5 ("Docker CLI availability") | Schema, hooks, report mechanism and Docker assumptions all change. Item 3 is already false today. |
| `jackin-agent-smith/README.md` | Lines 3-5 "not the final Claude runtime"; 11-16 contract; 26-28 plugins under `[claude].plugins`; 20-24 `/workspace` | Claude and `/workspace` leave; contract now includes policy and agents. |
| `jackin-the-architect/README.md` | Line 10 `--agent codex` alternative to "the claude CLI"; 13 Caveman-in-Codex note; 17 construct pin; 23 scheduled workflow; 24 `docker build --build-arg` bump instructions; 26-38 tool list incl. Caveman for claude + codex; 40-60 Plugins/Skills sections; 62-68 Runtime hooks; 70-72 PR workflow | Claude plugins, five agent skill targets, hook semantics, Docker build-arg guidance. |
| `jackin-the-architect/docs/pr-workflow.md` | Entire file | Claude Code slash-command reference and Claude plugin troubleshooting. |
| `jackin/CONTEXT.md` | Instance, Reconnect, Durable home, Default-home, First-seed, Role hook | Container-shaped v1 vocabulary (outside the three role repos). |

Files that need no change for correctness: all Velnor-generated `.github/**` (regenerated by the generator), `.velnor/**`, `config/fleet/**`, `LICENSE*`, `NOTICE`, `REUSE.toml`, `mise.lock`, `jackin-toolchain/**` (regenerated by the existing script).
