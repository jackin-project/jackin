# jackin

`jackin` is a CLI for orchestrating AI coding agents at scale. Each agent runs in an isolated Docker container with Docker-in-Docker enabled — a self-contained world to think, build, and execute in. You're the Operator. They're already inside.

Reference: <https://matrix.fandom.com/wiki/Jacking_in>

> **Current status:** jackin is built as a proof of concept around [Claude Code](https://docs.anthropic.com/en/docs/claude-code) as its first and only supported agent runtime. Support for additional agent runtimes — [Codex](https://github.com/openai/codex) and [Amp Code](https://ampcode.com) — is planned for future releases.

## Construct

`donbeave/jackin-construct:trixie` is the shared base image for every agent repo. In The Matrix, the construct is the base simulated environment you load before a mission. That maps directly to `jackin`'s shared runtime image: every agent starts from the same construct before layering on its own specialized environment.

## Commands

- `jackin launch` — fast interactive launcher for the current directory or a saved workspace.
- `jackin load agent-smith` — send an agent in using the current directory as the workspace.
- `jackin load agent-smith ~/Projects/chainargos/big-monorepo` — send an agent into a direct path workspace.
- `jackin load agent-smith -w big-monorepo` — use a saved workspace definition.
- `jackin hardline jackin-agent-smith` — reattach to a running agent.
- `jackin eject jackin-agent-smith` — pull one agent out.
- `jackin workspace add big-monorepo --workdir /workspace/project --mount ~/Projects/chainargos/big-monorepo:/workspace/project` — save a reusable workspace.

## Workspaces

`jackin launch` is the fastest way to start work. It shows two kinds of workspace choices:

- `Current directory` — a synthetic workspace that mounts the current directory to the same absolute path inside the container and uses that path as `workdir`
- saved workspaces — named local definitions stored in `~/.config/jackin/config.toml`

If the current directory exactly matches a saved workspace `workdir`, Jackin preselects that saved workspace in the launcher. You can still move to `Current directory` to force the raw direct-mount behavior.

`launch` is the human-first flow: pick a workspace, preview mounts and `workdir`, then choose an agent. `load` stays the explicit terminal-first path for current-directory mode, direct paths, saved workspaces, and fully custom one-off mount composition. If you need to hand-author `--mount ... --workdir ...`, do that in `load`, not in `launch`.

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
