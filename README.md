# jackin'

Jack your AI coding agents into the Matrix — their own isolated worlds, scoped access, full autonomy. You're the Operator. They're already inside.

Documentation: <https://jackin-project.github.io/jackin/>

Source code: <https://github.com/jackin-project/jackin>

> **Current status:** jackin' is built as a proof of concept around [Claude Code](https://docs.anthropic.com/en/docs/claude-code) as its first and only supported agent runtime. Support for additional runtimes is [on the roadmap](https://jackin-project.github.io/jackin/reference/roadmap/).

## Install

```sh
brew tap jackin-project/tap

# Stable
brew install jackin

# OR rolling preview channel
brew install jackin@preview
```

Or [build from source](https://jackin-project.github.io/jackin/getting-started/installation/) if you prefer.

## Quick Start

```sh
# Load an agent into your current project directory
jackin load agent-smith

# Or use the interactive TUI launcher
jackin launch
```

That's it. jackin' pulls the base image, builds the agent container, mounts your project, and drops you into Claude Code — fully autonomous inside an isolated environment.

See the [Quick Start guide](https://jackin-project.github.io/jackin/getting-started/quickstart/) for common workflows and next steps.

## What It Does

- **Isolates each agent** in its own Docker container with Docker-in-Docker enabled
- **Gives agents full autonomy** inside the container boundary (`--dangerously-skip-permissions`)
- **Separates tooling from file access** — agent classes define the environment, workspaces define which files are visible
- **Supports multiple agents simultaneously** — different tool profiles against the same or different projects
- **Persists agent state** between sessions (Claude history, GitHub CLI auth, plugins)

Learn more: [Why jackin'?](https://jackin-project.github.io/jackin/getting-started/why/) · [Core Concepts](https://jackin-project.github.io/jackin/getting-started/concepts/) · [Security Model](https://jackin-project.github.io/jackin/guides/security-model/) · [Comparison with Alternatives](https://jackin-project.github.io/jackin/guides/comparison/)

## Ecosystem

| Repository | Description |
|---|---|
| [jackin](https://github.com/jackin-project/jackin) | CLI source code (this repo) |
| [jackin-agent-smith](https://github.com/donbeave/jackin-agent-smith) | Default general-purpose agent |
| [jackin-the-architect](https://github.com/donbeave/jackin-the-architect) | Rust development agent (used for jackin' development) |
| [construct image source](https://github.com/jackin-project/jackin/tree/main/docker/construct) | Shared base Docker image for all agents |

## Documentation

The full documentation lives at **<https://jackin-project.github.io/jackin/>** and covers:

- [Installation](https://jackin-project.github.io/jackin/getting-started/installation/) — all install methods and prerequisites
- [Core Concepts](https://jackin-project.github.io/jackin/getting-started/concepts/) — operators, agents, constructs, and workspaces
- [Commands](https://jackin-project.github.io/jackin/commands/load/) — complete CLI reference
- [Creating Agents](https://jackin-project.github.io/jackin/developing/creating-agents/) — build your own agent repos
- [The Construct Image](https://jackin-project.github.io/jackin/developing/construct-image/) — what's inside the shared base image
- [Architecture](https://jackin-project.github.io/jackin/reference/architecture/) — how jackin' orchestrates containers and networks

## Development

To develop jackin' itself, use [The Architect](https://github.com/donbeave/jackin-the-architect) — a dedicated agent with the full Rust toolchain:

```sh
jackin load the-architect
```

## License

This project is licensed under the [Apache License 2.0](LICENSE).
