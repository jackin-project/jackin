# jackin

`jackin` is a Matrix-inspired CLI for orchestrating AI coding agents at scale. Each agent runs in an isolated Docker container with Docker-in-Docker enabled — a self-contained world to think, build, and execute in. You're the Operator. They're already inside.

Reference: <https://matrix.fandom.com/wiki/Jacking_in>

> **Current status:** jackin is built as a proof of concept around [Claude Code](https://docs.anthropic.com/en/docs/claude-code) as its first and only supported agent runtime. Support for additional agent runtimes — [Codex](https://github.com/openai/codex) and [Amp Code](https://ampcode.com) — is planned for future releases.

## Why

AI coding agents are most productive when they can run without permission prompts — reading files, executing commands, installing packages, and making changes freely. Claude Code calls this `--dangerously-skip-permissions` mode. But running an unrestricted agent directly on your host machine means it can see your entire filesystem, access your credentials, and modify anything.

jackin solves this by giving each agent its own isolated Docker container. The agent runs with full autonomy *inside* the container, but can only see the directories you explicitly mount and the tooling baked into its image. The operator controls the blast radius: which folders the agent can read or write, whether mounts are read-only, and which Docker network the agent lives on. The agent thinks it has free rein — but it's operating inside a construct you defined.

## Installation

### Homebrew (macOS/Linux)

```sh
brew tap donbeave/tap
brew install jackin
```

### From source

```sh
cargo install --git https://github.com/donbeave/jackin.git
```

## Quick Start

```sh
# Load an agent class into the current-directory workspace
jackin load agent-smith

# Interactive launcher — pick a workspace and agent class from a TUI
jackin launch
```

## Mental Model

There are three core ideas in `jackin`:

- **Agent class** — a reusable tool profile defined by an agent repo and loaded by name, such as `agent-smith`, `the-architect`, `chainargos/frontend-engineer`, or `chainargos/backend-engineer`
- **Workspace** — the file-access boundary for a project: which host directories are mounted and where they appear in the container. A workspace can be the current-directory workspace or a saved workspace. Saved workspaces can also restrict which agent classes are allowed and set a default.
- **Agent instance** — one running container created from an agent class and attached to one workspace

`agent-smith` is just the default starter class name in this project. It is not magic syntax. In a real company you might have classes like `frontend-engineer`, `backend-engineer`, `infra-operator`, or `security-reviewer`.

This distinction matters because `jackin` isolates two different things on purpose.

- A workspace answers: **which files can this agent see?**
- An agent class answers: **which tools, defaults, plugins, and runtime behavior does this agent have?**

That separation is useful even when the project stays the same. One project can intentionally use multiple agent classes:

- `chainargos/frontend-engineer` can mount the same monorepo workspace but carry Node, Playwright, design-system tooling, and UI-focused plugins
- `chainargos/backend-engineer` can mount that same workspace but carry Rust or Go tooling, database clients, and backend-oriented plugins

This is not duplication. It is how you create a smaller, more relevant runtime surface for the agent. A kitchen-sink image with every tool and every plugin gives the model more surface area to inspect and react to. A narrower environment usually produces better results because more of what the agent sees is relevant to the task.

This is also useful for controlling plugin behavior. If one agent class includes a privileged plugin or tool and another agent class does not, the second container genuinely cannot load it because it is not installed there. That is often more reliable than trying to "mostly disable" tools in one giant shared image.

## Construct

`donbeave/jackin-construct:trixie` is the shared base image for every agent repo. In The Matrix, the construct is the base simulated environment you load before a mission. That maps directly to `jackin`'s shared runtime image: every agent starts from the same construct before layering on its own specialized environment.

## Commands

### Loading Agents

```sh
# Current directory as workspace
jackin load agent-smith

# Direct path
jackin load agent-smith ~/Projects/my-app

# Path with custom container destination
jackin load agent-smith ~/Projects/my-app:/app

# Saved workspace
jackin load agent-smith big-monorepo

# Saved workspace with additional mounts
jackin load agent-smith big-monorepo --mount ~/extra-data

# Path with additional mounts
jackin load agent-smith ~/app --mount ~/cache:/cache:ro

# Interactive launcher
jackin launch
```

### Managing Running Agent Instances

```sh
# Reattach to a running agent
jackin hardline agent-smith

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

# Restrict which agent classes can use a workspace
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

The current-directory flow is great when you want to move fast from inside a project. But saved workspaces are more than shortcuts. They let you name a project boundary once and reuse it predictably.

A saved workspace is useful when you want to:

- launch the same project from anywhere without retyping mounts
- keep a multi-mount layout consistent across sessions
- let `jackin launch` auto-detect and preselect the right project
- set a default agent class for that project
- restrict sensitive workspaces to a smaller set of agent classes

`launch` is the human-first flow: pick a workspace, preview mounts and `workdir`, then choose an agent class. `load` stays the explicit terminal-first path: pass a path, a `path:container-dest` mapping, or a saved workspace name as the optional second argument. Use `--mount` to layer additional mounts on top of any target type.

Saved workspaces are local operator config. They define mounts, `workdir`, and optional allowed/default agent classes.

One useful pattern is to reuse the same workspace with different agent classes:

- `jackin load chainargos/frontend-engineer big-monorepo` for UI work
- `jackin load chainargos/backend-engineer big-monorepo` for API or database work

Another pattern is the opposite: reuse one agent class across many workspaces when the tooling stays the same but the projects differ.

## Naming Convention

Agent repos follow the `jackin-{class-name}` naming convention on GitHub:

- `jackin-agent-smith` — the default agent
- `jackin-neo` — a custom agent named "neo"
- `chainargos/jackin-backend-engineer` — a namespaced agent

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
- `~/.jackin/data/<container-name>/` — persisted `.claude`, `.claude.json`, `.config/gh`, and `plugins.json` for one agent instance.

## Agent Repo Contract

Each agent repo must contain:

- `jackin.agent.toml`
- a Dockerfile at the path declared by `jackin.agent.toml`

The manifest Dockerfile path must be relative and must stay inside the repo checkout.

Derived build-context generation currently rejects symlinks in the agent repo instead of following or preserving them.

Cached agent repos must stay clean. If the cached checkout's `origin` no longer matches the configured repo, or if the cache contains local changes or extra files, `jackin` refuses to load it until you clean or remove that cache directory.

The final Dockerfile stage must literally be `FROM donbeave/jackin-construct:trixie`, optionally with an alias such as `FROM donbeave/jackin-construct:trixie AS runtime`. Earlier stages may use any base image.

`agent-smith`-style agent repos only own their agent-specific environment layer. `jackin` owns the runtime wiring around that layer: validating the repo contract, generating the derived Dockerfile, installing Claude into the derived image, injecting the runtime entrypoint, mounting the resolved workspace paths into the runtime container, mounting persisted `.claude`, `.claude.json`, `.config/gh`, and `plugins.json`, and wiring the per-agent Docker-in-Docker runtime.

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

## License

This project is licensed under the [Apache License 2.0](LICENSE).
