# jackin'

Matrix-inspired CLI for orchestrating AI coding agents in isolated Docker containers. You're the Operator. They're already inside.

Documentation: <https://www.zhokhov.com/jackin/>

Source code: <https://github.com/donbeave/jackin>

> **Current status:** jackin is built as a proof of concept around [Claude Code](https://docs.anthropic.com/en/docs/claude-code) as its first and only supported agent runtime. Support for additional runtimes is [on the roadmap](https://www.zhokhov.com/jackin/reference/roadmap/).

## Install

```sh
brew tap donbeave/tap
brew install jackin
```

Or [build from source](https://www.zhokhov.com/jackin/getting-started/installation/) if you prefer.

## Quick Start

```sh
# Load an agent into your current project directory
jackin load agent-smith

# Or use the interactive TUI launcher
jackin launch
```

That's it. jackin pulls the base image, builds the agent container, mounts your project, and drops you into Claude Code — fully autonomous inside an isolated environment.

See the [Quick Start guide](https://www.zhokhov.com/jackin/getting-started/quickstart/) for common workflows and next steps.

## What It Does

- **Isolates each agent** in its own Docker container with Docker-in-Docker enabled
- **Gives agents full autonomy** inside the container boundary (`--dangerously-skip-permissions`)
- **Separates tooling from file access** — agent classes define the environment, workspaces define which files are visible
- **Supports multiple agents simultaneously** — different tool profiles against the same or different projects
- **Persists agent state** between sessions (Claude history, GitHub CLI auth, plugins)

Learn more: [Why jackin?](https://www.zhokhov.com/jackin/getting-started/why/) · [Core Concepts](https://www.zhokhov.com/jackin/getting-started/concepts/) · [Security Model](https://www.zhokhov.com/jackin/guides/security-model/) · [Comparison with Alternatives](https://www.zhokhov.com/jackin/guides/comparison/)

## Ecosystem

| Repository | Description |
|---|---|
| [jackin](https://github.com/donbeave/jackin) | CLI source code (this repo) |
| [jackin-agent-smith](https://github.com/donbeave/jackin-agent-smith) | Default general-purpose agent |
| [jackin-the-architect](https://github.com/donbeave/jackin-the-architect) | Rust development agent (used for jackin development) |
| [construct image source](https://github.com/donbeave/jackin/tree/main/docker/construct) | Shared base Docker image for all agents |

## Documentation

The full documentation lives at **<https://www.zhokhov.com/jackin/>** and covers:

- [Installation](https://www.zhokhov.com/jackin/getting-started/installation/) — all install methods and prerequisites
- [Core Concepts](https://www.zhokhov.com/jackin/getting-started/concepts/) — operators, agents, constructs, and workspaces
- [Commands](https://www.zhokhov.com/jackin/commands/load/) — complete CLI reference
- [Creating Agents](https://www.zhokhov.com/jackin/developing/creating-agents/) — build your own agent repos
- [The Construct Image](https://www.zhokhov.com/jackin/developing/construct-image/) — what's inside the shared base image
- [Architecture](https://www.zhokhov.com/jackin/reference/architecture/) — how jackin orchestrates containers and networks

## Development

To develop jackin itself, use [The Architect](https://github.com/donbeave/jackin-the-architect) — a dedicated agent with the full Rust toolchain:

```sh
jackin load the-architect
```

## License

This project is licensed under the [Apache License 2.0](LICENSE).
