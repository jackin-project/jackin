# Project Structure

Quick nav for AI agents and human contributors. **Canonical detailed module map lives in docs** ([`reference/getting-oriented/codebase-map`](https://jackin.tailrocks.com/reference/getting-oriented/codebase-map/), served from [docs/content/docs/reference/getting-oriented/codebase-map.mdx](docs/content/docs/reference/getting-oriented/codebase-map.mdx)). This file is the short pointer agents land on first; covers **multi-repo ecosystem** and per-PR **code ↔ docs contract**, sends you to docs for rest.

**For what a specific crate is for, its tier/allowed dependencies, its `src/` structure, and its public API, read that crate's README and AGENTS rules file directly** — they are the authoritative, always-current per-crate record (every `crates/*/` member carries both, plus a `CLAUDE.md` symlink, enforced by `cargo xtask lint agents`). The Codebase Map is the ecosystem/tier overview; the per-crate detail lives in the crate that owns it.

## What this file is for

- **Ecosystem table** below — which repo owns what.
- **Code ↔ docs cross-reference** at bottom — which docs page to touch when source area changes.
- Short **map of root files** agent might need fast.

Deeper questions — module layout, what each `src/` subdir owns, where to start changing runtime/console/config model — go to docs:

| Question | Page |
|---|---|
| "Where does the code for X live? / what does crate Y do?" | That crate's README + AGENTS file under `crates/<crate>/` (authoritative); [Behind jackin❯ — crates](https://jackin.tailrocks.com/reference/crates/) (generated from READMEs); [Codebase Map](https://jackin.tailrocks.com/reference/getting-oriented/codebase-map/) for the tier overview |
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
| [`jackin-project/homebrew-tap`](https://github.com/jackin-project/homebrew-tap) | Homebrew formulae — preview now, stable once jackin❯ reaches first stable release |
| [`jackin-project/jackin-marketplace`](https://github.com/jackin-project/jackin-marketplace) | Claude plugin marketplace consumed by role manifests |
| [`jackin-project/validate-agent-action`](https://github.com/jackin-project/validate-agent-action) | GitHub Action validating `jackin.role.toml` in role repos |
| [`jackin-project/jackin-dev`](https://github.com/jackin-project/jackin-dev) | Legacy/internal dev tooling and shared dotfiles; the installed PR verification binary now lives in this repo under `crates/jackin-dev/` |
| [`jackin-project/jackin-github-terraform`](https://github.com/jackin-project/jackin-github-terraform) | Terraform managing the `jackin-project` GitHub org |

## Root files and crates

Workspace Rust source lives under [crates/](crates/). For what each crate owns, its tier, structure, and public API, read that crate's README (authoritative) or the generated docs section [Behind jackin❯ — crates](https://jackin.tailrocks.com/reference/crates/) (built from those READMEs at docs-build time). The [Codebase Map](https://jackin.tailrocks.com/reference/getting-oriented/codebase-map/) is the ecosystem/tier overview only — do not duplicate per-crate prose here.

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
| Module structure in [crates/](crates/) (added/split/renamed module) | The affected `crates/<crate>/README.md` (see [`crates/AGENTS.md`](crates/AGENTS.md) "Per-crate README + AGENTS.md" rule); the docs build regenerates [Behind jackin❯ — crates](https://jackin.tailrocks.com/reference/crates/) from READMEs; update `docs/.../reference/getting-oriented/codebase-map.mdx` only for tier/DAG changes |

## Keeping the docs fresh

Per-crate README (source of truth), the generated crates section, the Codebase Map tier overview, and the cross-reference table above = the places structural changes show up first. If your PR adds a new module directory, splits a file into a subdir, introduces a new cross-cutting helper, or renames a public surface — **update the affected crate README in the same PR** (the docs build regenerates the site pages). Touch the Codebase Map only for tier/DAG changes. See [`crates/AGENTS.md`](crates/AGENTS.md) for the README-update rule and [`TODO.md`](TODO.md) for the stale-docs check every structural PR runs.
