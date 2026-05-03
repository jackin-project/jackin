# Construct Image

This directory contains the source for `projectjackin/construct:trixie` — the shared base Docker image that every [jackin](https://github.com/jackin-project/jackin) agent starts from.

The construct is the foundation layer providing system tools, shell environment, and container infrastructure that all roles inherit — one shared base image so every role starts from the same baseline.

For full details — including what's installed, how it is built, the image layer architecture, and how to extend it — see the [Construct Image](https://jackin.tailrocks.com/developing/construct-image/) documentation.

## Files

| File | Purpose |
|---|---|
| `Dockerfile` | Builds the construct image on Debian Trixie with core tools (git, Docker CLI, mise, ripgrep, fd, fzf, GitHub CLI, zsh, starship) and security tools (tirith, shellfirm) |
| `zshrc` | Shell configuration — sets up mise shims, starship prompt, and security tool shell hooks |
| `install-claude-plugins.sh` | Runtime script that installs Claude plugins from `~/.jackin/plugins.json` |
| `versions.env` | Pinned versions for security tools (tirith, shellfirm) used as Docker build-args |

The runtime entrypoint that launches the selected agent is at [`docker/runtime/entrypoint.sh`](../runtime/entrypoint.sh) — it configures git identity, authenticates with GitHub, runs agent-specific setup, and starts Claude or Codex.

## Image Layer Architecture

```
┌─────────────────────────────────┐
│  Derived Layer (jackin-managed) │  Claude Code, entrypoint, user mapping
├─────────────────────────────────┤
│  Agent Layer (your Dockerfile)  │  Rust, Node, Python, custom tools
├─────────────────────────────────┤
│  Construct (this image)         │  Debian, git, Docker CLI, mise, zsh
└─────────────────────────────────┘
```

Agent repos build on top of the construct. jackin then generates a derived layer on top of that. See [Architecture](https://jackin.tailrocks.com/reference/architecture/) for the full picture.

## Building

Construct image builds are defined by the repo-root `docker-bake.hcl` file and wrapped by the repo-root `Justfile`.

The supported local validation flow, architecture-specific debugging commands, advanced publish rehearsal workflow, CI behavior, and published tags are documented on the [Construct Image](https://jackin.tailrocks.com/developing/construct-image/) page.

## Related

- [Creating Agents](https://jackin.tailrocks.com/developing/creating-agents/) — how to build agent repos on top of the construct
- [jackin-agent-smith](https://github.com/jackin-project/jackin-agent-smith) — default agent (example of extending the construct)
- [jackin-the-architect](https://github.com/jackin-project/jackin-the-architect) — Rust development agent (another example)
