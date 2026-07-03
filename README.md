# Agent Brown

> **Internal — ChainArgos only.** This repository and the published Docker image are private and must remain private. See [AGENTS.md § Hard rules](./AGENTS.md#hard-rules-do-not-break-these).

**Agent Brown** is the ChainArgos `jackin` agent role (role identifier `agent-brown`). It provides the agent-specific environment layer for the ChainArgos Java/Rust monorepo.

`jackin` validates this repo's Dockerfile, derives the final image itself, and mounts the cached repo checkout into `/workspace` when you run:

```sh
jackin load chainargos/agent-brown
# or, with the codex or amp CLI instead of claude:
jackin load chainargos/agent-brown --agent codex
jackin load chainargos/agent-brown --agent amp
```

## Contract

- Final Dockerfile stage must use the digest-pinned `projectjackin/construct:<version>-trixie` base.
- Plugins are declared in `jackin.role.toml`
- Threat model and hard rules: see [AGENTS.md](./AGENTS.md)

## Environment

Versions pinned in `Dockerfile` ARGs (`RUST_VERSION`, `NODE_VERSION`, `JAVA_VERSION`, `PROTOC_VERSION`, `CAVEMAN_VERSION`); bump via `docker build --build-arg <NAME>=<value>`.

- **Node.js** (via mise)
- **Rust** (via mise)
- **Java** Oracle GraalVM (via mise)
- **Protobuf compiler** (via mise)
- **GitHub CLI** (`gh`, from the digest-pinned construct base)
- **Cargo tools:** cargo-nextest, ast-grep, rust-script, just, bottom
- **ctx7** (npm)
- **Caveman** + **RTK** + **Headroom** (token stack)

Shared shell/runtime tools come from the digest-pinned construct base.

## Plugins

Declared in [`jackin.role.toml`](./jackin.role.toml) under `[claude].plugins` and bootstrapped at runtime by jackin. Marketplaces beyond `@claude-plugins-official`:

- `@tailrocks-marketplace` — [tailrocks/tailrocks-marketplace](https://github.com/tailrocks/tailrocks-marketplace) (source of `tailrocks-skills`, including Rust guidance, Rust project setup, proposal, and research skills)
- `@caveman` — [JuliusBrussee/caveman](https://github.com/JuliusBrussee/caveman) (source of `caveman`; pinned to a tagged release via Dockerfile `CAVEMAN_VERSION`)

Trust rationale: see [AGENTS.md § Threat model](./AGENTS.md#threat-model).

## Skills

Installed at build time via `skills add` for supported Agent Skills hosts (`claude-code`, `codex`, `amp`, `opencode`, and `kimi-code-cli`):

- `improve` — [shadcn/improve](https://github.com/shadcn/improve): read-only advisor that audits a codebase and writes self-contained implementation plans under `plans/` for other agents to execute.

## Runtime hooks

Two hooks run before the agent CLI starts:

`hooks/source.sh` is sourced into the entrypoint shell so its exports propagate to the agent:

1. **PostgreSQL host** — from `JACKIN_DIND_HOSTNAME`.

`hooks/preflight.sh` runs as a child process for per-start validation and MCP setup:

1. **Context7** (non-interactive).
2. **Caveman** flag/statusline.
3. **RTK** + **Headroom** MCP/hook.

Agent Brown requires a jackin runtime that sets `JACKIN_DIND_HOSTNAME`.
