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
    paths: &jackin_core::JackinPaths,
    agent: jackin_core::Agent,
    auth_mode: jackin_config::AuthForwardMode,
    workspace_opt: Option<&WorkspaceName>,
    github_mode: jackin_config::GithubAuthMode,
    github_env_decls: &std::collections::BTreeMap<String, jackin_config::EnvValue>,
) {
    if agent != jackin_core::Agent::Codex {
        let _expiry_days = workspace_opt
            .filter(|_| auth_mode == jackin_config::AuthForwardMode::OAuthToken)
            .and_then(
                |workspace| match jackin_env::expiry_days_for_launch(paths, workspace) {
                    Ok(days) => days,
                    Err(error) => {
                        if let Some(run) = jackin_diagnostics::active_run() {
                            run.compact(
                                "auth",
                                &format!(
                                    "token expiry cache for workspace {workspace} is unreadable \
                                 ({error}); re-run `jackin workspace claude-token setup \
                                 {workspace}` to refresh"
                                ),
                            );
                        }
                        None
                    }
                },
            );
    }
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

pub(super) fn workspace_launch_config(
    selector: &jackin_core::RoleSelector,
    workspace: &jackin_config::ResolvedWorkspace,
    validated_repo: &jackin_manifest::repo::ValidatedRoleRepo,
    opts: &crate::runtime::launch::LoadOptions,
    materialized: &crate::isolation::materialize::MaterializedWorkspace,
    dirty_exit_policy: &str,
    exec_bindings: Vec<jackin_protocol::ExecBinding>,
) -> jackin_protocol::CapsuleConfig {
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
        opts.initial_provider(),
        dirty_exit_policy,
        isolated_worktrees,
    );
    launch_config.exec_bindings = exec_bindings;
    launch_config
}
