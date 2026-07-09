# AGENTS.md — jackin-image

Image generation and binary-artifact management for jackin❯.

## Hard rules (this crate)

- **Tier & dependencies:** L1 application (image subsystem). Allowed workspace deps: `jackin-core`, `jackin-manifest`, `jackin-docker`, `jackin-diagnostics`, `jackin-build-meta`. No presentation or runtime dependencies.
- **Keep `README.md` current:** update it when structure, public API, the build pipeline, or the image decision change (see `crates/AGENTS.md`).
- **Derived vs construct boundary.** This crate generates *derived* images on top of the operator-built `construct` base (`docker/construct/`); do not blur the two. The `construct` Dockerfile + its contributor flow live under `docker/construct/` and `developing/construct-image.mdx`.
- **Binary acquisition is cached and version-checked.** Agent/capsule binary fetches go through the acquisition + version-check helpers; never inline a one-off download.

## What lives here vs elsewhere

- This crate owns: derived-image Dockerfile generation, the build pipeline, agent/capsule binary acquisition + cache, image-decision (build/reuse/published), image naming.
- Docker daemon access lives in `jackin-docker`. Manifest data lives in `jackin-manifest`. The `construct` base image lives under `docker/construct/`.

Workspace rules: [../AGENTS.md](../AGENTS.md). Repo rules: [../../AGENTS.md](../../AGENTS.md).
