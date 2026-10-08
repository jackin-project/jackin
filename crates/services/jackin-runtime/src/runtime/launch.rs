// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `jackin load` pipeline: resolve source and trust, claim instance, build
//! image, prepare auth and mounts, launch runtime, attach, finalize.
//!
//! `load_role` is the public entry point; `load_role_with` is the pipeline
//! implementation. Key invariants:
//!
//! * Trust confirmation runs before the image build — an untrusted role may
//!   be cloned and resolved but not built until confirmed.
//! * Token-mode verification fails fast before auth state preparation or
//!   docker-in-docker launch, so a missing token never reaches container startup.
//! * Container slot claim runs before the launch summary is printed, so the
//!   name the operator sees is the final locked name that flows to the
//!   running container.
//! * Foreground-attach finalization runs before teardown classification —
//!   isolated worktrees are finalized before the preserve-vs-clean decision.
//! * `render_exit` is called on both success and error exits from
//!   `load_role_with`.

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
// `prewarmed_dind_state_container_name` had its hub re-export retired by S7
// split 93: the moved `dind_gc` module was its sole consumer and now
// names it through `jackin-runtime-launch-dind` directly.
pub use launch_dind::{
    DindSidecarPrewarm, prewarm_dind_sidecar_container_with_paths, write_prewarmed_dind_state,
};
use launch_dind::{adopt_prewarmed_dind_sidecar, run_dind_sidecar_headless};
// `prewarmed_dind_state_is_live` / `try_lock_prewarmed_dind` had their hub
// re-export retired by S7 split 94: the moved `prewarm_trigger` module was
// their sole consumer and now names them through
// `jackin-runtime-launch-dind` directly.

// Moved to jackin_runtime_launch_slot::launch_slot (S7
// split 77); the module re-export keeps every
// `launch::launch_slot::*` path stable.
pub(crate) use jackin_runtime_launch_slot::launch_slot;
#[cfg(test)]
pub(crate) use launch_slot::{
    claim_container_name, resolve_github_env_map, try_acquire_name_lock,
    verify_github_token_present,
};

mod trust;
#[cfg(test)]
pub(crate) use jackin_runtime_launch_trust::trust::{
    MISE_TRUSTED_CONFIG_PATHS_ENV, inject_workspace_mise_env, seed_codex_project_trust,
    workspace_mise_trusted_config_paths,
};

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

#[cfg(test)]
use crate::instance::InstanceStatus;
#[cfg(test)]
pub(crate) use crate::instance::{
    DockerResources, InstanceIndex, InstanceManifest, NewInstanceManifest, RoleState,
};
#[cfg(test)]
pub(crate) use crate::runtime::attach::ContainerState;
use jackin_core::RoleSelector;
#[cfg(test)]
pub(crate) use jackin_docker::docker_client::DockerApi;
#[cfg(test)]
pub(crate) use std::path::Path;

#[cfg(test)]
#[cfg(test)]
pub(crate) use launch_pipeline::load_role_with;
#[cfg(test)]
pub(crate) use launch_pipeline::manifest_env_timing_detail;
pub use launch_pipeline::{load_role, resolve_supported_agents_for_console};

// Moved to jackin_runtime_launch_load_options::load_options (S7 split 87);
// the item re-export keeps every `launch::LoadOptions` path stable.
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
// Moved to jackin_runtime_launch_exit_diagnosis::exit_diagnosis (S7 split
// 71); the module re-export keeps every `launch::exit_diagnosis::*` path
// stable.
pub(crate) use jackin_runtime_launch_exit_diagnosis::exit_diagnosis;
// Moved to jackin_runtime_launch_git_pull::git_pull (S7 split 69); the
// module re-export keeps every `launch::git_pull::*` path stable.
pub(crate) use jackin_runtime_launch_git_pull::git_pull;
mod mounts;
// Moved to jackin_runtime_launch_progress_helpers::progress_helpers (S7
// split 72); the module re-export keeps every
// `launch::progress_helpers::*` path stable.
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

#[cfg(test)]
pub(crate) use capsule_setup::extract_host_env_entries;
pub(crate) use capsule_setup::{
    capsule_config, capsule_config_contents, create_host_env_file, exec_binding_names,
    prepare_host_env_transport, prepare_socket_dir,
};

#[cfg(test)]
pub(crate) use exit_diagnosis::diagnose_premature_exit;
pub(crate) use exit_diagnosis::diagnose_with_state_by_id;
#[cfg(test)]
pub(crate) use exit_diagnosis::inspect_attach_outcome;
pub(crate) use exit_diagnosis::{
    ExitPhase, attach_failure_error, inspect_attach_outcome_by_id, is_known_socket_close,
};

#[cfg(test)]
pub(crate) use git_pull::pull_workspace_repos_with_git;
pub(crate) use git_pull::{
    git_pull_sources, print_git_pull_results, pull_git_sources_with_git, record_git_pull_results,
};

mod failure;
pub(crate) use failure::{
    launch_failure_cli_error, launch_failure_title, render_exit, resolve_launch_role_source,
    short_launch_diagnosis,
};

// Moved to jackin_runtime_launch_plan::launch_plan (S7 split 56); the
// re-export keeps the remaining `launch::LaunchPlan` / `launch::emit_*`
// paths stable. `emit_launch_plan_for_run` /
// `emit_rejected_launch_plan_for_run` lost their last hub user when
// `restore_resolve` moved out (S7 split 89); the leaf names them through
// `jackin_runtime_launch_plan::launch_plan` directly.
pub(crate) use jackin_runtime_launch_plan::launch_plan::{
    LaunchPlan, emit_image_materialization_plan, emit_launch_plan,
};
// `emit_prewarm_launch_plan` had its hub re-export retired by S7 split 92:
// the moved `launch_dind` module was its sole consumer and now names it
// through `jackin-runtime-launch-plan` directly.

// Moved to jackin_runtime_launch_load_cleanup::load_cleanup (S7 split 68);
// the module re-export keeps every `launch::load_cleanup::*` path stable.
pub(crate) use jackin_runtime_launch_load_cleanup::load_cleanup;
pub use load_cleanup::LoadCleanup;
pub(crate) use load_cleanup::write_if_changed_atomic;

mod restore_resolve;
#[cfg(test)]
pub(crate) use restore_resolve::resolve_restore_candidate;
pub(crate) use restore_resolve::{
    EarlyCurrentRestoreScan, RestoreResolution, UnselectedCurrentRestoreResolution,
    resolve_current_restore_candidate_timed, resolve_restore_candidate_reusing_early,
    resolve_unselected_current_restore_candidate_with_agent_timed,
};

mod launch_runtime;
#[cfg_attr(
    not(test),
    expect(
        unused_imports,
        reason = "re-export launch_runtime helpers for sibling modules and tests"
    )
)]
pub(crate) use launch_runtime::{
    LaunchContext, SelectedImageRefresh, SiblingAuthPrewarm, SiblingPrewarm,
    SidecarPrewarmReplenish, await_sibling_auth_prewarm, host_runtime_passthrough_env,
    launch_role_runtime, spawn_sibling_auth_prewarm,
};

/// Present the stale-instance decision. "Start fresh" is always the
/// default first option; recoverable instances follow. The rich launch
/// surface renders it as a forced-choice picker (no cancel). The operator
/// must pick.
mod restore;
#[cfg(test)]
use restore::{
    RelatedRestoreCandidate, recover_related_restore_candidate, restore_candidate_label,
    supersede_restore_candidates,
};
// Moved to jackin_runtime_launch_attach_outcome::attach_outcome (S7
// split 84); the item re-exports keep every `launch::*` path stable.
#[cfg(test)]
use jackin_runtime_launch_attach_outcome::attach_outcome::format_attach_outcome;
use jackin_runtime_launch_attach_outcome::attach_outcome::write_instance_attach_outcome;
pub(in crate::runtime) use jackin_runtime_launch_attach_outcome::attach_outcome::{
    record_instance_attach_outcome, write_instance_status,
};
pub(in crate::runtime) use restore::preserved_instance_status;
use restore::{
    manifest_host_workdir_fingerprint, related_restore_load_options,
    write_preserved_status_if_applicable,
};

// Moved to jackin_runtime_launch_auth_error::auth_error (S7 split 67); the
// module re-export keeps every `launch::auth_error::*` path stable.
#[cfg(test)]
pub(crate) use auth_error::append_no_proxy_host;
use auth_error::auth_token_source_reference;
pub(crate) use jackin_runtime_launch_auth_error::auth_error;

#[cfg(test)]
mod tests;
