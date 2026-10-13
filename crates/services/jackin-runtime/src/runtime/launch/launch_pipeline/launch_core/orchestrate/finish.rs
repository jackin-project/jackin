// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Launch finish phase.

use super::runtime_dispatch::RuntimeDispatch;
use crate::runtime::launch::launch_pipeline::launch_phases::{CleanupClassified, RuntimeLaunched};

use jackin_core::CommandRunner;
use jackin_docker::docker_client::DockerApi;

use super::{
    ClassifyCleanup, FinalizeSession, LaunchInitialized, classify_cleanup, finalize_session,
};

pub(crate) struct FinishLaunch<'a, D, R> {
    pub(crate) paths: &'a jackin_core::JackinPaths,
    pub(crate) config: &'a jackin_config::AppConfig,
    pub(crate) workspace_name: &'a Option<String>,
    pub(crate) docker: &'a D,
    pub(crate) runner: &'a mut R,
    pub(crate) container_name: &'a str,
    pub(crate) launched: RuntimeDispatch,
}

pub(crate) async fn finish_launch<D, R>(input: FinishLaunch<'_, D, R>) -> anyhow::Result<String>
where
    D: DockerApi,
    R: CommandRunner,
{
    let FinishLaunch {
        paths,
        config,
        workspace_name,
        docker,
        runner,
        container_name,
        launched,
    } = input;
    let (
        RuntimeLaunched {
            mut instance_manifest,
            container_state,
            mut cleanup,
            account_revision,
        },
        container_handle,
    ) = match launched {
        RuntimeDispatch::AppleContainer(container_name)
        | RuntimeDispatch::Detached(container_name) => {
            return Ok(container_name);
        }
        RuntimeDispatch::Docker {
            launched,
            container,
        } => (*launched, container),
    };
    let finalized = finalize_session(FinalizeSession {
        paths,
        config,
        workspace_name,
        admission_lease: &account_revision,
        docker,
        runner,
        container_name,
        container: &container_handle,
        container_state: &container_state,
        instance_manifest: &mut instance_manifest,
        cleanup: &mut cleanup,
    })
    .await?;
    let CleanupClassified { container_name } = classify_cleanup(ClassifyCleanup {
        paths,
        docker,
        runner,
        container_name,
        container_state: &container_state,
        instance_manifest: &mut instance_manifest,
        cleanup: &mut cleanup,
        finalized,
        container: &container_handle,
    })
    .await?;
    Ok(container_name)
}

pub(crate) struct ActiveLaunch<'a, D, R> {
    pub(crate) paths: &'a jackin_core::JackinPaths,
    pub(crate) config: &'a jackin_config::AppConfig,
    pub(crate) selector: &'a jackin_core::RoleSelector,
    pub(crate) workspace: &'a jackin_config::ResolvedWorkspace,
    pub(crate) docker: &'a D,
    pub(crate) runner: &'a mut R,
    pub(crate) opts: &'a crate::runtime::launch::LoadOptions,
    pub(crate) git: crate::runtime::identity::GitIdentity,
    pub(crate) workspace_name: Option<String>,
    pub(crate) steps: &'a mut crate::runtime::launch::StepCounter,
    pub(crate) role_key: String,
    pub(crate) agent_display_name: String,
    pub(crate) agent: jackin_core::Agent,
    pub(crate) supported_agents: Vec<jackin_core::Agent>,
    pub(crate) cached_repo: jackin_manifest::repo::CachedRepo,
    pub(crate) validated_repo: jackin_manifest::repo::ValidatedRoleRepo,
    pub(crate) source: jackin_config::RoleSource,
    pub(crate) auth_mode: jackin_core::AuthForwardMode,
    pub(crate) backend: crate::runtime::launch::Backend,
    pub(crate) image_decision: Option<crate::runtime::image::ImageDecision>,
    pub(crate) repo_lock: Option<crate::runtime::repo_cache::RepoLock>,
    pub(crate) restoring: bool,
    pub(crate) container_name: String,
    pub(crate) exec_bindings: Vec<jackin_protocol::ExecBinding>,
    pub(crate) recipe_role_git_sha: Option<String>,
    pub(crate) recipe_base_image_ref: Option<String>,
    pub(crate) selected_refresh_reason: Option<crate::runtime::image::ImageInvalidationReason>,
    pub(crate) resolved_env: jackin_env::ResolvedEnv,
    pub(crate) rebuild: bool,
    pub(crate) git_pull_join: Option<crate::runtime::launch::launch_pipeline::DeferredGitPull>,
    pub(crate) account_revision: crate::runtime::launch::account_identity::AccountConfigRevision,
    pub(crate) admission_config: jackin_config::AppConfig,
    pub(crate) initialized: LaunchInitialized,
}
