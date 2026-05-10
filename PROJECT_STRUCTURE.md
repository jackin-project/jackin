# Project Structure

Quick navigation for AI agents and human contributors working in this
repository. **The canonical, detailed module map lives in the docs**
([`reference/codebase-map`](https://jackin.tailrocks.com/reference/codebase-map/),
served from `docs/src/content/docs/reference/codebase-map.mdx`). This
file is the short pointer that agents land on first; it covers the
**multi-repo ecosystem** and the per-PR **code ↔ docs contract**, and
sends you to the docs for everything else.

## What this file is for

- The **ecosystem table** below — which repository owns what.
- The **code ↔ docs cross-reference** at the bottom — which docs page
  must be touched when a given source area changes.
- A short **map of root files** an agent might need to find quickly.

For any deeper question — module layout, what each `src/` subdirectory
owns, where to start when changing the runtime, the operator console,
the configuration model, etc. — go to the docs:

| Question | Page |
|---|---|
| "Where does the code for X live?" | [Codebase Map](https://jackin.tailrocks.com/reference/codebase-map/) (mirrored at `docs/src/content/docs/reference/codebase-map.mdx`) |
| "How does jackin' orchestrate containers?" | [Architecture](https://jackin.tailrocks.com/reference/architecture/) |
| "What does `~/.config/jackin/config.toml` look like?" | [Configuration File](https://jackin.tailrocks.com/reference/configuration/) |
| "How are role repositories structured?" | [Role Repositories](https://jackin.tailrocks.com/guides/role-repos/) |
| "What is on the roadmap?" | [Roadmap](https://jackin.tailrocks.com/reference/roadmap/) |

The docs are deliberately the single source of truth for the *narrative*
explanation of jackin's internals. This file stays terse on purpose —
prose belongs on a docs page; here we point at it.

## Ecosystem repositories

jackin' is intentionally split across multiple GitHub repositories. This
repo owns the CLI; sibling repos own roles, the construct image source,
the Homebrew tap, and the docs site (today the docs site lives inside
this repo — see roadmap item
[Move documentation to a separate repository](https://jackin.tailrocks.com/reference/roadmap/docs-separate-repository/)
for the discussion of moving it out).

| Repository | Owns |
|---|---|
| [`jackin-project/jackin`](https://github.com/jackin-project/jackin) (this repo) | CLI source, the `construct` Dockerfile under `docker/construct/`, the docs site under `docs/`, CI workflows |
| [`jackin-project/jackin-agent-smith`](https://github.com/jackin-project/jackin-agent-smith) | Default general-purpose role (`agent-smith`) |
| [`jackin-project/jackin-the-architect`](https://github.com/jackin-project/jackin-the-architect) | Rust-development role (`the-architect`) used to develop jackin' itself |
| [`jackin-project/homebrew-tap`](https://github.com/jackin-project/homebrew-tap) | Homebrew formulae for preview now and stable once jackin reaches its first stable release |
| [`jackin-project/jackin-marketplace`](https://github.com/jackin-project/jackin-marketplace) | Claude plugin marketplace consumed by role manifests |
| [`jackin-project/validate-agent-action`](https://github.com/jackin-project/validate-agent-action) | GitHub Action that validates `jackin.role.toml` in role repos |
| [`jackin-project/jackin-dev`](https://github.com/jackin-project/jackin-dev) | Internal development tooling and shared dotfiles |
| [`jackin-project/jackin-github-terraform`](https://github.com/jackin-project/jackin-github-terraform) | Terraform that manages the `jackin-project` GitHub org |

## Root files in this repo

The CLI source lives under `src/`; supporting files at the repo root:

| File | Purpose |
|---|---|
| `Cargo.toml` | Crate manifest — dependencies, lints, MSRV |
| `Cargo.lock` | Locked dependency versions |
| `AGENTS.md` | Shared instructions for all AI agents (testing, pre-commit, security, PR conventions) |
| `CLAUDE.md` | Claude-specific pointer to `AGENTS.md` |
| `RULES.md` | Project-wide conventions (docs go in `AGENTS.md`, not tool-specific files) |
| `BRANCHING.md` | Branch naming + merge policy |
| `COMMITS.md` | Conventional Commits format + DCO sign-off |
| `TESTING.md` | Test runner setup, commands, and pre-commit requirements |
| `CONTRIBUTING.md` | Contribution flow + DCO v1.1 text |
| `DEPRECATED.md` | Ledger of deprecated APIs / CLIs / config values |
| `TODO.md` | Small follow-ups and the per-PR stale-docs check |
| `release.toml` | Release configuration |
| `mise.toml` | Tool version management (bun + just) |
| `Justfile` | Construct image build commands |
| `docker-bake.hcl` | Declarative Docker Bake build graph for the construct image |
| `rust-toolchain.toml` | Pinned Rust toolchain (CI-enforced MSRV) |

For the **Rust source tree** — `src/app/`, `src/cli/`, `src/runtime/`,
`src/workspace/`, `src/console/`, etc., plus crate-root helpers like
`src/derived_image.rs` and `src/env_model.rs` — see the
[Codebase Map](https://jackin.tailrocks.com/reference/codebase-map/).
That page is updated in the same PR as any module-level structural
change, so it never falls behind.

## Documentation site (`docs/`)

Astro Starlight site. **Lives alongside source code today** — update
docs in the same commit as code changes (see roadmap item
[Move documentation to a separate repository](https://jackin.tailrocks.com/reference/roadmap/docs-separate-repository/)
for the longer-term discussion).

- Published at: <https://jackin.tailrocks.com/>
- Dev server: `cd docs && bun run dev`
- Build: `cd docs && bun run build`
- Package manager: **bun only** (not npm/pnpm/yarn)
- Has its own `AGENTS.md` and `CLAUDE.md` at `docs/`

The site sidebar is split by **three audiences**:

- **Operator** (Getting Started, Operator Guide, Commands) — uses
  jackin' as a product through CLI and TUI. Pages describe behaviour
  through CLI/TUI flows — no TOML schemas, no on-disk paths, no Rust
  internals.
- **Role author** (Role Authoring) — *also a user-facing audience*,
  but for users building their own role repos (`backend-engineer`,
  `docs-writer`, `security-reviewer`, …). These pages explain how to
  create a role from scratch, the manifest schema, and what tools
  ship in the `construct`. They do not require any knowledge of how
  jackin' is implemented.
- **Contributor** (Behind jackin' — Internals) — works on jackin'
  itself. Architecture, Configuration File schema, Codebase Map,
  Roadmap. This is where on-disk layouts, internal mechanisms, and
  Rust-level detail live.

Slugs are stable across the audience split — the audience
distinction is enforced in `docs/astro.config.ts`, not in URLs.

## Docker (`docker/`)

| Path | Purpose |
|---|---|
| `docker/construct/Dockerfile` | Shared base image all roles extend |
| `docker/construct/README.md` | `construct` image documentation |
| `docker/construct/zshrc` | Shell config injected into containers |
| `docker/runtime/entrypoint.sh` | Source for the runtime entrypoint copied into derived images at `/jackin/runtime/entrypoint.sh` |

For what each piece does at runtime, see
[The Construct Image](https://jackin.tailrocks.com/developing/construct-image/)
and [Architecture](https://jackin.tailrocks.com/reference/architecture/).

## CI/CD (`.github/workflows/`)

| Workflow | Triggers |
|---|---|
| `construct.yml` | Builds and publishes the `construct` base Docker image |
| `docs.yml` | Builds and deploys the documentation site |
| `release.yml` | Creates release artifacts |

## Code ↔ docs cross-reference

When changing behaviour, update both sides in the same PR. This table
is the **per-PR contract** every agent should consult before opening a
PR for the listed area:

| Code change in | Update docs in |
|---|---|
| `src/cli/**` (command flags or help text) | `docs/src/content/docs/commands/<cmd>.mdx` |
| `src/workspace/**` (mount logic) | `docs/.../guides/workspaces.mdx`, `docs/.../guides/mounts.mdx` |
| `src/config/**` (config format) | `docs/.../reference/configuration.mdx` |
| `src/runtime/**` (container lifecycle) | `docs/.../reference/architecture.mdx` |
| `src/runtime/caffeinate.rs` (keep_awake reconciler) | `docs/.../guides/workspaces.mdx` (keep_awake section) |
| `src/isolation/**` (per-mount isolation, materialization, finalizer) | `docs/.../guides/workspaces.mdx` (per-mount isolation section), `docs/.../guides/mounts.mdx` (isolation field), `docs/.../reference/configuration.mdx` (`MountConfig.isolation`), `docs/.../reference/architecture.mdx` (materialization + finalizer), `docs/.../commands/load.mdx` (`--force`), `docs/.../commands/workspace.mdx` (`--mount-isolation`, Isolation column), `docs/.../commands/purge.mdx` (running-agent guard + isolated cleanup) |
| `src/manifest/**` (`jackin.role.toml` schema or validation) | `docs/.../developing/role-manifest.mdx` |
| `src/instance/auth.rs` (auth-forward, credential handling) | `docs/.../guides/authentication.mdx`, `docs/.../guides/security-model.mdx` |
| `src/env_model.rs`, `src/env_resolver.rs` (env policy) | `docs/.../developing/role-manifest.mdx` (env section), `docs/.../guides/environment-variables.mdx` (reserved-name list) |
| `src/derived_image.rs` (Dockerfile gen) | `docs/.../developing/construct-image.mdx` |
| `src/repo.rs` / `src/repo_contract.rs` | `docs/.../guides/role-repos.mdx` |
| `docker/construct/Dockerfile` | `docs/.../developing/construct-image.mdx` |
| Module structure in `src/**` (added/split/renamed module) | `docs/.../reference/codebase-map.mdx` |

## Keeping the docs fresh

The Codebase Map and the cross-reference table above are the two
places where structural changes show up first. If your PR adds a new
module directory, splits a file into a subdirectory, introduces a new
cross-cutting helper, or renames a public surface — **update
`docs/.../reference/codebase-map.mdx` and (if relevant) the
cross-reference table above in the same PR**. See `TODO.md` for the
stale-docs check every structural PR should run.
