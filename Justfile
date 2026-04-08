# ---------------------------------------------------------------------------
# Justfile — construct image orchestration
# ---------------------------------------------------------------------------

set shell := ["bash", "-euo", "pipefail", "-c"]
set export

buildx_builder := "jackin-construct"

# Computed defaults — fallbacks ensure `just --list` works in partial checkouts
_default_git_sha := `git rev-parse --short=12 HEAD 2>/dev/null || echo dev`
_default_local_platform := `case "$(uname -m)" in x86_64|amd64) echo linux/amd64 ;; arm64|aarch64) echo linux/arm64 ;; *) printf "unsupported host architecture: %s\n" "$(uname -m)" >&2; exit 1 ;; esac`
_default_tirith_version := `awk -F= '$1 == "TIRITH_VERSION" { print $2 }' docker/construct/versions.env 2>/dev/null || echo ""`
_default_shellfirm_version := `awk -F= '$1 == "SHELLFIRM_VERSION" { print $2 }' docker/construct/versions.env 2>/dev/null || echo ""`

# Resolved build variables — env-var overrides take priority
REGISTRY_IMAGE := env_var_or_default("REGISTRY_IMAGE", "projectjackin/construct")
STABLE_TAG := env_var_or_default("STABLE_TAG", "trixie")
GIT_SHA := env_var_or_default("GIT_SHA", _default_git_sha)
SHA_TAG := STABLE_TAG + "-" + GIT_SHA
LOCAL_PLATFORM := env_var_or_default("LOCAL_PLATFORM", _default_local_platform)
TIRITH_VERSION := env_var_or_default("TIRITH_VERSION", _default_tirith_version)
SHELLFIRM_VERSION := env_var_or_default("SHELLFIRM_VERSION", _default_shellfirm_version)

default:
    @just --list

# Create and bootstrap the named Buildx builder
construct-init-buildx:
    #!/usr/bin/env bash
    set -euo pipefail
    if docker buildx inspect "{{buildx_builder}}" >/dev/null 2>&1; then
      docker buildx inspect "{{buildx_builder}}" --bootstrap
    else
      docker buildx create --name "{{buildx_builder}}" --driver docker-container --use
      docker buildx inspect "{{buildx_builder}}" --bootstrap
    fi

# Build the construct image for the host platform and load it locally
construct-build-local:
    docker buildx bake \
      --builder "{{buildx_builder}}" \
      --file docker-bake.hcl \
      --load \
      construct-local

# Build for a specific platform and load it locally
construct-build-platform platform:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{platform}}" in
      amd64) export LOCAL_PLATFORM="linux/amd64" ;;
      arm64) export LOCAL_PLATFORM="linux/arm64" ;;
      *)
        printf "platform must be amd64 or arm64, got %s\n" "{{platform}}" >&2
        exit 1
        ;;
    esac
    args=(
      docker buildx bake
      --builder "{{buildx_builder}}"
      --file docker-bake.hcl
      --load
    )
    if [ -n "${CACHE_FROM:-}" ]; then
      args+=(--set "construct-local.cache-from=${CACHE_FROM}")
    fi
    args+=(construct-local)
    "${args[@]}"

# Push a single-platform image to the registry (CI-only for canonical registry)
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
    staging_tag="${SHA_TAG}-{{platform}}"
    if [ -z "${CI:-}" ] && [ "$REGISTRY_IMAGE" = "projectjackin/construct" ]; then
      printf "Set REGISTRY_IMAGE to your own namespace before using construct-push-platform locally.\n" >&2
      exit 1
    fi
    export PLATFORMS="$docker_platform"
    args=(
      docker buildx bake
      --builder "{{buildx_builder}}"
      --file docker-bake.hcl
      --push
      --set "construct-publish.tags=${REGISTRY_IMAGE}:${staging_tag}"
    )
    if [ -n "${CACHE_FROM:-}" ]; then
      args+=(--set "construct-publish.cache-from=${CACHE_FROM}")
    fi
    if [ -n "${CACHE_TO:-}" ]; then
      args+=(--set "construct-publish.cache-to=${CACHE_TO}")
    fi
    args+=(construct-publish)
    "${args[@]}"

# NOTE: platform suffixes (-amd64, -arm64) are coupled to the PLATFORMS list in docker-bake.hcl.
#
# Combine per-platform staging images into a multi-platform manifest (CI-only for canonical registry)
construct-publish-manifest:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "${CI:-}" ] && [ "$REGISTRY_IMAGE" = "projectjackin/construct" ]; then
      printf "Set REGISTRY_IMAGE to your own namespace before using construct-publish-manifest locally.\n" >&2
      exit 1
    fi
    docker buildx imagetools create \
      --tag "${REGISTRY_IMAGE}:${STABLE_TAG}" \
      --tag "${REGISTRY_IMAGE}:${SHA_TAG}" \
      "${REGISTRY_IMAGE}:${SHA_TAG}-amd64" \
      "${REGISTRY_IMAGE}:${SHA_TAG}-arm64"
    docker buildx imagetools inspect "${REGISTRY_IMAGE}:${SHA_TAG}"

# Print the resolved Bake configuration (dry-run inspection)
construct-inspect:
    PLATFORMS="linux/amd64,linux/arm64" \
      docker buildx bake \
      --builder "{{buildx_builder}}" \
      --file docker-bake.hcl \
      --print \
      construct-local \
      construct-publish
