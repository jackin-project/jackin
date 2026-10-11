// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `LaunchRuntime` input and runtime dispatch.

use super::runtime_dispatch::{RuntimeDispatch, complete_docker_launch};
use crate::runtime::launch::launch_pipeline::launch_phases::{
    EnvironmentResolved, InstancePrepared, RuntimeLaunched, WorkspaceMaterialized,
};

use super::helpers::{reuse_sentinel, sidecar_replenish};
use jackin_core::CommandRunner;
use jackin_docker::docker_client::DockerApi;

use crate::instance::DockerResources;

use crate::runtime::docker_profile::{DockerSecurityProfile, EffectiveGrants, ProfileSource};

pub(crate) struct LaunchRuntime<'a, D, R> {
    pub(crate) paths: &'a jackin_core::JackinPaths,
    pub(crate) config: &'a jackin_config::AppConfig,
    pub(crate) selector: &'a jackin_core::RoleSelector,
    pub(crate) workspace: &'a jackin_config::ResolvedWorkspace,
    pub(crate) workspace_name: &'a Option<String>,
    pub(crate) docker: &'a D,
    pub(crate) runner: &'a mut R,
    pub(crate) opts: &'a crate::runtime::launch::LoadOptions,
    pub(crate) steps: &'a mut crate::runtime::launch::StepCounter,
    pub(crate) container_name: &'a str,
    pub(crate) role_key: &'a str,
    pub(crate) agent_display_name: &'a str,
    pub(crate) agent: jackin_core::Agent,
    pub(crate) source: &'a jackin_config::RoleSource,
    pub(crate) backend: crate::runtime::launch::Backend,
    pub(crate) validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    pub(crate) resolved_env: &'a jackin_env::ResolvedEnv,
    pub(crate) selected_refresh_reason: Option<crate::runtime::image::ImageInvalidationReason>,
    pub(crate) git: &'a crate::runtime::identity::GitIdentity,
    pub(crate) network: &'a str,
    pub(crate) dind: &'a str,
    pub(crate) certs_volume: &'a str,
    pub(crate) resolved_profile: (DockerSecurityProfile, ProfileSource),
    pub(crate) account_revision: crate::runtime::launch::account_identity::AccountConfigRevision,
    pub(crate) effective_grants: &'a EffectiveGrants,
    pub(crate) adopted_sidecar_was_used: bool,
    pub(crate) prepared: InstancePrepared,
    pub(crate) workspace_materialized: WorkspaceMaterialized,
    pub(crate) cleanup: crate::runtime::launch::LoadCleanup,
}

#[expect(
    clippy::too_many_lines,
    reason = "Docker dispatch arms each carry their own mount/cleanup/reconnect sequence; \
              the caller-bound lease checks are one line per arm and splitting the \
              dispatcher would scatter the linear launch/reconnect/cleanup flow."
)]
pub(crate) async fn launch_runtime<D, R>(
    input: LaunchRuntime<'_, D, R>,
) -> anyhow::Result<RuntimeDispatch>
where
    D: DockerApi,
    R: CommandRunner,
{
    let LaunchRuntime {
        paths,
        config,
        selector,
        workspace,
        workspace_name,
        docker,
        runner,
        opts,
        steps,
        container_name,
        role_key,
        agent_display_name,
        agent,
        source,
        backend,
        validated_repo,
        resolved_env,
        selected_refresh_reason,
        git,
        network,
        dind,
        certs_volume,
        resolved_profile,
        account_revision,
        effective_grants,
        adopted_sidecar_was_used,
        prepared:
            InstancePrepared {
                image,
                selected_image_reused,
                mut instance_manifest,
                container_state,
                host_workdir_fingerprint,
            },
        workspace_materialized:
            WorkspaceMaterialized {
                materialized,
                launch_config,
                environment:
                    EnvironmentResolved {
                        state,
                        github_resolved_env,
                        credential_scope,
                        workspace_name_str,
                        ..
                    },
            },
        cleanup,
    } = input;
    if backend == crate::runtime::launch::Backend::AppleContainer {
        let mut mounts = crate::runtime::launch::build_workspace_mounts(&materialized)?;
        mounts.extend(crate::runtime::launch::apple_agent_mounts(&state)?);
        cleanup.run(docker).await;
        account_revision.ensure_current(paths)?;
        crate::runtime::apple_container::launch(
            crate::runtime::apple_container::AppleContainerLaunch {
                paths,
                container_name,
                image: &image,
                workspace_name: workspace_name.as_deref(),
                workspace_label: workspace.label.as_str(),
                workdir: &workspace.workdir,
                role_key,
                role_display_name: agent_display_name,
                agent,
                role_source_git: &source.git,
                role_source_ref: opts.role_branch.as_deref(),
                image_tag: &image,
                env_pairs: &resolved_env.vars,
                mounts: &mounts,
                host_workdir_fingerprint: &host_workdir_fingerprint,
                capsule_config: &launch_config,
                state: &state,
                resolved_env,
                credential_scope: &credential_scope,
                debug: opts.debug,
                entry_claim: opts.entry_claim.as_deref(),
            },
        )
        .await?;
        return Ok(RuntimeDispatch::AppleContainer(container_name.to_owned()));
    }
    let reuse_staleness_sentinel = reuse_sentinel(
        selected_image_reused,
        paths,
        validated_repo,
        &image,
        source,
        opts.role_branch.as_deref(),
    );
    let role_handle_slot = cleanup.role_handle_slot();
    let ownership = crate::runtime::launch::launch_runtime::DockerLaunchOwnership {
        manifest: std::sync::Mutex::new(&mut instance_manifest),
        resources: DockerResources {
            role_container: container_name.to_owned(),
            dind_container: (adopted_sidecar_was_used
                || crate::runtime::docker_profile::dind_enabled(effective_grants))
            .then(|| dind.to_owned()),
            network: network.to_owned(),
            certs_volume: (adopted_sidecar_was_used
                || crate::runtime::docker_profile::dind_enabled(effective_grants))
            .then(|| certs_volume.to_owned()),
        },
        dind_handle_slot: cleanup.dind_handle_slot(),
        paths,
        state_dir: &container_state,
    };
    let ctx = crate::runtime::launch::LaunchContext {
        container_name,
        ownership: &ownership,
        role_handle_slot: &role_handle_slot,
        image: &image,
        network,
        dind,
        selector,
        agent_display_name,
        workspace: &materialized,
        state: &state,
        git,
        debug: opts.debug,
        git_coauthor_trailer: config.git.coauthor_trailer,
        git_dco: config.git.dco,
        agent,
        capsule_config: &launch_config,
        resolved_env,
        credential_scope: &credential_scope,
        github_env: &github_resolved_env,
        profile: resolved_profile.0,
        profile_source: resolved_profile.1,
        grants: effective_grants,
        paths,
        selected_image_refresh: selected_refresh_reason.map(|reason| {
            crate::runtime::launch::SelectedImageRefresh {
                role_git: &source.git,
                branch_override: opts.role_branch.as_deref(),
                reason,
            }
        }),
        reuse_staleness_sentinel,
        sidecar_prewarm_replenish: sidecar_replenish(adopted_sidecar_was_used),
        sibling_prewarm: crate::runtime::launch::SiblingPrewarm {
            role_git: &source.git,
            branch_override: opts.role_branch.as_deref(),
            validated_repo,
            selected_image_reused,
        },
        sibling_auth_prewarm: crate::runtime::launch::SiblingAuthPrewarm {
            manifest: &validated_repo.manifest,
            config,
            workspace_name: &workspace_name_str,
            role_key,
        },
        non_interactive: opts.non_interactive,
        account_revision: &account_revision,
        entry_claim: opts.entry_claim.as_deref(),
    };
    let launch_result =
        crate::runtime::launch::launch_role_runtime(&ctx, steps, docker, runner).await;
    drop(ctx);
    drop(ownership);
    complete_docker_launch(
        launch_result,
        RuntimeLaunched {
            instance_manifest,
            container_state,
            cleanup,
            account_revision,
        },
        paths,
        container_name,
        docker,
    )
    .await
}
