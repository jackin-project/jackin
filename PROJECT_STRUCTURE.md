# Project Structure

Quick nav for AI agents and human contributors. **Canonical detailed module map lives in docs** ([`reference/getting-oriented/codebase-map`](https://jackin.tailrocks.com/reference/getting-oriented/codebase-map/), served from [docs/content/docs/reference/getting-oriented/codebase-map.mdx](docs/content/docs/reference/getting-oriented/codebase-map.mdx)). This file is the short pointer agents land on first; covers **multi-repo ecosystem** and per-PR **code ↔ docs contract**, sends you to docs for rest.

## What this file is for

- **Ecosystem table** below — which repo owns what.
- **Code ↔ docs cross-reference** at bottom — which docs page to touch when source area changes.
- Short **map of root files** agent might need fast.

Deeper questions — module layout, what each `src/` subdir owns, where to start changing runtime/console/config model — go to docs:

| Question | Page |
|---|---|
| "Where does the code for X live?" | [Codebase Map](https://jackin.tailrocks.com/reference/getting-oriented/codebase-map/) (mirrored at [docs/content/docs/reference/getting-oriented/codebase-map.mdx](docs/content/docs/reference/getting-oriented/codebase-map.mdx)) |
| "How does jackin❯ orchestrate containers?" | [Architecture](https://jackin.tailrocks.com/reference/getting-oriented/architecture/) |
| "How do instance identity, restore, and parallel sessions work?" | [Runtime Instance Model](https://jackin.tailrocks.com/reference/runtime/runtime-instance-model/) |
| "What does `~/.config/jackin/config.toml` look like?" | [Configuration File](https://jackin.tailrocks.com/reference/runtime/configuration/) |
| "How are role repositories structured?" | [Role Repositories](https://jackin.tailrocks.com/guides/role-repos/) |
| "What is on the roadmap?" | [Roadmap](https://jackin.tailrocks.com/reference/roadmap/) |

Docs = single source of truth for *narrative* internals. This file stays terse on purpose — prose belongs on docs page; here we point at it.

## Ecosystem repositories

jackin❯ split across multiple GitHub repos. This repo owns CLI; siblings own roles, construct image source, Homebrew tap, docs site (docs live inside this repo today — see roadmap item [Move documentation to a separate repository](https://jackin.tailrocks.com/reference/roadmap/docs-separate-repository/)).

| Repository | Owns |
|---|---|
| [`jackin-project/jackin`](https://github.com/jackin-project/jackin) (this repo) | CLI source, `construct` Dockerfile under `docker/construct/`, docs site under `docs/`, CI workflows |
| [`jackin-project/jackin-agent-smith`](https://github.com/jackin-project/jackin-agent-smith) | Default general-purpose role (`agent-smith`) |
| [`jackin-project/jackin-the-architect`](https://github.com/jackin-project/jackin-the-architect) | Rust-development role (`the-architect`) used to develop jackin❯ itself |
| [`jackin-project/homebrew-tap`](https://github.com/jackin-project/homebrew-tap) | Homebrew formulae — preview now, stable once jackin reaches first stable release |
| [`jackin-project/jackin-marketplace`](https://github.com/jackin-project/jackin-marketplace) | Claude plugin marketplace consumed by role manifests |
| [`jackin-project/validate-agent-action`](https://github.com/jackin-project/validate-agent-action) | GitHub Action validating `jackin.role.toml` in role repos |
| [`jackin-project/jackin-dev`](https://github.com/jackin-project/jackin-dev) | Legacy/internal dev tooling and shared dotfiles; the installed PR verification binary now lives in this repo under `crates/jackin-dev/` |
| [`jackin-project/jackin-github-terraform`](https://github.com/jackin-project/jackin-github-terraform) | Terraform managing the `jackin-project` GitHub org |

## Root files in this repo

Workspace source under [crates/](crates/); supporting files at repo root:

| File | Purpose |
|---|---|
| [Cargo.toml](Cargo.toml) | Workspace manifest — members, dependencies, lints, MSRV |
| [Cargo.lock](Cargo.lock) | Locked dependency versions |
| [build.rs](build.rs) | Cargo build script (compile-time codegen / env) |
| [AGENTS.md](AGENTS.md) | Slim index of agent rules — one line per rule, linking to topic file with detail |
| [CLAUDE.md](CLAUDE.md) | Symlink to [AGENTS.md](AGENTS.md) (every dir with [AGENTS.md](AGENTS.md) has `CLAUDE.md` symlink beside it) |
| [RULES.md](RULES.md) | Doc-location + symlink convention, brand spelling, deprecations, TUI labels/keybindings/modals |
| [BRANCHING.md](BRANCHING.md) | Branch naming + merge policy + agent stay-on-active-branch rule |
| [COMMITS.md](COMMITS.md) | Conventional Commits format + DCO sign-off + push-after-commit |
| [PULL_REQUESTS.md](PULL_REQUESTS.md) | PR flow, body shape, review, roadmap & docs gates, solo-maintainer model |
| [TESTING.md](TESTING.md) | Test runner setup, commands, capsule fixtures, operator `--debug` validation |
| [ENGINEERING.md](ENGINEERING.md) | Cross-cutting code rules: prefer-libraries, DRY, two-tier telemetry, comments |
| [HOST_AND_CONTAINER.md](HOST_AND_CONTAINER.md) | Host-write ban + `/jackin/` container-path convention |
| [PRERELEASE.md](PRERELEASE.md) | Breaking-change policy, schema versioning gate, changelog hold |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Contribution flow + DCO v1.1 text |
| [DEPRECATED.md](DEPRECATED.md) | Ledger of deprecated APIs / CLIs / config values |
| [TODO.md](TODO.md) | Small follow-ups and per-PR stale-docs check |
| [release.toml](release.toml) | Release configuration |
| [mise.toml](mise.toml) | Tool versions and construct image task definitions |
| `crates/jackin-dev/` | Installed developer helper binary (`jackin-dev`) for local PR checkout/sync/isolation workflows |
| `crates/jackin-console-oppicker/` | Extracted pure 1Password picker model/planning crate used by the console TUI facade |
| `crates/jackin-xtask/` | Workspace automation binary (`cargo xtask`): construct image tasks + PTY fixture extraction; full command inventory at [Workspace Automation](https://jackin.tailrocks.com/reference/getting-oriented/xtasks/) |
| [docker-bake.hcl](docker-bake.hcl) | Declarative Docker Bake build graph for construct image |
| `rust-toolchain.toml` | Pinned Rust toolchain (CI-enforced MSRV) |

For **Rust source tree** — [crates/jackin/src/app/](crates/jackin/src/app/), [crates/jackin/src/cli/](crates/jackin/src/cli/), [crates/jackin-runtime/src/runtime/](crates/jackin-runtime/src/runtime/), [crates/jackin/src/workspace/](crates/jackin/src/workspace/), [crates/jackin/src/console/](crates/jackin/src/console/), and extracted subsystem crates under [crates/](crates/) — see [Codebase Map](https://jackin.tailrocks.com/reference/getting-oriented/codebase-map/). That page (and this) updated in same PR as any module-level structural change (R1 added core/ansi_tokens.rs + launch-tui/launch_output.rs; R2 flipped arch gate + CI to --strict; R3 split console editor state impls by responsibility; R4 carved the op-picker pure model into `jackin-console-oppicker`), so never falls behind.

## Documentation site (`docs/`)

Fumadocs site on TanStack Start and Vite. **Lives alongside source today** — update docs in same commit as code (see roadmap item [Move documentation to a separate repository](https://jackin.tailrocks.com/reference/roadmap/docs-separate-repository/)).

- Published at: <https://jackin.tailrocks.com/>
- Dev server: `cd docs && bun run dev`
- Build: `cd docs && bun run build`
- Package manager: **bun only** (not npm/pnpm/yarn)
- Has own [docs/AGENTS.md](docs/AGENTS.md) and [docs/CLAUDE.md](docs/CLAUDE.md)

Sidebar split by **three audiences**:

- **Operator** (Getting Started, Operator Guide, Commands) — uses jackin❯ as product through CLI/TUI. Pages describe behaviour through CLI/TUI flows — no TOML schemas, no on-disk paths, no Rust internals.
- **Role author** (Role Authoring) — *also user-facing*, but for users building own role repos (`backend-engineer`, `docs-writer`, `security-reviewer`, …). Explain how to create role from scratch, manifest schema, what tools ship in `construct`. No knowledge of jackin❯ implementation required.
- **Contributor** (Behind jackin❯ — Internals) — works on jackin❯ itself. Architecture, Configuration File schema, Codebase Map, Roadmap. On-disk layouts, internal mechanisms, Rust-level detail live here.

Slugs stable across audience split — parenthesized content group directories keep audience organization out of URLs.

## Docker (`docker/`)

| Path | Purpose |
|---|---|
| [docker/construct/Dockerfile](docker/construct/Dockerfile) | Shared base image all roles extend |
| [docker/construct/README.md](docker/construct/README.md) | `construct` image documentation |
| [docker/construct/zshrc](docker/construct/zshrc) | Shell config injected into containers |
| [docker/runtime/entrypoint.sh](docker/runtime/entrypoint.sh) | Source for runtime entrypoint copied into derived images at `/jackin/runtime/entrypoint.sh` |

For runtime behavior, see [The Construct Image](https://jackin.tailrocks.com/developing/construct-image/) and [Architecture](https://jackin.tailrocks.com/reference/getting-oriented/architecture/).

## CI/CD (`.github/workflows/`)

| Workflow | Triggers |
|---|---|
| `ci.yml` | Runs fmt, clippy, Rust test suite on PRs and pushes |
| `construct.yml` | Builds and publishes `construct` base Docker image |
| `docs.yml` | Builds and deploys documentation site |
| `preview.yml` | Publishes Homebrew preview formula (dispatch-from-main only) |
| `release.yml` | Creates release artifacts |
| `renovate.yml` | Self-hosted Renovate dependency update runner |
| `renovate-validate.yml` | Verifies upstream sources Renovate's `customManagers` point at still resolve |

## Code ↔ docs cross-reference

Changing behaviour: update both sides in same PR. This table = **per-PR contract** every agent consults before opening PR for listed area:

| Code change in | Update docs in |
|---|---|
| [crates/jackin/src/cli/](crates/jackin/src/cli/) (command flags or help text) | `docs/content/docs/commands/<cmd>.mdx` |
| [crates/jackin/src/workspace/](crates/jackin/src/workspace/) (mount logic) | `docs/.../guides/workspaces.mdx`, `docs/.../guides/mounts.mdx` |
| [crates/jackin-config/src/](crates/jackin-config/src/) (config format) | `docs/.../reference/runtime/configuration.mdx` |
| [crates/jackin-runtime/src/runtime/](crates/jackin-runtime/src/runtime/) (container lifecycle) | `docs/.../reference/getting-oriented/architecture.mdx`, `docs/.../reference/runtime/runtime-instance-model.mdx` |
| [crates/jackin-host/src/caffeinate.rs](crates/jackin-host/src/caffeinate.rs) (keep_awake reconciler) | `docs/.../guides/workspaces.mdx` (keep_awake section) |
| [crates/jackin-isolation/src/](crates/jackin-isolation/src/) (per-mount isolation, materialization, finalizer) | `docs/.../guides/workspaces.mdx` (per-mount isolation section), `docs/.../guides/mounts.mdx` (isolation field), `docs/.../reference/runtime/configuration.mdx` (`MountConfig.isolation`), `docs/.../reference/getting-oriented/architecture.mdx` (materialization + finalizer), `docs/.../commands/load.mdx` (`--force`), `docs/.../commands/workspace.mdx` (`--mount-isolation`, Isolation column), `docs/.../commands/purge.mdx` (running-agent guard + isolated cleanup) |
| [crates/jackin-instance/src/](crates/jackin-instance/src/) (instance identity, manifests, auth state preparation) | `docs/.../reference/runtime/runtime-instance-model.mdx`; auth-forward changes also update `docs/.../guides/authentication.mdx` and `docs/.../guides/security-model.mdx` |
| [crates/jackin-manifest/src/](crates/jackin-manifest/src/) (`jackin.role.toml` schema or validation) | `docs/.../developing/role-manifest.mdx` |
| [crates/jackin-instance/src/auth.rs](crates/jackin-instance/src/auth.rs) (auth-forward, credential handling) | `docs/.../guides/authentication.mdx`, `docs/.../guides/security-model.mdx` |
| [crates/jackin-core/src/env_model.rs](crates/jackin-core/src/env_model.rs), [crates/jackin-env/src/env_resolver.rs](crates/jackin-env/src/env_resolver.rs) (env policy) | `docs/.../developing/role-manifest.mdx` (env section), `docs/.../guides/environment-variables.mdx` (reserved-name list) |
| [crates/jackin-image/src/image_recipe.rs](crates/jackin-image/src/image_recipe.rs) (Dockerfile gen) | `docs/.../developing/construct-image.mdx` |
| [crates/jackin-manifest/src/repo.rs](crates/jackin-manifest/src/repo.rs) / role repo validation paths | `docs/.../guides/role-repos.mdx` |
| [docker/construct/Dockerfile](docker/construct/Dockerfile) | `docs/.../developing/construct-image.mdx` |
| Module structure in [crates/](crates/) (added/split/renamed module) | `docs/.../reference/getting-oriented/codebase-map.mdx` |

## Keeping the docs fresh

Codebase Map and cross-reference table above = two places structural changes show up first. If your PR adds new module directory, splits file into subdir, introduces new cross-cutting helper, or renames public surface — **update `docs/.../reference/getting-oriented/codebase-map.mdx` and (if relevant) cross-reference table above in same PR**. See [`TODO.md`](TODO.md) for stale-docs check every structural PR runs.
