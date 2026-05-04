# agent-brown

`agent-brown` is the ChainArgos `jackin` agent. It provides the agent-specific environment layer for the ChainArgos Java/Rust monorepo.

`jackin` validates this repo's Dockerfile, derives the final image itself, and mounts the cached repo checkout into `/workspace` when you run:

```sh
jackin load chainargos/agent-brown
```

## Contract

- Final Dockerfile stage must literally be `FROM projectjackin/construct:trixie`
- Plugins are declared in `jackin.role.toml`

## Environment

- **Node.js** LTS (via mise)
- **Rust** latest (via mise)
- **Java** Oracle GraalVM 25.0.1 (via mise)
- **Protobuf compiler** (via mise)
- **GitHub CLI** (gh)
- **Cargo tools:** cargo-nextest, ast-grep, rust-script, just, bottom
- **Tessl CLI**
- **ctx7** (npm)

Shared shell/runtime tools come from `jackin-construct:trixie`.

## Pre-launch hooks

The `hooks/pre-launch.sh` script runs before Claude Code starts:

1. **Context7 MCP** — configures the MCP server if `CONTEXT7_API_KEY` is provided (prompted at launch, skippable)
2. **GitHub CLI** — triggers device-flow login on first run; persists across sessions
3. **Git HTTPS rewrite** — rewrites SSH remotes to HTTPS for containerized auth

## Migration gaps from claude-code-docker

The following features from the custom `claude-code-docker/` setup cannot yet be ported:

| Feature | Status | Notes |
|---------|--------|-------|
| Cloudflare CA cert (system + Java truststore) | **Blocked** | Requires build-time secrets (`--mount=type=secret`) |
| GitHub token for `protoc` install | **Blocked** | Requires build-time secrets |
