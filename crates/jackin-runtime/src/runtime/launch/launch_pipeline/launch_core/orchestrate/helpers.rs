// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::WorkspaceName;

pub(super) const fn sidecar_replenish(
    adopted: bool,
) -> crate::runtime::launch::SidecarPrewarmReplenish {
    if adopted {
        crate::runtime::launch::SidecarPrewarmReplenish::AfterAttach
    } else {
        crate::runtime::launch::SidecarPrewarmReplenish::None
    }
}

pub(super) fn reuse_sentinel<'a>(
    selected_image_reused: bool,
    paths: &jackin_core::JackinPaths,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    image: &'a str,
    source: &'a jackin_config::RoleSource,
    branch_override: Option<&'a str>,
) -> Option<crate::runtime::launch::launch_runtime::ReuseStalenessSentinel<'a>> {
    (selected_image_reused
        && crate::runtime::image::reuse_needs_background_staleness_check(
            paths,
            validated_repo,
            image,
        ))
    .then_some(
        crate::runtime::launch::launch_runtime::ReuseStalenessSentinel {
            role_git: &source.git,
            branch_override,
            image,
        },
    )
}

pub(super) fn emit_auth_breadcrumbs(
    agent: jackin_core::Agent,
    auth_mode: jackin_config::AuthForwardMode,
    github_mode: jackin_config::GithubAuthMode,
    github_env_decls: &std::collections::BTreeMap<String, jackin_config::EnvValue>,
) {
    if let Some(run) = jackin_diagnostics::active_run() {
        run.compact("auth", &format!("{agent} auth resolved via {auth_mode}"));
        let token_key = jackin_core::GH_TOKEN_ENV_NAME;
        if matches!(github_mode, jackin_config::GithubAuthMode::Ignore) {
            run.compact("github_auth", "GitHub auth ignored by auth_forward=ignore");
        } else {
            let breadcrumb = github_env_decls.get(token_key).map_or_else(
                || token_key.to_owned(),
                |value| {
                    crate::runtime::launch::auth_token_source_reference(
                        token_key,
                        Some(value.as_display_str()),
                    )
                },
            );
            run.compact(
                "github_auth",
                &format!("resolved GitHub auth from {breadcrumb}"),
            );
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "launch config combines resolved role, workspace, auth, isolation, and policy state"
)]
pub(super) fn workspace_launch_config(
    config: &jackin_config::AppConfig,
    selector: &jackin_core::RoleSelector,
    workspace: &jackin_config::ResolvedWorkspace,
    workspace_name: Option<&WorkspaceName>,
    role_key: &str,
    agent: jackin_core::Agent,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    opts: &crate::runtime::launch::LoadOptions,
    materialized: &crate::isolation::materialize::MaterializedWorkspace,
    dirty_exit_policy: &str,
    exec_bindings: Vec<jackin_protocol::ExecBinding>,
    state: &crate::instance::RoleState,
) -> anyhow::Result<jackin_protocol::CapsuleConfig> {
    let instances =
        jackin_config::resolve_launch(config, workspace_name, role_key, None, Some(agent))?;
    let isolated_worktrees = materialized
        .mounts
        .iter()
        .filter(|mount| !mount.isolation.is_shared())
        .map(|mount| mount.dst.clone())
        .collect();
    let mut launch_config = crate::runtime::launch::capsule_config(
        selector,
        &workspace.workdir,
        &validated_repo.manifest,
        dirty_exit_policy,
        isolated_worktrees,
        &instances,
    );
    launch_config.auth_modes =
        crate::runtime::launch::capsule_setup::capsule_auth_modes(config, &instances)?;
    launch_config.exec_bindings = exec_bindings;
    crate::runtime::launch::capsule_setup::apply_account_models(
        &mut launch_config,
        config,
        &instances,
    )?;
    crate::runtime::launch::capsule_setup::apply_instance_dirs(
        &mut launch_config,
        &instances,
        &state.auth.slots,
    )?;
    // A per-launch model overrides the role manifest's `[<agent>].model` for
    // the agent this launch selected. The same value also travels as the
    // Codex role hook's config key, so the daemon that spawns the agent and
    // the hook that writes `$CODEX_HOME/config.toml` cannot disagree (D-078).
    if let (Some(model), Some(agent)) = (opts.model.as_deref(), opts.agent) {
        let model = model.trim();
        if !model.is_empty()
            && let Some(instance) = instances.iter().find(|i| i.agent == agent)
        {
            launch_config
                .models
                .insert(instance.config_id.clone(), model.to_owned());
        }
    }
    Ok(launch_config)
}

pub(super) struct ProvisionInputs {
    pub(super) instances: Vec<jackin_config::ResolvedInstance>,
    pub(super) credentials: jackin_protocol::AgentCredentialEnv,
}

pub(super) fn resolve_provision_inputs(
    config: &jackin_config::AppConfig,
    workspace: Option<&WorkspaceName>,
    role_key: &str,
    opts: &crate::runtime::launch::LoadOptions,
) -> anyhow::Result<ProvisionInputs> {
    let instances = jackin_config::resolve_launch(config, workspace, role_key, None, opts.agent)?;
    anyhow::ensure!(
        !instances.is_empty(),
        "no agent instances are admitted for role {role_key:?}"
    );
    let default_runner = jackin_env::OpCli::new();
    let credentials = jackin_env::resolve_instance_env_with(
        config,
        &instances,
        workspace,
        role_key,
        opts.op_runner.as_deref().unwrap_or(&default_runner),
        |name| match &opts.host_env {
            Some(env) => env.get(name).cloned().ok_or(std::env::VarError::NotPresent),
            None => std::env::var(name),
        },
    )?;
    Ok(ProvisionInputs {
        instances,
        credentials,
    })
}
