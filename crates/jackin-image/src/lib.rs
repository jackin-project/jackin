//! jackin-image: derived-image Dockerfile generation and build context.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`DerivedBuildContext`] — derived-image build context.

pub mod agent_binary;
pub mod binary_artifact;
pub mod capsule_binary;
pub mod derived_image;
mod error;
pub mod image_build;
pub mod image_decision;
pub mod image_recipe;
pub mod naming;
mod telemetry_boundary;
pub mod version_check;

pub use error::ImageError;

pub use naming::{
    HOST_IDENTITY_STRATEGY, LABEL_IMAGE_AGENT_VERSION_PREFIX, LABEL_IMAGE_CAPSULE_VERSION,
    LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION, LABEL_IMAGE_MANIFEST_VERSION,
    LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION, LABEL_IMAGE_ROLE_GIT_SHA, image_name,
    image_name_for_branch, role_base_image_name, short_git_sha,
};
