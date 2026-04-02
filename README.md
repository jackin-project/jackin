# jackin

`jackin` is a Matrix-inspired CLI for orchestrating AI coding agents at scale. Each agent runs in an isolated Docker container with Docker-in-Docker enabled — a self-contained world to think, build, and execute in. You're the Operator. They're already inside.

Reference: <https://matrix.fandom.com/wiki/Jacking_in>

> **Current status:** jackin is built as a proof of concept around [Claude Code](https://docs.anthropic.com/en/docs/claude-code) as its first and only supported agent runtime. Support for additional agent runtimes — [Codex](https://github.com/openai/codex) and [Amp Code](https://ampcode.com) — is planned for future releases.

## Construct

`donbeave/jackin-construct:trixie` is the shared base image for every agent repo. In The Matrix, the construct is the base simulated environment you load before a mission. That maps directly to `jackin`'s shared runtime image: every agent starts from the same construct before layering on its own specialized environment.

## Commands

- `jackin load agent-smith` — send an agent in.
- `jackin hardline jackin-agent-smith` — reattach to a running agent.
- `jackin eject jackin-agent-smith` — pull one agent out.
- `jackin eject agent-smith --all` — pull every Agent Smith out for one class scope.
- `jackin exile` — remove every running agent.
- `jackin purge agent-smith --all` — delete persisted state for one class.

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

`agent-smith`-style agent repos only own their agent-specific environment layer. `jackin` owns the runtime wiring around that layer: validating the repo contract, generating the derived Dockerfile, installing Claude into the derived image, injecting the runtime entrypoint, mounting the cached repo checkout at `/workspace`, mounting persisted `.claude`, `.claude.json`, and `plugins.json`, and wiring the per-agent Docker-in-Docker runtime.

## Roadmap

- [x] Claude Code agent runtime
- [ ] Kubernetes platform support
- [ ] [Codex](https://github.com/openai/codex) agent runtime
- [ ] [Amp Code](https://ampcode.com) agent runtime
