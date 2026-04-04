# Project Structure

Quick navigation reference for AI agents working in this repository.

## Root Files

| File | Purpose |
|---|---|
| `Cargo.toml` | Crate manifest — dependencies, lints, edition 2024, MSRV 1.87 |
| `Cargo.lock` | Locked dependency versions |
| `AGENTS.md` | Shared instructions for all AI agents (testing, pre-commit, security) |
| `CLAUDE.md` | Claude-specific pointer to `AGENTS.md` |
| `RULES.md` | Project-wide conventions (docs go in `AGENTS.md`, not tool-specific files) |
| `TODO.md` | Index of open design/engineering items (individual files in `todo/`) |
| `TESTING.md` | Test runner setup, commands, and pre-commit requirements |
| `SECURITY_REVIEW_FINDINGS.md` | Security audit results |
| `SECURITY_EXCEPTIONS.md` | Accepted security findings — do not re-flag |
| `release.toml` | Release configuration |
| `mise.toml` | Tool version management (bun) |
| `.gitignore` | Git ignore rules |

## Open Items — `todo/`

Each file is a self-contained design document with problem statement, options, and related source files.

| File | Topic |
|---|---|
| `construct-user-creation.md` | UID/GID remapping hack in derived images |
| `dind-tls.md` | Unauthenticated Docker daemon on agent network |
| `orphaned-dind-cleanup.md` | Sidecar left running when agent fails to start |
| `onepassword-integration.md` | First-class secret injection at launch time |
| `agent-source-trust.md` | Trust-on-first-use for third-party agent repos |
| `bollard-migration.md` | Replace string-matched Docker errors with typed API |
| `rootless-dind.md` | Reduce privileged container attack surface |
| `sensitive-mount-warnings.md` | Warn before mounting `~/.ssh`, `~/.aws`, etc. |
| `reproducibility-pinning.md` | Commit SHA pinning for agent repos |

## Source Code — `src/`

Rust CLI binary. All modules are flat (no subdirectories).

| Module | Responsibility |
|---|---|
| `main.rs` | Entry point — parses CLI and calls `run()` |
| `lib.rs` | Library root — module declarations, `run()` orchestration, target classification |
| `cli.rs` | Clap command definitions (load, launch, eject, exile, purge, workspace, config) |
| `runtime.rs` | Core engine — agent lifecycle (build, run, attach, eject, purge), Docker image management |
| `docker.rs` | Docker command builder — shell execution abstraction |
| `workspace.rs` | Workspace resolution — mount specs, workdir, saved workspace lookup |
| `config.rs` | TOML config persistence — agent registry, workspaces, mount scopes |
| `manifest.rs` | Agent manifest parser (`jackin.toml` inside agent repos) |
| `derived_image.rs` | Dockerfile generation for agent images from base construct |
| `repo.rs` | Agent repo validation — required files, path traversal checks |
| `repo_contract.rs` | Enforces agent Dockerfiles use the construct base image |
| `instance.rs` | Container naming, clone indices, plugin state preparation |
| `selector.rs` | Agent selector parsing — `owner/repo`, builtins, container names |
| `launch.rs` | Interactive TUI launcher logic — agent/workspace selection |
| `tui.rs` | Ratatui terminal UI components — prompts, hints, display helpers |
| `paths.rs` | XDG-compliant data and config directory resolution |

## Documentation — `docs/`

Astro Starlight site. **Lives alongside source code intentionally** — update docs in the same commit as code changes.

- Published at: https://www.zhokhov.com/jackin/
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
| | `guides/security-model.mdx` | Isolation and permissions |
| | `guides/comparison.mdx` | Comparison with alternatives |
| Commands | `commands/load.mdx` | `jackin load` |
| | `commands/launch.mdx` | `jackin launch` |
| | `commands/hardline.mdx` | `jackin hardline` |
| | `commands/eject.mdx` | `jackin eject` |
| | `commands/exile.mdx` | `jackin exile` |
| | `commands/purge.mdx` | `jackin purge` |
| | `commands/workspace.mdx` | `jackin workspace` |
| | `commands/config.mdx` | `jackin config` |
| Developing Agents | `developing/creating-agents.mdx` | How to build agent repos |
| | `developing/construct-image.mdx` | Base Docker image contents |
| | `developing/agent-manifest.mdx` | `jackin.toml` reference |
| Reference | `reference/configuration.mdx` | Config file format |
| | `reference/architecture.mdx` | Container orchestration internals |
| | `reference/roadmap.mdx` | Planned features |

### Docs Config

| File | Purpose |
|---|---|
| `docs/astro.config.mjs` | Sidebar structure, site metadata, edit links |
| `docs/package.json` | Bun dependencies |
| `docs/bun.lock` | Locked deps |
| `docs/src/styles/custom.css` | Theme overrides |
| `docs/src/content.config.ts` | Astro content collection config |

## Docker — `docker/`

| Path | Purpose |
|---|---|
| `docker/construct/Dockerfile` | Shared base image all agents extend |
| `docker/construct/README.md` | Construct image documentation |
| `docker/construct/install-plugins.sh` | Plugin installation script for the base image |
| `docker/construct/zshrc` | Shell config injected into containers |
| `docker/runtime/entrypoint.sh` | Container entrypoint — UID/GID remapping, DinD setup |

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
| `src/cli.rs` (command flags) | `docs/src/content/docs/commands/<cmd>.mdx` |
| `src/workspace.rs` (mount logic) | `docs/.../guides/workspaces.mdx`, `docs/.../guides/mounts.mdx` |
| `src/config.rs` (config format) | `docs/.../reference/configuration.mdx` |
| `src/runtime.rs` (container lifecycle) | `docs/.../reference/architecture.mdx` |
| `src/manifest.rs` (jackin.toml) | `docs/.../developing/agent-manifest.mdx` |
| `src/derived_image.rs` (Dockerfile gen) | `docs/.../developing/construct-image.mdx` |
| `src/repo.rs` / `src/repo_contract.rs` | `docs/.../guides/agent-repos.mdx` |
| `docker/construct/Dockerfile` | `docs/.../developing/construct-image.mdx` |
