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
    claim_container_name, resolve_github_env_map, verify_github_token_present,
};

mod trust;
#[cfg(test)]
pub(crate) use trust::{
    MISE_TRUSTED_CONFIG_PATHS_ENV, inject_workspace_mise_env, seed_codex_project_trust,
    workspace_mise_trusted_config_paths,
};

mod image_plan;
pub use image_plan::{LaunchImagePlan, resolve_launch_image_plan};

mod programmatic;
pub use account_identity::{
    account_admission_matches, account_configuration_fingerprint, account_configuration_matches,
};
pub use programmatic::{
    CLAUDE_EFFORT_ENV, CLAUDE_MODEL_ENV, CODEX_LANE_EFFORT_ENV, CODEX_LANE_MODEL_ENV, IdentitySink,
    LaunchedInstance, LoadOptionsError, lane_agent_env, with_account_selection,
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
#[cfg(test)]
pub(crate) use launch_pipeline::load_role_with;
#[cfg(test)]
pub(crate) use launch_pipeline::manifest_env_timing_detail;
pub use launch_pipeline::{load_role, resolve_supported_agents_for_console};

#[expect(
    missing_debug_implementations,
    reason = "LoadOptions contains an injected OpRunner trait object that cannot expose Debug."
)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "LoadOptions is a caller-supplied options bag whose flags (debug, rebuild, force, non_interactive) are independent switches, not a state machine."
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
    /// Non-TTY programmatic launch: every decision the interactive path would
    /// prompt for is pre-supplied, no dialog may be drawn, and the launch does
    /// not attach a foreground session. A missing decision is a validation
    /// error (see `LoadOptions::validate_programmatic`), never a prompt.
    pub non_interactive: bool,

    /// Registered account ID selected for this launch. A workspace must allow
    /// the account; the override applies only to the selected agent.
    pub account: Option<String>,

    /// Exact model id for the launched agent, overriding the role manifest's
    /// `[<agent>].model`. Also passed to the in-container Codex role hook so
    /// the hook and the capsule daemon cannot disagree (D-078).
    pub model: Option<String>,

    /// Reasoning effort for the launched agent.
    pub effort: Option<jackin_core::ReasoningEffort>,

    /// Env values injected at launch on top of the resolved manifest and
    /// operator env. Reserved names are rejected by validation.
    pub env: std::collections::BTreeMap<String, String>,

    /// On-demand credential bindings the caller already approved. Merged with
    /// the bindings collected from config, so a daemon needs no interactive
    /// credential picker (D-082).
    pub on_demand_bindings: Vec<jackin_protocol::ExecBinding>,

    /// Extra bind mounts appended to the resolved workspace's mounts for this
    /// launch only, mirroring repeated `--mount` on the CLI.
    pub extra_mounts: Vec<jackin_config::MountConfig>,

    /// Initial prompt handed to the agent's first session.
    pub prompt: Option<String>,

    /// Slot the launch writes its claimed instance identity into.
    pub identity_sink: Option<IdentitySink>,

    /// Test seam for workspace `git pull` so fast-restore tests can prove the
    /// pull path did not run without mutating process-wide PATH.
    #[cfg(test)]
    pub git_program: Option<std::path::PathBuf>,
}

impl LoadOptions {
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
    Backend, agent_mounts, build_workspace_mount_strings, build_workspace_mounts,
    github_config_mount, resolve_backend,
};

#[cfg(test)]
pub(crate) use capsule_setup::extract_host_env_entries;
pub(crate) use capsule_setup::{
    capsule_config, capsule_config_contents, create_host_env_file, exec_binding_names,
    prepare_host_env_transport, prepare_socket_dir,
};

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
    SidecarPrewarmReplenish, host_runtime_passthrough_env, launch_role_runtime,
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
pub(crate) use auth_error::append_no_proxy_host;
use auth_error::auth_token_source_reference;

#[cfg(test)]
mod tests;
