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
- **Caveman** token-compression hooks + skills (claude + codex + amp profiles, pinned to a tagged release)

Shared shell/runtime tools come from the digest-pinned construct base.

## Plugins

Declared in [`jackin.role.toml`](./jackin.role.toml) under `[claude].plugins` and bootstrapped at runtime by jackin. Marketplaces beyond `@claude-plugins-official`:

- `@tailrocks-marketplace` — [tailrocks/tailrocks-marketplace](https://github.com/tailrocks/tailrocks-marketplace) (source of `rust-best-practices`)
- `@caveman` — [JuliusBrussee/caveman](https://github.com/JuliusBrussee/caveman) (source of `caveman`; pinned to a tagged release via Dockerfile `CAVEMAN_VERSION`)

Trust rationale: see [AGENTS.md § Threat model](./AGENTS.md#threat-model).

## Runtime hooks

Two hooks run before the agent CLI starts:

`hooks/source.sh` is sourced into the entrypoint shell so its exports propagate to the agent:

1. **PostgreSQL host** — sets `POSTGRESQL_DB_HOST` from jackin's runtime-managed `JACKIN_DIND_HOSTNAME` so Gradle can reach the docker-compose Postgres. Hard-fails if `JACKIN_DIND_HOSTNAME` is unset, since that means the role is being launched outside the jackin runtime contract.

`hooks/preflight.sh` runs as a child process for per-start validation and MCP setup:

1. **Context7 MCP** — configures the MCP server if `CONTEXT7_API_KEY` is provided (prompted at launch, skippable)
2. **caveman-shrink MCP** — registers caveman-shrink as a Claude MCP middleware (claude agent only, idempotent; fails launch if the claude CLI is missing — see hook header for the contract)
3. **Codex caveman skills check** — fails launch if `~/.agents/skills/caveman/SKILL.md` is missing (codex agent only); a missing file means the image was built wrong

Agent Brown requires a jackin runtime that sets `JACKIN_DIND_HOSTNAME`.
