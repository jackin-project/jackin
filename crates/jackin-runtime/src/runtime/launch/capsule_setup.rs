//! Capsule config and socket dir helpers extracted from launch coordinator.

use std::path::Path;

use jackin_protocol;

/// Comma-join the on-demand credential binding names for the
/// `JACKIN_EXEC_BINDINGS` env var. Shared by the Docker and apple-container
/// launch paths so the two cannot format the list differently.
#[must_use]
pub(crate) fn exec_binding_names(bindings: &[jackin_protocol::ExecBinding]) -> String {
    bindings
        .iter()
        .map(|b| b.name.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn capsule_config(
    selector: &jackin_core::selector::RoleSelector,
    workdir: &str,
    manifest: &jackin_manifest::RoleManifest,
    initial_provider: Option<jackin_protocol::InitialProvider>,
    dirty_exit_policy: &str,
    isolated_worktrees: Vec<String>,
) -> jackin_protocol::CapsuleConfig {
    let mut agents = Vec::new();
    let mut models = std::collections::BTreeMap::new();
    let mut provider_models = std::collections::BTreeMap::new();
    for agent in manifest.supported_agents() {
        agents.push(agent.slug().to_owned());
        let model = manifest.agent_model(agent);
        if let Some(model) = model {
            models.insert(agent.slug().to_owned(), model.to_owned());
        }
        let per_provider = manifest.agent_provider_models(agent);
        if !per_provider.is_empty() {
            let inner = per_provider
                .into_iter()
                .map(|(id, model)| (id.to_owned(), model.to_owned()))
                .collect();
            provider_models.insert(agent.slug().to_owned(), inner);
        }
    }
    jackin_protocol::CapsuleConfig {
        role: selector.key(),
        workdir: workdir.to_owned(),
        agents,
        models,
        provider_models,
        initial_provider,
        claude_marketplaces: Vec::new(),
        claude_plugins: Vec::new(),
        // Populated by the launch pipeline once the operator env is known; the
        // manifest alone does not carry on-demand workspace credentials.
        exec_bindings: Vec::new(),
        dirty_exit_policy: Some(dirty_exit_policy.to_owned()),
        isolated_worktrees,
    }
}

/// Create the per-container socket dir and write Capsule's launch config
/// (`agent.toml`) into it. The dir is bind-mounted to `/jackin/run`, so the
/// in-container capsule reads `agent.toml` at startup and the host.sock
/// credential-resolver socket lands beside it. Shared by both launch paths:
/// the apple-container path (`apple_container::launch`) and the Docker path
/// (`launch_role_runtime`, which calls it inside its socket-dir `spawn_blocking`
/// alongside the extrausers passwd write). The dir is created under the default
/// umask; it is tightened to `0o700` only when the `exec_host` listener binds
/// the socket, which happens only for workspaces that declare on-demand
/// credentials.
pub(crate) fn prepare_socket_dir(
    socket_dir: &Path,
    capsule_config_contents: &str,
) -> std::io::Result<()> {
    std::fs::create_dir_all(socket_dir)?;
    std::fs::write(
        socket_dir.join(jackin_protocol::CAPSULE_CONFIG_FILENAME),
        capsule_config_contents,
    )
}
