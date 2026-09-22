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
            binding.xdg_roots = instance.xdg_roots.clone();
            binding.source_provider = account.source_directory().map(|_| account.provider);
            binding.source_selector = match &account.credential {
                jackin_config::AccountCredential::Profile {
                    source_selector, ..
                } => source_selector.clone(),
                _ => None,
            };
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

/// Resolve the exact model map used by both account materialization and the
/// Capsule. The role model is the base, the selected account model replaces it
/// when present, and a launch model override fans out to every admitted slot
/// for the selected runtime.
pub(crate) fn resolved_instance_models(
    config: &jackin_config::AppConfig,
    manifest: &jackin_manifest::RoleManifest,
    instances: &[jackin_config::ResolvedInstance],
    selected_agent: jackin_core::Agent,
    model_override: Option<&str>,
) -> anyhow::Result<std::collections::BTreeMap<String, String>> {
    let mut launch = jackin_protocol::CapsuleConfig::default();
    for instance in instances {
        if let Some(model) = manifest.agent_model(instance.agent) {
            let model = if instance.agent == jackin_core::Agent::Opencode {
                let account = config
                    .accounts
                    .get(&instance.account_id)
                    .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
                super::account_config::opencode_model(account.provider, model)?
            } else {
                model.to_owned()
            };
            launch.models.insert(instance.config_id.clone(), model);
        }
    }
    apply_account_models(&mut launch, config, instances)?;
    if let Some(model) = model_override
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        for instance in instances
            .iter()
            .filter(|instance| instance.agent == selected_agent)
        {
            let model = if instance.agent == jackin_core::Agent::Opencode {
                let account = config
                    .accounts
                    .get(&instance.account_id)
                    .ok_or_else(|| anyhow::anyhow!("unknown account {:?}", instance.account_id))?;
                super::account_config::opencode_model(account.provider, model)?
            } else {
                model.to_owned()
            };
            launch.models.insert(instance.config_id.clone(), model);
        }
    }
    Ok(launch.models)
}

/// Fan out one requested effort to the same runtime's admitted slots. The
/// Capsule owns this map so no process-wide env value can make two slots
/// disagree.
pub(crate) fn resolved_instance_efforts(
    instances: &[jackin_config::ResolvedInstance],
    selected_agent: jackin_core::Agent,
    effort: Option<jackin_core::ReasoningEffort>,
) -> std::collections::BTreeMap<String, String> {
    effort.map_or_else(std::collections::BTreeMap::new, |effort| {
        instances
            .iter()
            .filter(|instance| instance.agent == selected_agent)
            .map(|instance| (instance.config_id.clone(), effort.as_str().to_owned()))
            .collect()
    })
}

fn forwarded_credential_mount_paths(
    agent: jackin_core::Agent,
    slot: &crate::instance::ProvisionedInstanceAuth,
) -> Vec<String> {
    if matches!(agent, jackin_core::Agent::Kimi | jackin_core::Agent::Hermes) {
        return vec![format!(
            "{}/{}",
            jackin_core::container_paths::JACKIN_ROOT,
            slot.container_store_rel
        )];
    }

    let include_missing_credentials = agent != jackin_core::Agent::Claude;
    slot.credential_paths
        .iter()
        .filter_map(|path| {
            let file_name = path.file_name()?.to_str()?;
            (include_missing_credentials || path.exists()).then(|| {
                format!(
                    "{}/{}/{}",
                    jackin_core::container_paths::JACKIN_ROOT,
                    slot.container_store_rel,
                    file_name
                )
            })
        })
        .collect()
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
    const FIRST_SESSION_UID: u32 = 2_000;
    anyhow::ensure!(
        instances.len() < 1_000,
        "too many admitted instances for the capsule session UID range"
    );
    for (index, instance) in instances.iter().enumerate() {
        let slot = slots.get(&instance.config_id).ok_or_else(|| {
            anyhow::anyhow!(
                "instance {:?} has no provisioned auth slot in role state",
                instance.config_id
            )
        })?;
        launch
            .instance_home_dirs
            .insert(instance.config_id.clone(), slot.folder_target.clone());
        if let Some(cache_rel) = &slot.container_cache_rel {
            launch.instance_cache_dirs.insert(
                instance.config_id.clone(),
                format!("/home/agent/{cache_rel}"),
            );
        }
        launch.instance_forwarded_dirs.insert(
            instance.config_id.clone(),
            format!(
                "{}/{}",
                jackin_core::container_paths::JACKIN_ROOT,
                slot.container_store_rel
            ),
        );
        launch.instance_credential_files.insert(
            instance.config_id.clone(),
            jackin_protocol::account_credentials_container_path(&instance.config_id),
        );
        launch.instance_identities.insert(
            instance.config_id.clone(),
            jackin_protocol::SessionIdentity {
                uid: FIRST_SESSION_UID + index as u32,
                gid: FIRST_SESSION_UID + index as u32,
            },
        );

        let paths = instance.agent.runtime().state_paths();
        let mut mount_paths = vec![format!("/home/agent/{}", slot.container_home_rel)];
        if let Some(cache_rel) = &slot.container_cache_rel {
            mount_paths.push(format!("/home/agent/{cache_rel}"));
        }
        mount_paths.extend(
            paths
                .home_dirs()
                .filter(|entry| *entry != paths.credential_dir)
                .map(|entry| {
                    format!(
                        "/home/agent/{}",
                        crate::instance::slot_home_rel(entry, slot.slot_suffix.as_deref())
                    )
                }),
        );
        if slot.forward_auth {
            mount_paths.extend(forwarded_credential_mount_paths(instance.agent, slot));
        }
        launch
            .instance_mount_paths
            .insert(instance.config_id.clone(), mount_paths);
    }
    let shell_uid = FIRST_SESSION_UID + instances.len() as u32;
    launch.shell_identity = Some(jackin_protocol::SessionIdentity {
        uid: shell_uid,
        gid: shell_uid,
    });
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
) -> anyhow::Result<String> {
    validate_capsule_workdir(config)?;
    let mut projected = config.clone();
    for binding in &mut projected.exec_bindings {
        match binding.kind {
            jackin_protocol::ExecKind::Op | jackin_protocol::ExecKind::Env => {}
            jackin_protocol::ExecKind::Literal => {
                binding.source = CAPSULE_LITERAL_SOURCE.to_owned();
            }
        }
    }
    Ok(toml::to_string(&projected)?)
}

/// Keep the host-to-capsule handoff fail-closed even when a caller constructs
/// a `CapsuleConfig` without going through workspace validation. A recursive
/// workspace Landlock grant must not overlap capsule roots or any private
/// instance mount destination.
fn validate_capsule_workdir(config: &jackin_protocol::CapsuleConfig) -> anyhow::Result<()> {
    let workdir = Path::new(config.workdir.trim());
    anyhow::ensure!(
        !config.workdir.trim().is_empty() && workdir.is_absolute(),
        "capsule workdir must be a non-empty absolute path"
    );
    let workdir = jackin_core::container_paths::normalize_path(workdir);
    for protected_root in ["/home/agent", jackin_core::container_paths::JACKIN_ROOT] {
        let protected_root =
            jackin_core::container_paths::normalize_path(Path::new(protected_root));
        anyhow::ensure!(
            !jackin_core::container_paths::paths_overlap(&workdir, &protected_root),
            "capsule workdir {} overlaps protected root {}",
            workdir.display(),
            protected_root.display()
        );
    }
    for (instance, paths) in &config.instance_mount_paths {
        for path in paths {
            let mount = Path::new(path);
            anyhow::ensure!(
                mount.is_absolute(),
                "private mount destination for instance {instance} must be absolute"
            );
            let mount = jackin_core::container_paths::normalize_path(mount);
            anyhow::ensure!(
                !jackin_core::container_paths::paths_overlap(&workdir, &mount),
                "capsule workdir {} overlaps protected mount destination {} for instance {instance}",
                workdir.display(),
                mount.display()
            );
        }
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "capsule config combines role, workspace, policy, mount, and instance state"
)]
pub(crate) fn capsule_config(
    selector: &jackin_core::RoleSelector,
    workdir: &str,
    manifest: &jackin_manifest::RoleManifest,
    dirty_exit_policy: &str,
    isolated_worktrees: Vec<String>,
    workspace_mounts: Vec<String>,
    worktree_git_targets: Vec<String>,
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
        efforts: std::collections::BTreeMap::new(),
        auth_modes: std::collections::BTreeMap::new(),
        accounts,
        usage_capabilities: std::collections::BTreeMap::new(),
        credential_provider_surfaces: std::collections::BTreeMap::new(),
        labels,
        // Populated by `apply_instance_dirs` once role state is
        // prepared; the manifest alone does not carry slot layout.
        instance_home_dirs: std::collections::BTreeMap::new(),
        instance_cache_dirs: std::collections::BTreeMap::new(),
        instance_forwarded_dirs: std::collections::BTreeMap::new(),
        instance_credential_files: std::collections::BTreeMap::new(),
        instance_mount_paths: std::collections::BTreeMap::new(),
        instance_identities: std::collections::BTreeMap::new(),
        shell_identity: None,
        claude_marketplaces: Vec::new(),
        claude_plugins: Vec::new(),
        // Populated by the launch pipeline once the operator env is known; the
        // manifest alone does not carry on-demand workspace credentials.
        exec_bindings: Vec::new(),
        dirty_exit_policy: Some(dirty_exit_policy.to_owned()),
        isolated_worktrees,
        workspace_mounts,
        worktree_git_targets,
    }
}

/// Create the per-container socket dir and write Capsule's launch config
/// (`agent.toml`) into it. Docker bind-mounts the directory to `/jackin/run`;
/// Apple Container mounts the config and any host sockets as individual files.
/// Shared by both launch paths: the apple-container path
/// (`apple_container::launch`) and the Docker path (`launch_role_runtime`,
/// which calls it inside its socket-dir `spawn_blocking` alongside the
/// extrausers passwd write). The directory is private before the config write,
/// including when no host credential listener will be started.
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
    environment: Vec<String>,
}

impl HostEnvTransport {
    pub(crate) fn append_arguments<'a>(&'a self, arguments: &mut Vec<&'a str>) {
        arguments.extend(self.arguments.iter().map(String::as_str));
    }

    pub(crate) fn environment(&self) -> &[String] {
        &self.environment
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
        environment: entries
            .into_iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect(),
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
