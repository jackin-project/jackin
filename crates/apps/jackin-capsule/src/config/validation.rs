// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Config boundary, overlap, and instance admission validation.

use anyhow::Result;
use jackin_protocol::CapsuleConfig;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub(crate) fn is_descendant(path: &str, root: &str) -> bool {
    jackin_core::container_paths::path_is_ancestor_or_equal(Path::new(root), Path::new(path))
}

pub(crate) fn is_strict_descendant(path: &str, root: &str) -> bool {
    let path = jackin_core::container_paths::normalize_path(Path::new(path));
    let root = jackin_core::container_paths::normalize_path(Path::new(root));
    path != root && jackin_core::container_paths::path_is_ancestor_or_equal(&root, &path)
}

/// Resolve an existing container path through symlinks and normalize paths
/// that are not present yet. A path that exists but cannot be canonicalized is
/// rejected by the caller rather than being treated as a harmless alias.
pub(crate) fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
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
pub(crate) fn validate_workdir_boundary(config: &CapsuleConfig) -> Result<()> {
    let lexical_workdir = jackin_core::container_paths::normalize_path(Path::new(&config.workdir));
    let workdir = normalize_existing_path(&lexical_workdir)?;
    ensure_no_protected_overlap(config, "capsule workdir", &lexical_workdir, &workdir)?;
    for mount in &config.workspace_mounts {
        let lexical_mount = jackin_core::container_paths::normalize_path(Path::new(mount));
        let mount = normalize_existing_path(&lexical_mount)?;
        ensure_no_protected_overlap(config, "capsule workspace mount", &lexical_mount, &mount)?;
    }
    for target in &config.worktree_git_targets {
        validate_worktree_git_target(target)?;
    }
    Ok(())
}

/// Aux git dirs live under `/jackin/host` by construction
/// (`/jackin/host/<dst>/.git`), a subtree the workspace boundary policy can
/// never admit. Admit exactly strict descendants of that root so in-container
/// git can follow the worktree gitdir pointer; anything else fails closed.
pub(crate) fn validate_worktree_git_target(target: &str) -> Result<()> {
    anyhow::ensure!(
        !target.split('/').any(|component| component == ".."),
        "capsule worktree git target {target} must not contain .."
    );
    let lexical_target = jackin_core::container_paths::normalize_path(Path::new(target));
    let target = normalize_existing_path(&lexical_target)?;
    for candidate in [&lexical_target, &target] {
        let candidate = candidate.to_string_lossy();
        anyhow::ensure!(
            is_strict_descendant(&candidate, jackin_core::container_paths::HOST_DIR),
            "capsule worktree git target {candidate} is outside {}",
            jackin_core::container_paths::HOST_DIR
        );
    }
    Ok(())
}

pub(crate) fn ensure_no_protected_overlap(
    config: &CapsuleConfig,
    label: &str,
    lexical: &Path,
    canonical: &Path,
) -> Result<()> {
    for protected_root in ["/home/agent", jackin_core::container_paths::JACKIN_ROOT] {
        let lexical_root = jackin_core::container_paths::normalize_path(Path::new(protected_root));
        anyhow::ensure!(
            !jackin_core::container_paths::paths_overlap(lexical, &lexical_root),
            "{label} {} overlaps protected root {}",
            lexical.display(),
            lexical_root.display()
        );
        let protected_root = normalize_existing_path(&lexical_root)?;
        anyhow::ensure!(
            !jackin_core::container_paths::paths_overlap(canonical, &protected_root),
            "{label} {} overlaps protected root {}",
            canonical.display(),
            protected_root.display()
        );
    }
    for (instance, paths) in &config.instance_mount_paths {
        for path in paths {
            let lexical_mount = jackin_core::container_paths::normalize_path(Path::new(path));
            anyhow::ensure!(
                !jackin_core::container_paths::paths_overlap(lexical, &lexical_mount),
                "{label} {} overlaps protected mount destination {} for instance {instance}",
                lexical.display(),
                lexical_mount.display()
            );
            let mount = normalize_existing_path(&lexical_mount)?;
            anyhow::ensure!(
                !jackin_core::container_paths::paths_overlap(canonical, &mount),
                "{label} {} overlaps protected mount destination {} for instance {instance}",
                canonical.display(),
                mount.display()
            );
        }
    }
    Ok(())
}

pub(crate) fn validate_instance(
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

pub(crate) fn validate(config: &CapsuleConfig) -> Result<()> {
    if config.workdir.trim().is_empty() {
        anyhow::bail!("{} workdir is empty", jackin_protocol::CAPSULE_CONFIG_PATH);
    }
    validate_workdir_boundary(config)?;
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
    for (instance, surface) in &config.credential_provider_surfaces {
        anyhow::ensure!(
            config.instances.contains(instance),
            "credential provider surfaces name an instance outside the configured allowlist"
        );
        anyhow::ensure!(
            !surface.trim().is_empty(),
            "credential provider surface for instance {instance:?} is empty"
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
