// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule runtime configuration: load and validate `CapsuleConfig` from the
//! TOML file written by the host at container launch.
//!
//! Not responsible for: config schema definition (see `jackin-protocol`) or
//! host-side config serialization.

use anyhow::{Context, Result};
use jackin_protocol::CapsuleConfig;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// # Errors
///
/// Returns an error when the capsule configuration cannot be read, parsed, or
/// validated.
pub fn load() -> Result<CapsuleConfig> {
    let contents = std::fs::read_to_string(jackin_protocol::CAPSULE_CONFIG_PATH)
        .with_context(|| format!("reading {}", jackin_protocol::CAPSULE_CONFIG_PATH))?;
    let config: CapsuleConfig = toml::from_str(&contents)
        .with_context(|| format!("parsing {}", jackin_protocol::CAPSULE_CONFIG_PATH))?;
    validate(&config)?;
    Ok(config)
}

#[must_use]
pub fn load_optional() -> Option<CapsuleConfig> {
    let contents = match std::fs::read_to_string(jackin_protocol::CAPSULE_CONFIG_PATH) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            let _error = jackin_telemetry::record_error(
                jackin_telemetry::schema::enums::ErrorType::ConfigError,
            );
            crate::output::stderr_line(format_args!(
                "[jackin-capsule] ignoring unreadable {}: {error:#}",
                jackin_protocol::CAPSULE_CONFIG_PATH
            ));
            return None;
        }
    };
    let config = match toml::from_str::<CapsuleConfig>(&contents) {
        Ok(config) => config,
        Err(error) => {
            let _error = jackin_telemetry::record_error(
                jackin_telemetry::schema::enums::ErrorType::ConfigError,
            );
            crate::output::stderr_line(format_args!(
                "[jackin-capsule] ignoring invalid {}: {error:#}",
                jackin_protocol::CAPSULE_CONFIG_PATH
            ));
            return None;
        }
    };
    if let Err(error) = validate(&config) {
        let _error =
            jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::ConfigError);
        crate::output::stderr_line(format_args!(
            "[jackin-capsule] ignoring invalid {}: {error:#}",
            jackin_protocol::CAPSULE_CONFIG_PATH
        ));
        return None;
    }
    Some(config)
}

fn is_descendant(path: &str, root: &str) -> bool {
    jackin_core::container_paths::path_is_ancestor_or_equal(Path::new(root), Path::new(path))
}

fn is_strict_descendant(path: &str, root: &str) -> bool {
    let path = jackin_core::container_paths::normalize_path(Path::new(path));
    let root = jackin_core::container_paths::normalize_path(Path::new(root));
    path != root && jackin_core::container_paths::path_is_ancestor_or_equal(&root, &path)
}

/// Resolve an existing container path through symlinks and normalize paths
/// that are not present yet. A path that exists but cannot be canonicalized is
/// rejected by the caller rather than being treated as a harmless alias.
fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
    anyhow::ensure!(path.is_absolute(), "container path must be absolute");
    let normalized = jackin_core::container_paths::normalize_path(path);
    match std::fs::canonicalize(&normalized) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(normalized),
        Err(error) => Err(error.into()),
    }
}

/// Reject a cwd whose recursive workspace grant could cover capsule state,
/// agent-private homes, or an admitted private mount destination.
fn validate_workdir_boundary(config: &CapsuleConfig) -> Result<()> {
    let lexical_workdir = jackin_core::container_paths::normalize_path(Path::new(&config.workdir));
    let workdir = normalize_existing_path(&lexical_workdir)?;
    for protected_root in ["/home/agent", jackin_core::container_paths::JACKIN_ROOT] {
        let lexical_root = jackin_core::container_paths::normalize_path(Path::new(protected_root));
        anyhow::ensure!(
            !jackin_core::container_paths::paths_overlap(&lexical_workdir, &lexical_root),
            "capsule workdir {} overlaps protected root {}",
            lexical_workdir.display(),
            lexical_root.display()
        );
        let protected_root = normalize_existing_path(&lexical_root)?;
        anyhow::ensure!(
            !jackin_core::container_paths::paths_overlap(&workdir, &protected_root),
            "capsule workdir {} overlaps protected root {}",
            workdir.display(),
            protected_root.display()
        );
    }
    for (instance, paths) in &config.instance_mount_paths {
        for path in paths {
            let lexical_mount = jackin_core::container_paths::normalize_path(Path::new(path));
            anyhow::ensure!(
                !jackin_core::container_paths::paths_overlap(&lexical_workdir, &lexical_mount),
                "capsule workdir {} overlaps protected mount destination {} for instance {instance}",
                lexical_workdir.display(),
                lexical_mount.display()
            );
            let mount = normalize_existing_path(&lexical_mount)?;
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

fn validate_instance(
    config: &CapsuleConfig,
    instance: &str,
    identities: &mut BTreeSet<(u32, u32)>,
) -> Result<()> {
    let mode = config.auth_mode_for_instance(instance).ok_or_else(|| {
        anyhow::anyhow!("missing bounded auth mode for configured instance {instance}")
    })?;
    if !matches!(mode, "sync" | "api_key" | "oauth_token" | "ignore") {
        anyhow::bail!("invalid bounded auth mode for configured instance {instance}");
    }
    anyhow::ensure!(
        config.agent_for_instance(instance).is_some()
            && config.home_for_instance(instance).is_some()
            && config.forwarded_for_instance(instance).is_some()
            && config.identity_for_instance(instance).is_some()
            && !config.mount_paths_for_instance(instance).is_empty(),
        "instance {instance:?} is missing an admitted isolation record"
    );
    let Some(identity) = config.identity_for_instance(instance) else {
        anyhow::bail!("instance {instance:?} has no Unix identity");
    };
    anyhow::ensure!(
        identity.uid > 0 && identity.gid > 0 && identity.uid < 65_536 && identity.gid < 65_536,
        "instance {instance:?} has an invalid non-root Unix identity"
    );
    anyhow::ensure!(
        identities.insert((identity.uid, identity.gid)),
        "duplicate Unix identity for configured instance {instance:?}"
    );
    let home = config
        .home_for_instance(instance)
        .ok_or_else(|| anyhow::anyhow!("instance {instance:?} has no private home path"))?;
    anyhow::ensure!(
        is_strict_descendant(home, "/home/agent")
            && !home.split('/').any(|component| component == ".."),
        "instance {instance:?} has an invalid private home path"
    );
    let cache_root = config.cache_for_instance(instance);
    if let Some(cache_root) = cache_root {
        anyhow::ensure!(
            is_strict_descendant(cache_root, "/home/agent/.cache")
                && !cache_root.split('/').any(|component| component == ".."),
            "instance {instance:?} has an invalid XDG cache path"
        );
    }
    // Amp/OpenCode export their data directory through XDG_DATA_HOME, but
    // persist settings under a sibling `.config/<agent>` directory. The
    // host mounts both roots for the same slot; admit only the exact
    // adapter-defined sibling rather than widening the allowlist to all
    // of `/home/agent`.
    let paired_xdg_config_root = config
        .agent_for_instance(instance)
        .and_then(jackin_core::Agent::from_slug)
        .filter(|agent| {
            matches!(
                agent.runtime().state_paths().folder_env_var,
                Some(jackin_core::FolderVar {
                    kind: jackin_core::FolderVarKind::XdgRoot,
                    ..
                })
            )
        })
        .and_then(|agent| agent.runtime().state_paths().config_dir)
        .map(|relative| format!("/home/agent/{relative}"));
    let forwarded = config
        .forwarded_for_instance(instance)
        .ok_or_else(|| anyhow::anyhow!("instance {instance:?} has no private auth path"))?;
    anyhow::ensure!(
        is_strict_descendant(forwarded, jackin_core::container_paths::JACKIN_ROOT)
            && !forwarded.split('/').any(|component| component == "..")
            && ![
                jackin_core::container_paths::RUN_DIR,
                jackin_core::container_paths::STATE_DIR,
                jackin_core::container_paths::RUNTIME_DIR,
                jackin_core::container_paths::DEFAULT_HOME_DIR,
                jackin_protocol::ACCOUNT_CREDENTIALS_DIR,
            ]
            .iter()
            .any(|root| is_descendant(forwarded, root)),
        "instance {instance:?} has an invalid private auth path"
    );
    let path = config
        .credential_file_for_instance(instance)
        .ok_or_else(|| anyhow::anyhow!("instance {instance:?} has no credential file path"))?;
    anyhow::ensure!(
        path == jackin_protocol::account_credentials_container_path(instance),
        "instance {instance:?} has an invalid credential mount path"
    );
    for path in config.mount_paths_for_instance(instance) {
        let normalized_path = jackin_core::container_paths::normalize_path(Path::new(path));
        let normalized_path = normalized_path.to_string_lossy();
        let is_private_home = is_strict_descendant(&normalized_path, "/home/agent")
            && (is_descendant(&normalized_path, home)
                || paired_xdg_config_root
                    .as_deref()
                    .is_some_and(|root| is_descendant(&normalized_path, root))
                || cache_root.is_some_and(|root| is_descendant(&normalized_path, root)));
        let is_forwarded_auth = is_descendant(&normalized_path, forwarded);
        anyhow::ensure!(
            normalized_path != "/home/agent"
                && normalized_path != jackin_core::container_paths::JACKIN_ROOT
                && normalized_path != jackin_core::container_paths::RUN_DIR
                && normalized_path != jackin_core::container_paths::STATE_DIR
                && normalized_path != jackin_core::container_paths::RUNTIME_DIR
                && !is_descendant(&normalized_path, jackin_protocol::ACCOUNT_CREDENTIALS_DIR)
                && (is_private_home || is_forwarded_auth)
                && !path.split('/').any(|component| component == ".."),
            "instance {instance:?} has an invalid private mount path"
        );
    }
    Ok(())
}

/// Reject an auxiliary isolated-workspace destination whose recursive grant
/// could cover capsule state, agent-private homes, or an admitted private
/// mount destination. Mirrors [`validate_workdir_boundary`] without the
/// blanket `/jackin` rejection: `/jackin/work/...` destinations are
/// legitimate workspaces.
fn validate_isolated_worktrees(config: &CapsuleConfig) -> Result<()> {
    for entry in &config.isolated_worktrees {
        let mount = entry.dst.as_str();
        anyhow::ensure!(
            !mount.trim().is_empty(),
            "capsule isolated workspace destination is empty"
        );
        anyhow::ensure!(
            Path::new(mount).is_absolute(),
            "capsule isolated workspace destination {mount} must be absolute"
        );
        anyhow::ensure!(
            !mount.split('/').any(|component| component == ".."),
            "capsule isolated workspace destination {mount} must not contain `..`"
        );
        let lexical_mount = jackin_core::container_paths::normalize_path(Path::new(mount));
        let normalized = normalize_existing_path(&lexical_mount)?;
        for protected_root in [
            "/home/agent",
            jackin_core::container_paths::RUN_DIR,
            jackin_core::container_paths::STATE_DIR,
            jackin_core::container_paths::RUNTIME_DIR,
            jackin_core::container_paths::DEFAULT_HOME_DIR,
            jackin_core::container_paths::HOST_DIR,
            jackin_protocol::ACCOUNT_CREDENTIALS_DIR,
        ] {
            let lexical_root =
                jackin_core::container_paths::normalize_path(Path::new(protected_root));
            anyhow::ensure!(
                !jackin_core::container_paths::paths_overlap(&lexical_mount, &lexical_root),
                "capsule isolated workspace destination {} overlaps protected root {}",
                lexical_mount.display(),
                lexical_root.display()
            );
            let protected_root = normalize_existing_path(&lexical_root)?;
            anyhow::ensure!(
                !jackin_core::container_paths::paths_overlap(&normalized, &protected_root),
                "capsule isolated workspace destination {} overlaps protected root {}",
                normalized.display(),
                protected_root.display()
            );
        }
        for (instance, paths) in &config.instance_mount_paths {
            for path in paths {
                let lexical_private = jackin_core::container_paths::normalize_path(Path::new(path));
                anyhow::ensure!(
                    !jackin_core::container_paths::paths_overlap(&lexical_mount, &lexical_private),
                    "capsule isolated workspace destination {} overlaps protected mount destination {} for instance {instance}",
                    lexical_mount.display(),
                    lexical_private.display()
                );
                let private = normalize_existing_path(&lexical_private)?;
                anyhow::ensure!(
                    !jackin_core::container_paths::paths_overlap(&normalized, &private),
                    "capsule isolated workspace destination {} overlaps protected mount destination {} for instance {instance}",
                    normalized.display(),
                    private.display()
                );
            }
        }
    }
    Ok(())
}

fn validate(config: &CapsuleConfig) -> Result<()> {
    if config.workdir.trim().is_empty() {
        anyhow::bail!("{} workdir is empty", jackin_protocol::CAPSULE_CONFIG_PATH);
    }
    validate_workdir_boundary(config)?;
    validate_isolated_worktrees(config)?;
    let mut identities = BTreeSet::new();
    for instance in &config.instances {
        validate_instance(config, instance, &mut identities)?;
    }
    for (left_index, left_instance) in config.instances.iter().enumerate() {
        for right_instance in config.instances.iter().skip(left_index + 1) {
            for left_path in config.mount_paths_for_instance(left_instance) {
                for right_path in config.mount_paths_for_instance(right_instance) {
                    anyhow::ensure!(
                        !(left_path == right_path
                            || is_descendant(left_path, right_path)
                            || is_descendant(right_path, left_path)),
                        "private mount paths for instances {left_instance:?} and {right_instance:?} overlap"
                    );
                }
            }
        }
    }
    for instance in config.instance_credential_files.keys() {
        anyhow::ensure!(
            config.instances.contains(instance),
            "credential file names an instance outside the configured allowlist"
        );
    }
    for instance in config.instance_mount_paths.keys() {
        anyhow::ensure!(
            config.instances.contains(instance),
            "mount paths name an instance outside the configured allowlist"
        );
    }
    for instance in config.instance_home_dirs.keys() {
        anyhow::ensure!(
            config.instances.contains(instance),
            "home paths name an instance outside the configured allowlist"
        );
    }
    for instance in config.instance_cache_dirs.keys() {
        anyhow::ensure!(
            config.instances.contains(instance),
            "cache paths name an instance outside the configured allowlist"
        );
    }
    for instance in config.instance_forwarded_dirs.keys() {
        anyhow::ensure!(
            config.instances.contains(instance),
            "forwarded paths name an instance outside the configured allowlist"
        );
    }
    for instance in config.instance_identities.keys() {
        anyhow::ensure!(
            config.instances.contains(instance),
            "Unix identities name an instance outside the configured allowlist"
        );
    }
    if config
        .auth_modes
        .keys()
        .any(|instance| !config.instances.contains(instance))
    {
        anyhow::bail!("auth mode names an instance outside the configured allowlist");
    }
    let shell = config
        .shell_identity
        .ok_or_else(|| anyhow::anyhow!("capsule config has no isolated shell identity"))?;
    anyhow::ensure!(
        shell.uid > 0 && shell.gid > 0 && shell.uid < 65_536 && shell.gid < 65_536,
        "capsule shell identity must be a non-root Unix identity"
    );
    anyhow::ensure!(
        identities.insert((shell.uid, shell.gid)),
        "shell identity overlaps an admitted instance identity"
    );
    Ok(())
}

#[cfg(test)]
mod tests;

/// Load protected account data without including file contents in diagnostics.
pub(crate) fn load_agent_credentials(
    config: &CapsuleConfig,
) -> std::io::Result<jackin_protocol::AgentCredentialEnv> {
    let mut instances = std::collections::BTreeMap::new();
    for (instance, path) in &config.instance_credential_files {
        let expected = jackin_protocol::account_credentials_container_path(instance);
        if path != &expected {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credential path is outside the admitted mount",
            ));
        }
        let raw = match std::fs::read(path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let staged = parse_staged_credential(&raw)?;
        if staged.instance != *instance {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credential file does not match its admitted instance",
            ));
        }
        if instances
            .insert(instance.clone(), staged.credential)
            .is_some()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "duplicate protected account credential instance",
            ));
        }
    }
    let credentials = jackin_protocol::AgentCredentialEnv::new(instances);
    validate_agent_credentials(config, &credentials)?;
    Ok(credentials)
}

/// Decode one staged protected-credentials file. Anything else (a legacy
/// container-wide envelope, missing version, or malformed JSON) is an
/// explicit restart/upgrade error, never a silent misread. Diagnostics never
/// carry file contents.
fn parse_staged_credential(
    raw: &[u8],
) -> std::io::Result<jackin_protocol::StagedInstanceCredential> {
    let credential: jackin_protocol::StagedInstanceCredential = serde_json::from_slice(raw)
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid protected account credentials: expected a single-instance \
                 staged credential file; restart the container \
                 from an upgraded host",
            )
        })?;
    if credential.schema_version != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unsupported protected account credentials schema: expected the single-instance \
             staged credential schema; restart the container \
             from an upgraded host",
        ));
    }
    Ok(credential)
}

fn validate_agent_credentials(
    config: &CapsuleConfig,
    credentials: &jackin_protocol::AgentCredentialEnv,
) -> std::io::Result<()> {
    for instance in &config.instances {
        if config.agent_for_instance(instance).is_none() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "launch config instance has no agent runtime",
            ));
        }
        if matches!(
            config.auth_mode_for_instance(instance),
            Some("api_key" | "oauth_token")
        ) && credentials
            .for_instance(instance)
            .is_none_or(std::collections::BTreeMap::is_empty)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "missing protected credentials for configured account",
            ));
        }
    }
    for (instance, entry) in credentials.iter() {
        let Some(expected_agent) = config.agent_for_instance(instance) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credentials name an instance without an agent runtime",
            ));
        };
        let Some(expected_account) = config.account_for_instance(instance) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credentials name an instance without an account",
            ));
        };
        if !config.instances.contains(instance)
            || !matches!(
                config.auth_mode_for_instance(instance),
                Some("api_key" | "oauth_token")
            )
            || entry.agent != expected_agent
            || entry.account_id != expected_account
            || entry
                .env
                .iter()
                .any(|(name, value)| !jackin_core::is_account_env(name) || value.trim().is_empty())
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected account credentials violate instance admission",
            ));
        }
    }
    Ok(())
}
