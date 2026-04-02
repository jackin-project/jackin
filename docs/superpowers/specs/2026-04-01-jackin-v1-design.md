# Jackin V1 Design

## Summary

`jackin` is a  local CLI for spawning isolated Claude Code agents. Each loaded agent runs in its own Docker container with a dedicated Docker-in-Docker sidecar and its own persisted Claude state.

For v1, `jackin` is local-only. One operator machine manages multiple agent containers on the same Docker host. The design intentionally favors a small, strict contract over flexibility so the first version can be dependable and easy to understand.

The Matrix framing is part of the product language:

- `load` sends an agent into the Matrix
- `hardline` opens a direct connection to a running agent
- `eject` pulls one or more agents out
- `exile` ejects all running agents
- `purge` removes persisted per-agent state

The branding and README should preserve the concept of “jacking in” and keep the reference link to <https://matrix.fandom.com/wiki/Jacking_in>.

## Goals

- Provide a Rust CLI that launches isolated Claude Code agent environments.
- Keep one cached Git checkout per agent class source.
- Keep one persisted runtime data directory per loaded container instance.
- Rebuild the Docker image on every `load`, matching the behavior of the current prototype.
- Use a repo-local manifest to define the minimal runtime contract for each agent class.
- Keep v1 small and explicit so later enhancements can be layered on safely.

## Non-Goals

- Remote orchestration across multiple hosts.
- Multi-profile repos.
- Shared cache strategy for `.gradle`, `.npm`, `.cargo`, or similar build caches.
- Rich repo manifests for prompts, roles, templates, or advanced inheritance.
- Source discovery beyond the built-in default profile and the explicit `owner/repo` load flow.

## Core Model

### Storage Layout

`jackin` uses three storage layers:

1. Global app config:
   - `~/.config/jackin/config.toml`
2. Cached agent class repositories:
   - `~/.jackin/agents/agent-smith`
   - `~/.jackin/agents/chainargos/the-architect`
3. Persisted runtime data per loaded container:
   - `~/.jackin/data/jackin-agent-smith`
   - `~/.jackin/data/jackin-agent-smith-clone-1`
   - `~/.jackin/data/jackin-chainargos-the-architect`

The cached repo is shared by every instance of the same class source. The runtime data directory is unique per running or previously run container instance.

### Agent Class Contract

Each agent class source is exactly one Git repository in v1.

That repo must contain:

- `Dockerfile`
- `jackin.agent.toml`

If either file is missing, the repo is not considered a valid `jackin` agent repo.

### Naming Rules

Built-in or unscoped classes:

- selector: `agent-smith`
- primary container: `jackin-agent-smith`
- clones: `jackin-agent-smith-clone-1`, `jackin-agent-smith-clone-2`, ...

Namespaced classes:

- selector: `chainargos/the-architect`
- primary container: `jackin-chainargos-the-architect`
- clones: `jackin-chainargos-the-architect-clone-1`, `jackin-chainargos-the-architect-clone-2`, ...

Agent repos follow the `jackin-{class-name}` naming convention on GitHub:

- `jackin-agent-smith` — the default agent
- `chainargos/jackin-the-architect` — a namespaced agent

The class name is what you use with `jackin load`. The repo name adds the `jackin-` prefix for discoverability.

Clones reuse the same cached repo checkout and only get their own per-instance runtime data directory.

## Commands

### `jackin load <selector>`

Loads exactly one agent instance and immediately enters its Claude Code session.

Examples:

- `jackin load agent-smith`
- `jackin load chainargos/the-architect`

Behavior:

- resolve the selector from global config
- clone or update the cached repo
- validate `Dockerfile` and `jackin.agent.toml`
- choose the next available container name for that class family
- create or reuse the per-instance runtime data directory
- rebuild the Docker image
- start the container, sidecar, and supporting Docker network
- attach the operator directly into Claude Code in the new container

`load` always launches one agent at a time.

### `jackin hardline <container-name>`

Connects directly to an already running container.

Examples:

- `jackin hardline jackin-agent-smith`
- `jackin hardline jackin-chainargos-the-architect-clone-1`

This is the equivalent of opening a direct line into an already-running Matrix instance.

### `jackin eject <selector> [--all] [--purge]`

Stops and removes running containers.

Examples:

- `jackin eject jackin-agent-smith`
- `jackin eject agent-smith`
- `jackin eject agent-smith --all`
- `jackin eject chainargos/the-architect --all`
- `jackin eject jackin-agent-smith --purge`

Rules:

- explicit `jackin-*` container names target one concrete container
- class selectors without `--all` target the primary instance name for that class
- class selectors with `--all` target the full family for that exact class scope only
- `--purge` additionally deletes the targeted runtime data directories under `~/.jackin/data/...`

Examples of class-family matching:

- `jackin eject agent-smith --all` matches `jackin-agent-smith`, `jackin-agent-smith-clone-*`
- `jackin eject chainargos/the-architect --all` matches `jackin-chainargos-the-architect`, `jackin-chainargos-the-architect-clone-*`

There is no cross-namespace matching by default.

### `jackin exile`

Stops and removes all running agent containers and their supporting runtime infrastructure.

`exile` does not delete cached repos or persisted data directories in v1.

### `jackin purge <selector> [--all]`

Deletes persisted runtime data under `~/.jackin/data/...`.

Examples:

- `jackin purge agent-smith`
- `jackin purge agent-smith-clone-1`
- `jackin purge agent-smith --all`

`purge` follows the same selector and `--all` rules as `eject`, but operates on runtime data instead of container processes.

## Configuration

### Global Config

Path:

- `~/.config/jackin/config.toml`

Purpose:

- define the catalog of known selectors and their Git source URLs
- hold the operator-side configuration for the `jackin` app itself

First-run behavior:

- if the file does not exist, `jackin` creates it automatically
- the generated default file includes a built-in `agent-smith` entry
- if the operator runs `jackin load owner/repo` and that selector is missing, `jackin` derives the GitHub source from the selector, adds it to the config, and continues

Example shape:

```toml
[agents.agent-smith]
git = "git@github.com:donbeave/jackin-agent-smith.git"

[agents."chainargos/the-architect"]
git = "git@github.com:chainargos/jackin-the-architect.git"
```

V1 behavior for configured selectors:

- selectors are resolved from this file
- namespaced selectors such as `chainargos/the-architect` are persisted here after first use
- the file is operator-managed and auto-created on first use

### Repo Manifest

Path inside the cached repo:

- `jackin.agent.toml`

Purpose:

- define the minimal runtime contract for one agent class
- specify which Dockerfile to build
- specify which Claude Code plugins should be installed

Example shape:

```toml
dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = [
  "code-review@claude-plugins-official",
  "feature-dev@claude-plugins-official",
  "typescript-lsp@claude-plugins-official",
]
```

The optional `[identity]` section allows an agent to declare a display name. When omitted, the class selector name is used instead.

V1 intentionally keeps this file minimal. More runtime settings can be added later if the basic contract proves stable.

## Runtime Lifecycle

### Load Flow

For `jackin load agent-smith`, the runtime flow is:

1. Ensure `~/.config/jackin/config.toml` exists.
2. Create the default config if missing.
3. Resolve `agent-smith` to its configured Git URL.
4. Ensure the cached repo exists at `~/.jackin/agents/agent-smith`.
5. If the repo does not exist, clone it.
6. If the repo exists, run `git pull` before launching.
7. Verify the repo contains `Dockerfile` and `jackin.agent.toml`.
8. Determine the target instance name:
   - first instance: `jackin-agent-smith`
   - next concurrent instance: `jackin-agent-smith-clone-1`
   - subsequent concurrent instances: increment the clone suffix
9. Ensure the per-instance runtime data directory exists at `~/.jackin/data/<container-name>`.
10. Rebuild the image from the cached repo.
11. Start the isolated Docker runtime for that container.
12. Attach directly into Claude Code inside the new container.

The same flow applies to namespaced selectors, with namespaced cache paths and container names.

For namespaced selectors not yet present in the config, `jackin` treats the selector as a GitHub `owner/repo` source, writes the derived Git URL into `~/.config/jackin/config.toml`, and then proceeds with the normal load flow.

### Persisted Per-Instance Data

For v1, the per-instance runtime data directory persists only Claude-specific state:

- `.claude`
- `.claude.json`

This lives under `~/.jackin/data/<container-name>/`.

Build caches such as `.gradle`, `.npm`, and `.cargo` are explicitly deferred to a later design pass.

### Runtime Isolation

Each running agent instance gets:

- its own main container
- its own Docker-in-Docker sidecar
- its own supporting network/runtime names
- its own persisted Claude state directory

This keeps simultaneous agents isolated from each other while still allowing multiple clones of the same class source.

## Validation and Error Handling

The CLI should fail early and clearly for invalid state.

V1 should validate:

- global config existence or first-run creation success
- selector existence in global config
- cached repo clone or pull success
- presence of `Dockerfile`
- presence of `jackin.agent.toml`
- ability to parse `jackin.agent.toml`
- Docker build success
- container and sidecar startup success
- attach/hardline target existence for direct connection commands

Example error categories:

- unknown selector
- invalid agent repo format
- Git operation failure
- manifest parse failure
- Docker build failure
- Docker runtime startup failure
- requested container not found

Error messages should be explicit and operator-focused rather than implementation-heavy.

## Testing Strategy

V1 should emphasize a few high-value checks instead of a large test matrix.

Recommended verification areas:

- config bootstrap behavior when `~/.config/jackin/config.toml` is missing
- selector-to-path/container-name resolution
- clone naming behavior for repeated loads
- repo validation for missing `Dockerfile` or `jackin.agent.toml`
- `eject`, `purge`, and `--all` selector matching rules
- end-to-end smoke test for a valid agent repo if practical in the local environment

Given the Docker-heavy workflow, a small number of focused unit tests plus one or two smoke tests is preferable to broad low-signal coverage.

## Deferred Topics

These are intentionally postponed until after v1 is working:

- shared or class-level caches for `.gradle`, `.npm`, `.cargo`
- richer manifest schema beyond Dockerfile path and Claude plugin list
- remote Docker hosts or clustered orchestration
- support for multiple profiles in a single repo
- advanced configuration editing commands
- additional runtime customization per class or per load invocation

## Recommended README Direction

The README should explain both the product purpose and the Matrix metaphor.

Suggested core wording:

> `jackin` is a CLI for orchestrating Claude Code agents at scale. Each agent runs in an isolated Docker container with Docker-in-Docker enabled — a self-contained world to think, build, and execute in. You're the Operator. They're already inside.

It should also explain the meaning of `load`, `hardline`, `eject`, `exile`, and `purge`, and include the “jacking in” reference link.

## Recommendation

Implement v1 as a config-driven Rust CLI with a small command surface and a stable storage model. Preserve the proven Docker lifecycle from the earlier prototype, but make the terminology, config, repo contract, and container naming native to `jackin`.
