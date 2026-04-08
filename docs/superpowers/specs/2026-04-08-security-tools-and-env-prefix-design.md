# Security Tools Integration and Environment Variable Prefix Design

This design adds terminal security tools (tirith and shellfirm) to the construct
image and standardizes all jackin-defined environment variables under the
`JACKIN_` prefix.

## Goals

- Install tirith and shellfirm in the construct layer so every agent gets
  security guardrails by default.
- Provide a disable mechanism for agents that explicitly opt out.
- Register both tools as MCP servers so Claude Code can self-check commands
  before execution.
- Rename jackin-defined environment variables to use the `JACKIN_` prefix for
  clear namespacing.

## Non-Goals

- Adding configuration files or custom policies for either security tool.
- Changing agent-layer Dockerfiles.
- Adding new CI workflows for the security tools.

## Environment Variable Rename

All jackin-defined environment variables must use the `JACKIN_` prefix. Two
existing variables need renaming:

| Current | New | Rationale |
|---------|-----|-----------|
| `CLAUDE_ENV` | `JACKIN_CLAUDE_ENV` | Jackin-defined, should have prefix |
| `CLAUDE_DEBUG` | `JACKIN_DEBUG` | Jackin's debug flag, not Claude's |

### Files Affected

**Rust source (`CLAUDE_ENV` to `JACKIN_CLAUDE_ENV`):**

- `src/manifest.rs` — constant `JACKIN_RUNTIME_ENV_NAME` value
- `src/manifest.rs` — test referencing reserved env var name
- `src/env_resolver.rs` — test using `"CLAUDE_ENV"` string
- `src/runtime.rs` — test assertion for `-e CLAUDE_ENV=jackin`
- `src/derived_image.rs` — test assertion checking entrypoint does not set
  `CLAUDE_ENV=` (updated to check for `JACKIN_CLAUDE_ENV=`)

**Rust source (`CLAUDE_DEBUG` to `JACKIN_DEBUG`):**

- `src/runtime.rs` — debug env var passed to container

**Shell scripts (`CLAUDE_DEBUG` to `JACKIN_DEBUG`):**

- `docker/runtime/entrypoint.sh` — two occurrences
- `docker/construct/install-plugins.sh` — one occurrence

## Security Tools Installation

### Multi-Stage Docker Build

A builder stage compiles both tools from source using the official Rust
toolchain. Only the final binaries are copied into the construct image, adding
zero toolchain overhead.

```dockerfile
FROM rust:trixie AS security-tools

ARG TIRITH_VERSION
ARG SHELLFIRM_VERSION

RUN cargo install tirith --version "${TIRITH_VERSION}" --locked && \
    cargo install shellfirm --version "${SHELLFIRM_VERSION}" --locked
```

Version arguments have no defaults and must be provided at build time. This
prevents silent fallback to stale versions.

The construct stage copies the compiled binaries:

```dockerfile
FROM debian:trixie

COPY --from=security-tools /usr/local/cargo/bin/tirith /usr/local/bin/tirith
COPY --from=security-tools /usr/local/cargo/bin/shellfirm /usr/local/bin/shellfirm
```

### Why Multi-Stage From Source

- Build-from-source is reproducible and auditable.
- The Rust toolchain never appears in the final image.
- Using `rust:trixie` as the builder matches the construct's Debian version,
  avoiding glibc or shared library mismatches.
- No dependency on external binary release artifacts or npm packages.

## Shell Hook Integration

Both tools are wired into the construct's version-controlled `zshrc` using the
same `eval` pattern already used for starship:

```zsh
export PATH="$HOME/.local/share/mise/shims:$HOME/.local/bin:$PATH"
eval "$(starship init zsh)"

# Security tools (disable with JACKIN_DISABLE_TIRITH=1 / JACKIN_DISABLE_SHELLFIRM=1)
[[ "${JACKIN_DISABLE_TIRITH:-0}" != "1" ]] && eval "$(tirith init --shell zsh)"
[[ "${JACKIN_DISABLE_SHELLFIRM:-0}" != "1" ]] && eval "$(shellfirm init --shell zsh)"
```

### Why Manual Eval Lines

Both tools offer `init --install` commands that auto-modify shell rc files. We
use manual eval lines instead because:

- The construct owns its `.zshrc` — keeping all hooks explicit and
  version-controlled.
- Consistent pattern with how starship is already wired.
- The disable mechanism lives right next to the hook.

## MCP Server Registration

Both tools expose MCP servers that let Claude Code self-check commands before
execution. Registration happens in `docker/runtime/entrypoint.sh` because the
construct layer does not have Claude Code installed — it is added in the derived
layer.

```bash
# Register security tool MCP servers
if [[ "${JACKIN_DISABLE_TIRITH:-0}" != "1" ]]; then
    run_maybe_quiet claude mcp add tirith -- tirith mcp-server
fi
if [[ "${JACKIN_DISABLE_SHELLFIRM:-0}" != "1" ]]; then
    run_maybe_quiet claude mcp add shellfirm -- shellfirm mcp-server
fi
```

This is placed after plugin installation and before the pre-launch hook,
using the existing `run_maybe_quiet` wrapper.

## Disable Mechanism

Agent Dockerfiles can opt out of either tool via environment variables:

```dockerfile
ENV JACKIN_DISABLE_TIRITH=1
ENV JACKIN_DISABLE_SHELLFIRM=1
```

Both shell hooks and MCP server registration check the same variables. The
convention follows the existing `1`/`0` pattern used by `JACKIN_DEBUG`.

## README Update

The `docker/construct/README.md` files table is updated to reflect the new
tools:

| File | Purpose |
|---|---|
| `Dockerfile` | Builds the construct image on Debian Trixie with core tools (git, Docker CLI, mise, ripgrep, fd, fzf, GitHub CLI, zsh, starship) and security tools (tirith, shellfirm) |
| `zshrc` | Shell configuration — sets up mise shims, starship prompt, and security tool shell hooks |
| `install-plugins.sh` | Runtime script that installs Claude plugins from `~/.jackin/plugins.json` |

## Tool Coverage Summary

The two tools are complementary:

- **Tirith** focuses on supply-chain and injection attacks: homograph URLs,
  pipe-to-shell patterns, base64 decode-execute chains, credential exfiltration,
  Unicode attacks, ANSI injection, and file/directory scanning.
- **Shellfirm** focuses on accidental destructive operations: `rm -rf`,
  `git push --force`, `kubectl delete`, `terraform destroy`, `docker system
  prune`, with context-aware risk escalation for SSH, root, and production
  environments.

Both provide shell hooks for defense-in-depth and MCP servers for AI agent
self-checking.

## Verification Plan

1. `cargo fmt -- --check && cargo clippy && cargo nextest run` passes with all
   env var renames.
2. Docker build completes with both security tools compiled and copied.
3. `tirith --version` and `shellfirm --version` work inside the built image.
4. Shell hooks activate in a new zsh session (when not disabled).
5. Shell hooks do not activate when `JACKIN_DISABLE_TIRITH=1` or
   `JACKIN_DISABLE_SHELLFIRM=1` is set.
6. MCP servers are registered in the entrypoint (when not disabled).
7. MCP servers are not registered when disabled.
