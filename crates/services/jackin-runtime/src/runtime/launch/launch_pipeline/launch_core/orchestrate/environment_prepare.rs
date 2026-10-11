// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Role state and environment preparation.

use crate::runtime::launch::launch_pipeline::emit_auth_provision_launch_plan;
use crate::runtime::launch::launch_pipeline::launch_phases::{EnvironmentResolved, TrustSeeded};

use super::helpers::resolve_provision_inputs;

use jackin_docker::docker_client::DockerApi;

use crate::instance::RoleState;
use crate::runtime::launch::trust::seed_codex_project_trust;
use std::future::Future;
use std::pin::Pin;

use super::{PrepareEnvironment, prewarm_sibling_auth_before_admission};

pub(crate) struct RoleStatePreparation {
    paths: jackin_core::JackinPaths,
    container_name: String,
    manifest: jackin_manifest::RoleManifest,
    config: jackin_config::AppConfig,
    github: crate::instance::GithubAuthContext,
    instances: Vec<jackin_config::ResolvedInstance>,
    credentials: jackin_protocol::AgentCredentialEnv,
    agent: jackin_core::Agent,
    model_override: Option<String>,
    effort: Option<jackin_core::ReasoningEffort>,
}

pub(crate) async fn prepare_role_state(
    input: RoleStatePreparation,
) -> anyhow::Result<(RoleState, crate::instance::AuthProvisionOutcome)> {
    let RoleStatePreparation {
        paths,
        container_name,
        manifest,
        config,
        github,
        instances,
        credentials,
        agent,
        model_override,
        effort,
    } = input;
    jackin_telemetry::spawn::joined_blocking(move || {
        let bindings =
            crate::runtime::launch::capsule_setup::instance_auth_bindings(&config, &instances)?;
        let mut prepared = RoleState::prepare_for_bindings(
            &paths,
            &container_name,
            &manifest,
            &bindings,
            &github,
            &paths.home_dir,
            agent,
        )?;
        crate::runtime::launch::account_identity::write_account_credentials(
            &prepared.0.root,
            &credentials,
        )?;
        let models = crate::runtime::launch::capsule_setup::resolved_instance_models(
            &config,
            &manifest,
            &instances,
            agent,
            model_override.as_deref(),
        )?;
        let efforts = crate::runtime::launch::capsule_setup::resolved_instance_efforts(
            &instances, agent, effort,
        );
        prepared.0.provider_config_mounts =
            crate::runtime::launch::account_config::configure_accounts(
                &prepared.0.root,
                &config,
                &instances,
                &prepared.0.auth.slots,
                &models,
                &efforts,
            )?;
        Ok(prepared)
    })
    .await
    .map_err(|error| anyhow::anyhow!("RoleState::prepare task panicked: {error}"))?
}

pub(crate) async fn prepare_environment<D, S>(
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
        opts,
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
    let github_ctx_owned = configured.github_ctx.clone();
    let model_override_owned = opts.model.clone();
    let effort_owned = opts.effort;
    let provision = resolve_provision_inputs(
        config,
        configured.workspace_opt.as_ref(),
        role_key,
        agent,
        opts,
    )?;
    let instances = provision.instances;
    let admitted = instances.clone();
    let credentials = provision.credentials;
    let credential_scope = crate::usage_relay::usage_credential_scope_for_staged_launch(
        config,
        &instances,
        &credentials,
    )?;
    // Auth prewarm mutates paths that the foreground launch may mount.
    // Complete it before RoleState acquires mount leases.
    if let Err(error) = prewarm_sibling_auth_before_admission(
        paths,
        container_name,
        &validated_repo.manifest,
        config,
        &configured.workspace_name_str,
        role_key,
        agent,
    )
    .await
    {
        cleanup.run(docker).await;
        return Err(error);
    }
    let role_state_future = prepare_role_state(RoleStatePreparation {
        paths: paths_owned,
        container_name: container_name_owned,
        manifest: manifest_owned,
        config: config_owned,
        github: github_ctx_owned,
        instances,
        credentials,
        agent,
        model_override: model_override_owned,
        effort: effort_owned,
    });
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
            credential_scope,
        },
        instances: admitted,
    })
}
