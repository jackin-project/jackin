# jackin

`jackin` is a CLI for orchestrating Claude Code agents at scale. Each agent runs in an isolated Docker container with Docker-in-Docker enabled — a self-contained world to think, build, and execute in. You're the Operator. They're already inside.

Reference: <https://matrix.fandom.com/wiki/Jacking_in>

## Construct

`jackin/construct:trixie` is the shared base image for every agent repo. In The Matrix, the construct is the base simulated environment you load before a mission. That maps directly to `jackin`'s shared runtime image: every agent starts from the same construct before layering on its own specialized environment.

## Commands

- `jackin load smith` — send an agent in.
- `jackin hardline agent-smith` — reattach to a running agent.
- `jackin eject agent-smith` — pull one agent out.
- `jackin eject smith --all` — pull every Smith out for one class scope.
- `jackin exile` — remove every running agent.
- `jackin purge smith --all` — delete persisted state for one class.

## Storage

- `~/.config/jackin/config.toml` — operator config.
- `~/.jackin/agents/...` — cached agent repositories.
- `~/.jackin/data/<container-name>/` — persisted `.claude`, `.claude.json`, and `plugins.json` for one agent instance.

## Agent Repo Contract

Each agent repo must contain:

- `jackin.agent.toml`
- a Dockerfile at the path declared by `jackin.agent.toml`

The final Dockerfile stage must literally be `FROM jackin/construct:trixie`.

`jackin` validates that Dockerfile, generates the final Claude-ready derived image itself, and mounts the cached repo checkout into `/workspace` at runtime.
