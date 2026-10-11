// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Docker image build pipeline: prepare binaries, build derived image, tag and cache.

mod version;

mod published;

mod prewarm;

mod build;

pub use build::{build_agent_image, git_head_sha};

pub use build::{cache_bust_value_for_build, ensure_local_role_base, should_mint_fresh_cache_bust};

#[cfg(not(test))]
pub use prewarm::prewarm_role_images;

pub use prewarm::{ImagePrewarmStatus, RoleImagePrewarmRow};

pub use version::*;

#[cfg(any(test, feature = "test-support"))]
pub use jackin_image::image_recipe::{
    build_image_recipe_with_construct_image, expected_image_recipe_for_test,
    image_recipe_label_map_for_install_test, image_recipe_label_map_for_test,
};

pub use jackin_image::image_decision::{
    ImageDecision, ImageInvalidationReason, build_decision, classify_image_labels,
    decision_base_image_override, emit_image_decision, emit_image_reuse,
};

pub use jackin_image::image_build::{
    BuildContextStats, DockerBuildStep, build_context_stats, collect_build_context_stats,
    compact_image_warning_line, docker_build_env, docker_info_uses_containerd_store,
    dockerfile_body_requests_role_git_sha_arg, dockerfile_requests_role_git_sha_arg,
    emit_build_context_snapshot, emit_compact_image_warning, emit_image_build_source,
    emit_non_containerd_image_store_note, is_buildkit_step_description, local_image_output_arg,
    parse_buildkit_duration_ms, parse_buildkit_line, parse_completed_buildkit_step,
    parse_docker_build_steps, should_stream_build_output, split_buildkit_duration,
};

mod binaries;
mod decision;
mod prewarm_run;
mod prewarm_spawn;
mod refresh;
mod sibling;
mod staleness;
mod validated;

use jackin_core::{Agent, CommandRunner, JackinPaths};
use jackin_image::derived_image::AgentInstall;
use jackin_image::version_check;
use jackin_runtime_naming::naming::{
    LABEL_IMAGE_CONSTRUCT, LABEL_IMAGE_CONSTRUCT_VERSION, LABEL_IMAGE_ROLE_GIT_SHA,
};
use std::collections::HashMap;

pub use binaries::{
    PreparedRuntimeBinaries, agent_binary_prepare_summary, prepare_agent_binaries,
    prepare_runtime_binaries_for_agents,
};
pub use decision::{decide_role_image, local_image_build_args};
#[cfg(not(test))]
pub use prewarm_run::{prewarm_agent_image, reuse_staleness_sentinel};

pub use prewarm_spawn::{spawn_sibling_image_prewarm, spawn_sibling_runtime_prewarm};
pub use refresh::{reuse_needs_background_staleness_check, spawn_selected_image_refresh};
#[cfg(not(test))]
pub use sibling::prewarm_sibling_image;
pub use sibling::role_git_sha_for_recipe;
#[cfg(not(test))]
pub use staleness::SiblingImagePrewarmOutcome;
pub use staleness::{sibling_agents, spawn_reuse_staleness_sentinel};
#[cfg(not(test))]
pub use validated::{
    prewarm_agent_image_from_validated_repo, refresh_agent_image_from_validated_repo,
};

#[cfg(test)]
mod tests;
