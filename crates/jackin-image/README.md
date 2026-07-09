# jackin-image

Image generation and binary-artifact management for jackin❯. Builds the derived role image, acquires/caches the agent and capsule binaries, and decides the image-build vs reuse path.

## What this crate owns

- Derived-image Dockerfile generation (`derived_image`, `image_recipe`) and the build pipeline (`image_build`).
- Agent + capsule binary acquisition and caching (`agent_binary`, `capsule_binary`, `binary_artifact`).
- The image *decision* — build vs reuse vs published (`image_decision`, `version_check`) and image naming (`naming`).

## Architecture tier and allowed dependencies

**L1 application (image subsystem).** Allowed workspace dependencies: `jackin-core`, `jackin-manifest`, `jackin-docker`, `jackin-diagnostics`, `jackin-build-meta`. No presentation or runtime dependencies — image materialization is a domain/app concern consumed by `jackin-runtime`.

## Structure

- `src/derived_image.rs`, `src/image_recipe.rs`, `src/image_build.rs` — Dockerfile generation + build
- `src/agent_binary.rs`, `src/capsule_binary.rs`, `src/binary_artifact.rs` — binary acquisition/caching
- `src/image_decision.rs`, `src/version_check.rs`, `src/naming.rs` — decision, version check, naming
- subdirs (`image_recipe/`, `image_decision/`, `derived_image/`, `binary_artifact/`, `image_build/`, `version_check/`, `capsule_binary/`, `agent_binary/`) — module bodies + tests

## Public API

The image-build decision and materialization entry points consumed by `jackin-runtime`. The `construct` Dockerfile (operator-built base) lives under `docker/construct/`; this crate generates *derived* images on top of it.

## How to verify

```sh
cargo nextest run -p jackin-image
cargo clippy -p jackin-image --all-targets -- -D warnings
```

See [../AGENTS.md](../AGENTS.md) for workspace-wide Rust rules and [../../AGENTS.md](../../AGENTS.md) for repo rules.
