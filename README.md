# jackin

`jackin` is a CLI for orchestrating Claude Code agents at scale. Each agent runs in an isolated Docker container with Docker-in-Docker enabled — a self-contained world to think, build, and execute in. You're the Operator. They're already inside.

Reference: <https://matrix.fandom.com/wiki/Jacking_in>

## Commands

- `jackin load smith` — send an agent in.
- `jackin hardline agent-smith` — open a direct line into a running agent.
- `jackin eject agent-smith` — pull one agent out.
- `jackin eject smith --all` — pull every built-in Smith out.
- `jackin exile` — remove every running agent.
- `jackin purge smith --all` — delete persisted state for one class.

## Storage

- `~/.config/jackin/config.toml` — operator config.
- `~/.jackin/agents/...` — cached agent repositories.
- `~/.jackin/data/<container-name>/` — persisted `.claude` and `.claude.json` for one agent instance.

## Agent Repo Contract

Each agent repo must contain:

- `Dockerfile`
- `jackin.agent.toml`

Example `jackin.agent.toml`:

```toml
dockerfile = "Dockerfile"

[claude]
plugins = [
  "code-review@claude-plugins-official",
  "feature-dev@claude-plugins-official",
]
```
