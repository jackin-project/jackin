// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::print_stderr,
    reason = "launch flow emits operator-visible pull and spacing diagnostics"
)]

mod account_config;
mod account_identity;
pub(crate) use account_identity::{
    AccountConfigRevision, GenerationLeaseViolation, ensure_current_or_remove_stale_container,
};
mod launch_dind;
pub use launch_dind::DIND_IMAGE;
pub(super) use launch_dind::create_role_network;
pub use launch_dind::{
    DindSidecarPrewarm, prewarm_dind_sidecar_container_with_paths, write_prewarmed_dind_state,
};
use launch_dind::{adopt_prewarmed_dind_sidecar, run_dind_sidecar_headless};

pub(crate) use jackin_runtime_launch_slot::launch_slot;

mod trust;

mod image_plan;
pub use image_plan::{LaunchImagePlan, resolve_launch_image_plan};

mod dry_run;
pub use dry_run::{
    DryRunIdentity, DryRunModelProjection, resolve_dry_run_identity,
    resolve_dry_run_model_projection,
};
mod programmatic;
pub use account_identity::{
    account_admission_matches, account_configuration_fingerprint, account_configuration_matches,
};
pub use programmatic::{
    CLAUDE_EFFORT_ENV, CLAUDE_MODEL_ENV, CODEX_LANE_EFFORT_ENV, CODEX_LANE_MODEL_ENV, IdentitySink,
    LaunchedInstance, LoadOptionsError, lane_agent_env, with_account_selection,
    with_configuration_selection,
};

mod launch_pipeline;
pub use launch_pipeline::launch_phases::{
    GrantPhaseInput, GrantsValidated, ImagePhaseClass, ImagePhaseClassified, classify_image_phase,
    validate_launch_grants,
};

use jackin_core::RoleSelector;

pub use launch_pipeline::{load_role, resolve_supported_agents_for_console};

pub use jackin_runtime_launch_load_options::load_options::LoadOptions;

pub(super) fn validate_agent_supported(
    selector: &RoleSelector,
    manifest: &jackin_manifest::RoleManifest,
    agent: jackin_core::Agent,
) -> anyhow::Result<()> {
    let supported = manifest.supported_agents();
    if supported.contains(&agent) {
        return Ok(());
    }

    let supported_list = supported
        .iter()
        .map(|h| h.slug())
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::bail!(
        "role \"{}\" does not support agent \"{}\"; supported: [{}]",
        selector.key(),
        agent.slug(),
        supported_list
    );
}

mod capsule_setup;
pub(crate) use jackin_runtime_launch_exit_diagnosis::exit_diagnosis;
pub(crate) use jackin_runtime_launch_git_pull::git_pull;
mod mounts;
pub(crate) use jackin_runtime_launch_progress_helpers::progress_helpers;
use progress_helpers::{
    LaunchEnvPrompter, StepCounter, launch_mount_lines, launch_target_kind, launch_target_label,
    sensitive_mount_prompt,
};

pub(crate) use jackin_runtime_launch_mounts::mounts::{
    Backend, agent_mounts, apple_agent_mounts, build_workspace_mount_strings,
    build_workspace_mounts, ensure_apple_provider_authority_not_exposed, github_config_mount,
    resolve_backend,
};

pub(crate) use capsule_setup::{
    capsule_config, capsule_config_contents, create_host_env_file, exec_binding_names,
    prepare_host_env_transport, prepare_socket_dir,
};

pub(crate) use exit_diagnosis::diagnose_with_state_by_id;

pub(crate) use exit_diagnosis::{
    ExitPhase, attach_failure_error, inspect_attach_outcome_by_id, is_known_socket_close,
};

pub(crate) use git_pull::{
    git_pull_sources, print_git_pull_results, pull_git_sources_with_git, record_git_pull_results,
};

mod failure;
pub(crate) use failure::{
    launch_failure_cli_error, launch_failure_title, render_exit, resolve_launch_role_source,
    short_launch_diagnosis,
};

pub(crate) use jackin_runtime_launch_plan::launch_plan::{
    LaunchPlan, emit_image_materialization_plan, emit_launch_plan,
};

pub(crate) use jackin_runtime_launch_load_cleanup::load_cleanup;
pub use load_cleanup::LoadCleanup;
pub(crate) use load_cleanup::write_if_changed_atomic;

mod restore_resolve;

#[cfg(test)]
mod tests;

pub(crate) use restore_resolve::{
    EarlyCurrentRestoreScan, RestoreResolution, UnselectedCurrentRestoreResolution,
    resolve_current_restore_candidate_timed, resolve_restore_candidate_reusing_early,
    resolve_unselected_current_restore_candidate_with_agent_timed,
};

mod launch_runtime;
pub(crate) use launch_runtime::{
    LaunchContext, SelectedImageRefresh, SiblingAuthPrewarm, SiblingPrewarm,
    SidecarPrewarmReplenish, await_sibling_auth_prewarm, launch_role_runtime,
    spawn_sibling_auth_prewarm,
};

mod restore;

use jackin_runtime_launch_attach_outcome::attach_outcome::write_instance_attach_outcome;
pub(in crate::runtime) use jackin_runtime_launch_attach_outcome::attach_outcome::{
    record_instance_attach_outcome, write_instance_status,
};
pub(in crate::runtime) use restore::preserved_instance_status;

use restore::{
    manifest_host_workdir_fingerprint, related_restore_load_options,
    write_preserved_status_if_applicable,
};

use auth_error::auth_token_source_reference;
pub(crate) use jackin_runtime_launch_auth_error::auth_error;
