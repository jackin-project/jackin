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

mod launch_dind;
pub use launch_dind::DIND_IMAGE;
pub(super) use launch_dind::create_role_network;
pub(crate) use launch_dind::prewarmed_dind_state_container_name;
pub use launch_dind::{
    DindSidecarPrewarm, prewarm_dind_sidecar_container, write_prewarmed_dind_state,
};
use launch_dind::{adopt_prewarmed_dind_sidecar, run_dind_sidecar_headless};
#[cfg(not(test))]
pub(crate) use launch_dind::{prewarmed_dind_state_is_live, try_lock_prewarmed_dind};

mod launch_slot;
#[cfg(test)]
pub(crate) use launch_slot::{
    claim_container_name, resolve_github_env_map, verify_credential_env_present,
    verify_github_token_present,
};

mod trust;
#[cfg(test)]
pub(crate) use trust::{
    MISE_TRUSTED_CONFIG_PATHS_ENV, inject_workspace_mise_env, seed_codex_project_trust,
    workspace_mise_trusted_config_paths,
};

mod launch_pipeline;
pub use launch_pipeline::launch_phases::{
    GrantPhaseInput, GrantsValidated, ImagePhaseClass, ImagePhaseClassified, classify_image_phase,
    cleanup_after_grant_failure, validate_launch_grants,
};

use super::discovery::list_running_agent_names;

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
pub(crate) use launch_pipeline::emit_auth_provision_launch_plan;
#[cfg(test)]
pub(crate) use launch_pipeline::load_role_with;
#[cfg(test)]
pub(crate) use launch_pipeline::manifest_env_timing_detail;
pub use launch_pipeline::{load_role, resolve_supported_agents_for_console};
#[cfg(test)]
use std::path::PathBuf;

#[expect(
    missing_debug_implementations,
    reason = "LoadOptions contains an injected OpRunner trait object that cannot expose Debug."
)]
#[derive(Default)]
pub struct LoadOptions {
    pub debug: bool,
    pub rebuild: bool,

    /// Bypass interactive preflight gates (e.g. dirty host repo).
    /// Wired through to `PreflightContext.force` during workspace
    /// materialization.
    pub force: bool,

    /// Optional test seam: inject a custom `OpRunner` for `op://`
    /// resolution. `None` (the production default) means
    /// `resolve_operator_env` picks the default `OpCli::new()`.
    pub op_runner: Option<Box<dyn jackin_env::OpRunner>>,

    /// Optional test seam: inject a host-env lookup map. `None` (the
    /// production default) means `resolve_operator_env` reads from
    /// `std::env::var`. When `Some(map)`, `$NAME` / `${NAME}`
    /// references are resolved by looking up `name` in `map`.
    pub host_env: Option<std::collections::BTreeMap<String, String>>,

    /// CLI override for the agent. `None` defers to (in order) workspace
    /// `default_agent`, the role's single supported agent, or a rich launch
    /// dialog. A launch against a multi-agent role with no resolved choice is
    /// an error when the rich dialog is unavailable.
    pub agent: Option<jackin_core::Agent>,

    /// When set, resolve this branch of the role repo instead of the default
    /// branch, build the image locally from the branch's Dockerfile (ignoring
    /// any `published_image`), and tag it with a branch-specific name so the
    /// stable image is not overwritten.
    pub role_branch: Option<String>,

    /// Docker security profile override for this launch.
    pub docker_profile: Option<crate::runtime::docker_profile::DockerSecurityProfile>,

    /// Exact missing instance to restore instead of scanning for candidates.
    pub restore_container_base: Option<String>,

    /// Role source URL captured in the instance manifest for restore paths.
    pub restore_role_source_git: Option<String>,
    /// Provider selected for the initial session (e.g. Z.AI's Anthropic
    /// redirect). When set, the first attach carries the provider's env
    /// overrides and label into the capsule's initial spawn.
    pub provider: Option<jackin_protocol::Provider>,

    /// Test seam for workspace `git pull` so fast-restore tests can prove the
    /// pull path did not run without mutating process-wide PATH.
    #[cfg(test)]
    pub git_program: Option<PathBuf>,
}

impl LoadOptions {
    pub fn initial_provider(&self) -> Option<jackin_protocol::InitialProvider> {
        // Label only: the daemon re-derives the env redirection from it and
        // backfills the token from the container's provider key env var.
        self.provider
            .map(|provider| jackin_protocol::InitialProvider {
                label: provider.label().to_owned(),
            })
    }

    /// Build options for `jackin load`.
    pub fn for_load(debug: bool, rebuild: bool) -> Self {
        Self {
            debug,
            rebuild,
            ..Self::default()
        }
    }

    /// Build options for the operator console (`jackin console`).
    pub fn for_launch(debug: bool) -> Self {
        Self {
            debug,
            ..Self::default()
        }
    }
}
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
mod exit_diagnosis;
mod git_pull;
mod mounts;
mod progress_helpers;
use progress_helpers::{
    LaunchEnvPrompter, StepCounter, launch_mount_lines, launch_target_kind, launch_target_label,
    sensitive_mount_prompt,
};

pub(crate) use mounts::{
    Backend, agent_mounts, build_workspace_mount_pairs, build_workspace_mount_strings,
    github_config_mount, resolve_backend,
};

pub(crate) use capsule_setup::{capsule_config, exec_binding_names, prepare_socket_dir};

#[cfg(test)]
pub(crate) use exit_diagnosis::{ExitPhase, diagnose_premature_exit};
pub(crate) use exit_diagnosis::{
    attach_failure_error, diagnose_with_state, inspect_attach_outcome,
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

mod launch_plan;
pub(crate) use launch_plan::{
    LaunchPlan, emit_image_materialization_plan, emit_launch_plan, emit_launch_plan_for_run,
    emit_prewarm_launch_plan, emit_rejected_launch_plan_for_run,
};

mod load_cleanup;
pub use load_cleanup::LoadCleanup;
pub(crate) use load_cleanup::write_if_changed_atomic;

mod restore_resolve;
pub(crate) use restore_resolve::{
    EarlyCurrentRestoreScan, RestoreResolution, UnselectedCurrentRestoreResolution,
    resolve_current_restore_candidate_timed, resolve_restore_candidate_reusing_early,
    resolve_unselected_current_restore_candidate_with_agent_timed,
};
#[cfg(test)]
pub(crate) use restore_resolve::{
    resolve_restore_candidate, resolve_unselected_current_restore_candidate_timed,
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
    SidecarPrewarmReplenish, host_runtime_passthrough_env, launch_role_runtime,
    spawn_sibling_auth_prewarm,
};

/// Present the stale-instance decision. "Start fresh" is always the
/// default first option; recoverable instances follow. The rich launch
/// surface renders it as a forced-choice picker (no cancel). The operator
/// must pick.
mod restore;
#[cfg(test)]
use restore::{
    RelatedRestoreCandidate, format_attach_outcome, recover_related_restore_candidate,
    restore_candidate_label, supersede_restore_candidates,
};
use restore::{
    manifest_host_workdir_fingerprint, related_restore_load_options, write_instance_attach_outcome,
    write_preserved_status_if_applicable,
};
pub(in crate::runtime) use restore::{
    preserved_instance_status, record_instance_attach_outcome, write_instance_status,
};

mod auth_error;
#[cfg(test)]
use auth_error::LaunchError;
#[cfg(not(test))]
use auth_error::LaunchError;
#[cfg(test)]
pub(crate) use auth_error::append_no_proxy_host;
use auth_error::{
    EnvLayerState, auth_token_source_reference, build_env_layer_states, build_mode_resolution,
};

#[cfg(test)]
mod tests;
