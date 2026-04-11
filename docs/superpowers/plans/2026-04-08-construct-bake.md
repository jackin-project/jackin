# Construct Bake Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the construct image's workflow-only Docker build with a repo-root Docker Bake definition, a repo-root `Justfile`, a split native GitHub Actions publish flow, and matching contributor docs.

**Architecture:** Keep the build graph in `docker-bake.hcl` and keep orchestration in a small root `Justfile`. GitHub Actions becomes a thin wrapper that checks out the repo, installs `just`, bootstraps buildx, runs native per-platform `just` commands on `amd64` and `arm64` runners, then assembles the final multi-platform manifest from commit-scoped staging tags.

**Tech Stack:** Docker Buildx Bake, Just, GitHub Actions, Markdown, Astro Starlight, Bun.

**Assumption:** V1 stays construct-only and does not add helper scripts or a custom Rust wrapper. Optional GitHub Actions cache wiring is deferred unless it is needed to keep parity with current CI behavior.

---

## File Map

- Create: `docker-bake.hcl` — declarative construct build targets, variables, tags, labels, platforms, and build args.
- Create: `Justfile` — stable contributor-facing commands for buildx bootstrap, local builds, per-platform pushes, manifest publishing, and Bake inspection.
- Modify: `.github/workflows/construct.yml` — split the single workflow job into native `amd64` and `arm64` build jobs plus a manifest publish job, and trigger on the new root build inputs.
- Modify: `README.md` — document `just` as the supported construct build wrapper for repository contributors.
- Modify: `docker/construct/README.md` — replace workflow-only build guidance with local-first `just` commands and registry-safe publish rehearsal guidance.
- Modify: `docs/src/content/docs/developing/construct-image.mdx` — document the Bake + Just model, `versions.env` ownership, local validation commands, and CI trigger scope.

### Task 1: Add the Bake Definition and Just Wrapper

**Files:**
- Create: `docker-bake.hcl`
- Create: `Justfile`

- [ ] **Step 1: Confirm the repo still lacks the new build entrypoints**

Run:

```bash
test ! -f Justfile
test ! -f docker-bake.hcl
! rg -n "construct-build-local|construct-publish-manifest" .github/workflows/construct.yml
```

Expected: the two `test` commands succeed because `Justfile` and `docker-bake.hcl` do not exist yet, and the negated `rg` command succeeds because the workflow still has no `construct-build-local` or `construct-publish-manifest` references.

- [ ] **Step 2: Create `docker-bake.hcl` with the construct-only targets and variables**

Create `docker-bake.hcl` with this content:

```hcl
variable "REGISTRY_IMAGE" {
  default = "projectjackin/construct"
}

variable "STABLE_TAG" {
  default = "trixie"
}

variable "GIT_SHA" {
  default = "dev"
}

variable "SHA_TAG" {
  default = "trixie-dev"
}

variable "LOCAL_PLATFORM" {
  default = "linux/amd64"
}

variable "PLATFORMS" {
  default = "linux/amd64,linux/arm64"
}

variable "TIRITH_VERSION" {
  default = ""
}

variable "SHELLFIRM_VERSION" {
  default = ""
}

target "_construct-common" {
  context    = "docker/construct"
  dockerfile = "Dockerfile"

  args = {
    TIRITH_VERSION    = "${TIRITH_VERSION}"
    SHELLFIRM_VERSION = "${SHELLFIRM_VERSION}"
  }

  labels = {
    "org.opencontainers.image.title"       = "jackin construct"
    "org.opencontainers.image.description" = "Shared base image for jackin agents"
    "org.opencontainers.image.source"      = "https://github.com/jackin-project/jackin"
    "org.opencontainers.image.url"         = "https://jackin.tailrocks.com/developing/construct-image/"
    "org.opencontainers.image.revision"    = "${GIT_SHA}"
  }
}

target "construct-local" {
  inherits = ["_construct-common"]

  tags = [
    "${REGISTRY_IMAGE}:${STABLE_TAG}",
    "${REGISTRY_IMAGE}:${SHA_TAG}",
  ]

  platforms = ["${LOCAL_PLATFORM}"]
}

target "construct-publish" {
  inherits = ["_construct-common"]

  tags = [
    "${REGISTRY_IMAGE}:${STABLE_TAG}",
    "${REGISTRY_IMAGE}:${SHA_TAG}",
  ]

  platforms = split(",", PLATFORMS)
}
```

- [ ] **Step 3: Verify Bake can resolve the new file and target names**

Run:

```bash
docker buildx bake --file docker-bake.hcl --list
```

Expected: the target list includes `_construct-common`, `construct-local`, and `construct-publish`.

- [ ] **Step 4: Create the repo-root `Justfile` with the supported construct commands**

Create `Justfile` with this content:

```just
set shell := ["bash", "-euo", "pipefail", "-c"]

buildx_builder := "jackin-construct"
default_registry_image := "projectjackin/construct"
default_stable_tag := "trixie"
default_git_sha := `git rev-parse --short=12 HEAD`
default_local_platform := `bash -euo pipefail -c 'case "$(uname -m)" in x86_64|amd64) echo linux/amd64 ;; arm64|aarch64) echo linux/arm64 ;; *) printf "unsupported host architecture: %s\n" "$(uname -m)" >&2; exit 1 ;; esac'`
default_tirith_version := `awk -F= '$1 == "TIRITH_VERSION" { print $2 }' docker/construct/versions.env`
default_shellfirm_version := `awk -F= '$1 == "SHELLFIRM_VERSION" { print $2 }' docker/construct/versions.env`

default:
    @just --list

construct-init-buildx:
    #!/usr/bin/env bash
    set -euo pipefail
    if docker buildx inspect "{{buildx_builder}}" >/dev/null 2>&1; then
      docker buildx inspect "{{buildx_builder}}" --bootstrap
    else
      docker buildx create --name "{{buildx_builder}}" --driver docker-container --use
      docker buildx inspect "{{buildx_builder}}" --bootstrap
    fi

construct-build-local:
    #!/usr/bin/env bash
    set -euo pipefail
    registry_image="${REGISTRY_IMAGE:-{{default_registry_image}}}"
    stable_tag="${STABLE_TAG:-{{default_stable_tag}}}"
    git_sha="${GIT_SHA:-{{default_git_sha}}}"
    local_platform="${LOCAL_PLATFORM:-{{default_local_platform}}}"
    tirith_version="${TIRITH_VERSION:-{{default_tirith_version}}}"
    shellfirm_version="${SHELLFIRM_VERSION:-{{default_shellfirm_version}}}"

    REGISTRY_IMAGE="$registry_image" \
    STABLE_TAG="$stable_tag" \
    GIT_SHA="$git_sha" \
    SHA_TAG="${stable_tag}-${git_sha}" \
    LOCAL_PLATFORM="$local_platform" \
    TIRITH_VERSION="$tirith_version" \
    SHELLFIRM_VERSION="$shellfirm_version" \
      docker buildx bake \
      --builder "{{buildx_builder}}" \
      --file docker-bake.hcl \
      --load \
      construct-local

construct-build-platform platform:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{platform}}" in
      amd64) docker_platform="linux/amd64" ;;
      arm64) docker_platform="linux/arm64" ;;
      *)
        printf "platform must be amd64 or arm64, got %s\n" "{{platform}}" >&2
        exit 1
        ;;
    esac

    registry_image="${REGISTRY_IMAGE:-{{default_registry_image}}}"
    stable_tag="${STABLE_TAG:-{{default_stable_tag}}}"
    git_sha="${GIT_SHA:-{{default_git_sha}}}"
    tirith_version="${TIRITH_VERSION:-{{default_tirith_version}}}"
    shellfirm_version="${SHELLFIRM_VERSION:-{{default_shellfirm_version}}}"

    REGISTRY_IMAGE="$registry_image" \
    STABLE_TAG="$stable_tag" \
    GIT_SHA="$git_sha" \
    SHA_TAG="${stable_tag}-${git_sha}" \
    LOCAL_PLATFORM="$docker_platform" \
    TIRITH_VERSION="$tirith_version" \
    SHELLFIRM_VERSION="$shellfirm_version" \
      docker buildx bake \
      --builder "{{buildx_builder}}" \
      --file docker-bake.hcl \
      --load \
      construct-local

construct-push-platform platform:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{platform}}" in
      amd64) docker_platform="linux/amd64" ;;
      arm64) docker_platform="linux/arm64" ;;
      *)
        printf "platform must be amd64 or arm64, got %s\n" "{{platform}}" >&2
        exit 1
        ;;
    esac

    registry_image="${REGISTRY_IMAGE:-{{default_registry_image}}}"
    stable_tag="${STABLE_TAG:-{{default_stable_tag}}}"
    git_sha="${GIT_SHA:-{{default_git_sha}}}"
    tirith_version="${TIRITH_VERSION:-{{default_tirith_version}}}"
    shellfirm_version="${SHELLFIRM_VERSION:-{{default_shellfirm_version}}}"
    staging_tag="${stable_tag}-${git_sha}-{{platform}}"

    if [ -z "${CI:-}" ] && [ "$registry_image" = "{{default_registry_image}}" ]; then
      printf "Set REGISTRY_IMAGE to your own namespace before using construct-push-platform locally.\n" >&2
      exit 1
    fi

    REGISTRY_IMAGE="$registry_image" \
    STABLE_TAG="$stable_tag" \
    GIT_SHA="$git_sha" \
    SHA_TAG="${stable_tag}-${git_sha}" \
    PLATFORMS="$docker_platform" \
    TIRITH_VERSION="$tirith_version" \
    SHELLFIRM_VERSION="$shellfirm_version" \
      docker buildx bake \
      --builder "{{buildx_builder}}" \
      --file docker-bake.hcl \
      --push \
      --set construct-publish.tags="${registry_image}:${staging_tag}" \
      construct-publish

construct-publish-manifest:
    #!/usr/bin/env bash
    set -euo pipefail
    registry_image="${REGISTRY_IMAGE:-{{default_registry_image}}}"
    stable_tag="${STABLE_TAG:-{{default_stable_tag}}}"
    git_sha="${GIT_SHA:-{{default_git_sha}}}"

    if [ -z "${CI:-}" ] && [ "$registry_image" = "{{default_registry_image}}" ]; then
      printf "Set REGISTRY_IMAGE to your own namespace before using construct-publish-manifest locally.\n" >&2
      exit 1
    fi

    docker buildx imagetools create \
      --tag "${registry_image}:${stable_tag}" \
      --tag "${registry_image}:${stable_tag}-${git_sha}" \
      "${registry_image}:${stable_tag}-${git_sha}-amd64" \
      "${registry_image}:${stable_tag}-${git_sha}-arm64"

    docker buildx imagetools inspect "${registry_image}:${stable_tag}-${git_sha}"

construct-inspect:
    #!/usr/bin/env bash
    set -euo pipefail
    registry_image="${REGISTRY_IMAGE:-{{default_registry_image}}}"
    stable_tag="${STABLE_TAG:-{{default_stable_tag}}}"
    git_sha="${GIT_SHA:-{{default_git_sha}}}"
    local_platform="${LOCAL_PLATFORM:-{{default_local_platform}}}"
    tirith_version="${TIRITH_VERSION:-{{default_tirith_version}}}"
    shellfirm_version="${SHELLFIRM_VERSION:-{{default_shellfirm_version}}}"

    REGISTRY_IMAGE="$registry_image" \
    STABLE_TAG="$stable_tag" \
    GIT_SHA="$git_sha" \
    SHA_TAG="${stable_tag}-${git_sha}" \
    LOCAL_PLATFORM="$local_platform" \
    PLATFORMS="linux/amd64,linux/arm64" \
    TIRITH_VERSION="$tirith_version" \
    SHELLFIRM_VERSION="$shellfirm_version" \
      docker buildx bake \
      --builder "{{buildx_builder}}" \
      --file docker-bake.hcl \
      --print \
      construct-local \
      construct-publish
```

- [ ] **Step 5: Verify `just` exposes the new command surface and prints resolved Bake config**

Run:

```bash
just --list
just construct-init-buildx
just construct-inspect
```

Expected: `just --list` shows `construct-init-buildx`, `construct-build-local`, `construct-build-platform`, `construct-push-platform`, `construct-publish-manifest`, and `construct-inspect`. `just construct-init-buildx` bootstraps `jackin-construct`, and `construct-inspect` prints JSON for `construct-local` and `construct-publish` with `TIRITH_VERSION` and `SHELLFIRM_VERSION` resolved from `docker/construct/versions.env`.

- [ ] **Step 6: Commit the build tooling files**

```bash
git add docker-bake.hcl Justfile
git commit -m "build: add construct bake workflow"
```

---

### Task 2: Rewrite the Construct GitHub Actions Workflow Around `just`

**Files:**
- Modify: `.github/workflows/construct.yml`

- [ ] **Step 1: Confirm the current workflow still uses the inline Docker action flow**

Run:

```bash
rg -n "metadata-action|build-push-action|build-and-push|ubuntu-latest" .github/workflows/construct.yml
```

Expected: matches for `docker/metadata-action`, `docker/build-push-action`, a single `build-and-push` job, and `ubuntu-latest`.

- [ ] **Step 2: Replace the workflow with native per-platform jobs plus a manifest job**

Replace `.github/workflows/construct.yml` with this content:

```yaml
name: Build and Push Construct Image

on:
  push:
    branches: [main]
    paths:
      - '.github/workflows/construct.yml'
      - 'Justfile'
      - 'docker-bake.hcl'
      - 'docker/construct/**'
  pull_request:
    branches: [main]
    paths:
      - '.github/workflows/construct.yml'
      - 'Justfile'
      - 'docker-bake.hcl'
      - 'docker/construct/**'
  workflow_dispatch:

jobs:
  build-amd64:
    runs-on: ubuntu-24.04
    permissions:
      contents: read

    steps:
      - name: Checkout repository
        uses: actions/checkout@v5

      - name: Install just
        uses: extractions/setup-just@v3

      - name: Install Docker Buildx
        uses: docker/setup-buildx-action@v3
        with:
          name: jackin-construct
          driver: docker-container

      - name: Log in to Docker Hub
        if: github.event_name != 'pull_request'
        uses: docker/login-action@v3
        with:
          username: ${{ secrets.DOCKERHUB_USERNAME }}
          password: ${{ secrets.DOCKERHUB_TOKEN }}

      - name: Bootstrap buildx
        run: just construct-init-buildx

      - name: Build amd64 for pull requests
        if: github.event_name == 'pull_request'
        run: just construct-build-platform amd64

      - name: Push amd64 staging image
        if: github.event_name != 'pull_request'
        run: just construct-push-platform amd64

  build-arm64:
    runs-on: ubuntu-24.04-arm
    permissions:
      contents: read

    steps:
      - name: Checkout repository
        uses: actions/checkout@v5

      - name: Install just
        uses: extractions/setup-just@v3

      - name: Install Docker Buildx
        uses: docker/setup-buildx-action@v3
        with:
          name: jackin-construct
          driver: docker-container

      - name: Log in to Docker Hub
        if: github.event_name != 'pull_request'
        uses: docker/login-action@v3
        with:
          username: ${{ secrets.DOCKERHUB_USERNAME }}
          password: ${{ secrets.DOCKERHUB_TOKEN }}

      - name: Bootstrap buildx
        run: just construct-init-buildx

      - name: Build arm64 for pull requests
        if: github.event_name == 'pull_request'
        run: just construct-build-platform arm64

      - name: Push arm64 staging image
        if: github.event_name != 'pull_request'
        run: just construct-push-platform arm64

  publish-manifest:
    if: github.event_name != 'pull_request'
    needs: [build-amd64, build-arm64]
    runs-on: ubuntu-24.04
    permissions:
      contents: read

    steps:
      - name: Checkout repository
        uses: actions/checkout@v5

      - name: Install just
        uses: extractions/setup-just@v3

      - name: Install Docker Buildx
        uses: docker/setup-buildx-action@v3
        with:
          name: jackin-construct
          driver: docker-container

      - name: Log in to Docker Hub
        uses: docker/login-action@v3
        with:
          username: ${{ secrets.DOCKERHUB_USERNAME }}
          password: ${{ secrets.DOCKERHUB_TOKEN }}

      - name: Bootstrap buildx
        run: just construct-init-buildx

      - name: Publish multi-platform manifest
        run: just construct-publish-manifest
```

- [ ] **Step 3: Verify the workflow shape and trigger scope**

Run:

```bash
rg -n "build-amd64|build-arm64|publish-manifest|construct-push-platform|docker-bake.hcl|Justfile|ubuntu-24.04-arm" .github/workflows/construct.yml
```

Expected: matches for all three jobs, both new root-level path filters, the `construct-push-platform` recipe calls, and the native `ubuntu-24.04-arm` runner.

- [ ] **Step 4: Commit the workflow refactor**

```bash
git add .github/workflows/construct.yml
git commit -m "ci: split construct image builds by platform"
```

---

### Task 3: Update the Contributor-Facing Construct Documentation

**Files:**
- Modify: `README.md`
- Modify: `docker/construct/README.md`
- Modify: `docs/src/content/docs/developing/construct-image.mdx`

- [ ] **Step 1: Confirm the docs still describe workflow-only construct builds**

Run:

```bash
rg -n "automatically built and pushed|changes are made to this directory|GitHub Actions when changes are made to that directory" README.md docker/construct/README.md docs/src/content/docs/developing/construct-image.mdx
```

Expected: matches in `docker/construct/README.md` and `docs/src/content/docs/developing/construct-image.mdx` that still describe the old workflow-only trigger model.

- [ ] **Step 2: Add a construct build section to the repo root README**

Insert this section in `README.md` after the `## Development` section:

````md
## Construct Image Development

Construct image changes use checked-in Docker build tooling instead of workflow-only commands. Contributors working on the base image should install [`just`](https://github.com/casey/just), bootstrap buildx once, and validate the image locally before opening a pull request:

```sh
just construct-init-buildx
just construct-build-local
```

The declarative build graph lives in `docker-bake.hcl`, and GitHub Actions reuses the same `just` commands for native `amd64` and `arm64` builds.
````

- [ ] **Step 3: Rewrite `docker/construct/README.md` to document the new local-first build flow**

Replace the current `## Building` section in `docker/construct/README.md` with this block:

````md
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
````

- [ ] **Step 4: Update the Starlight construct page with Bake, Just, and trigger-scope details**

Replace the existing `## How it's built` section in `docs/src/content/docs/developing/construct-image.mdx` with this block:

````mdx
## How it's built

The construct source code lives at [`docker/construct/`](https://github.com/jackin-project/jackin/tree/main/docker/construct) in the jackin' repository. The declarative build definition lives in the repo-root `docker-bake.hcl`, and the supported command wrapper is the repo-root `Justfile`.

Before opening a pull request for construct changes, validate the image locally:

```bash
just construct-init-buildx
just construct-build-local
```

To force a specific target architecture during local debugging:

```bash
just construct-build-platform amd64
just construct-build-platform arm64
```

`docker/construct/versions.env` remains the source of truth for the pinned `tirith` and `shellfirm` build args used by `docker/construct/Dockerfile`.

GitHub Actions reuses the same `just` commands on native `ubuntu-24.04` and `ubuntu-24.04-arm` runners. Construct CI triggers when changes touch `docker/construct/**`, `docker-bake.hcl`, `Justfile`, or `.github/workflows/construct.yml`.

The image is tagged as:
- `projectjackin/construct:trixie` — the stable tag
- `projectjackin/construct:trixie-{sha}` — commit-specific tags
````

- [ ] **Step 5: Build the docs site to verify the Markdown and MDX changes**

Run:

```bash
bun install --frozen-lockfile
bun run build
```

Working directory: `docs/`

Expected: Astro/Starlight build completes successfully with no MDX parse errors.

- [ ] **Step 6: Commit the docs updates**

```bash
git add README.md docker/construct/README.md docs/src/content/docs/developing/construct-image.mdx
git commit -m "docs: document construct bake workflow"
```

---

### Task 4: Run the End-to-End Verification Commands

**Files:**
- Verify: `docker-bake.hcl`
- Verify: `Justfile`
- Verify: `.github/workflows/construct.yml`
- Verify: `README.md`
- Verify: `docker/construct/README.md`
- Verify: `docs/src/content/docs/developing/construct-image.mdx`

- [ ] **Step 1: Bootstrap or re-bootstrap the named buildx builder**

Run:

```bash
just construct-init-buildx
```

Expected: Docker reports that the `jackin-construct` builder exists and is bootstrapped.

- [ ] **Step 2: Re-print the resolved Bake configuration after all changes**

Run:

```bash
just construct-inspect
```

Expected: the printed configuration still contains `construct-local` and `construct-publish`, with `TIRITH_VERSION` and `SHELLFIRM_VERSION` populated.

- [ ] **Step 3: Build and load the default host-platform construct image**

Run:

```bash
just construct-build-local
docker image inspect projectjackin/construct:trixie --format '{{.Os}}/{{.Architecture}}'
```

Expected: the build completes successfully and the image inspect command prints the host platform, such as `linux/arm64` on Apple Silicon or `linux/amd64` on x86_64.

- [ ] **Step 4: Run the explicit single-platform recipe for the current host architecture**

Run:

```bash
platform="$(case "$(uname -m)" in x86_64|amd64) echo amd64 ;; arm64|aarch64) echo arm64 ;; *) exit 1 ;; esac)"
just construct-build-platform "$platform"
```

Expected: the host-platform-specific recipe completes successfully without changing the tag model or pushing to any registry.

- [ ] **Step 5: Run the repository-required Rust verification commands before final handoff**

Run:

```bash
cargo fmt -- --check
cargo clippy
cargo nextest run
```

Expected: all three commands pass with zero warnings and zero failures.

- [ ] **Step 6: Confirm the working tree contains only the intended implementation changes**

Run:

```bash
git status --short -- docker-bake.hcl Justfile .github/workflows/construct.yml README.md docker/construct/README.md docs/src/content/docs/developing/construct-image.mdx
```

Expected: the path-limited status output is either empty because everything is committed, or it mentions only the six implementation files above if a follow-up fix changed one of them during verification.
