//! Dockerfile validation re-exports — behavior now in `jackin-manifest`.

pub(crate) use jackin_manifest::repo_contract::{
    BASE_DOCKERFILE_FROM, DOCKERFILE_NAME, MANIFEST_FILENAME, published_image_labels,
    published_image_repository,
};

#[cfg(test)]
pub(crate) use jackin_manifest::repo::RoleRepoValidationError;
#[cfg(test)]
pub(crate) use jackin_manifest::repo_contract::{
    CONSTRUCT_IMAGE, CONSTRUCT_REGISTRY_IMAGE, CONSTRUCT_STABLE_TAG, validate_agent_dockerfile,
};

#[cfg(test)]
mod tests;
