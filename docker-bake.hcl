// ---------------------------------------------------------------------------
// docker-bake.hcl — declarative build graph for the construct image
// ---------------------------------------------------------------------------
// Usage:
//   docker buildx bake --file docker-bake.hcl construct-local
//   docker buildx bake --file docker-bake.hcl --set 'construct-publish.output=type=image,name=user/app,push=true' construct-publish
//   docker buildx bake --file docker-bake.hcl --print   (inspect resolved config)
// ---------------------------------------------------------------------------

variable "REGISTRY_IMAGE" {
  default = "projectjackin/construct"
}

variable "LOCAL_REGISTRY_IMAGE" {
  default = "jackin-local/construct"
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

// ---------------------------------------------------------------------------
// Default group — all construct targets
// ---------------------------------------------------------------------------
group "default" {
  targets = ["construct-local"]
}

// ---------------------------------------------------------------------------
// Base target — shared context, args, and OCI labels
// ---------------------------------------------------------------------------
target "_construct-common" {
  context    = "docker/construct"
  dockerfile = "Dockerfile"
  args = {
    TIRITH_VERSION    = TIRITH_VERSION
    SHELLFIRM_VERSION = SHELLFIRM_VERSION
  }
  labels = {
    "org.opencontainers.image.title"       = "jackin construct"
    "org.opencontainers.image.description" = "Shared base image for jackin agents"
    "org.opencontainers.image.source"      = "https://github.com/jackin-project/jackin"
    "org.opencontainers.image.url"         = "https://jackin.tailrocks.com/developing/construct-image/"
    "org.opencontainers.image.revision"    = GIT_SHA
  }
}

// ---------------------------------------------------------------------------
// Local build — single platform, loaded into the Docker daemon
// ---------------------------------------------------------------------------
target "construct-local" {
  inherits = ["_construct-common"]
  tags = [
    "${LOCAL_REGISTRY_IMAGE}:${STABLE_TAG}",
    "${LOCAL_REGISTRY_IMAGE}:${SHA_TAG}",
  ]
  platforms = [LOCAL_PLATFORM]
}

// ---------------------------------------------------------------------------
// Publish build — multi-platform, intended for digest pushes and manifest assembly
// ---------------------------------------------------------------------------
target "construct-publish" {
  inherits = ["_construct-common"]
  output = [
    "type=image,name=${REGISTRY_IMAGE}",
  ]
  platforms = split(",", PLATFORMS)
}
