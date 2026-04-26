# Project Structure

Quick navigation reference for AI agents working in this repository.

## Root Files

| File | Purpose |
|---|---|
| `Cargo.toml` | Crate manifest — dependencies and lints |
| `Cargo.lock` | Locked dependency versions |
| `AGENTS.md` | Shared instructions for all AI agents (testing, pre-commit, security) |
| `CLAUDE.md` | Claude-specific pointer to `AGENTS.md` |
| `RULES.md` | Project-wide conventions (docs go in `AGENTS.md`, not tool-specific files) |
| `TODO.md` | Small follow-ups (especially upstream-dependency tracking), per-PR stale-docs checklist, and `TODO(<topic>)` marker convention. Larger feature work lives under `docs/src/content/docs/reference/roadmap/` |
| `TESTING.md` | Test runner setup, commands, and pre-commit requirements |
| `release.toml` | Release configuration |
| `mise.toml` | Tool version management (bun) |
| `Justfile` | Construct image build commands — contributor-facing `just` wrapper |
| `docker-bake.hcl` | Declarative Docker Bake build graph for the construct image |
| `.gitignore` | Git ignore rules |

## Roadmap — `docs/src/content/docs/reference/roadmap/`

Self-contained design docs live alongside the rest of the Starlight
docs site. Each page includes problem statement, options, and related
source files. Browse via the sidebar (`Reference → Roadmap`) or on
the deployed site at <https://jackin.tailrocks.com/reference/roadmap/>.

## Source Code — `src/`

Rust CLI binary. Every significant concern has its own directory;
crate-root files are reserved for items that don't cluster into
a domain group.

### Crate root

| File | Responsibility |
|---|---|
| `main.rs` | Entry point — constructs `Cli` and calls `jackin::run()` |
| `lib.rs` | Thin crate root (~20 LOC) — module declarations + `pub use app::run` |
| `bin/validate.rs` | Separate binary for validating agent manifests (`jackin-validate`) |

### Module tree

| Module | Owns |
|---|---|
| `app/` | `run()` command dispatch (`mod.rs`) and context helpers (`context.rs`): target classification, workspace-for-cwd, agent-from-context, last-agent persistence |
| `cli/` | Clap schema split by topic: `root.rs` (`Cli` + `Command` enum), `agent.rs` (Load/Hardline/Launch args, `--force`), `cleanup.rs` (Eject/Purge args), `workspace.rs` (`WorkspaceCommand`, `--mount-isolation`, `--delete-isolated-state`), `cd.rs` (`jackin cd <container> [dst]` — child shell into an isolated worktree), `config.rs` (`ConfigCommand` + sub-enums), `dispatch.rs` (bare-`jackin`/`console`/`launch` classification — `classify`, `is_tui_capable`, deprecation shims) |
| `workspace/` | Workspace model and planning. `mod.rs` (types, re-exports), `paths.rs` (expand_tilde, resolve_path), `mounts.rs` (parse/validate), `planner.rs` (`plan_create`, `plan_edit`, `plan_collapse`), `resolve.rs` (runtime resolution), `sensitive.rs` (sensitive-mount detection) |
| `config/` | TOML config model and persistence. `mod.rs` (types, `require_workspace` helper), `persist.rs` (load/save), `agents.rs` (builtin sync, trust, auth-forward), `mounts.rs` (global mount registry), `workspaces.rs` (workspace CRUD) |
| `manifest/` | Agent-manifest (`jackin.agent.toml`) schema + validator. `mod.rs` (schema structs, `load`, `display_name`), `validate.rs` (`validate`, `is_valid_env_var_name`) |
| `runtime/` | Container lifecycle. `mod.rs` (thin re-exports), `naming.rs` (labels, container/image naming, family matching), `identity.rs` (git/host identity), `repo_cache.rs` (repo lock + fetch), `image.rs` (docker build), `launch.rs` (`launch_agent_runtime`, `load_agent`, `load_agent_with`, runs the foreground finalizer after attach returns), `attach.rs` (attach + hardline + DinD readiness — calls the same finalizer post-attach), `discovery.rs` (list managed agents), `cleanup.rs` (eject, purge, orphan GC), `test_support.rs` (shared `FakeRunner`) |
| `isolation/` | Per-mount isolation. `mod.rs` (`MountIsolation` enum), `branch.rs` (scratch-branch naming), `materialize.rs` (worktree creation + `MaterializedWorkspace`), `state.rs` (`isolation.json` IO), `finalize.rs` (post-attach foreground finalizer — Preserved / Cleaned / ReturnToAgent decision), `cleanup.rs` (force/safe cleanup helpers shared by `purge` and the finalizer) |
| `console/` | Interactive operator-console TUI. `mod.rs` (`run_console` entrypoint), `state.rs` (`ConsoleState`, `WorkspaceChoice`), `input.rs` (event handling), `preview.rs` (workspace preview + detail lines), `render.rs` (all drawing functions), `manager/` (workspace-manager TUI subsystem — `state.rs`, `input.rs`, `render.rs`, `create.rs`, `mount_info.rs`), `widgets/` (reusable modal/widget components — `file_browser`, `text_input`, `confirm`, `confirm_save`, `error_popup`, `mount_dst_choice`, `workdir_pick`, `github_picker`, `save_discard`, `panel_rain`) |
| `instance/` | Per-container state preparation. `mod.rs` (`AgentState`, orchestration), `naming.rs` (container slug + clone naming + class-family matching), `auth.rs` (auth-forward modes + credential handling + symlink safety), `plugins.rs` (plugin-marketplace serialization) |
| `tui/` | General terminal UI helpers (separate from the operator console). `mod.rs` (shared palette, `DEBUG_MODE`), `animation.rs` (intro/outro, digital rain), `output.rs` (tables, hints, fatal, logo, title), `prompt.rs` (`prompt_choice`, `spin_wait`, `require_interactive_stdin`) |

### Flat helper files at crate root

| File | Responsibility |
|---|---|
| `env_model.rs` | Single source of truth for env policy — reserved-runtime-env list, `is_reserved`, `extract_interpolation_refs`, `topological_env_order` (cycle detection) |
| `env_resolver.rs` | Runtime env resolution — `resolve_env`, interpolation, interactive prompts |
| `selector.rs` | Agent selector parsing — `ClassSelector`, `Selector`, `TryFrom<&str>` impls |
| `repo.rs` | Agent repo validation — required files, path traversal checks |
| `repo_contract.rs` | Enforces agent `Dockerfile`s extend the `construct` base image |
| `derived_image.rs` | Dockerfile generation for agent images from the base construct |
| `docker.rs` | Docker command builder — `CommandRunner` trait and `ShellRunner` |
| `terminal_prompter.rs` | Interactive env-var prompting for manifest resolution |
| `version_check.rs` | Claude CLI version detection for image cache-bust |
| `paths.rs` | XDG-compliant data and config directory resolution (`JackinPaths`) |

## Documentation — `docs/`

Astro Starlight site. **Lives alongside source code intentionally** — update docs in the same commit as code changes.

- Published at: https://jackin.tailrocks.com/
- Dev server: `cd docs && bun run dev`
- Build: `cd docs && bun run build`
- Package manager: **bun only** (not npm/pnpm/yarn)
- Has its own `AGENTS.md` and `CLAUDE.md` at `docs/`

### Docs Content — `docs/src/content/docs/`

Maps 1:1 with the published site sidebar:

| Section | Files | Covers |
|---|---|---|
| Getting Started | `getting-started/why.mdx` | Why jackin' exists |
| | `getting-started/installation.mdx` | Install methods + prerequisites |
| | `getting-started/quickstart.mdx` | First-run walkthrough |
| | `getting-started/concepts.mdx` | Operators, agents, constructs, workspaces |
| Guides | `guides/workspaces.mdx` | Workspace configuration |
| | `guides/mounts.mdx` | Mount specs and scoping |
| | `guides/agent-repos.mdx` | Agent repository structure |
| | `guides/authentication.mdx` | Credential forwarding / in-container auth |
| | `guides/security-model.mdx` | Isolation and permissions |
| | `guides/comparison.mdx` | Comparison with alternatives |
| Commands | `commands/load.mdx` | `jackin load` |
| | `commands/console.mdx` | `jackin console` (bare `jackin` dispatches here) |
| | `commands/cd.mdx` | `jackin cd` (open a child shell in an isolated worktree) |
| | `commands/launch.mdx` | `jackin launch` (deprecated alias for `jackin console`) |
| | `commands/hardline.mdx` | `jackin hardline` |
| | `commands/eject.mdx` | `jackin eject` |
| | `commands/exile.mdx` | `jackin exile` |
| | `commands/purge.mdx` | `jackin purge` |
| | `commands/workspace.mdx` | `jackin workspace` |
| | `commands/config.mdx` | `jackin config` |
| Developing Agents | `developing/creating-agents.mdx` | How to build agent repos |
| | `developing/construct-image.mdx` | Base Docker image contents |
| | `developing/agent-manifest.mdx` | `jackin.agent.toml` reference |
| Reference | `reference/configuration.mdx` | Config file format |
| | `reference/architecture.mdx` | Container orchestration internals |
| | `reference/roadmap.mdx` | Planned features |

### Docs Config

| File | Purpose |
|---|---|
| `docs/astro.config.ts` | Sidebar structure, site metadata, edit links, component overrides (TypeScript — all config is TS, no `.mjs`) |
| `docs/package.json` | Bun dependencies |
| `docs/bun.lock` | Locked deps |
| `docs/src/styles/fonts.css` | Self-hosted fontsource imports + `Inter Black` `@font-face` for the wordmark |
| `docs/src/styles/docs-theme.css` | Starlight chrome → brand tokens mapping |
| `docs/src/styles/global.css` | Tailwind v4 entry + landing utility tokens |
| `docs/src/content.config.ts` | Astro content collection config (Content Layer API via `docsLoader()`) |
| `docs/src/components/overrides/` | Starlight component overrides (Head, SiteTitle, ThemeSelect, PageSidebar, SocialIcons) |
| `docs/src/components/landing/` | React islands + standalone CSS for the landing route |
| `docs/src/pages/index.astro` | Landing route — plain Astro page, NOT a Starlight content entry |
| `docs/src/pages/og/[...slug].png.ts` | Per-page OG card generator (astro-og-canvas + local fontsource files) |

## Docker — `docker/`

| Path | Purpose |
|---|---|
| `docker/construct/Dockerfile` | Shared base image all agents extend |
| `docker/construct/README.md` | Construct image documentation |
| `docker/construct/install-plugins.sh` | Plugin installation script for the base image |
| `docker/construct/zshrc` | Shell config injected into containers |
| `docker/runtime/entrypoint.sh` | Container entrypoint at runtime — git identity setup, `gh auth setup-git` when gh is already authenticated (never performs login itself), plugin install, MCP server registration, pre-launch hook, then `exec claude`. UID/GID remapping happens during the derived-image build (`src/derived_image.rs`), not here. |

## CI/CD — `.github/workflows/`

| Workflow | Triggers |
|---|---|
| `construct.yml` | Builds and publishes the construct base Docker image |
| `docs.yml` | Builds and deploys the documentation site |
| `release.yml` | Creates release artifacts |

## Code ↔ Docs Cross-Reference

When changing behavior, update both sides:

| Code change in | Update docs in |
|---|---|
| `src/cli/**` (command flags or help text) | `docs/src/content/docs/commands/<cmd>.mdx` |
| `src/workspace/**` (mount logic) | `docs/.../guides/workspaces.mdx`, `docs/.../guides/mounts.mdx` |
| `src/config/**` (config format) | `docs/.../reference/configuration.mdx` |
| `src/runtime/**` (container lifecycle) | `docs/.../reference/architecture.mdx` |
| `src/isolation/**` (per-mount isolation, materialization, finalizer) | `docs/.../guides/workspaces.mdx` (per-mount isolation section), `docs/.../guides/mounts.mdx` (isolation field), `docs/.../reference/configuration.mdx` (`MountConfig.isolation`), `docs/.../reference/architecture.mdx` (materialization + finalizer), `docs/.../commands/load.mdx` (`--force`), `docs/.../commands/workspace.mdx` (`--mount-isolation`, Isolation column), `docs/.../commands/cd.mdx`, `docs/.../commands/purge.mdx` (running-agent guard + isolated cleanup) |
| `src/manifest/**` (`jackin.agent.toml` schema or validation) | `docs/.../developing/agent-manifest.mdx` |
| `src/instance/auth.rs` (auth-forward, credential handling) | `docs/.../guides/authentication.mdx`, `docs/.../guides/security-model.mdx` |
| `src/env_model.rs`, `src/env_resolver.rs` (env policy) | `docs/.../developing/agent-manifest.mdx` (env section) |
| `src/derived_image.rs` (Dockerfile gen) | `docs/.../developing/construct-image.mdx` |
| `src/repo.rs` / `src/repo_contract.rs` | `docs/.../guides/agent-repos.mdx` |
| `docker/construct/Dockerfile` | `docs/.../developing/construct-image.mdx` |

## Keeping this file fresh

If a PR changes module boundaries — adds a new module directory,
splits a file into a subdirectory, introduces a new cross-cutting
helper — **update the module tree above in the same PR**. See
[`TODO.md`](TODO.md) for the stale-docs check every structural PR
should run.
