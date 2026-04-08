# ---------------------------------------------------------------------------
# Justfile — construct image orchestration
# ---------------------------------------------------------------------------

set shell := ["bash", "-euo", "pipefail", "-c"]
set export

buildx_builder := env_var_or_default("BUILDX_BUILDER", "jackin-construct")

# Computed defaults — fallbacks ensure `just --list` works in partial checkouts
_default_git_sha := `git rev-parse --short=12 HEAD 2>/dev/null || echo dev`
_default_local_platform := `case "$(uname -m)" in x86_64|amd64) echo linux/amd64 ;; arm64|aarch64) echo linux/arm64 ;; *) printf "unsupported host architecture: %s\n" "$(uname -m)" >&2; exit 1 ;; esac`
_default_tirith_version := `awk -F= '$1 == "TIRITH_VERSION" { print $2 }' docker/construct/versions.env 2>/dev/null || echo ""`
_default_shellfirm_version := `awk -F= '$1 == "SHELLFIRM_VERSION" { print $2 }' docker/construct/versions.env 2>/dev/null || echo ""`

# Resolved build variables — env-var overrides take priority
REGISTRY_IMAGE := env_var_or_default("REGISTRY_IMAGE", "projectjackin/construct")
LOCAL_REGISTRY_IMAGE := env_var_or_default("LOCAL_REGISTRY_IMAGE", "jackin-local/construct")
STABLE_TAG := env_var_or_default("STABLE_TAG", "trixie")
GIT_SHA := env_var_or_default("GIT_SHA", _default_git_sha)
SHA_TAG := STABLE_TAG + "-" + GIT_SHA
LOCAL_PLATFORM := env_var_or_default("LOCAL_PLATFORM", _default_local_platform)
TIRITH_VERSION := env_var_or_default("TIRITH_VERSION", _default_tirith_version)
SHELLFIRM_VERSION := env_var_or_default("SHELLFIRM_VERSION", _default_shellfirm_version)
DIGEST_DIR := env_var_or_default("DIGEST_DIR", "/tmp/jackin-construct-digests")

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

# Inspect the configured Buildx builder and list available builders
construct-doctor-buildx:
    #!/usr/bin/env bash
    set -euo pipefail
    docker buildx ls
    docker buildx inspect "{{buildx_builder}}" --bootstrap

# Recreate the configured Buildx builder from scratch
construct-reset-buildx:
    #!/usr/bin/env bash
    set -euo pipefail
    docker buildx rm --force "{{buildx_builder}}" >/dev/null 2>&1 || true
    docker buildx create --name "{{buildx_builder}}" --driver docker-container --use
    docker buildx inspect "{{buildx_builder}}" --bootstrap

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
    if [ -n "${CACHE_TO:-}" ]; then
      args+=(--set "construct-local.cache-to=${CACHE_TO}")
    fi
    args+=(construct-local)
    "${args[@]}"

# Push a single-platform image by digest (CI-only for canonical registry)
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
    if [ -z "${CI:-}" ] && [ "$REGISTRY_IMAGE" = "projectjackin/construct" ]; then
      printf "Set REGISTRY_IMAGE to your own namespace before using construct-push-platform locally.\n" >&2
      exit 1
    fi
    mkdir -p "${DIGEST_DIR}"
    metadata_file="$(mktemp "${TMPDIR:-/tmp}/jackin-construct-{{platform}}.XXXXXX.json")"
    trap 'rm -f "${metadata_file}"' EXIT
    digest_file="${DIGEST_FILE:-${DIGEST_DIR}/{{platform}}.digest}"
    export PLATFORMS="$docker_platform"
    args=(
      docker buildx bake
      --builder "{{buildx_builder}}"
      --file docker-bake.hcl
      --metadata-file "${metadata_file}"
      --set "construct-publish.output=type=image,name=${REGISTRY_IMAGE},push-by-digest=true,name-canonical=true,push=true"
    )
    if [ -n "${CACHE_FROM:-}" ]; then
      args+=(--set "construct-publish.cache-from=${CACHE_FROM}")
    fi
    if [ -n "${CACHE_TO:-}" ]; then
      args+=(--set "construct-publish.cache-to=${CACHE_TO}")
    fi
    args+=(construct-publish)
    "${args[@]}"
    digest="$(tr -d '[:space:]' < "${metadata_file}" | sed -n 's/.*"containerimage.digest":"\([^"]*\)".*/\1/p')"
    if [ -z "${digest}" ]; then
      printf "Unable to determine pushed digest from %s\n" "${metadata_file}" >&2
      exit 1
    fi
    printf '%s\n' "${digest}" > "${digest_file}"
    printf 'Wrote %s digest to %s\n' "{{platform}}" "${digest_file}"

# Combine per-platform digest pushes into a multi-platform manifest (CI-only for canonical registry)
construct-publish-manifest:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "${CI:-}" ] && [ "$REGISTRY_IMAGE" = "projectjackin/construct" ]; then
      printf "Set REGISTRY_IMAGE to your own namespace before using construct-publish-manifest locally.\n" >&2
      exit 1
    fi
    refs=()
    for platform in amd64 arm64; do
      digest_file="${DIGEST_DIR}/${platform}.digest"
      if [ ! -f "${digest_file}" ]; then
        printf "Missing digest file for %s at %s\n" "${platform}" "${digest_file}" >&2
        printf "Run construct-push-platform %s first or set DIGEST_DIR to the downloaded digest artifacts.\n" "${platform}" >&2
        exit 1
      fi
      digest="$(tr -d '[:space:]' < "${digest_file}")"
      if [ -z "${digest}" ]; then
        printf "Digest file %s is empty\n" "${digest_file}" >&2
        exit 1
      fi
      refs+=("${REGISTRY_IMAGE}@${digest}")
    done
    docker buildx imagetools create \
      --tag "${REGISTRY_IMAGE}:${STABLE_TAG}" \
      --tag "${REGISTRY_IMAGE}:${SHA_TAG}" \
      "${refs[@]}"
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
