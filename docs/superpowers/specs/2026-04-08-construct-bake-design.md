# Construct Image Build Orchestration With Docker Bake and Just

This design replaces the current workflow-only construct image publishing logic
with a local-first build interface built on Docker Bake and a repo-local
`Justfile`.

The goal is to make the construct image build reproducible outside GitHub
Actions, while still supporting native `linux/amd64` and `linux/arm64`
publishing in CI.

## Goals

- Make the construct image build reproducible locally with checked-in commands.
- Keep GitHub Actions as a thin runner-and-credentials wrapper around shared
  repo commands.
- Support native single-platform builds for local development and split-runner
  CI.
- Support multi-platform publish by combining independently built platform
  images into one manifest.
- Document `just` as the contributor-facing command surface for construct image
  work.

## Non-Goals

- Generalizing the system for every future Docker image in the repository.
- Replacing the construct `Dockerfile`.
- Switching to Cloud Native Buildpacks.
- Requiring contributors to understand GitHub Actions internals to build or
  validate the construct locally.

## Why Docker Bake

Docker Bake is the right build engine for this repository because it provides:

- Declarative build definitions in version-controlled files.
- Local and CI reuse through the same `docker buildx bake` targets.
- Explicit platform configuration.
- Command-line overrides for platforms, outputs, tags, and variables.
- Native support for `--load`, `--push`, `--print`, and `--list` workflows.

This keeps the actual build graph in one place while avoiding long,
workflow-only `docker buildx build ...` command strings.

Cloud Native Buildpacks are not a good fit here. They are designed to build
application images without maintaining Dockerfiles, while `jackin` explicitly
owns a curated `docker/construct/Dockerfile` and needs direct control over
platforms, tags, and manifest publishing.

## Design Principles

### Local-First

The build contract must be executable by contributors on their own machines
before they open a pull request. CI should reuse the same commands, not define
its own separate image-publishing logic.

### Thin Wrappers

The declarative build graph should live in Bake. Human-friendly commands should
live in a `Justfile`. Only logic that is awkward to express in Bake or Just
alone should move into small helper scripts.

### Native Platform Builds

`amd64` and `arm64` should build natively in CI on matching GitHub-hosted
runners. Local developers should be able to build their default host platform
without paying a multi-platform or emulation cost.

### Stable Public Interface

Contributors should use short stable commands such as `just construct-build-local`
instead of memorizing Bake target names or GitHub Actions YAML.

## Files Introduced

- `docker-bake.hcl` — source of truth for construct image build targets and
  variables.
- `Justfile` — contributor-facing wrapper for construct build, push, and
  manifest commands.

No new binary or Rust-based wrapper is introduced in v1.

## Bake File Responsibilities

The root-level `docker-bake.hcl` becomes the declarative definition of the
construct image build.

It should define:

- A default registry/image name variable for `projectjackin/construct`.
- Shared tags for the stable `trixie` tag and commit-specific tag.
- Shared labels and build metadata.
- Shared context and Dockerfile path pointing at `docker/construct/`.
- Shared default multi-platform configuration containing:
  - `linux/amd64`
  - `linux/arm64`
- Separate targets for local loading and publish-oriented builds.

### Recommended Target Shape

The Bake file should define a small target set, for example:

- `_construct-common`
  - internal shared target
  - context: `docker/construct`
  - tags and labels derived from variables
  - default platforms: `linux/amd64`, `linux/arm64`

- `construct-local`
  - inherits from `_construct-common`
  - intended for local development
  - no registry push by default
  - used with `--load` and optional platform override

- `construct-publish`
  - inherits from `_construct-common`
  - intended for publish flows
  - used with `--push` in CI or advanced local workflows

This separation keeps local and publish workflows obvious without duplicating
the actual build configuration.

## Variable Model

Bake variables should cover the parts developers and CI need to override
without editing files:

- `REGISTRY_IMAGE`
  - default: `projectjackin/construct`
- `LOCAL_REGISTRY_IMAGE`
  - default: `jackin-local/construct`
- `STABLE_TAG`
  - default: `trixie`
- `SHA_TAG`
  - default derived from the current commit hash via environment or wrapper
- `PLATFORMS`
  - default: `linux/amd64,linux/arm64`

The wrapper layer can set these variables before calling Bake so both local and
CI usage stay consistent.

## Justfile Responsibilities

The `Justfile` is the supported human-facing interface. It should be documented
as a contributor prerequisite for construct image work.

### Command Surface

The command surface should be explicit and small:

- `just construct-init-buildx`
  - ensure a usable buildx builder exists locally

- `just construct-build-local`
  - build the construct for the default local platform
  - load it into the local Docker image store
  - no push

- `just construct-build-platform platform`
  - build exactly one requested platform locally
  - example: `just construct-build-platform amd64`
  - example: `just construct-build-platform arm64`

- `just construct-push-platform platform`
  - push exactly one platform image
  - used by split native CI jobs and optional advanced local publishing

- `just construct-publish-manifest`
  - assemble the final multi-platform manifest from previously pushed platform
    images
  - publish:
    - `projectjackin/construct:trixie`
    - `projectjackin/construct:trixie-<sha>`

- `just construct-inspect`
  - print the resolved Bake config or available Bake targets for debugging

### Why Just

`just` provides:

- a readable command catalog via `just --list`
- low-maintenance wrappers without building a custom CLI first
- easy reuse in CI via identical commands

This gives contributors a stable interface while Bake remains the underlying
build engine.

## Local Development Experience

### Default Local Flow

The normal contributor flow for construct work should be:

1. `just construct-init-buildx`
2. `just construct-build-local`

This is optimized for the developer's current machine and should avoid pushing
anything to a registry. It should also avoid overwriting the canonical
`projectjackin/construct:trixie` tag in the developer's local Docker daemon by
default. Maintainers can still opt into canonical local tags explicitly when
they need to test `jackin` against a locally built construct.

### Explicit Single-Platform Debugging

When a contributor needs deterministic control over target architecture, they
should use:

1. `just construct-build-platform amd64`
2. `just construct-build-platform arm64`

This supports debugging architecture-specific issues without editing CI files.

### Optional Advanced Local Publishing

Advanced contributors who want to rehearse the full release flow can use:

1. `just construct-push-platform amd64`
2. `just construct-push-platform arm64`
3. `just construct-publish-manifest`

This mirrors the CI publish model but remains optional for day-to-day
development. The per-platform push step should record digests for later
manifest assembly instead of relying on public architecture-suffixed tags.

## CI Workflow Design

The construct workflow should stop embedding raw Docker build logic and instead
call the same `just` commands contributors use locally.

### Build Jobs

Replace the current single publish job in
`.github/workflows/construct.yml` with two native build jobs:

- `build-amd64`
  - runs on `ubuntu-24.04`
  - runs `just construct-push-platform amd64` on `main`
  - runs the non-push local-equivalent validation path on pull requests

- `build-arm64`
  - runs on `ubuntu-24.04-arm`
  - runs `just construct-push-platform arm64` on `main`
  - runs the non-push local-equivalent validation path on pull requests

### Manifest Job

Add a final `publish-manifest` job that:

- depends on both native build jobs
- runs only after both succeed
- creates and publishes the multi-platform manifest tags

This ensures there is no partial release where only one architecture is
published.

### Pull Request Behavior

Pull requests should build both architectures natively but should not push
images. This validates the construct on both platforms before merge.

## Manifest Strategy

The manifest publish step should remain explicit and separate from the native
per-platform build steps.

Responsibilities of `just construct-publish-manifest`:

- collect the expected platform image references
- create a multi-arch manifest from those platform-specific images
- push the stable public tags
- consume per-platform digests rather than public `-amd64` / `-arm64` tags

The public tag surface should stay intentionally small:

- `projectjackin/construct:trixie`
- `projectjackin/construct:trixie-<sha>`

V1 does not publish permanent `-amd64` or `-arm64` public convenience tags.
That keeps the registry surface simpler and avoids encouraging consumers to pin
the wrong shape. Split CI jobs should therefore push platform images by digest
and assemble the final manifest from those digests.

## Buildx Bootstrap

Local reproducibility requires a predictable buildx bootstrap path.

`just construct-init-buildx` should:

- create a named buildx builder if one does not exist
- select it for subsequent commands
- inspect or bootstrap it so contributors can verify the local setup quickly

This replaces the implicit GitHub Actions-only buildx setup with a documented
local command.

## Documentation Changes

The implementation should document `just` explicitly in the contributor docs for
construct image work.

At minimum, update:

- `README.md`
- `docker/construct/README.md`
- relevant docs pages under `docs/src/content/docs/developing/`

The docs should explain:

- `just` is the supported command wrapper for construct image builds
- Bake is the underlying declarative build definition
- local contributors should validate construct changes with `just` before
  opening a pull request

## Why Not A Custom Rust Wrapper Yet

A dedicated Rust CLI could eventually provide stronger validation and richer
ergonomics, but it is not needed for v1.

Using Bake plus Just keeps the system:

- local-first
- explicit
- easy to inspect
- much smaller than introducing a new maintained build tool immediately

If the repository later grows into multiple images with shared conventions,
revisiting a custom wrapper can make sense. For now, Bake plus Just is the
smallest robust design.

## Verification Plan

1. `just construct-init-buildx` succeeds on a fresh contributor machine.
2. `just construct-build-local` produces a locally loadable construct image.
3. `just construct-build-platform amd64` and
   `just construct-build-platform arm64` both work when supported by the local
   builder setup.
4. Pull requests validate both native architectures without pushing.
5. Pushes to `main` build both native platform images and publish a final
   multi-platform manifest.
6. `docker buildx imagetools inspect projectjackin/construct:trixie` shows both
   `linux/amd64` and `linux/arm64` after publish.
