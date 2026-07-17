// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Phase chain body for `run_launch_core` (typed `#[must_use]` handoffs).

mod helpers;

use super::super::launch_phases::{
    CleanupClassified, EnvironmentResolved, GrantsValidated, ImageMaterialized,
    ImagePhaseClassified, InstancePrepared, RuntimeLaunched, SessionFinalized, TrustSeeded,
    WorkspaceMaterialized,
};
use super::super::{emit_auth_provision_launch_plan, purge_or_mark_clean_exited};
use super::LaunchCore;
use helpers::{emit_auth_breadcrumbs, reuse_sentinel, sidecar_replenish, workspace_launch_config};
use jackin_core::{CommandRunner, ContainerId, WorkspaceName};
use jackin_docker::docker_client::DockerApi;

use anyhow::Context;
use std::future::Future;
use std::pin::Pin;

use super::super::super::trust::seed_codex_project_trust;
use crate::instance::{
    DockerResources, InstanceManifest, InstanceStatus, NewInstanceManifest, PrepareResolvers,
    RoleState,
};
use crate::runtime::attach::{
    AgentSessionInventory, ContainerState, inspect_agent_sessions,
    start_or_reconnect_capsule_client,
};
use crate::runtime::docker_profile::{DockerSecurityProfile, EffectiveGrants, ProfileSource};

use super::super::super::launch_slot::{
    github_env_declarations_for_mode, resolve_github_env_map, verify_credential_env_present,
    verify_github_token_present,
};

async fn poll_sidecar_while<T, F, S>(
    work: F,
    mut sidecar: Pin<&mut S>,
    early_sidecar_result: &mut Option<anyhow::Result<()>>,
) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>>,
    S: Future<Output = anyhow::Result<()>>,
{
    if early_sidecar_result.is_some() {
        return work.await;
    }

    let mut work = std::pin::pin!(work);
    tokio::select! {
        biased;
        result = sidecar.as_mut() => {
            *early_sidecar_result = Some(result);
            work.await
        }
        result = &mut work => result,
    }
}

struct FinalizeSession<'a, D, R> {
    paths: &'a jackin_core::JackinPaths,
    config: &'a jackin_config::AppConfig,
    workspace_name: &'a Option<String>,
    docker: &'a D,
    runner: &'a mut R,
    container_name: &'a str,
    container_state: &'a std::path::Path,
    instance_manifest: &'a mut InstanceManifest,
    cleanup: &'a mut super::super::super::LoadCleanup,
}

async fn finalize_session<D, R>(
    input: FinalizeSession<'_, D, R>,
) -> anyhow::Result<SessionFinalized>
where
    D: DockerApi,
    R: CommandRunner,
{
    let FinalizeSession {
        paths,
        config,
        workspace_name,
        docker,
        runner,
        container_name,
        container_state,
        instance_manifest,
        cleanup,
    } = input;
    let finalize_result: anyhow::Result<crate::isolation::finalize::FinalizeDecision> = async {
        super::super::super::write_instance_status(
            paths,
            container_state,
            instance_manifest,
            InstanceStatus::Running,
        )?;
        let interactive_finalize = true;
        let mut prompt = crate::isolation::finalize::ExitActionPrompt {
            state_dir: paths.data_dir.join(container_name).join("state"),
        };
        let dirty_exit_policy = config.resolve_dirty_exit_policy(
            workspace_name
                .as_deref()
                .and_then(|name| config.workspaces.get(name)),
        );
        let outcome = super::super::super::inspect_attach_outcome(docker, container_name).await?;
        super::super::super::write_instance_attach_outcome(
            paths,
            container_state,
            instance_manifest,
            outcome,
        )?;
        let mut decision = crate::isolation::finalize::finalize_foreground_session(
            container_name,
            &paths.data_dir.join(container_name),
            outcome,
            interactive_finalize,
            dirty_exit_policy,
            &mut prompt,
            docker,
            runner,
        )
        .await?;
        super::super::super::write_preserved_status_if_applicable(
            decision,
            paths,
            container_state,
            instance_manifest,
        )?;
        if matches!(
            decision,
            crate::isolation::finalize::FinalizeDecision::ReturnToAgent
        ) {
            start_or_reconnect_capsule_client(paths, container_name, docker, runner).await?;
            let outcome =
                super::super::super::inspect_attach_outcome(docker, container_name).await?;
            super::super::super::write_instance_attach_outcome(
                paths,
                container_state,
                instance_manifest,
                outcome,
            )?;
            decision = crate::isolation::finalize::finalize_foreground_session(
                container_name,
                &paths.data_dir.join(container_name),
                outcome,
                interactive_finalize,
                dirty_exit_policy,
                &mut prompt,
                docker,
                runner,
            )
            .await?;
            super::super::super::write_preserved_status_if_applicable(
                decision,
                paths,
                container_state,
                instance_manifest,
            )?;
        }
        Ok(decision)
    }
    .await;
    match finalize_result {
        Ok(decision) => Ok(SessionFinalized { decision }),
        Err(error) => {
            cleanup.run(docker).await;
            Err(error)
        }
    }
}

struct ClassifyCleanup<'a, D, R> {
    paths: &'a jackin_core::JackinPaths,
    docker: &'a D,
    runner: &'a mut R,
    container_name: &'a str,
    container_state: &'a std::path::Path,
    instance_manifest: &'a mut InstanceManifest,
    cleanup: &'a mut super::super::super::LoadCleanup,
    finalized: SessionFinalized,
}

async fn classify_cleanup<D, R>(
    input: ClassifyCleanup<'_, D, R>,
) -> anyhow::Result<CleanupClassified>
where
    D: DockerApi,
    R: CommandRunner,
{
    let ClassifyCleanup {
        paths,
        docker,
        runner,
        container_name,
        container_state,
        instance_manifest,
        cleanup,
        finalized: SessionFinalized { decision },
    } = input;
    let is_preserved = matches!(
        decision,
        crate::isolation::finalize::FinalizeDecision::Preserved
    );
    let teardown_result: anyhow::Result<()> = async {
        match docker.inspect_container_state(container_name).await {
            ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
                if is_preserved {
                    let sessions =
                        inspect_agent_sessions(docker, container_name, &ContainerState::Running)
                            .await;
                    if let AgentSessionInventory::Unavailable(_) = sessions {
                        let _warning = jackin_telemetry::record_recovered_degradation();
                    }
                    if matches!(&sessions, AgentSessionInventory::Sessions(v) if v.is_empty()) {
                        super::super::super::write_instance_status(
                            paths,
                            container_state,
                            instance_manifest,
                            InstanceStatus::CleanExited,
                        )?;
                        cleanup.run(docker).await;
                    } else {
                        cleanup.disarm();
                    }
                } else {
                    super::super::super::write_instance_status(
                        paths,
                        container_state,
                        instance_manifest,
                        InstanceStatus::CleanExited,
                    )?;
                    cleanup.run(docker).await;
                }
            }
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } if is_preserved => cleanup.run(docker).await,
            ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            } => {
                cleanup.run(docker).await;
                purge_or_mark_clean_exited(
                    paths,
                    container_name,
                    container_state,
                    instance_manifest,
                    docker,
                    runner,
                )
                .await?;
            }
            ContainerState::Stopped { .. }
            | ContainerState::Created
            | ContainerState::Removing
            | ContainerState::Dead => {
                super::super::super::write_instance_status(
                    paths,
                    container_state,
                    instance_manifest,
                    InstanceStatus::Crashed,
                )?;
                cleanup.run(docker).await;
            }
            ContainerState::InspectUnavailable(reason) => {
                cleanup.disarm();
                anyhow::bail!(
                    "{}",
                    crate::runtime::attach::docker_unavailable_msg(
                        &format!("inspect container `{container_name}` after the session"),
                        &reason,
                    )
                );
            }
            ContainerState::NotFound if is_preserved => {
                cleanup.run(docker).await;
            }
            ContainerState::NotFound => {
                cleanup.run(docker).await;
                purge_or_mark_clean_exited(
                    paths,
                    container_name,
                    container_state,
                    instance_manifest,
                    docker,
                    runner,
                )
                .await?;
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = teardown_result {
        cleanup.run(docker).await;
        return Err(error);
    }
    Ok(CleanupClassified {
        container_name: container_name.to_owned(),
    })
}

struct PrepareInstance<'a, D> {
    paths: &'a jackin_core::JackinPaths,
    workspace: &'a jackin_config::ResolvedWorkspace,
    workspace_name: &'a Option<String>,
    container_name: &'a str,
    role_key: &'a str,
    agent_display_name: &'a str,
    agent: jackin_core::Agent,
    source: &'a jackin_config::RoleSource,
    opts: &'a super::super::super::LoadOptions,
    dind_started: bool,
    dind: &'a str,
    network: &'a str,
    certs_volume: &'a str,
    recipe_role_git_sha: Option<String>,
    recipe_base_image_ref: Option<String>,
    supported_agents: &'a [jackin_core::Agent],
    restoring: bool,
    docker: &'a D,
    cleanup: &'a super::super::super::LoadCleanup,
    image: ImageMaterialized,
}

async fn prepare_instance<D>(input: PrepareInstance<'_, D>) -> anyhow::Result<InstancePrepared>
where
    D: DockerApi,
{
    let PrepareInstance {
        paths,
        workspace,
        workspace_name,
        container_name,
        role_key,
        agent_display_name,
        agent,
        source,
        opts,
        dind_started,
        dind,
        network,
        certs_volume,
        recipe_role_git_sha,
        recipe_base_image_ref,
        supported_agents,
        restoring,
        docker,
        cleanup,
        image: ImageMaterialized {
            image,
            selected_image_reused,
        },
    } = input;
    let host_workdir_fingerprint =
        super::super::super::manifest_host_workdir_fingerprint(workspace);
    let new_manifest = InstanceManifest::new(NewInstanceManifest {
        container_base: container_name,
        workspace_name: workspace_name.as_deref(),
        workspace_label: workspace.label.as_str(),
        workdir: &workspace.workdir,
        host_workdir_fingerprint: &host_workdir_fingerprint,
        role_key,
        role_display_name: agent_display_name,
        agent_runtime: agent,
        role_source_git: &source.git,
        role_source_ref: opts.role_branch.as_deref(),
        image_tag: &image,
        docker: DockerResources {
            role_container: container_name.to_owned(),
            dind_container: dind_started.then(|| dind.to_owned()),
            network: network.to_owned(),
            certs_volume: dind_started.then(|| certs_volume.to_owned()),
        },
        role_git_sha: recipe_role_git_sha,
        base_image_ref: recipe_base_image_ref,
        base_image_digest: None,
        supported_agents: supported_agents.to_vec(),
    });
    let container_state = paths.data_dir.join(container_name);
    let mut instance_manifest = if restoring {
        match InstanceManifest::read_optional(&container_state).with_context(|| {
            format!(
                "restoring container `{container_name}`: existing manifest is unreadable; \
                 repair or remove the file, or run `jackin eject {container_name} --purge` to discard the recorded identity"
            )
        }) {
            Ok(Some(existing)) => existing,
            Ok(None) => new_manifest,
            Err(error) => {
                cleanup.run(docker).await;
                return Err(error);
            }
        }
    } else {
        new_manifest
    };
    if let Err(error) = super::super::super::write_instance_status(
        paths,
        &container_state,
        &mut instance_manifest,
        InstanceStatus::Active,
    ) {
        cleanup.run(docker).await;
        return Err(error);
    }
    Ok(InstancePrepared {
        image,
        selected_image_reused,
        instance_manifest,
        container_state,
        host_workdir_fingerprint,
    })
}

enum RuntimeDispatch {
    AppleContainer(String),
    Docker(Box<RuntimeLaunched>),
}

struct LaunchRuntime<'a, D, R> {
    paths: &'a jackin_core::JackinPaths,
    config: &'a jackin_config::AppConfig,
    selector: &'a jackin_core::RoleSelector,
    workspace: &'a jackin_config::ResolvedWorkspace,
    workspace_name: &'a Option<String>,
    docker: &'a D,
    runner: &'a mut R,
    opts: &'a super::super::super::LoadOptions,
    steps: &'a mut super::super::super::StepCounter,
    container_name: &'a str,
    role_key: &'a str,
    agent_display_name: &'a str,
    agent: jackin_core::Agent,
    source: &'a jackin_config::RoleSource,
    backend: super::super::super::Backend,
    validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    resolved_env: &'a jackin_env::ResolvedEnv,
    selected_refresh_reason: Option<crate::runtime::image::ImageInvalidationReason>,
    git: &'a crate::runtime::identity::GitIdentity,
    network: &'a str,
    dind: &'a str,
    resolved_profile: (DockerSecurityProfile, ProfileSource),
    effective_grants: &'a EffectiveGrants,
    adopted_sidecar_was_used: bool,
    prepared: InstancePrepared,
    workspace_materialized: WorkspaceMaterialized,
    cleanup: super::super::super::LoadCleanup,
}

async fn handle_launch_failure<D: DockerApi>(
    paths: &jackin_core::JackinPaths,
    container_state: &std::path::Path,
    instance_manifest: &mut InstanceManifest,
    container_name: &str,
    cleanup: &super::super::super::LoadCleanup,
    docker: &D,
) {
    if let Err(status_error) = super::super::super::write_instance_status(
        paths,
        container_state,
        instance_manifest,
        InstanceStatus::FailedSetup,
    ) && let Some(run) = jackin_diagnostics::active_run()
    {
        run.compact(
            "status",
            &format!(
                "jackin: warning: failed to mark FailedSetup for {container_name} \
                 after launch error: {status_error:#}; on-disk status may be stale"
            ),
        );
    }
    cleanup.run(docker).await;
}

struct MaterializeWorkspace<'a, D, R> {
    paths: &'a jackin_core::JackinPaths,
    config: &'a jackin_config::AppConfig,
    selector: &'a jackin_core::RoleSelector,
    workspace: &'a jackin_config::ResolvedWorkspace,
    docker: &'a D,
    runner: &'a mut R,
    opts: &'a super::super::super::LoadOptions,
    steps: &'a mut super::super::super::StepCounter,
    container_name: &'a str,
    role_key: &'a str,
    agent: jackin_core::Agent,
    auth_mode: jackin_config::AuthForwardMode,
    validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    exec_bindings: Vec<jackin_protocol::ExecBinding>,
    git_pull_join: Option<super::super::DeferredGitPull>,
    prepared: &'a mut InstancePrepared,
    cleanup: &'a super::super::super::LoadCleanup,
    trust: TrustSeeded,
}

struct EnvironmentConfigured {
    workspace_name_str: String,
    workspace_opt: Option<WorkspaceName>,
    github_mode: jackin_config::GithubAuthMode,
    github_env_decls: std::collections::BTreeMap<String, jackin_config::EnvValue>,
    github_resolved_env: std::collections::BTreeMap<String, String>,
    github_ctx: crate::instance::GithubAuthContext,
}

struct ResolveEnvironment<'a, D> {
    config: &'a jackin_config::AppConfig,
    agent: jackin_core::Agent,
    auth_mode: jackin_config::AuthForwardMode,
    operator_env: &'a std::collections::BTreeMap<String, String>,
    opts: &'a super::super::super::LoadOptions,
    role_key: &'a str,
    workspace_name: &'a Option<String>,
    cleanup: &'a super::super::super::LoadCleanup,
    docker: &'a D,
}

async fn resolve_environment<D: DockerApi>(
    input: ResolveEnvironment<'_, D>,
) -> anyhow::Result<EnvironmentConfigured> {
    let ResolveEnvironment {
        config,
        agent,
        auth_mode,
        operator_env,
        opts,
        role_key,
        workspace_name,
        cleanup,
        docker,
    } = input;
    let workspace_name_str = workspace_name.as_deref().unwrap_or("");
    let workspace_opt = if workspace_name_str.is_empty() {
        None
    } else {
        Some(WorkspaceName::parse(workspace_name_str).map_err(anyhow::Error::from)?)
    };
    let workspace_for_verify = match workspace_opt.as_ref() {
        Some(workspace) => workspace.clone(),
        None => WorkspaceName::parse("adhoc").map_err(anyhow::Error::from)?,
    };
    let mode_resolution =
        super::super::super::build_mode_resolution(config, agent, workspace_opt.as_ref(), role_key);
    let env_layers = agent
        .required_env_var(auth_mode)
        .map_or_else(Vec::new, |env_var| {
            super::super::super::build_env_layer_states(
                config,
                workspace_opt.as_ref(),
                role_key,
                env_var,
            )
        });
    if let Err(error) = verify_credential_env_present(
        agent,
        auth_mode,
        operator_env,
        &mode_resolution,
        &env_layers,
        &workspace_for_verify,
        role_key,
    ) {
        cleanup.run(docker).await;
        return Err(error.into());
    }
    let github_mode = jackin_config::resolve_github_mode(config, workspace_opt.as_ref(), role_key);
    let github_env_decls =
        jackin_config::build_github_env_layers(config, workspace_opt.as_ref(), role_key);
    let required = github_env_declarations_for_mode(&github_env_decls, github_mode);
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Credentials,
        "github_env",
        None,
    );
    let skipped = required.is_empty();
    let resolved = if skipped {
        Ok(std::collections::BTreeMap::new())
    } else {
        resolve_github_env_map(&required, opts)
    };
    let github_resolved_env = match resolved {
        Ok(env) => {
            let detail = if matches!(github_mode, jackin_config::GithubAuthMode::Ignore) {
                "skipped_ignore".to_owned()
            } else if skipped {
                "skipped_no_required_keys".to_owned()
            } else {
                format!("{} vars", env.len())
            };
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Credentials,
                "github_env",
                Some(&detail),
            );
            env
        }
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Credentials,
                "github_env",
                Some("error"),
            );
            cleanup.run(docker).await;
            return Err(error);
        }
    };
    let github_ctx = crate::instance::GithubAuthContext {
        mode: github_mode,
        token: github_resolved_env
            .get(jackin_core::GH_TOKEN_ENV_NAME)
            .cloned(),
    };
    if let Err(error) = verify_github_token_present(
        github_mode,
        github_ctx.token.as_deref(),
        &workspace_for_verify,
        role_key,
    ) {
        cleanup.run(docker).await;
        return Err(error);
    }
    Ok(EnvironmentConfigured {
        workspace_name_str: workspace_name_str.to_owned(),
        workspace_opt,
        github_mode,
        github_env_decls,
        github_resolved_env,
        github_ctx,
    })
}

struct PrepareEnvironment<'a, D> {
    paths: &'a jackin_core::JackinPaths,
    config: &'a jackin_config::AppConfig,
    agent: jackin_core::Agent,
    container_name: &'a str,
    validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    role_key: &'a str,
    workspace: &'a jackin_config::ResolvedWorkspace,
    steps: &'a mut super::super::super::StepCounter,
    cleanup: &'a super::super::super::LoadCleanup,
    docker: &'a D,
    configured: EnvironmentConfigured,
}

async fn prepare_environment<D, S>(
    input: PrepareEnvironment<'_, D>,
    mut sidecar: Pin<&mut S>,
    early_sidecar_result: &mut Option<anyhow::Result<()>>,
) -> anyhow::Result<TrustSeeded>
where
    D: DockerApi,
    S: Future<Output = anyhow::Result<()>>,
{
    let PrepareEnvironment {
        paths,
        config,
        agent,
        container_name,
        validated_repo,
        role_key,
        workspace,
        steps,
        cleanup,
        docker,
        configured,
    } = input;
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Credentials,
        "role_state_prepare",
        None,
    );
    let paths_owned = paths.clone();
    let container_name_owned = container_name.to_owned();
    let manifest_owned = validated_repo.manifest.clone();
    let config_owned = config.clone();
    let workspace_opt_owned = configured.workspace_opt.clone();
    let role_key_owned = role_key.to_owned();
    let github_ctx_owned = configured.github_ctx.clone();
    let role_state_future = async move {
        jackin_telemetry::spawn::joined_blocking(move || {
            let resolve_mode = |candidate| {
                jackin_config::resolve_mode(
                    &config_owned,
                    candidate,
                    workspace_opt_owned.as_ref(),
                    &role_key_owned,
                )
            };
            let resolve_sync_src = |candidate| {
                jackin_config::resolve_sync_source_dir(
                    &config_owned,
                    candidate,
                    workspace_opt_owned.as_ref(),
                    &role_key_owned,
                )
            };
            let provision_agents = manifest_owned.supported_agents();
            RoleState::prepare_for_agents(
                &paths_owned,
                &container_name_owned,
                &manifest_owned,
                &PrepareResolvers {
                    auth_modes: &resolve_mode,
                    sync_source_dirs: &resolve_sync_src,
                },
                &github_ctx_owned,
                &paths_owned.home_dir,
                agent,
                &provision_agents,
            )
        })
        .await
        .map_err(|error| anyhow::anyhow!("RoleState::prepare task panicked: {error}"))?
    };
    let mut role_state_future = std::pin::pin!(role_state_future);
    let select_role_state = async {
        if early_sidecar_result.is_some() {
            (&mut role_state_future).await
        } else {
            tokio::select! {
                result = sidecar.as_mut() => {
                    *early_sidecar_result = Some(result);
                    (&mut role_state_future).await
                }
                result = &mut role_state_future => result,
            }
        }
    };
    let role_state_result = if let Some(progress) = steps.progress_mut() {
        progress.while_waiting(select_role_state).await
    } else {
        select_role_state.await
    };
    let (state, _) = match role_state_result {
        Ok(prepared) => prepared,
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Credentials,
                "role_state_prepare",
                Some("error"),
            );
            cleanup.run(docker).await;
            return Err(error);
        }
    };
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Credentials,
        "role_state_prepare",
        Some("prepared"),
    );
    emit_auth_provision_launch_plan(&state, container_name);
    if let Err(error) = seed_codex_project_trust(&state, workspace) {
        cleanup.run(docker).await;
        return Err(error);
    }
    Ok(TrustSeeded {
        environment: EnvironmentResolved {
            state,
            github_resolved_env: configured.github_resolved_env,
            workspace_name_str: configured.workspace_name_str,
            workspace_opt: configured.workspace_opt,
            github_mode: configured.github_mode,
            github_env_decls: configured.github_env_decls,
        },
    })
}

struct MaterializeImage<'a, D, R> {
    paths: &'a jackin_core::JackinPaths,
    selector: &'a jackin_core::RoleSelector,
    cached_repo: &'a jackin_manifest::repo::CachedRepo,
    validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    agent: jackin_core::Agent,
    supported_agents: &'a [jackin_core::Agent],
    rebuild: bool,
    opts: &'a super::super::super::LoadOptions,
    steps: &'a mut super::super::super::StepCounter,
    docker: &'a D,
    runner: &'a mut R,
    restoring: bool,
    container_name: &'a str,
    repo_lock: &'a mut Option<std::fs::File>,
    cleanup: &'a super::super::super::LoadCleanup,
    classified: ImagePhaseClassified,
    decision: Option<crate::runtime::image::ImageDecision>,
}

struct BuildImage<'a, D, R> {
    common: MaterializeImage<'a, D, R>,
    reason: crate::runtime::image::ImageInvalidationReason,
    role_git_sha: Option<String>,
    base_image_override: Option<String>,
}

async fn build_image<D, R, S>(
    input: BuildImage<'_, D, R>,
    mut sidecar: Pin<&mut S>,
    early_sidecar_result: &mut Option<anyhow::Result<()>>,
) -> anyhow::Result<ImageMaterialized>
where
    D: DockerApi,
    R: CommandRunner,
    S: Future<Output = anyhow::Result<()>>,
{
    let BuildImage {
        common,
        reason,
        role_git_sha,
        base_image_override,
    } = input;
    super::super::super::emit_image_materialization_plan(
        false,
        reason.as_str(),
        common.restoring,
        common.container_name,
    );
    common.steps.next("Preparing runtime binaries").await?;
    let image_agents = common.supported_agents.to_vec();
    let binaries = poll_sidecar_while(
        async {
            crate::runtime::image::prepare_runtime_binaries_for_agents(
                common.paths,
                common.validated_repo,
                &image_agents,
                common.steps.progress_mut(),
            )
            .await
        },
        sidecar.as_mut(),
        early_sidecar_result,
    )
    .await;
    let binaries = match binaries {
        Ok(binaries) => binaries,
        Err(error) => {
            common.cleanup.run(common.docker).await;
            return Err(error);
        }
    };
    common.steps.next("Preparing derived image").await?;
    let Some(repo_lock) = common.repo_lock.take() else {
        common.cleanup.run(common.docker).await;
        return Err(anyhow::anyhow!("repo lock already consumed"));
    };
    let image = poll_sidecar_while(
        async {
            crate::runtime::image::build_agent_image(
                common.paths,
                common.selector,
                common.cached_repo,
                common.validated_repo,
                common.agent,
                binaries,
                common.rebuild,
                reason,
                base_image_override.as_deref(),
                common.opts.debug,
                common.opts.role_branch.as_deref(),
                common.docker,
                common.runner,
                repo_lock,
                role_git_sha.as_deref(),
                common.steps.progress_mut(),
            )
            .await
        },
        sidecar,
        early_sidecar_result,
    )
    .await;
    match image {
        Ok(image) => Ok(ImageMaterialized {
            image,
            selected_image_reused: false,
        }),
        Err(error) => {
            common.cleanup.run(common.docker).await;
            Err(error)
        }
    }
}

async fn materialize_image_phase<D, R, S>(
    mut input: MaterializeImage<'_, D, R>,
    sidecar: Pin<&mut S>,
    early_sidecar_result: &mut Option<anyhow::Result<()>>,
) -> anyhow::Result<ImageMaterialized>
where
    D: DockerApi,
    R: CommandRunner,
    S: Future<Output = anyhow::Result<()>>,
{
    let Some(decision) = input.decision.take() else {
        input.cleanup.run(input.docker).await;
        return Err(anyhow::anyhow!("image decision already consumed"));
    };
    match (input.classified.class, decision) {
        (
            super::super::launch_phases::ImagePhaseClass::ReuseOrBackgroundRefresh,
            decision @ (crate::runtime::image::ImageDecision::Reuse { .. }
            | crate::runtime::image::ImageDecision::RefreshInBackground { .. }),
        ) => {
            let (image, reason) = match decision {
                crate::runtime::image::ImageDecision::Reuse { image } => {
                    (image, "recipe_hash_match")
                }
                crate::runtime::image::ImageDecision::RefreshInBackground { image, reason } => {
                    (image, reason.as_str())
                }
                _ => unreachable!(),
            };
            super::super::super::emit_image_materialization_plan(
                true,
                reason,
                input.restoring,
                input.container_name,
            );
            drop(input.repo_lock.take());
            input.steps.stage_skipped(
                crate::runtime::progress::LaunchStage::AgentBinaries,
                "image reused",
            );
            input.steps.stage_done(
                crate::runtime::progress::LaunchStage::DerivedImage,
                "reused local image",
            );
            Ok(ImageMaterialized {
                image,
                selected_image_reused: true,
            })
        }
        (
            super::super::launch_phases::ImagePhaseClass::BuildRequired,
            crate::runtime::image::ImageDecision::BuildFromPublished {
                reason,
                role_git_sha,
                base_image,
            },
        ) => {
            build_image(
                BuildImage {
                    common: input,
                    reason,
                    role_git_sha,
                    base_image_override: Some(base_image),
                },
                sidecar,
                early_sidecar_result,
            )
            .await
        }
        (
            super::super::launch_phases::ImagePhaseClass::BuildRequired,
            crate::runtime::image::ImageDecision::BuildFromWorkspace {
                reason,
                role_git_sha,
            },
        ) => {
            build_image(
                BuildImage {
                    common: input,
                    reason,
                    role_git_sha,
                    base_image_override: None,
                },
                sidecar,
                early_sidecar_result,
            )
            .await
        }
        _ => {
            input.cleanup.run(input.docker).await;
            Err(anyhow::anyhow!(
                "internal: image phase class does not match ImageDecision variant"
            ))
        }
    }
}

struct InitializeLaunch<'a, D> {
    paths: &'a jackin_core::JackinPaths,
    config: &'a jackin_config::AppConfig,
    selector: &'a jackin_core::RoleSelector,
    workspace: &'a jackin_config::ResolvedWorkspace,
    docker: &'a D,
    opts: &'a super::super::super::LoadOptions,
    validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    image_decision: &'a crate::runtime::image::ImageDecision,
    container_name: &'a str,
}

struct LaunchInitialized {
    adopted_sidecar_was_used: bool,
    network: String,
    dind: String,
    certs_volume: String,
    cleanup: super::super::super::LoadCleanup,
    effective_grants: EffectiveGrants,
    resolved_profile: (DockerSecurityProfile, ProfileSource),
    dind_started: bool,
    image_phase: ImagePhaseClassified,
}

async fn initialize_launch<D: DockerApi>(
    input: InitializeLaunch<'_, D>,
) -> anyhow::Result<LaunchInitialized> {
    let InitializeLaunch {
        paths,
        config,
        selector,
        workspace,
        docker,
        opts,
        validated_repo,
        image_decision,
        container_name,
    } = input;
    let container_id = ContainerId::parse(container_name).context("validating container name")?;
    let adopted = super::super::super::adopt_prewarmed_dind_sidecar(paths, docker).await;
    let adopted_sidecar_was_used = adopted.is_some();
    let resources = adopted.as_ref().map_or_else(
        || DockerResources::from_container_id(&container_id),
        |sidecar| DockerResources {
            role_container: container_name.to_owned(),
            dind_container: Some(sidecar.sidecar.dind.clone()),
            network: sidecar.sidecar.network.clone(),
            certs_volume: Some(sidecar.sidecar.certs_volume.clone()),
        },
    );
    let network = resources.network;
    let dind = resources
        .dind_container
        .unwrap_or_else(|| crate::instance::naming::dind_container_name(container_name));
    let certs_volume = resources
        .certs_volume
        .unwrap_or_else(|| crate::instance::naming::dind_certs_volume(container_name));
    let cleanup = super::super::super::LoadCleanup::new(
        container_name.to_owned(),
        dind.clone(),
        certs_volume.clone(),
        network.clone(),
        paths.jackin_home.join("sockets").join(container_name),
    );
    let grants = super::super::launch_phases::validate_launch_grants(
        super::super::launch_phases::GrantPhaseInput {
            config,
            workspace_label: workspace.label.as_str(),
            workspace_docker: None,
            opts_docker_profile: opts.docker_profile,
            selector,
            role_manifest: &validated_repo.manifest,
        },
    );
    let GrantsValidated {
        effective_grants,
        resolved_profile,
        profile_source,
        dind_started,
    } = match grants {
        Ok(grants) => grants,
        Err(error) => {
            super::super::launch_phases::cleanup_after_grant_failure(&cleanup, docker).await;
            return Err(error);
        }
    };
    Ok(LaunchInitialized {
        adopted_sidecar_was_used,
        network,
        dind,
        certs_volume,
        cleanup,
        effective_grants,
        resolved_profile: (resolved_profile, profile_source),
        dind_started,
        image_phase: super::super::launch_phases::classify_image_phase(image_decision),
    })
}

struct FinishLaunch<'a, D, R> {
    paths: &'a jackin_core::JackinPaths,
    config: &'a jackin_config::AppConfig,
    workspace_name: &'a Option<String>,
    docker: &'a D,
    runner: &'a mut R,
    container_name: &'a str,
    launched: RuntimeDispatch,
}

async fn finish_launch<D, R>(input: FinishLaunch<'_, D, R>) -> anyhow::Result<String>
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
    let RuntimeLaunched {
        mut instance_manifest,
        container_state,
        mut cleanup,
    } = match launched {
        RuntimeDispatch::AppleContainer(container_name) => return Ok(container_name),
        RuntimeDispatch::Docker(launched) => *launched,
    };
    let finalized = finalize_session(FinalizeSession {
        paths,
        config,
        workspace_name,
        docker,
        runner,
        container_name,
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
    })
    .await?;
    Ok(container_name)
}

struct ActiveLaunch<'a, D, R> {
    paths: &'a jackin_core::JackinPaths,
    config: &'a jackin_config::AppConfig,
    selector: &'a jackin_core::RoleSelector,
    workspace: &'a jackin_config::ResolvedWorkspace,
    docker: &'a D,
    runner: &'a mut R,
    opts: &'a super::super::super::LoadOptions,
    git: crate::runtime::identity::GitIdentity,
    workspace_name: Option<String>,
    steps: &'a mut super::super::super::StepCounter,
    role_key: String,
    agent_display_name: String,
    agent: jackin_core::Agent,
    supported_agents: Vec<jackin_core::Agent>,
    cached_repo: jackin_manifest::repo::CachedRepo,
    validated_repo: jackin_manifest::repo::ValidatedRoleRepo,
    source: jackin_config::RoleSource,
    auth_mode: jackin_core::AuthForwardMode,
    backend: super::super::super::Backend,
    image_decision: Option<crate::runtime::image::ImageDecision>,
    repo_lock: Option<std::fs::File>,
    restoring: bool,
    container_name: String,
    exec_bindings: Vec<jackin_protocol::ExecBinding>,
    recipe_role_git_sha: Option<String>,
    recipe_base_image_ref: Option<String>,
    selected_refresh_reason: Option<crate::runtime::image::ImageInvalidationReason>,
    resolved_env: jackin_env::ResolvedEnv,
    rebuild: bool,
    operator_env: std::collections::BTreeMap<String, String>,
    git_pull_join: Option<super::super::DeferredGitPull>,
    initialized: LaunchInitialized,
}

async fn prepare_active_launch<D, R, S>(
    launch: &mut ActiveLaunch<'_, D, R>,
    mut sidecar: Pin<&mut S>,
    early_sidecar_result: &mut Option<anyhow::Result<()>>,
) -> anyhow::Result<(InstancePrepared, WorkspaceMaterialized)>
where
    D: DockerApi,
    R: CommandRunner,
    S: Future<Output = anyhow::Result<()>>,
{
    let Some(image_decision) = launch.image_decision.take() else {
        launch.initialized.cleanup.run(launch.docker).await;
        return Err(anyhow::anyhow!("image decision already consumed"));
    };
    let image = materialize_image_phase(
        MaterializeImage {
            paths: launch.paths,
            selector: launch.selector,
            cached_repo: &launch.cached_repo,
            validated_repo: &launch.validated_repo,
            agent: launch.agent,
            supported_agents: &launch.supported_agents,
            rebuild: launch.rebuild,
            opts: launch.opts,
            steps: launch.steps,
            docker: launch.docker,
            runner: launch.runner,
            restoring: launch.restoring,
            container_name: &launch.container_name,
            repo_lock: &mut launch.repo_lock,
            cleanup: &launch.initialized.cleanup,
            classified: launch.initialized.image_phase,
            decision: Some(image_decision),
        },
        sidecar.as_mut(),
        early_sidecar_result,
    )
    .await?;
    let mut prepared = prepare_instance(PrepareInstance {
        paths: launch.paths,
        workspace: launch.workspace,
        workspace_name: &launch.workspace_name,
        container_name: &launch.container_name,
        role_key: &launch.role_key,
        agent_display_name: &launch.agent_display_name,
        agent: launch.agent,
        source: &launch.source,
        opts: launch.opts,
        dind_started: launch.initialized.dind_started,
        dind: &launch.initialized.dind,
        network: &launch.initialized.network,
        certs_volume: &launch.initialized.certs_volume,
        recipe_role_git_sha: launch.recipe_role_git_sha.take(),
        recipe_base_image_ref: launch.recipe_base_image_ref.take(),
        supported_agents: &launch.supported_agents,
        restoring: launch.restoring,
        docker: launch.docker,
        cleanup: &launch.initialized.cleanup,
        image,
    })
    .await?;
    let configured = resolve_environment(ResolveEnvironment {
        config: launch.config,
        agent: launch.agent,
        auth_mode: launch.auth_mode,
        operator_env: &launch.operator_env,
        opts: launch.opts,
        role_key: &launch.role_key,
        workspace_name: &launch.workspace_name,
        cleanup: &launch.initialized.cleanup,
        docker: launch.docker,
    })
    .await?;
    let trust = prepare_environment(
        PrepareEnvironment {
            paths: launch.paths,
            config: launch.config,
            agent: launch.agent,
            container_name: &launch.container_name,
            validated_repo: &launch.validated_repo,
            role_key: &launch.role_key,
            workspace: launch.workspace,
            steps: launch.steps,
            cleanup: &launch.initialized.cleanup,
            docker: launch.docker,
            configured,
        },
        sidecar.as_mut(),
        early_sidecar_result,
    )
    .await?;
    let workspace = materialize_workspace_phase(
        MaterializeWorkspace {
            paths: launch.paths,
            config: launch.config,
            selector: launch.selector,
            workspace: launch.workspace,
            docker: launch.docker,
            runner: launch.runner,
            opts: launch.opts,
            steps: launch.steps,
            container_name: &launch.container_name,
            role_key: &launch.role_key,
            agent: launch.agent,
            auth_mode: launch.auth_mode,
            validated_repo: &launch.validated_repo,
            exec_bindings: std::mem::take(&mut launch.exec_bindings),
            git_pull_join: launch.git_pull_join.take(),
            prepared: &mut prepared,
            cleanup: &launch.initialized.cleanup,
            trust,
        },
        sidecar,
        early_sidecar_result.take(),
    )
    .await?;
    Ok((prepared, workspace))
}

async fn execute_active_launch<D, R>(
    launch: ActiveLaunch<'_, D, R>,
    prepared: InstancePrepared,
    workspace_materialized: WorkspaceMaterialized,
) -> anyhow::Result<String>
where
    D: DockerApi,
    R: CommandRunner,
{
    let launched = launch_runtime(LaunchRuntime {
        paths: launch.paths,
        config: launch.config,
        selector: launch.selector,
        workspace: launch.workspace,
        workspace_name: &launch.workspace_name,
        docker: launch.docker,
        runner: launch.runner,
        opts: launch.opts,
        steps: launch.steps,
        container_name: &launch.container_name,
        role_key: &launch.role_key,
        agent_display_name: &launch.agent_display_name,
        agent: launch.agent,
        source: &launch.source,
        backend: launch.backend,
        validated_repo: &launch.validated_repo,
        resolved_env: &launch.resolved_env,
        selected_refresh_reason: launch.selected_refresh_reason,
        git: &launch.git,
        network: &launch.initialized.network,
        dind: &launch.initialized.dind,
        resolved_profile: launch.initialized.resolved_profile,
        effective_grants: &launch.initialized.effective_grants,
        adopted_sidecar_was_used: launch.initialized.adopted_sidecar_was_used,
        prepared,
        workspace_materialized,
        cleanup: launch.initialized.cleanup,
    })
    .await?;
    finish_launch(FinishLaunch {
        paths: launch.paths,
        config: launch.config,
        workspace_name: &launch.workspace_name,
        docker: launch.docker,
        runner: launch.runner,
        container_name: &launch.container_name,
        launched,
    })
    .await
}

async fn run_active_launch<D, R, S>(
    mut launch: ActiveLaunch<'_, D, R>,
    sidecar: Pin<&mut S>,
) -> anyhow::Result<String>
where
    D: DockerApi,
    R: CommandRunner,
    S: Future<Output = anyhow::Result<()>>,
{
    let mut early_sidecar_result = None;
    let (prepared, workspace) =
        prepare_active_launch(&mut launch, sidecar, &mut early_sidecar_result).await?;
    execute_active_launch(launch, prepared, workspace).await
}

async fn materialize_workspace_phase<D, R, S>(
    input: MaterializeWorkspace<'_, D, R>,
    mut sidecar: Pin<&mut S>,
    early_sidecar_result: Option<anyhow::Result<()>>,
) -> anyhow::Result<WorkspaceMaterialized>
where
    D: DockerApi,
    R: CommandRunner,
    S: Future<Output = anyhow::Result<()>>,
{
    let MaterializeWorkspace {
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        opts,
        steps,
        container_name,
        role_key,
        agent,
        auth_mode,
        validated_repo,
        exec_bindings,
        git_pull_join,
        prepared,
        cleanup,
        trust: TrustSeeded { environment },
    } = input;
    emit_auth_breadcrumbs(
        paths,
        agent,
        auth_mode,
        environment.workspace_opt.as_ref(),
        environment.github_mode,
        &environment.github_env_decls,
    );
    let workspace_label = workspace
        .as_workspace_label()
        .map_err(anyhow::Error::from)?;
    if let Some(git_pull_join) = git_pull_join {
        super::super::finish_deferred_git_pull(git_pull_join, steps).await?;
    }
    steps.stage_started(
        crate::runtime::progress::LaunchStage::Workspace,
        "materializing workspace",
    );
    let preflight = crate::isolation::materialize::PreflightContext {
        workspace_label: workspace_label.clone(),
        force: opts.force,
        interactive: true,
    };
    let materialize = crate::isolation::materialize::materialize_workspace(
        workspace,
        &prepared.container_state,
        role_key,
        container_name,
        &workspace_label,
        &preflight,
        runner,
    );
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Workspace,
        "materialize_workspace",
        None,
    );
    let materialize_wait = async {
        if let Some(progress) = steps.progress_mut() {
            progress.while_waiting(materialize).await
        } else {
            materialize.await
        }
    };
    let sidecar_wait = async {
        if let Some(result) = early_sidecar_result {
            result
        } else {
            sidecar.as_mut().await
        }
    };
    let (sidecar_result, materialize_result) = tokio::join!(sidecar_wait, materialize_wait);
    steps.stage_done(crate::runtime::progress::LaunchStage::Network, "isolated");
    if let Err(error) = sidecar_result {
        super::super::launch_phases::mark_failed_setup_then_cleanup(
            paths,
            &prepared.container_state,
            container_name,
            &mut prepared.instance_manifest,
            cleanup,
            docker,
            "sidecar error",
        )
        .await;
        return Err(error);
    }
    let materialized = match materialize_result {
        Ok(materialized) => materialized,
        Err(error) => {
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Workspace,
                "materialize_workspace",
                Some("error"),
            );
            super::super::launch_phases::mark_failed_setup_then_cleanup(
                paths,
                &prepared.container_state,
                container_name,
                &mut prepared.instance_manifest,
                cleanup,
                docker,
                "workspace materialization error",
            )
            .await;
            return Err(error);
        }
    };
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Workspace,
        "materialize_workspace",
        Some("materialized"),
    );
    steps.stage_done(
        crate::runtime::progress::LaunchStage::Workspace,
        "materialized",
    );
    let dirty_exit_policy =
        config.resolve_dirty_exit_policy(config.workspaces.get(workspace_label.as_str()));
    let launch_config = workspace_launch_config(
        config,
        selector,
        workspace,
        environment.workspace_opt.as_ref(),
        role_key,
        validated_repo,
        opts,
        &materialized,
        dirty_exit_policy.as_str(),
        exec_bindings,
    );
    Ok(WorkspaceMaterialized {
        materialized,
        launch_config,
        environment,
    })
}

async fn launch_runtime<D, R>(input: LaunchRuntime<'_, D, R>) -> anyhow::Result<RuntimeDispatch>
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
        resolved_profile,
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
                        workspace_name_str,
                        ..
                    },
            },
        mut cleanup,
    } = input;
    match backend {
        super::super::super::Backend::Docker => {}
        super::super::super::Backend::AppleContainer => {
            cleanup.run(docker).await;
            let mount_pairs = super::super::super::build_workspace_mount_pairs(&materialized);
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
                    mount_pairs: &mount_pairs,
                    host_workdir_fingerprint: &host_workdir_fingerprint,
                    capsule_config: &launch_config,
                    debug: opts.debug,
                },
            )
            .await?;
            return Ok(RuntimeDispatch::AppleContainer(container_name.to_owned()));
        }
    }
    let reuse_staleness_sentinel = reuse_sentinel(
        selected_image_reused,
        paths,
        validated_repo,
        &image,
        source,
        opts.role_branch.as_deref(),
    );
    let ctx = super::super::super::LaunchContext {
        container_name,
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
        github_env: &github_resolved_env,
        profile: resolved_profile.0,
        profile_source: resolved_profile.1,
        grants: effective_grants,
        paths,
        selected_image_refresh: selected_refresh_reason.map(|reason| {
            super::super::super::SelectedImageRefresh {
                role_git: &source.git,
                branch_override: opts.role_branch.as_deref(),
                reason,
            }
        }),
        reuse_staleness_sentinel,
        sidecar_prewarm_replenish: sidecar_replenish(adopted_sidecar_was_used),
        sibling_prewarm: super::super::super::SiblingPrewarm {
            role_git: &source.git,
            branch_override: opts.role_branch.as_deref(),
            validated_repo,
            selected_image_reused,
        },
        sibling_auth_prewarm: super::super::super::SiblingAuthPrewarm {
            manifest: &validated_repo.manifest,
            config,
            workspace_name: &workspace_name_str,
            role_key,
        },
    };
    let launch_result = super::super::super::launch_role_runtime(&ctx, steps, docker, runner).await;
    if launch_result.is_err() {
        handle_launch_failure(
            paths,
            &container_state,
            &mut instance_manifest,
            container_name,
            &cleanup,
            docker,
        )
        .await;
    }
    launch_result?;
    cleanup.keep_socket_dir();
    Ok(RuntimeDispatch::Docker(Box::new(RuntimeLaunched {
        instance_manifest,
        container_state,
        cleanup,
    })))
}

pub(super) async fn run_launch_phases<D, R>(ctx: LaunchCore<'_, D, R>) -> anyhow::Result<String>
where
    D: DockerApi,
    R: CommandRunner,
{
    // Destructure captured names so verbatim original block statements work.
    let LaunchCore {
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        opts,
        git,
        workspace_name,
        steps,
        role_key,
        agent_display_name,
        agent,
        supported_agents,
        cached_repo,
        validated_repo,
        source,
        auth_mode,
        backend,
        image_decision,
        repo_lock,
        restoring,
        container_name,
        exec_bindings,
        recipe_role_git_sha,
        recipe_base_image_ref,
        selected_refresh_reason,
        resolved_env,
        rebuild,
        restore_pinned_sha: _,
        operator_env,
        git_pull_join,
        ..
    } = ctx;
    let initialized = initialize_launch(InitializeLaunch {
        paths,
        config,
        selector,
        workspace,
        docker,
        opts,
        validated_repo: &validated_repo,
        image_decision: &image_decision,
        container_name: &container_name,
    })
    .await?;
    let launch = ActiveLaunch {
        paths,
        config,
        selector,
        workspace,
        docker,
        runner,
        opts,
        git,
        workspace_name,
        steps,
        role_key,
        agent_display_name,
        agent,
        supported_agents,
        cached_repo,
        validated_repo,
        source,
        auth_mode,
        backend,
        image_decision: Some(image_decision),
        repo_lock,
        restoring,
        container_name,
        exec_bindings,
        recipe_role_git_sha,
        recipe_base_image_ref,
        selected_refresh_reason,
        resolved_env,
        rebuild,
        operator_env,
        git_pull_join,
        initialized,
    };
    // Start the sidecar future before image materialization so network/DinD
    // setup can make progress while runtime binaries and Docker build run.
    launch.steps.stage_started(
        crate::runtime::progress::LaunchStage::Network,
        "wiring private network",
    );
    let sidecar_container = launch.container_name.clone();
    let sidecar_network = launch.initialized.network.clone();
    let sidecar_dind = launch.initialized.dind.clone();
    let sidecar_certs_volume = launch.initialized.certs_volume.clone();
    let sidecar_dind_grant = launch.initialized.effective_grants.dind;
    let sidecar_network_disabled =
        crate::runtime::docker_profile::network_disabled(&launch.initialized.effective_grants);
    let role_network_internal = crate::runtime::docker_profile::role_network_internal(
        launch.initialized.resolved_profile.0,
    );
    let adopted_sidecar_was_used = launch.initialized.adopted_sidecar_was_used;
    let dind_started = launch.initialized.dind_started;
    let docker = launch.docker;
    let sidecar = async move {
        if adopted_sidecar_was_used {
            Ok(())
        } else if dind_started {
            super::super::super::run_dind_sidecar_headless(
                &sidecar_container,
                &sidecar_network,
                &sidecar_dind,
                &sidecar_certs_volume,
                sidecar_dind_grant,
                docker,
            )
            .await
        } else if sidecar_network_disabled {
            Ok(())
        } else {
            super::super::super::create_role_network(
                &sidecar_container,
                &sidecar_network,
                role_network_internal,
                docker,
            )
            .await
        }
    };
    let mut sidecar = std::pin::pin!(sidecar);
    run_active_launch(launch, sidecar.as_mut()).await
}
