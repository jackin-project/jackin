// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule config and socket dir helpers extracted from launch coordinator.

use std::io::Write as _;
use std::path::{Component, Path};

use jackin_protocol;

const CAPSULE_LITERAL_SOURCE: &str = "literal";

pub(crate) fn account_auth_selections(
    config: &jackin_config::AppConfig,
    workspace_name: Option<&jackin_core::WorkspaceName>,
    role_key: &str,
    agents: &[jackin_core::Agent],
) -> anyhow::Result<
    std::collections::BTreeMap<
        jackin_core::Agent,
        (jackin_config::AuthForwardMode, Option<std::path::PathBuf>),
    >,
> {
    agents
        .iter()
        .copied()
        .map(|agent| {
            let account = jackin_config::resolve_account(config, agent, workspace_name, role_key)?;
            let selection =
                account.map_or((jackin_config::AuthForwardMode::Ignore, None), |account| {
                    (
                        account.auth_mode(),
                        account.source_directory().map(Path::to_path_buf),
                    )
                });
            Ok((agent, selection))
        })
        .collect()
}

pub(crate) fn capsule_auth_modes(
    config: &jackin_config::AppConfig,
    workspace_name: Option<&jackin_core::WorkspaceName>,
    role_key: &str,
    manifest: &jackin_manifest::RoleManifest,
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    Ok(account_auth_selections(
        config,
        workspace_name,
        role_key,
        &manifest.supported_agents(),
    )?
    .into_iter()
    .map(|(agent, (mode, _))| (agent.slug().to_owned(), mode.to_string()))
    .collect())
}

/// Account models must override role defaults: a role's native-provider model
/// may be invalid for the selected account's provider. Explicit launch options
/// are applied afterwards by the coordinator.
pub(crate) fn apply_account_models(
    launch: &mut jackin_protocol::CapsuleConfig,
    config: &jackin_config::AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    role: &str,
    agents: &[jackin_core::Agent],
) -> anyhow::Result<()> {
    for &agent in agents {
        let Some(account) = jackin_config::resolve_account(config, agent, workspace, role)? else {
            continue;
        };
        if let jackin_config::AccountCredential::ApiKey {
            model: Some(model), ..
        } = &account.credential
        {
            let model = if agent == jackin_core::Agent::Opencode {
                super::account_config::opencode_model(account.provider, model)?
            } else {
                model.clone()
            };
            launch.models.insert(agent.slug().to_owned(), model);
        }
    }
    Ok(())
}

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

/// Serialize the container-visible launch config without host-owned literal
/// credential values.
pub(crate) fn capsule_config_contents(
    config: &jackin_protocol::CapsuleConfig,
) -> Result<String, toml::ser::Error> {
    let mut projected = config.clone();
    for binding in &mut projected.exec_bindings {
        match binding.kind {
            jackin_protocol::ExecKind::Op | jackin_protocol::ExecKind::Env => {}
            jackin_protocol::ExecKind::Literal => {
                binding.source = CAPSULE_LITERAL_SOURCE.to_owned();
            }
        }
    }
    toml::to_string(&projected)
}

pub(crate) fn capsule_config(
    selector: &jackin_core::RoleSelector,
    workdir: &str,
    manifest: &jackin_manifest::RoleManifest,
    dirty_exit_policy: &str,
    isolated_worktrees: Vec<String>,
) -> jackin_protocol::CapsuleConfig {
    let mut agents = Vec::new();
    let mut models = std::collections::BTreeMap::new();
    for agent in manifest.supported_agents() {
        agents.push(agent.slug().to_owned());
        let model = manifest.agent_model(agent);
        if let Some(model) = model {
            models.insert(agent.slug().to_owned(), model.to_owned());
        }
    }
    jackin_protocol::CapsuleConfig {
        role: selector.key(),
        workdir: workdir.to_owned(),
        agents,
        models,
        auth_modes: std::collections::BTreeMap::new(),
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
/// alongside the extrausers passwd write). The directory is private before the
/// config write, including when no host credential listener will be started.
pub(crate) fn prepare_socket_dir(
    socket_dir: &Path,
    capsule_config_contents: &str,
) -> std::io::Result<()> {
    create_private_dir(socket_dir)?;
    std::fs::write(
        socket_dir.join(jackin_protocol::CAPSULE_CONFIG_FILENAME),
        capsule_config_contents,
    )
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};

        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700).create(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(path)
    }
}

/// A short-lived host-only env file removed when the runtime invocation ends.
pub(crate) struct HostEnvFile {
    file: tempfile::NamedTempFile,
}

impl HostEnvFile {
    #[must_use]
    pub(crate) fn path(&self) -> &Path {
        self.file.path()
    }
}

impl std::fmt::Debug for HostEnvFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("HostEnvFile(REDACTED_PATH)")
    }
}

/// Owns a temporary env file and the runtime arguments that reference it.
pub(crate) struct HostEnvTransport {
    _file: Option<HostEnvFile>,
    arguments: Vec<String>,
}

impl HostEnvTransport {
    pub(crate) fn append_arguments<'a>(&'a self, arguments: &mut Vec<&'a str>) {
        arguments.extend(self.arguments.iter().map(String::as_str));
    }
}

impl std::fmt::Debug for HostEnvTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("HostEnvTransport(REDACTED)")
    }
}

/// Write runtime environment values to a private host-only file.
///
/// The directory is a sibling of `sockets/`, never its child, so it is not part
/// of the `/jackin/run` bind mount. Values that cannot be represented exactly by
/// the runtime env-file grammar fail closed instead of falling back to argv.
pub(crate) fn create_host_env_file(
    jackin_home: &Path,
    container_name: &str,
    entries: &[(String, String)],
) -> std::io::Result<Option<HostEnvFile>> {
    if entries.is_empty() {
        return Ok(None);
    }
    if !matches!(
        Path::new(container_name)
            .components()
            .collect::<Vec<_>>()
            .as_slice(),
        [Component::Normal(_)]
    ) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "container name is not a single path component",
        ));
    }

    let contents = render_env_file(entries)?;
    let directory = jackin_home.join("runtime-env");
    create_private_dir(&directory)?;
    let mut file = tempfile::Builder::new()
        .prefix(&format!("{container_name}-"))
        .suffix(".env")
        .tempfile_in(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(contents.as_bytes())?;
    file.as_file().sync_all()?;
    Ok(Some(HostEnvFile { file }))
}

/// Split non-metadata env values from argv and prepare their private file.
pub(crate) fn prepare_host_env_transport(
    jackin_home: &Path,
    container_name: &str,
    arguments: &mut Vec<&str>,
) -> std::io::Result<HostEnvTransport> {
    let entries = extract_host_env_entries(arguments)?;
    let file = create_host_env_file(jackin_home, container_name, &entries)?;
    let runtime_arguments = match &file {
        Some(file) => vec![
            "--env-file".to_owned(),
            file.path()
                .to_str()
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "host runtime env path contains non-UTF-8 bytes",
                    )
                })?
                .to_owned(),
        ],
        None => Vec::new(),
    };
    Ok(HostEnvTransport {
        _file: file,
        arguments: runtime_arguments,
    })
}

fn render_env_file(entries: &[(String, String)]) -> std::io::Result<String> {
    let mut output = String::new();
    for (name, value) in entries {
        if name.is_empty()
            || name.contains(['=', '\n', '\r', '\0'])
            || value.contains(['\n', '\r', '\0'])
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "environment entry cannot be represented by env-file transport",
            ));
        }
        output.push_str(name);
        output.push('=');
        output.push_str(value);
        output.push('\n');
    }
    Ok(output)
}

/// Retain non-sensitive `JACKIN_*` metadata inline and remove every other env
/// value from container-runtime argv for host-only env-file transport.
pub(crate) fn extract_host_env_entries(
    args: &mut Vec<&str>,
) -> std::io::Result<Vec<(String, String)>> {
    let mut inline = Vec::with_capacity(args.len());
    let mut host_only = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let argument = args[index];
        if argument != "-e" {
            inline.push(argument);
            index += 1;
            continue;
        }
        let Some(entry) = args.get(index + 1).copied() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "container env flag is missing its value",
            ));
        };
        let Some((name, value)) = entry.split_once('=') else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "container env entry is not a name/value pair",
            ));
        };
        if name.starts_with("JACKIN_") {
            inline.extend([argument, entry]);
        } else {
            host_only.push((name.to_owned(), value.to_owned()));
        }
        index += 2;
    }
    *args = inline;
    Ok(host_only)
}
