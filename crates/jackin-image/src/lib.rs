//! jackin-image: image generation and binary artifact management for jackin❯.
//!
//! Provides derived-image Dockerfile generation, agent binary acquisition and
//! caching, jackin-capsule binary acquisition, shared artifact helpers, and
//! cached image version checks.

pub mod agent_binary;
pub mod binary_artifact;
pub mod capsule_binary;
pub mod derived_image;
pub mod version_check;
