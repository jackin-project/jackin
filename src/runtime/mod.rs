pub(crate) mod apple_container;
pub(crate) mod attach;
pub mod build_log;
mod caffeinate;
mod cleanup;
mod discovery;
mod exit_summary;
mod identity;
mod image;
mod launch;
pub mod logs;
mod naming;
pub mod progress;
mod repo_cache;
pub mod snapshot;
mod universe;

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
pub use self::test_support::FakeRunner;

pub(crate) use self::attach::docker_unavailable_msg;
pub use self::attach::{
    AgentSession, AgentSessionInventory, ContainerState, describe_agent_session_count,
    hardline_agent, hardline_agent_with_focus, inspect_agent_sessions, inspect_hardline_instance,
    spawn_agent_session, spawn_shell_session,
};
pub use self::caffeinate::reconcile as reconcile_keep_awake;
pub use self::cleanup::{
    eject_role, exile_all, prune_all_instances, prune_cache, prune_diagnostics, prune_images,
    prune_instances, prune_jackin_home, prune_roles, purge_class_data, purge_container_state,
};
pub(crate) use self::discovery::list_role_names;
pub use self::discovery::{
    list_managed_role_names, list_running_agent_display_names, list_running_agent_names,
};
pub use self::launch::{LoadOptions, load_role};
pub use self::naming::matching_family;
pub(crate) use self::repo_cache::{RepoError, normalize_github_url};
pub(crate) use self::universe::{
    EntryClaim, StartKind, claim_entry as claim_construct_entry, force_boundary_intro_enabled,
    release_entry_if_idle,
};

pub use self::launch::resolve_supported_agents_for_console;

/// Create the host-side socket directory for a container, write Capsule's
/// normalized launch config (`agent.toml`) into it, and lock it to `0o700`.
///
/// Both backends bind-mount this directory to `/jackin/run`; the capsule daemon
/// reads `agent.toml` from there at startup and refuses to start without it.
/// Sharing the routine keeps the Docker and apple-container paths from drifting
/// — the apple-container path previously created the dir but never wrote the
/// config, so its daemon could never come up.
pub(crate) fn prepare_socket_dir(
    socket_dir: &std::path::Path,
    capsule_config: &jackin_protocol::CapsuleConfig,
) -> anyhow::Result<()> {
    use anyhow::Context as _;
    let contents = toml::to_string(capsule_config)
        .context("serializing Capsule launch config for /jackin/run/agent.toml")?;
    std::fs::create_dir_all(socket_dir)
        .with_context(|| format!("creating host-side socket dir {}", socket_dir.display()))?;
    std::fs::write(
        socket_dir.join(jackin_protocol::CAPSULE_CONFIG_FILENAME),
        contents,
    )
    .with_context(|| format!("writing capsule config into {}", socket_dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("chmod 0o700 {}", socket_dir.display()))?;
    }
    Ok(())
}

pub(crate) async fn register_agent_repo(
    paths: &crate::paths::JackinPaths,
    selector: &crate::selector::RoleSelector,
    git_url: &str,
    runner: &mut impl crate::docker::CommandRunner,
    debug: bool,
) -> anyhow::Result<(crate::repo::CachedRepo, crate::repo::ValidatedRoleRepo)> {
    self::repo_cache::register_agent_repo(paths, selector, git_url, runner, debug).await
}
