// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Mount construction helpers extracted from the launch coordinator.
//! All items re-exported from the parent to preserve `super::` call sites
//! in `launch_role_runtime` and `launch_pipeline.rs`.

use std::path::{Component, Path, PathBuf};

use jackin_config::AppConfig;

use jackin_isolation::materialize::MaterializedWorkspace;
use jackin_runtime_apple_container_client::apple_container_client::AppleContainerMount;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AppleContainerMountError {
    #[error(
        "mount {destination} requires read-only file overlays, but the apple-container backend rejects single-file bind mounts; use the docker backend or change this mount to shared isolation"
    )]
    WorktreeFileOverlays { destination: String },
}

/// Emit the durable-home bind mounts for one provisioned slot. The
/// data home comes from the slot's kind-aware container rel
/// (`/home/agent/.claude-<suffix>`, or a unique parent child for
/// parent-scoped folder vars); paired config roots come from the
/// agent's [`AgentStatePaths`](jackin_core::AgentStatePaths) with the
/// slot suffix applied. Primary slots keep the legacy destinations.
fn push_slot_home_mounts(
    mounts: &mut Vec<String>,
    root: &Path,
    agent: jackin_core::Agent,
    slot: &jackin_instance::ProvisionedInstanceAuth,
) {
    let paths = agent.runtime().state_paths();
    let home = root.join("home");
    mounts.push(format!(
        "{}:/home/agent/{}",
        home.join(&slot.container_home_rel).display(),
        slot.container_home_rel
    ));
    if let (Some(source), Some(rel)) = (&slot.cache_source_dir, &slot.container_cache_rel) {
        mounts.push(format!("{}:/home/agent/{rel}", source.display()));
    }
    for entry in paths
        .home_dirs()
        .filter(|entry| *entry != paths.credential_dir)
    {
        let rel = jackin_instance::slot_home_rel(entry, slot.slot_suffix.as_deref());
        mounts.push(format!("{}:/home/agent/{rel}", home.join(&rel).display()));
    }
}

/// Emit the auth-handoff mounts for one provisioned slot under its
/// container store dir (`/jackin/<agent>` for primary slots,
/// `/jackin/<agent>-<suffix>` for secondary same-agent slots).
///
/// File-credential agents mount each admitted file by file name;
/// Kimi/Hermes mount their admitted credential directory. Claude keeps
/// its optional per-file behavior: a missing admitted file is omitted,
/// while every other forwarded path fails closed.
fn push_slot_auth_mounts(
    mounts: &mut Vec<String>,
    root: &Path,
    state: &jackin_instance::RoleState,
    slot: &jackin_instance::ProvisionedInstanceAuth,
) -> anyhow::Result<()> {
    use jackin_core::Agent;
    if !slot.forward_auth {
        return Ok(());
    }
    if matches!(slot.agent, Agent::Kimi | Agent::Hermes) {
        let store = root.join(&slot.container_store_rel);
        anyhow::ensure!(
            state.auth_mount_directory_allowed(&store)?,
            "private auth store is not admitted for mount: {}",
            store.display()
        );
        mounts.push(format!(
            "{}:/jackin/{}:ro",
            store.display(),
            slot.container_store_rel
        ));
        return Ok(());
    }
    for path in &slot.credential_paths {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            anyhow::bail!("auth mount path has no valid file name: {}", path.display());
        };
        if state.auth_mount_file_allowed(path)? {
            mounts.push(format!(
                "{}:/jackin/{}/{}:ro",
                path.display(),
                slot.container_store_rel,
                file_name
            ));
        } else if slot.agent != Agent::Claude {
            anyhow::bail!("auth mount path is no longer admitted: {}", path.display());
        }
    }
    Ok(())
}

#[derive(Debug)]
struct ProviderConfigOverlay {
    source: PathBuf,
    target: PathBuf,
}

#[derive(Debug)]
struct ParsedDockerBind {
    source: Option<PathBuf>,
    raw_source: Option<PathBuf>,
    target: PathBuf,
    readonly: bool,
}

fn contains_parent_dir(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

/// Resolve a host mount path for overlap checks, including aliases through an
/// existing symlinked ancestor while retaining lexical components that do not
/// exist yet. Docker interprets the source path at launch time, so comparing
/// only the original strings would let an alias escape the authority audit.
fn canonical_mount_path(path: &Path) -> anyhow::Result<PathBuf> {
    use std::collections::VecDeque;
    use std::ffi::OsString;

    anyhow::ensure!(
        path.is_absolute(),
        "host mount path must be absolute: {}",
        path.display()
    );
    let mut pending: VecDeque<OsString> = path
        .components()
        .map(|component| component.as_os_str().to_owned())
        .collect();
    let mut resolved = PathBuf::from("/");
    let mut symlink_hops = 0;
    while let Some(component) = pending.pop_front() {
        match Path::new(&component).components().next() {
            Some(Component::RootDir) => {
                resolved = PathBuf::from("/");
                continue;
            }
            Some(Component::CurDir) => continue,
            Some(Component::ParentDir) => {
                resolved.pop();
                continue;
            }
            Some(Component::Normal(_)) => {}
            _ => anyhow::bail!("unsupported host mount path component: {}", path.display()),
        }
        let candidate = resolved.join(&component);
        match std::fs::symlink_metadata(&candidate) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                symlink_hops += 1;
                anyhow::ensure!(
                    symlink_hops <= 40,
                    "host mount path exceeds symlink hop limit: {}",
                    path.display()
                );
                let target = std::fs::read_link(&candidate).map_err(|error| {
                    anyhow::anyhow!(
                        "reading host mount symlink {}: {error}",
                        candidate.display()
                    )
                })?;
                for next in target.components().rev() {
                    pending.push_front(next.as_os_str().to_owned());
                }
            }
            Ok(_) => resolved = candidate,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Retain a future suffix; keep following any existing symlinks
                // after .. returns resolution to an existing ancestor.
                resolved = candidate;
            }
            Err(error) => {
                anyhow::bail!("resolving host mount path {}: {error}", candidate.display())
            }
        }
    }
    Ok(resolved)
}

fn provider_config_overlays(
    state: &jackin_instance::RoleState,
) -> anyhow::Result<(PathBuf, Vec<ProviderConfigOverlay>)> {
    let authority = canonical_mount_path(&state.root.join("provider-config"))?;
    let mut overlays = Vec::with_capacity(state.provider_config_mounts.len());

    for (source, target) in &state.provider_config_mounts {
        anyhow::ensure!(
            source.is_absolute(),
            "provider config mount source must be absolute: {}",
            source.display()
        );
        anyhow::ensure!(
            !contains_parent_dir(source),
            "provider config mount source must not contain parent traversal: {}",
            source.display()
        );
        anyhow::ensure!(
            Path::new(target).is_absolute(),
            "provider config mount destination must be absolute: {target}"
        );
        anyhow::ensure!(
            source.to_str().is_some(),
            "provider config mount source contains non-UTF-8 bytes: {}",
            source.display()
        );
        anyhow::ensure!(
            state.mount_file_allowed(source)?,
            "provider config mount source is missing or not a regular file: {}",
            source.display()
        );

        let source = canonical_mount_path(source)?;
        anyhow::ensure!(
            jackin_core::container_paths::path_is_ancestor_or_equal(&authority, &source)
                && source != authority,
            "provider config mount source must stay inside the provider authority directory: {}",
            source.display()
        );
        overlays.push(ProviderConfigOverlay {
            source,
            target: jackin_core::container_paths::normalize_path(Path::new(target)),
        });
    }

    for (index, left) in overlays.iter().enumerate() {
        for right in overlays.iter().skip(index + 1) {
            anyhow::ensure!(
                !jackin_core::container_paths::paths_overlap(&left.target, &right.target),
                "provider config mount destinations overlap: {} and {}",
                left.target.display(),
                right.target.display()
            );
        }
    }

    Ok((authority, overlays))
}

fn instance_data_parent(state: &jackin_instance::RoleState) -> anyhow::Result<PathBuf> {
    let root = canonical_mount_path(&state.root)?;
    match root.parent() {
        Some(parent) => canonical_mount_path(parent),
        None => Ok(root),
    }
}

/// Return the authority directory for the instance child containing `source`.
/// The sibling child need not exist yet: the path schema itself is enough to
/// protect a future instance's provider-config authority.
fn sibling_authority_for_source(
    data_parent: &Path,
    source: &Path,
) -> anyhow::Result<Option<PathBuf>> {
    if !jackin_core::container_paths::path_is_ancestor_or_equal(data_parent, source)
        || source == data_parent
    {
        return Ok(None);
    }
    let Some(relative) = source.strip_prefix(data_parent).ok() else {
        return Ok(None);
    };
    let Some(Component::Normal(instance)) = relative.components().next() else {
        return Ok(None);
    };
    Ok(Some(canonical_mount_path(
        &data_parent.join(instance).join("provider-config"),
    )?))
}

fn exposed_provider_authority(
    data_parent: &Path,
    current_authority: &Path,
    source: &Path,
) -> anyhow::Result<Option<PathBuf>> {
    if jackin_core::container_paths::path_is_ancestor_or_equal(source, data_parent) {
        return Ok(Some(data_parent.to_owned()));
    }
    if jackin_core::container_paths::paths_overlap(current_authority, source) {
        return Ok(Some(current_authority.to_owned()));
    }
    let Some(sibling) = sibling_authority_for_source(data_parent, source)? else {
        return Ok(None);
    };
    Ok(jackin_core::container_paths::paths_overlap(&sibling, source).then_some(sibling))
}

fn parse_docker_bind(bind: &str) -> anyhow::Result<ParsedDockerBind> {
    let parts = bind.split(':').collect::<Vec<_>>();
    anyhow::ensure!(
        (2..=3).contains(&parts.len()),
        "ambiguous Docker bind mount with unsupported colon count: {bind}"
    );
    let source = parts[0];
    let target = parts[1];
    let source_path = Path::new(source);
    anyhow::ensure!(
        !source.is_empty() && !target.is_empty(),
        "Docker bind mount has an empty source or destination: {bind}"
    );
    anyhow::ensure!(
        !contains_parent_dir(source_path),
        "Docker bind mount source must not contain parent traversal: {bind}"
    );
    anyhow::ensure!(
        Path::new(target).is_absolute(),
        "Docker bind mount destination must be absolute: {bind}"
    );

    let mut saw_readonly = false;
    let mut saw_writable = false;
    if let Some(options) = parts.get(2) {
        for option in options.split(',') {
            match option {
                "ro" => saw_readonly = true,
                "rw" => saw_writable = true,
                _ => {}
            }
        }
    }
    anyhow::ensure!(
        !(saw_readonly && saw_writable),
        "Docker bind mount has conflicting ro/rw options: {bind}"
    );

    Ok(ParsedDockerBind {
        source: source_path
            .is_absolute()
            .then(|| canonical_mount_path(source_path))
            .transpose()?,
        raw_source: source_path.is_absolute().then(|| source_path.to_owned()),
        target: jackin_core::container_paths::normalize_path(Path::new(target)),
        readonly: saw_readonly,
    })
}

fn ensure_source_parents_outside_writable_roots(
    raw_source: &Path,
    writable_roots: &[PathBuf],
    backend: &str,
) -> anyhow::Result<()> {
    let mut ancestor = raw_source.parent();
    while let Some(path) = ancestor {
        let canonical = canonical_mount_path(path)?;
        for writable in writable_roots {
            if jackin_core::container_paths::path_is_ancestor_or_equal(writable, &canonical) {
                anyhow::bail!(
                    "{backend} bind source {} has parent {} inside writable bind source {}",
                    raw_source.display(),
                    canonical.display(),
                    writable.display()
                );
            }
        }
        ancestor = path.parent();
    }
    Ok(())
}

/// Host coordinator files must never enter a container, even read-only:
/// an exclusive flock needs no write access to the lock file.
fn ensure_protected_host_roots_not_exposed(
    raw_source: &Path,
    protected_host_roots: &[PathBuf],
    backend: &str,
) -> anyhow::Result<()> {
    let source = canonical_mount_path(raw_source)?;
    let lexical_source = jackin_core::container_paths::normalize_path(raw_source);
    for root in protected_host_roots {
        anyhow::ensure!(
            root.is_absolute() && !contains_parent_dir(root),
            "protected host root must be absolute without parent traversal: {}",
            root.display()
        );
        let lexical_root = jackin_core::container_paths::normalize_path(root);
        let canonical_root = canonical_mount_path(root)?;
        anyhow::ensure!(
            !jackin_core::container_paths::paths_overlap(&canonical_root, &source)
                && !jackin_core::container_paths::paths_overlap(&lexical_root, &lexical_source),
            "{backend} bind source {} exposes protected host root {}",
            raw_source.display(),
            root.display()
        );
        // A source reached through a symlink inside the protected namespace
        // still exposes its directory entry, even if the final target escapes.
        let mut ancestor = raw_source.parent();
        while let Some(parent) = ancestor {
            let lexical_parent = jackin_core::container_paths::normalize_path(parent);
            let canonical_parent = canonical_mount_path(parent)?;
            anyhow::ensure!(
                !jackin_core::container_paths::path_is_ancestor_or_equal(
                    &canonical_root,
                    &canonical_parent
                ) && !jackin_core::container_paths::path_is_ancestor_or_equal(
                    &lexical_root,
                    &lexical_parent
                ),
                "{backend} bind source {} has parent inside protected host root {}",
                raw_source.display(),
                root.display()
            );
            ancestor = parent.parent();
        }
    }
    Ok(())
}

/// Audit every Docker bind against the host-only provider configuration
/// authority. The generated files are the only authority paths allowed to
/// cross the container boundary, and each must be an exact read-only file
/// bind. Directory ancestors (including read-only ones) would expose staging,
/// locks, or another account's generated state; writable source overlap also
/// defeats the overlay's read-only guarantee.
pub fn ensure_provider_authority_not_writable(
    state: &jackin_instance::RoleState,
    mounts: &[String],
    protected_host_roots: &[PathBuf],
) -> anyhow::Result<()> {
    let (authority, overlays) = provider_config_overlays(state)?;
    let data_parent = instance_data_parent(state)?;
    let parsed = mounts
        .iter()
        .map(|mount| {
            parse_docker_bind(mount).map_err(|error| error.context("auditing Docker bind mounts"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let writable_roots = parsed
        .iter()
        .filter(|mount| !mount.readonly)
        .filter_map(|mount| mount.source.clone())
        .collect::<Vec<_>>();
    for mount in &parsed {
        if let Some(raw_source) = &mount.raw_source {
            ensure_protected_host_roots_not_exposed(raw_source, protected_host_roots, "Docker")?;
        }
    }
    for mount in &parsed {
        if let Some(raw_source) = &mount.raw_source {
            ensure_source_parents_outside_writable_roots(raw_source, &writable_roots, "Docker")?;
        }
    }

    for overlay in &overlays {
        anyhow::ensure!(
            parsed.iter().any(|mount| {
                mount.readonly
                    && mount.target == overlay.target
                    && mount.source.as_ref() == Some(&overlay.source)
            }),
            "provider config overlay is not mounted exactly read-only: {} -> {}",
            overlay.source.display(),
            overlay.target.display()
        );
    }

    for mount in &parsed {
        for overlay in &overlays {
            let exact_overlay = mount.readonly
                && mount.target == overlay.target
                && mount.source.as_ref() == Some(&overlay.source);
            if exact_overlay {
                continue;
            }

            anyhow::ensure!(
                !(mount.target == overlay.target
                    || jackin_core::container_paths::path_is_ancestor_or_equal(
                        &overlay.target,
                        &mount.target
                    )),
                "Docker bind target {} collides with protected provider config target {}",
                mount.target.display(),
                overlay.target.display()
            );
        }

        let Some(source) = &mount.source else {
            continue;
        };
        let exact_source = overlays.iter().any(|overlay| {
            overlay.source == *source && mount.target == overlay.target && mount.readonly
        });
        if exact_source {
            continue;
        }

        if let Some(exposed) = exposed_provider_authority(&data_parent, &authority, source)? {
            anyhow::bail!(
                "Docker bind source {} exposes the provider configuration authority {}",
                source.display(),
                exposed.display()
            );
        }
        anyhow::ensure!(
            overlays
                .iter()
                .all(|overlay| !jackin_core::container_paths::paths_overlap(
                    &overlay.source,
                    source
                )),
            "Docker bind source {} overlaps a protected provider config file",
            source.display()
        );
    }

    Ok(())
}

/// Audit Apple Container's complete typed bind list. Apple receives no
/// provider file overlays, so every host source that overlaps the current or
/// a sibling instance authority is rejected, including a source that targets
/// a future sibling directory which does not exist yet.
pub fn ensure_apple_provider_authority_not_exposed(
    state: &jackin_instance::RoleState,
    mounts: &[AppleContainerMount],
    protected_host_roots: &[PathBuf],
) -> anyhow::Result<()> {
    anyhow::ensure!(
        state.provider_config_mounts.is_empty(),
        "Apple Container cannot mount generated provider config file overlays"
    );
    let (authority, _) = provider_config_overlays(state)?;
    let data_parent = instance_data_parent(state)?;
    let writable_roots = mounts
        .iter()
        .filter(|mount| !mount.readonly)
        .map(|mount| canonical_mount_path(&mount.source))
        .collect::<anyhow::Result<Vec<_>>>()?;
    for mount in mounts {
        anyhow::ensure!(
            !contains_parent_dir(&mount.source),
            "Apple Container bind source must not contain parent traversal: {}",
            mount.source.display()
        );
        anyhow::ensure!(
            mount.source.is_absolute(),
            "Apple Container bind source must be absolute: {}",
            mount.source.display()
        );
        ensure_protected_host_roots_not_exposed(&mount.source, protected_host_roots, "Apple")?;
    }
    for mount in mounts {
        ensure_source_parents_outside_writable_roots(&mount.source, &writable_roots, "Apple")?;
        let source = canonical_mount_path(&mount.source)?;
        if let Some(exposed) = exposed_provider_authority(&data_parent, &authority, &source)? {
            anyhow::bail!(
                "Apple Container bind source {} exposes the provider configuration authority {}",
                source.display(),
                exposed.display()
            );
        }
    }
    Ok(())
}

fn append_provider_config_mounts(
    mounts: &mut Vec<String>,
    state: &jackin_instance::RoleState,
) -> anyhow::Result<()> {
    let (_, overlays) = provider_config_overlays(state)?;
    for overlay in overlays {
        mounts.push(format!(
            "{}:{}:ro",
            overlay.source.display(),
            overlay.target.display()
        ));
    }
    Ok(())
}

/// Returns the per-slot mount strings in jackin❯'s `src:dst[:ro]` idiom for
/// `docker run -v`.
///
/// Every provisioned slot is represented on `state.auth`, so the mount
/// block iterates slots rather than matching the selected-agent
/// variant. The foreground launch path provisions all admitted
/// instances so sibling tabs find their homes bind-mounted from the
/// start. Agents keep a fixed order; each agent's primary slot (legacy
/// destinations) mounts before its secondary slots in key order.
pub fn agent_mounts(state: &jackin_instance::RoleState) -> anyhow::Result<Vec<String>> {
    use jackin_core::Agent;
    let state_dir = state.root.join("state");
    anyhow::ensure!(
        state.mount_directory_allowed(&state_dir)?,
        "per-instance state directory is missing before docker launch"
    );
    let mut mounts = vec![format!("{}:/jackin/state", state_dir.display())];

    for agent in Agent::ALL {
        let mut slots: Vec<(&String, &jackin_instance::ProvisionedInstanceAuth)> = state
            .auth
            .slots
            .iter()
            .filter(|(_, slot)| slot.agent == *agent)
            .collect();
        slots.sort_by(|(a_key, a), (b_key, b)| {
            (a.slot_suffix.is_some(), *a_key).cmp(&(b.slot_suffix.is_some(), *b_key))
        });
        for (instance, slot) in slots {
            let credential = state
                .root
                .join("credentials")
                .join(jackin_protocol::account_credentials_filename(instance));
            if state.mount_file_allowed(&credential)? {
                mounts.push(format!(
                    "{}:{}:ro",
                    credential.display(),
                    jackin_protocol::account_credentials_container_path(instance)
                ));
            }
            push_slot_home_mounts(&mut mounts, &state.root, *agent, slot);
            push_slot_auth_mounts(&mut mounts, &state.root, state, slot)?;
        }
    }

    append_provider_config_mounts(&mut mounts, state)?;
    ensure_provider_authority_not_writable(state, &mounts, &[])?;

    Ok(mounts)
}

/// Build the directory-only equivalent of [`agent_mounts`] for
/// apple/container. That backend rejects single-file bind sources, so each
/// slot's already-unique auth store is mounted read-only as a directory. The
/// credentials transport is likewise a root-only directory mount; session
/// Landlock rules contain no access rule for it.
pub fn apple_agent_mounts(
    state: &jackin_instance::RoleState,
) -> anyhow::Result<Vec<AppleContainerMount>> {
    use jackin_core::Agent;

    anyhow::ensure!(
        state.provider_config_mounts.is_empty(),
        "Apple Container backend cannot mount generated provider config file overlays; use the Docker backend"
    );

    let credentials = state.root.join("credentials");
    anyhow::ensure!(
        state.mount_directory_allowed(&credentials)?,
        "per-instance credential directory is missing before apple/container launch"
    );
    let state_dir = state.root.join("state");
    anyhow::ensure!(
        state.mount_directory_allowed(&state_dir)?,
        "per-instance state directory is missing before apple/container launch"
    );
    let mut mounts = vec![
        AppleContainerMount::new(state_dir, "/jackin/state", false),
        AppleContainerMount::new(credentials, jackin_protocol::ACCOUNT_CREDENTIALS_DIR, true),
    ];

    for agent in Agent::ALL {
        let mut slots: Vec<(&String, &jackin_instance::ProvisionedInstanceAuth)> = state
            .auth
            .slots
            .iter()
            .filter(|(_, slot)| slot.agent == *agent)
            .collect();
        slots.sort_by(|(a_key, a), (b_key, b)| {
            (a.slot_suffix.is_some(), *a_key).cmp(&(b.slot_suffix.is_some(), *b_key))
        });
        for (_, slot) in slots {
            let paths = agent.runtime().state_paths();
            let home = state.root.join("home");
            mounts.push(AppleContainerMount::new(
                home.join(&slot.container_home_rel),
                format!("/home/agent/{}", slot.container_home_rel),
                false,
            ));
            if let (Some(source), Some(rel)) = (&slot.cache_source_dir, &slot.container_cache_rel) {
                mounts.push(AppleContainerMount::new(
                    source.clone(),
                    format!("/home/agent/{rel}"),
                    false,
                ));
            }
            for entry in paths
                .home_dirs()
                .filter(|entry| *entry != paths.credential_dir)
            {
                let rel = jackin_instance::slot_home_rel(entry, slot.slot_suffix.as_deref());
                mounts.push(AppleContainerMount::new(
                    home.join(&rel),
                    format!("/home/agent/{rel}"),
                    false,
                ));
            }
            if slot.forward_auth {
                let store = state.root.join(&slot.container_store_rel);
                anyhow::ensure!(
                    state.auth_mount_directory_allowed(&store)?,
                    "private auth store is missing before apple/container launch: {}",
                    store.display()
                );
                mounts.push(AppleContainerMount::new(
                    store,
                    format!("/jackin/{}", slot.container_store_rel),
                    true,
                ));
            }
        }
    }
    ensure_apple_provider_authority_not_exposed(state, &mounts, &[])?;
    Ok(mounts)
}

pub fn github_config_mount(state: &jackin_instance::RoleState) -> anyhow::Result<Option<String>> {
    if matches!(
        state.gh_provision_outcome,
        jackin_instance::GithubProvisionOutcome::Skipped
    ) && !state.mount_directory_allowed(&state.gh_config_dir)?
    {
        Ok(None)
    } else {
        anyhow::ensure!(
            state.mount_directory_allowed(&state.gh_config_dir)?,
            "GitHub config directory is missing before container launch"
        );
        Ok(Some(format!(
            "{}:/home/agent/.config/gh",
            state.gh_config_dir.display()
        )))
    }
}

/// Translate a [`MaterializedWorkspace`] into the `-v` argument values
/// for `docker run`. Pulled out of `load_role_with` so the mount-flag
/// shape — including the `:ro` placement on worktree-mode override
/// files — can be unit-tested without docker mocks.
///
/// For each mount, the worktree dir / shared bind goes first; when the
/// mount is worktree-mode, three auxiliary entries follow:
///
/// 1. Host's `.git/` at `/jackin/host/<dst-stripped>/.git` (rw).
///    Includes the per-worktree admin dir at `worktrees/<container>/`
///    natively (no separate admin mount).
/// 2. `.git` pointer override at `<dst>/.git` (`:ro`). Redirects gitdir
///    to the admin entry inside the host `.git/` mount.
/// 3. `gitdir` back-pointer override at
///    `/jackin/host/<dst-stripped>/.git/worktrees/<container>/gitdir`
///    (`:ro`). Matches the worktree's `<dst>/.git` location so git's
///    verification check passes inside the container.
///
/// `:ro` on the override files is defensive hardening: git only reads
/// them during normal role work, and a misbehaving role could
/// otherwise rewrite the gitdir pointer to redirect operations at a
/// different repo entirely.
pub fn build_workspace_mount_strings(workspace: &MaterializedWorkspace) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for mount in jackin_isolation::materialize::mount_order_for_docker(workspace) {
        let suffix = if mount.readonly { ":ro" } else { "" };
        out.push(format!("{}:{}{}", mount.bind_src, mount.dst, suffix));
        if let Some(aux) = &mount.worktree_aux {
            out.push(format!("{}:{}", aux.host_git_dir, aux.host_git_target));
            out.push(format!(
                "{}:{}:ro",
                aux.git_file_override, aux.git_file_target
            ));
            out.push(format!(
                "{}:{}:ro",
                aux.gitdir_back_override, aux.gitdir_back_target
            ));
        }
    }
    out
}

/// The container backend selected for a launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Docker,
    AppleContainer,
}

/// Resolve the container backend for a launch. A per-workspace
/// `[runtime].backend` overrides the host-wide `[runtime].default_backend`,
/// which defaults to Docker when unset.
///
/// The backend fields are free-text strings in the config schema, so an
/// unrecognised value is rejected here rather than silently falling through to
/// Docker — a typo must fail closed, not launch the wrong (weaker-isolation)
/// backend behind the operator's back.
pub fn resolve_backend(
    config: &AppConfig,
    workspace_name: Option<&str>,
) -> anyhow::Result<Backend> {
    let selected = workspace_name
        .and_then(|name| config.workspaces.get(name))
        .and_then(|ws| ws.runtime.backend.as_deref())
        .or(config.runtime.default_backend.as_deref());
    match selected {
        None
        | Some(
            jackin_runtime_apple_container_client::apple_container_client::DOCKER_BACKEND_NAME,
        ) => Ok(Backend::Docker),
        Some(jackin_runtime_apple_container_client::apple_container_client::BACKEND_NAME) => {
            Ok(Backend::AppleContainer)
        }
        Some(other) => anyhow::bail!(
            "unknown runtime backend {other:?}: expected `{}` or `{}`",
            jackin_runtime_apple_container_client::apple_container_client::DOCKER_BACKEND_NAME,
            jackin_runtime_apple_container_client::apple_container_client::BACKEND_NAME,
        ),
    }
}

/// Translate a [`MaterializedWorkspace`] into typed apple-container mounts.
/// Apple `container` v0.11.0+ accepts Docker-compatible `:ro` options on `-v`
/// directory mounts but rejects single-file bind sources. Shared mounts retain
/// their configured permissions; worktree isolation fails closed because its
/// two read-only pointer-file overlays cannot be represented safely.
pub fn build_workspace_mounts(
    workspace: &MaterializedWorkspace,
) -> Result<Vec<AppleContainerMount>, AppleContainerMountError> {
    let mut out = Vec::new();
    for mount in jackin_isolation::materialize::mount_order_for_docker(workspace) {
        if mount.worktree_aux.is_some() {
            return Err(AppleContainerMountError::WorktreeFileOverlays {
                destination: mount.dst.clone(),
            });
        }
        out.push(AppleContainerMount::new(
            &mount.bind_src,
            &mount.dst,
            mount.readonly,
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
