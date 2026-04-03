# jackin

`jackin` is a CLI for orchestrating AI coding agents at scale. Each agent runs in an isolated Docker container with Docker-in-Docker enabled — a self-contained world to think, build, and execute in. You're the Operator. They're already inside.

Reference: <https://matrix.fandom.com/wiki/Jacking_in>

> **Current status:** jackin is built as a proof of concept around [Claude Code](https://docs.anthropic.com/en/docs/claude-code) as its first and only supported agent runtime. Support for additional agent runtimes — [Codex](https://github.com/openai/codex) and [Amp Code](https://ampcode.com) — is planned for future releases.

## Quick Start

```sh
# Load an agent into the current directory
jackin load agent-smith

# Interactive launcher — pick workspace and agent from a TUI
jackin launch
```

## Construct

`donbeave/jackin-construct:trixie` is the shared base image for every agent repo. In The Matrix, the construct is the base simulated environment you load before a mission. That maps directly to `jackin`'s shared runtime image: every agent starts from the same construct before layering on its own specialized environment.

## Commands

### Loading Agents

```sh
# Current directory as workspace
jackin load agent-smith

# Direct path
jackin load agent-smith ~/Projects/my-app

# Saved workspace
jackin load agent-smith -w big-monorepo

# Custom mounts
jackin load agent-smith --mount ~/src:/workspace/src --workdir /workspace/src

# Interactive launcher
jackin launch
```

### Managing Running Agents

```sh
# Reattach to a running agent
jackin hardline jackin-agent-smith

# Stop an agent
jackin eject agent-smith

# Stop all instances of an agent class
jackin eject agent-smith --all

# Stop and delete persisted state
jackin eject agent-smith --purge

# Stop every running agent
jackin exile

# Delete persisted state without stopping
jackin purge agent-smith
```

### Workspaces

```sh
# Save a workspace — workdir is auto-mounted at the same path
jackin workspace add my-app --workdir ~/Projects/my-app

# Add extra mounts alongside the auto-mounted workdir
jackin workspace add my-app --workdir ~/Projects/my-app --mount ~/cache:/cache:ro

# Disable auto-mount to control all mounts explicitly
jackin workspace add monorepo --workdir /workspace --no-workdir-mount --mount ~/src:/workspace

# Restrict which agents can use a workspace
jackin workspace add secure --workdir ~/app --allowed-agent agent-smith --default-agent agent-smith

# List, show, edit, remove
jackin workspace list
jackin workspace show my-app
jackin workspace edit my-app --mount ~/new-cache:/cache:ro
jackin workspace remove my-app
```

By default, `workspace add` automatically mounts the `--workdir` path into the container at the same location. This keeps the host and container directory layouts identical, which is the common case. Pass `--no-workdir-mount` when you need the workdir to differ from the mount layout (e.g. `--workdir /workspace` with `--mount ~/src:/workspace`).

### Global Mounts

```sh
# Add a global mount applied to all agents
jackin config mount add gradle-cache --src ~/.gradle/caches --dst /home/claude/.gradle/caches --readonly

# Scope a mount to specific agents
jackin config mount add secrets --src ~/.chainargos/secrets --dst /secrets --readonly --scope "chainargos/*"

# List and remove
jackin config mount list
jackin config mount remove gradle-cache
```

### Mount Spec Format

The `--mount` flag accepts two formats:

- **`path`** — mounts the path identically in the container (e.g. `~/Projects/my-app` becomes `~/Projects/my-app:~/Projects/my-app`)
- **`src:dst`** — explicit host and container paths (e.g. `~/src:/workspace/src`)

Append `:ro` to make a mount read-only (e.g. `~/cache:/cache:ro`).

## Workspaces

`jackin launch` is the fastest way to start work. It shows two kinds of workspace choices:

- `Current directory` — a synthetic workspace that mounts the current directory to the same absolute path inside the container and uses that path as `workdir`
- saved workspaces — named local definitions stored in `~/.config/jackin/config.toml`

If the current directory exactly matches a saved workspace `workdir`, Jackin preselects that saved workspace in the launcher. You can still move to `Current directory` to force the raw direct-mount behavior.

`launch` is the human-first flow: pick a workspace, preview mounts and `workdir`, then choose an agent. `load` stays the explicit terminal-first path for current-directory mode, direct paths, saved workspaces, and fully custom one-off mount composition.

Saved workspaces are local operator config. They define mounts, `workdir`, and optional allowed/default agents.

## Naming Convention

Agent repos follow the `jackin-{class-name}` naming convention on GitHub:

- `jackin-agent-smith` — the default agent
- `jackin-neo` — a custom agent named "neo"
- `chainargos/jackin-the-architect` — a namespaced agent

The class name is what you use with `jackin load`. The repo name adds the `jackin-` prefix for discoverability.

## Agent Identity

Agents can declare a display name in `jackin.agent.toml`:

```toml
[identity]
name = "Agent Smith"
```

This name is used for visualization in jackin. When omitted, the class selector name is used instead.

## Storage

- `~/.config/jackin/config.toml` — operator config.
- `~/.jackin/agents/...` — cached agent repositories.
- `~/.jackin/data/<container-name>/` — persisted `.claude`, `.claude.json`, and `plugins.json` for one agent instance.

## Agent Repo Contract

Each agent repo must contain:

- `jackin.agent.toml`
- a Dockerfile at the path declared by `jackin.agent.toml`

The manifest Dockerfile path must be relative and must stay inside the repo checkout.

Derived build-context generation currently rejects symlinks in the agent repo instead of following or preserving them.

The final Dockerfile stage must literally be `FROM donbeave/jackin-construct:trixie`, optionally with an alias such as `FROM donbeave/jackin-construct:trixie AS runtime`. Earlier stages may use any base image.

`agent-smith`-style agent repos only own their agent-specific environment layer. `jackin` owns the runtime wiring around that layer: validating the repo contract, generating the derived Dockerfile, installing Claude into the derived image, injecting the runtime entrypoint, mounting the resolved workspace paths into the runtime container, mounting persisted `.claude`, `.claude.json`, and `plugins.json`, and wiring the per-agent Docker-in-Docker runtime.

## Development

To develop and test jackin itself, use [The Architect](https://github.com/donbeave/jackin-the-architect) — a dedicated agent with the full Rust toolchain:

```sh
jackin load the-architect
```

## Roadmap

- [x] Claude Code agent runtime
- [ ] Kubernetes platform support
- [ ] [Codex](https://github.com/openai/codex) agent runtime
- [ ] [Amp Code](https://ampcode.com) agent runtime
