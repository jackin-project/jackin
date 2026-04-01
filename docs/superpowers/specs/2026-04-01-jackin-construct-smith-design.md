# Jackin Construct And Smith Design

## Summary

This design evolves `jackin` from the current direct-build agent contract into a stricter two-layer model:

- `jackin` owns the shared runtime contract, the final Claude-ready image layer, and the operator lifecycle.
- Each agent repo owns only its agent-specific environment layer.

The first delivery is a paired change across two repositories:

- `donbeave/jackin`
- `donbeave/smith`

The shared base image is the canonical `donbeave/jackin-construct:trixie` image. The name comes from The Matrix: the construct is the base simulated environment loaded before a mission. That maps directly to this image's role as the common environment every agent starts from before its own specialized layer is applied.

For v1 of this redesign, `jackin` validates agent Dockerfiles with a structured parser, generates a derived Dockerfile for the final Claude-ready runtime, and keeps the loaded agent lifecycle close to the older `claude-code-docker` model while remaining public-friendly and local-only.

## Goals

- Keep `jackin` as a local-first Rust CLI product.
- Introduce a strict, explicit, validated base-image contract for agent repos.
- Make derived Dockerfile generation the standard build path for all agent repos.
- Keep shared Claude runtime concerns in `jackin`, not in each agent repo.
- Create `smith` as the first real public-friendly agent repo that follows the new contract.
- Preserve the current runtime model where the cached agent repo checkout is mounted as the main in-container workspace.
- Keep the Matrix framing in product docs, including the rationale for the `construct` name.

## Non-Goals

- Support loose or pattern-based base-image matching in v1.
- Support agent repos that fully own the final Claude-ready image.
- Preserve the old direct-build repo contract as a parallel supported mode.
- Add company-specific secrets, CA setup, private mirrors, or internal-only assumptions to `smith`.
- Add Context7 bootstrapping in this phase.
- Build a broad detached-runtime supervisor system beyond `eject` and `exile`.

## Current State

The current `jackin` implementation validates a repo by checking for `jackin.agent.toml` and a manifest-selected Dockerfile, then builds directly from that Dockerfile. Runtime startup creates a DinD sidecar, builds the image from the cached repo checkout, starts the main container detached, installs plugins with `docker exec`, and enters Claude with `docker exec`.

That behavior is implemented today in files such as `src/repo.rs`, `src/runtime.rs`, and `src/manifest.rs`, and it will need to evolve to support the new construct-based derived-build pipeline.

## Architecture

### Paired Repo Delivery

This redesign is delivered across two repositories with a clean ownership split.

`donbeave/jackin` owns:

- the `donbeave/jackin-construct:trixie` base image source under `docker/construct/`
- Dockerfile validation using a structured parser library
- derived Dockerfile generation
- runtime-owned injected files and metadata
- lifecycle orchestration for `load`, `hardline`, `eject`, `exile`, and `purge`
- documentation for the shared contract

`donbeave/smith` owns:

- `jackin.agent.toml`
- its base agent Dockerfile
- only the agent-specific environment layer on top of `donbeave/jackin-construct:trixie`
- public-facing repo documentation for the first agent repo

### Base Image Contract

The v1 contract is strict and explicit.

- The only approved base image reference is `donbeave/jackin-construct:trixie`.
- An agent Dockerfile may use multiple stages.
- Earlier builder stages are unrestricted.
- The final stage must literally be `FROM donbeave/jackin-construct:trixie`, with only an optional `AS <name>` alias.
- Indirection through `ARG`, alternative tags, digest equivalents, or pattern matching is not supported in v1.

`jackin` validates this contract with a parser library rather than string matching or regex.

### Construct Image Responsibilities

The `donbeave/jackin-construct:trixie` image is the shared runtime foundation. Its source of truth lives in `donbeave/jackin/docker/construct/`.

For v1 it provides:

- Debian trixie baseline
- `claude` user
- `bash` and `zsh`
- common CLI tools already expected by the shared runtime
- `mise`
- Docker CLI and compose plugin
- starship and a basic shell environment
- the shared `.zshrc`
- a generic `/home/claude/install-plugins.sh`

It does not include company-specific secrets or assumptions.

## Agent Repo Contract

Each agent repo remains one Git repository with a small, explicit contract.

The repo must include:

- `jackin.agent.toml`
- a Dockerfile selected by that manifest

The manifest remains the source of truth for Claude plugin declarations.

The repo Dockerfile remains the source of truth for the agent-specific environment layer only. It does not own:

- Claude installation
- the shared `.zshrc`
- plugin bootstrap script ownership
- the final runtime `WORKDIR`
- the final `ENTRYPOINT`

### Smith Repo Shape

`smith` is the first real public-friendly `jackin` agent repo.

It should:

- be created as `donbeave/smith`
- be runnable outside company infrastructure
- avoid 1Password lookups, custom CA injection, private mirrors, and internal secrets
- keep the mounted cached repo checkout as the live `/workspace` repo inside the container

For v1, `smith` is intentionally serious but minimal.

- It preinstalls `node@lts` during image build.
- It does not carry over the old Java versions.
- It does not carry over `protoc`.
- It does not carry over Context7 setup.
- It adds no speculative project-specific tools beyond what `construct` already provides and the chosen `node@lts` install.

## Derived Build Pipeline

### Validation

On `jackin load`, `jackin`:

1. resolves and clones or updates the cached agent repo
2. loads `jackin.agent.toml`
3. resolves the manifest Dockerfile path inside the repo
4. parses the Dockerfile with a structured parser
5. validates that the final stage literally uses `donbeave/jackin-construct:trixie`

If validation fails, `load` stops before any Docker build begins.

### Generated Final Dockerfile

After validation, `jackin` generates a temporary derived Dockerfile instead of building directly from the repo Dockerfile.

The agent Dockerfile remains the base. `jackin` appends the final runtime-owned layers and settings.

For v1, the derived layer is responsible for:

- switching to `USER root` for `jackin`-owned setup when needed
- installing Claude with `curl -fsSL https://claude.ai/install.sh | bash`
- verifying Claude with `claude --version`
- copying the injected `entrypoint.sh`
- forcing `WORKDIR /workspace`
- switching back to `USER claude`
- setting `ENTRYPOINT ["/home/claude/entrypoint.sh"]`

The shared `.zshrc` and `install-plugins.sh` are already part of `construct`, not injected per repo.

### Temporary Build Context

`jackin` prepares a temporary build context for the derived build.

This temp context contains:

- the agent repo contents needed for normal Dockerfile-relative build behavior
- the injected `entrypoint.sh`

`jackin` should not write temporary injected files into the cached repo itself.

## Plugin Bootstrapping

Plugin declarations stay in `jackin.agent.toml`.

`smith` does not provide `install-plugins.sh` and does not own plugin installation logic.

For each loaded container instance, `jackin load` writes plugin metadata into the persisted state directory under:

- `~/.jackin/data/<container-name>/plugins.json`

That host-side file is the authoritative plugin metadata for the running instance because it is easy to inspect and tied directly to per-container state.

At runtime, `docker run` mounts that file into the container at:

- `/home/claude/.jackin/plugins.json`

The shared `/home/claude/install-plugins.sh` from `construct` reads that mounted file with `jq`.

This keeps plugin ownership clean:

- manifest declares plugins
- `jackin` materializes plugin metadata for the specific container instance
- `construct` provides the generic installer script

## Runtime Model

### Workspace And State Mounts

The cached agent repo checkout remains mounted as the main workspace at:

- `/workspace`

Persisted state remains explicit through known mounts, not a single monolithic runtime directory mount.

For v1, `jackin` keeps explicit mounts for known files and directories such as:

- `/home/claude/.claude`
- `/home/claude/.claude.json`
- `/home/claude/.jackin/plugins.json`

### Container Startup

`load` starts the main agent container attached with `docker run -it` semantics so Claude is the main foreground process, close to the old `claude-code-docker` flow.

Each loaded agent gets:

- one main agent container
- one dedicated DinD sidecar
- one dedicated Docker network for that specific loaded agent instance

`hardline` reconnects with `docker attach`.

### Entry Point Behavior

The injected `entrypoint.sh` is intentionally close to the current prototype behavior.

For v1 it should:

- run `/home/claude/install-plugins.sh`, optionally quietly unless `CLAUDE_DEBUG=1`
- clear the screen
- `exec env CLAUDE_ENV=docker claude --dangerously-skip-permissions --verbose`

Context7 setup is removed from this phase.

### Detach And Exit Behavior

`jackin` keeps Docker's standard detach sequence:

- `Ctrl-P`, `Ctrl-Q`

If the operator detaches intentionally:

- the main Claude process keeps running
- the main container keeps running
- the DinD sidecar keeps running
- the per-agent network remains in place
- `hardline` can later reattach with `docker attach`

If the foreground Claude process exits during an attached `load` session without a detach handoff, `jackin` treats that as the agent leaving the Matrix and cleans up the runtime it created.

If a detached runtime is later meant to be removed, the operator uses:

- `jackin eject`
- `jackin exile`

For v1, there is no separate watcher process to clean up a detached runtime automatically after later exit.

## Failure Handling

Failure handling is intentionally strict and boring.

### Contract Errors

`jackin load` should fail fast with clear contract errors for cases such as:

- missing `jackin.agent.toml`
- invalid manifest Dockerfile path
- missing Dockerfile
- unparsable Dockerfile
- final stage not literally using `donbeave/jackin-construct:trixie`

These should be reported as agent repo contract violations, not generic Docker failures.

### Build Failures

If validation passes but derived build generation or `docker build` fails, `load` returns the build failure and does not proceed into runtime startup.

### Startup Failures

If `load` has already started creating runtime infrastructure and then fails before a successful detach handoff, it cleans up what it created:

- main agent container
- DinD sidecar
- per-agent Docker network

It preserves the per-container state directory under `~/.jackin/data/<container-name>/`.

### Lifecycle Rule

The resulting lifecycle is:

- persisted state is durable until the operator removes it
- runtime infrastructure is durable only when the operator intentionally leaves an agent running
- normal attached exit cleans up runtime infrastructure
- detach leaves runtime infrastructure in place until `eject` or `exile`
- `purge` still only removes persisted state and does not manage running processes

## Testing And Documentation

### Tests

This redesign should add focused regression coverage around the new contract boundary in `jackin`.

High-value tests include:

- Dockerfile validation of final-stage `FROM donbeave/jackin-construct:trixie`
- rejection of invalid final-stage references
- derived Dockerfile generation with the expected injected steps
- runtime preparation that writes the expected per-container plugin metadata
- updated expectations where current tests assume direct builds from the agent repo Dockerfile

The goal is not broad end-to-end Docker integration coverage in unit tests, but strong coverage for the parser, generator, and lifecycle transitions introduced by this redesign.

### Documentation

`jackin` documentation should:

- keep the Matrix framing
- explain why the base image is called `construct`
- describe the strict `donbeave/jackin-construct:trixie` contract
- explain that `jackin` generates the final Claude-ready image layer

`smith` documentation should:

- explain that it is a `jackin` agent repo
- document that it is public-friendly
- document that its repo is mounted as `/workspace`
- describe its intentionally minimal environment layer

## Implementation Notes

This spec replaces the current assumption that agent repos are built directly as final runtimes. The existing v1 docs and plan in `docs/superpowers/specs/2026-04-01-jackin-v1-design.md` and `docs/superpowers/plans/2026-04-01-jackin-v1.md` remain useful historical context, but they are not the final source of truth for this redesign.
