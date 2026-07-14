// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Core launch pipeline: typed `#[must_use]` phase chain over `LaunchCore`.
//!
//! `run_launch_core` is a thin entry that delegates to [`orchestrate`] where the
//! ten phase tokens are produced and consumed by value.

mod orchestrate;

use jackin_config::AppConfig;
use jackin_core::CommandRunner;
use jackin_core::paths::JackinPaths;
use jackin_core::selector::RoleSelector;
use jackin_docker::docker_client::DockerApi;

pub(super) struct LaunchCore<'a, D, R>
where
    D: DockerApi,
    R: CommandRunner,
{
    pub paths: &'a JackinPaths,
    pub config: &'a mut AppConfig,
    pub selector: &'a RoleSelector,
    pub workspace: &'a jackin_config::ResolvedWorkspace,
    pub docker: &'a D,
    pub runner: &'a mut R,
    pub opts: &'a super::super::LoadOptions,

    pub git: crate::runtime::identity::GitIdentity,
    pub workspace_name: Option<String>,
    pub steps: &'a mut super::super::StepCounter,
    pub role_key: String,
    pub agent_display_name: String,
    pub agent: jackin_core::agent::Agent,
    pub supported_agents: Vec<jackin_core::agent::Agent>,
    pub cached_repo: jackin_manifest::repo::CachedRepo,
    pub validated_repo: jackin_manifest::repo::ValidatedRoleRepo,
    pub source: jackin_config::RoleSource,
    pub auth_mode: jackin_core::AuthForwardMode,
    pub backend: super::super::Backend,

    pub image_decision: crate::runtime::image::ImageDecision,
    pub repo_lock: Option<std::fs::File>,
    pub restoring: bool,
    pub container_name: String,
    pub exec_bindings: Vec<jackin_protocol::ExecBinding>,
    pub recipe_role_git_sha: Option<String>,
    pub recipe_base_image_ref: Option<String>,
    pub selected_refresh_reason: Option<crate::runtime::image::ImageInvalidationReason>,
    pub resolved_env: jackin_env::ResolvedEnv,

    pub rebuild: bool,
    #[expect(
        dead_code,
        reason = "deferred R4 launch-split field; read once a restore-pin consumer lands"
    )]
    pub restore_pinned_sha: Option<String>,
    pub operator_env: std::collections::BTreeMap<String, String>,
    pub git_pull_join: Option<super::DeferredGitPull>,
}

/// Typed phase chain for the `jackin load` critical path.
///
/// # Teardown `?`-path audit (plan 016)
/// Every fallible step between `LoadCleanup` arm and disarm either runs
/// `cleanup.run(docker)` before returning `Err`, or is covered by suite-A /
/// launch harness tests (grant, materialize, credential, role-state, trust,
/// sidecar, workspace, launch, post-success finalization).
pub(super) async fn run_launch_core<D, R>(ctx: LaunchCore<'_, D, R>) -> anyhow::Result<String>
where
    D: DockerApi,
    R: CommandRunner,
{
    orchestrate::run_launch_phases(ctx).await
}
