# Construct Image

This directory contains the source for `projectjackin/construct:trixie` — the shared base Docker image that every [jackin](https://github.com/donbeave/jackin) agent starts from.

In The Matrix, the construct is the base simulated environment loaded before a mission. In jackin, it's the foundation layer providing system tools, shell environment, and container infrastructure that all agents inherit.

For full details — including what's installed, the image layer architecture, and how to extend it — see the [Construct Image](https://jackin-project.github.io/jackin/developing/construct-image/) documentation.

## Files

| File | Purpose |
|---|---|
| `Dockerfile` | Builds the construct image on Debian Trixie with core tools (git, Docker CLI, mise, ripgrep, fd, fzf, GitHub CLI, zsh, starship) |
| `zshrc` | Shell configuration — sets up mise shims and starship prompt |
| `install-plugins.sh` | Runtime script that installs Claude plugins from `~/.jackin/plugins.json` |

The runtime entrypoint that launches Claude Code is at [`docker/runtime/entrypoint.sh`](../runtime/entrypoint.sh) — it configures git identity, authenticates with GitHub, installs plugins, and starts Claude.

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

Agent repos build on top of the construct. jackin then generates a derived layer on top of that. See [Architecture](https://jackin-project.github.io/jackin/reference/architecture/) for the full picture.

## Building

The image is automatically built and pushed to Docker Hub via GitHub Actions when changes are made to this directory. Tags:

- `projectjackin/construct:trixie` — stable tag
- `projectjackin/construct:trixie-{sha}` — commit-specific tags

## Related

- [Creating Agents](https://jackin-project.github.io/jackin/developing/creating-agents/) — how to build agent repos on top of the construct
- [jackin-agent-smith](https://github.com/donbeave/jackin-agent-smith) — default agent (example of extending the construct)
- [jackin-the-architect](https://github.com/donbeave/jackin-the-architect) — Rust development agent (another example)
