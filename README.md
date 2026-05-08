# Agent Brown

> **Internal — ChainArgos only.** This repository and the published Docker image are private and must remain private. See [AGENTS.md § Hard rules](./AGENTS.md#hard-rules-do-not-break-these).

**Agent Brown** is the ChainArgos `jackin` agent role (role identifier `agent-brown`). It provides the agent-specific environment layer for the ChainArgos Java/Rust monorepo.

`jackin` validates this repo's Dockerfile, derives the final image itself, and mounts the cached repo checkout into `/workspace` when you run:

```sh
jackin load chainargos/agent-brown
# or, with the codex CLI instead of claude:
jackin load chainargos/agent-brown --agent codex
```

## Contract

- Final Dockerfile stage must literally be `FROM projectjackin/construct:trixie`
- Plugins are declared in `jackin.role.toml`
- Threat model and hard rules: see [AGENTS.md](./AGENTS.md)

## Environment

Versions pinned in `Dockerfile` ARGs (`RUST_VERSION`, `NODE_VERSION`, `JAVA_VERSION`, `PROTOC_VERSION`, `TESSL_VERSION`, `CAVEMAN_VERSION`); bump via `docker build --build-arg <NAME>=<value>`.

- **Node.js** (via mise)
- **Rust** (via mise)
- **Java** Oracle GraalVM (via mise)
- **Protobuf compiler** (via mise)
- **GitHub CLI** (`gh`, from `projectjackin/construct:trixie` base)
- **Cargo tools:** cargo-nextest, ast-grep, rust-script, just, bottom
- **Tessl CLI** (pinned)
- **ctx7** (npm)
- **Caveman** token-compression hooks + skills (claude + codex profiles, pinned to a tagged release)

Shared shell/runtime tools come from `jackin-construct:trixie`.

## Plugins

Declared in [`jackin.role.toml`](./jackin.role.toml) under `[claude].plugins` and bootstrapped at runtime by jackin. Marketplaces beyond `@claude-plugins-official`:

- `@tailrocks-marketplace` — [tailrocks/tailrocks-marketplace](https://github.com/tailrocks/tailrocks-marketplace) (source of `rust-best-practices`)
- `@caveman` — [JuliusBrussee/caveman](https://github.com/JuliusBrussee/caveman) (source of `caveman`; pinned to a tagged release via Dockerfile `CAVEMAN_VERSION`)

Trust rationale: see [AGENTS.md § Threat model](./AGENTS.md#threat-model).

## Pre-launch hooks

The `hooks/pre-launch.sh` script runs before the agent CLI starts:

1. **PostgreSQL host** — derives `POSTGRESQL_DB_HOST` from `JACKIN_DIND_HOSTNAME` / `DOCKER_HOST` so Gradle can reach the docker-compose Postgres
2. **Context7 MCP** — configures the MCP server if `CONTEXT7_API_KEY` is provided (prompted at launch, skippable)
3. **caveman-shrink MCP** — registers caveman-shrink as a Claude MCP middleware (claude agent only, idempotent; fails launch if the claude CLI is missing — see hook header for the contract)
4. **Codex caveman skills check** — fails launch if `~/.agents/skills/caveman/SKILL.md` is missing (codex agent only); a missing file means the image was built wrong

## Migration gaps from claude-code-docker

The following features from the custom `claude-code-docker/` setup cannot yet be ported:

| Feature | Status | Notes |
|---------|--------|-------|
| Cloudflare CA cert (system + Java truststore) | **Blocked** | Requires build-time secrets (`--mount=type=secret`) |
| GitHub token for `protoc` install | **Blocked** | Requires build-time secrets |
