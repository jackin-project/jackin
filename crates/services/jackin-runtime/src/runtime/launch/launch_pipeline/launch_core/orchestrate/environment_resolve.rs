// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Environment resolution and sibling auth prewarm.

use jackin_core::WorkspaceName;
use jackin_docker::docker_client::DockerApi;

use crate::runtime::launch::launch_slot::{
    github_env_declarations_for_mode, resolve_github_env_map, verify_github_token_present,
};

pub(crate) struct EnvironmentConfigured {
    pub(crate) workspace_name_str: String,
    pub(crate) workspace_opt: Option<WorkspaceName>,
    pub(crate) github_mode: jackin_config::GithubAuthMode,
    pub(crate) github_env_decls: std::collections::BTreeMap<String, jackin_config::EnvValue>,
    pub(crate) github_resolved_env: std::collections::BTreeMap<String, String>,
    pub(crate) github_ctx: crate::instance::GithubAuthContext,
}

pub(crate) struct ResolveEnvironment<'a, D> {
    pub(crate) config: &'a jackin_config::AppConfig,
    pub(crate) opts: &'a crate::runtime::launch::LoadOptions,
    pub(crate) role_key: &'a str,
    pub(crate) workspace_name: &'a Option<String>,
    pub(crate) cleanup: &'a crate::runtime::launch::LoadCleanup,
    pub(crate) docker: &'a D,
}

pub(crate) async fn resolve_environment<D: DockerApi>(
    input: ResolveEnvironment<'_, D>,
) -> anyhow::Result<EnvironmentConfigured> {
    let ResolveEnvironment {
        config,
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
        resolve_github_env_map(&required, opts.op_runner.as_deref(), opts.host_env.as_ref())
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
    let workspace_for_verify = workspace_opt
        .clone()
        .unwrap_or(WorkspaceName::parse("adhoc")?);
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

pub(crate) struct PrepareEnvironment<'a, D> {
    pub(crate) paths: &'a jackin_core::JackinPaths,
    pub(crate) config: &'a jackin_config::AppConfig,
    pub(crate) agent: jackin_core::Agent,
    pub(crate) container_name: &'a str,
    pub(crate) validated_repo: &'a jackin_manifest::repo::ValidatedRoleRepo,
    pub(crate) role_key: &'a str,
    pub(crate) workspace: &'a jackin_config::ResolvedWorkspace,
    pub(crate) steps: &'a mut crate::runtime::launch::StepCounter,
    pub(crate) cleanup: &'a crate::runtime::launch::LoadCleanup,
    pub(crate) docker: &'a D,
    pub(crate) configured: EnvironmentConfigured,
    pub(crate) opts: &'a crate::runtime::launch::LoadOptions,
}

pub(crate) async fn prewarm_sibling_auth_before_admission(
    paths: &jackin_core::JackinPaths,
    container_name: &str,
    manifest: &jackin_manifest::RoleManifest,
    config: &jackin_config::AppConfig,
    workspace_name: &str,
    role_key: &str,
    agent: jackin_core::Agent,
) -> anyhow::Result<()> {
    let prewarm = crate::runtime::launch::SiblingAuthPrewarm {
        manifest,
        config,
        workspace_name,
        role_key,
    };
    let prewarm =
        crate::runtime::launch::spawn_sibling_auth_prewarm(paths, container_name, &prewarm, agent);
    crate::runtime::launch::await_sibling_auth_prewarm(prewarm).await
}
