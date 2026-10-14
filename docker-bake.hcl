








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

variable "MISE_VERSION" {
  default = ""
}

variable "STARSHIP_VERSION" {
  default = ""
}




group "default" {
  targets = ["construct-local"]
}




target "_construct-common" {
  context    = "docker/construct"
  dockerfile = "Dockerfile"
  args = {
    TIRITH_VERSION    = TIRITH_VERSION
    SHELLFIRM_VERSION = SHELLFIRM_VERSION
    MISE_VERSION      = MISE_VERSION
    STARSHIP_VERSION  = STARSHIP_VERSION
  }
  labels = {
    "org.opencontainers.image.title"       = "jackin construct"
    "org.opencontainers.image.description" = "Shared base image for jackin agents"
    "org.opencontainers.image.source"      = "https://github.com/jackin-project/jackin"
    "org.opencontainers.image.url"         = "https://jackin.tailrocks.com/developing/construct-image/"
    "org.opencontainers.image.revision"    = GIT_SHA
  }
}




target "construct-local" {
  inherits = ["_construct-common"]
  tags = [
    "${LOCAL_REGISTRY_IMAGE}:${STABLE_TAG}",
    "${LOCAL_REGISTRY_IMAGE}:${SHA_TAG}",
  ]
  platforms = [LOCAL_PLATFORM]
}




target "construct-publish" {
  inherits = ["_construct-common"]
  attest = [
    {
      type = "provenance"
      mode = "max"
    },
    {
      type = "sbom"
    },
  ]
  output = [
    "type=image,name=${REGISTRY_IMAGE}",
  ]
  platforms = split(",", PLATFORMS)
}
