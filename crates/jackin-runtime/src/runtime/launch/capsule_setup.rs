// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule config and socket dir helpers extracted from launch coordinator.

use std::io::Write as _;
use std::path::{Component, Path};

use jackin_protocol;

const CAPSULE_LITERAL_SOURCE: &str = "literal";

/// Auth transport per admitted instance, keyed by config id.
///
/// Instances are already authorized by [`jackin_config::resolve_launch`};
/// each entry carries its account's forward mode plus the profile source
/// directory for `sync` transports.
pub(crate) fn account_auth_selections(
    config: &jackin_config::AppConfig,
    instances: &[jackin_config::ResolvedInstance],
) -> anyhow::Result<
    std::collections::BTreeMap<
        String,
        (jackin_config::AuthForwardMode, Option<std::path::PathBuf>),
    >,
> {
    instances
        .iter()
        .map(|instance| {
            let account = config
                .accounts
                .get(&instance.account_id)
                .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
            Ok((
                instance.config_id.clone(),
                (
                    account.auth_mode(),
                    account.source_directory().map(Path::to_path_buf),
                ),
            ))
        })
        .collect()
}

/// One [`InstanceAuthBinding`](crate::instance::InstanceAuthBinding)
/// per admitted instance, keyed by config ID in launch order. Fails on
/// the first unknown account; every returned binding resolves.
pub(crate) fn instance_auth_bindings(
    config: &jackin_config::AppConfig,
    instances: &[jackin_config::ResolvedInstance],
) -> anyhow::Result<Vec<crate::instance::InstanceAuthBinding>> {
    instances
        .iter()
        .map(|instance| {
            let account = config
                .accounts
                .get(&instance.account_id)
                .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
            let mut binding = crate::instance::InstanceAuthBinding::new(
                instance.account_id.clone(),
                instance.agent,
                account.auth_mode(),
                account.source_directory().map(Path::to_path_buf),
            );
            binding.key = instance.config_id.clone();
            Ok(binding)
        })
        .collect()
}

/// Per-instance auth modes for [`jackin_protocol::CapsuleConfig`], keyed by
/// instance config ID in launch order.
pub(crate) fn capsule_auth_modes(
    config: &jackin_config::AppConfig,
    instances: &[jackin_config::ResolvedInstance],
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    let selections = account_auth_selections(config, instances)?;
    Ok(selections
        .into_iter()
        .map(|(config_id, (mode, _))| (config_id, mode.to_string()))
        .collect())
}

/// Account models must override role defaults: a role's native-provider model
/// may be invalid for the selected account's provider. Explicit launch options
/// are applied afterwards by the coordinator. The effective instance model
/// (configuration override, else account default) wins per instance.
pub(crate) fn apply_account_models(
    launch: &mut jackin_protocol::CapsuleConfig,
    config: &jackin_config::AppConfig,
    instances: &[jackin_config::ResolvedInstance],
) -> anyhow::Result<()> {
    for instance in instances {
        let account = config
            .accounts
            .get(&instance.account_id)
            .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
        let Some(model) = instance.model.as_deref() else {
            continue;
        };
        let model = if instance.agent == jackin_core::Agent::Opencode {
            super::account_config::opencode_model(account.provider, model)?
        } else {
            model.to_owned()
        };
        launch.models.insert(instance.config_id.clone(), model);
    }
    Ok(())
}

/// Fill the per-instance container dirs from prepared role-state
/// slots, keyed by instance config ID. The folder-var target comes
/// straight from the slot; the forwarded dir joins `/jackin` with the
/// slot's store rel. A missing slot fails the launch closed: the
/// daemon cannot spawn an instance it cannot place.
pub(crate) fn apply_instance_dirs(
    launch: &mut jackin_protocol::CapsuleConfig,
    instances: &[jackin_config::ResolvedInstance],
    slots: &std::collections::BTreeMap<String, crate::instance::ProvisionedInstanceAuth>,
) -> anyhow::Result<()> {
    for instance in instances {
        let slot = slots.get(&instance.config_id).ok_or_else(|| {
            anyhow::anyhow!(
                "instance {:?} has no provisioned auth slot in role state",
                instance.config_id
            )
        })?;
        launch
            .instance_home_dirs
            .insert(instance.config_id.clone(), slot.folder_target.clone());
        launch.instance_forwarded_dirs.insert(
            instance.config_id.clone(),
            format!(
                "{}/{}",
                jackin_core::container_paths::JACKIN_ROOT,
                slot.container_store_rel
            ),
        );
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
    instances: &[jackin_config::ResolvedInstance],
) -> jackin_protocol::CapsuleConfig {
    let mut models = std::collections::BTreeMap::new();
    let mut agents = std::collections::BTreeMap::new();
    let mut accounts = std::collections::BTreeMap::new();
    let mut labels = std::collections::BTreeMap::new();
    for instance in instances {
        agents.insert(instance.config_id.clone(), instance.agent.slug().to_owned());
        accounts.insert(instance.config_id.clone(), instance.account_id.clone());
        labels.insert(instance.config_id.clone(), instance.label.clone());
        if let Some(model) = manifest.agent_model(instance.agent) {
            models.insert(instance.config_id.clone(), model.to_owned());
        }
    }
    jackin_protocol::CapsuleConfig {
        role: selector.key(),
        workdir: workdir.to_owned(),
        instances: instances
            .iter()
            .map(|instance| instance.config_id.clone())
            .collect(),
        agents,
        models,
        auth_modes: std::collections::BTreeMap::new(),
        accounts,
        labels,
        // Populated by `apply_instance_dirs` once role state is
        // prepared; the manifest alone does not carry slot layout.
        instance_home_dirs: std::collections::BTreeMap::new(),
        instance_forwarded_dirs: std::collections::BTreeMap::new(),
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

#[cfg(test)]
mod tests;
