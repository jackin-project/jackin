//! jackin-image: image generation and binary artifact management for jackin❯.
//!
//! Provides derived-image Dockerfile generation, agent binary acquisition and
//! caching, jackin-capsule binary acquisition, shared artifact helpers, and
//! cached image version checks.
//!
//! **Architecture Invariant:** L1 application crate (image subsystem).
//! Allowed dependencies: `jackin-core`, `jackin-manifest`, `jackin-docker`,
//! `jackin-diagnostics`. Single home for image build/naming/decision logic
//! — `runtime/image.rs` (and its split-out sub-modules `image_recipe`,
//! `image_decision`, `image_build`) live here after D1 PARTs E/F/G.

pub mod agent_binary;
pub mod binary_artifact;
pub mod capsule_binary;
pub mod derived_image;
pub mod image_recipe;
pub mod naming;
pub mod version_check;

pub use naming::{
    HOST_IDENTITY_STRATEGY, LABEL_IMAGE_AGENT_VERSION_PREFIX, LABEL_IMAGE_CAPSULE_VERSION,
    LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION, LABEL_IMAGE_MANIFEST_VERSION,
    LABEL_IMAGE_RECIPE_HASH, LABEL_IMAGE_RECIPE_VERSION, LABEL_IMAGE_ROLE_GIT_SHA, image_name,
    image_name_for_branch, role_base_image_name, short_git_sha,
};
