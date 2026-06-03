//! Dockerfile validation re-exports — behavior now in `jackin-manifest`.

pub use jackin_manifest::repo::{RoleRepoValidationError};
pub use jackin_manifest::repo_contract::{
    BASE_DOCKERFILE_FROM, CONSTRUCT_IMAGE, CONSTRUCT_PINNED_TAG, CONSTRUCT_REGISTRY_IMAGE,
    CONSTRUCT_STABLE_TAG, DOCKERFILE_NAME, MANIFEST_FILENAME, ValidatedDockerfile,
    construct_image, validate_agent_dockerfile,
};

#[cfg(test)]
mod tests;
