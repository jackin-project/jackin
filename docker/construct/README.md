# Construct Image

This directory contains the source for `projectjackin/construct:trixie` — the shared base Docker image that every [jackin](https://github.com/donbeave/jackin) agent starts from.

In The Matrix, the construct is the base simulated environment loaded before a mission. In jackin, it's the foundation layer providing system tools, shell environment, and container infrastructure that all agents inherit.

For full details — including what's installed, the image layer architecture, and how to extend it — see the [Construct Image](https://jackin.tailrocks.com/developing/construct-image/) documentation.

## Files

| File | Purpose |
|---|---|
| `Dockerfile` | Builds the construct image on Debian Trixie with core tools (git, Docker CLI, mise, ripgrep, fd, fzf, GitHub CLI, zsh, starship) and security tools (tirith, shellfirm) |
| `zshrc` | Shell configuration — sets up mise shims, starship prompt, and security tool shell hooks |
| `install-plugins.sh` | Runtime script that installs Claude plugins from `~/.jackin/plugins.json` |
| `versions.env` | Pinned versions for security tools (tirith, shellfirm) used as Docker build-args |

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

Agent repos build on top of the construct. jackin then generates a derived layer on top of that. See [Architecture](https://jackin.tailrocks.com/reference/architecture/) for the full picture.

## Building

Construct image builds are defined by the repo-root `docker-bake.hcl` file and wrapped by the repo-root `Justfile`. Install [`just`](https://github.com/casey/just), then bootstrap buildx and build the image locally:

```sh
just construct-init-buildx
just construct-build-local
```

To debug a specific architecture locally, run one of these commands:

```sh
just construct-build-platform amd64
just construct-build-platform arm64
```

To rehearse publishing, point `REGISTRY_IMAGE` at your own namespace instead of the canonical `projectjackin/construct` repository:

```sh
REGISTRY_IMAGE=ttl.sh/jackin-construct-$USER just construct-push-platform amd64
REGISTRY_IMAGE=ttl.sh/jackin-construct-$USER just construct-push-platform arm64
REGISTRY_IMAGE=ttl.sh/jackin-construct-$USER just construct-publish-manifest
```

Construct CI now triggers when changes touch any construct build input, including `docker/construct/**`, `docker-bake.hcl`, `Justfile`, and `.github/workflows/construct.yml`.

Public tags remain:

- `projectjackin/construct:trixie` — stable tag
- `projectjackin/construct:trixie-<sha>` — commit-specific tag

## Related

- [Creating Agents](https://jackin.tailrocks.com/developing/creating-agents/) — how to build agent repos on top of the construct
- [jackin-agent-smith](https://github.com/donbeave/jackin-agent-smith) — default agent (example of extending the construct)
- [jackin-the-architect](https://github.com/donbeave/jackin-the-architect) — Rust development agent (another example)
