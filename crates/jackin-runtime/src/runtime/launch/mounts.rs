// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Mount construction helpers extracted from the launch coordinator.
//! All items re-exported from the parent to preserve `super::` call sites
//! in `launch_role_runtime` and `launch_pipeline.rs`.

use std::path::Path;

use jackin_config::AppConfig;

use crate::apple_container_client::AppleContainerMount;
use crate::isolation::materialize::MaterializedWorkspace;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum AppleContainerMountError {
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
    slot: &crate::instance::ProvisionedInstanceAuth,
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
        let rel = crate::instance::slot_home_rel(entry, slot.slot_suffix.as_deref());
        mounts.push(format!("{}:/home/agent/{rel}", home.join(&rel).display()));
    }
}

/// Emit the auth-handoff mounts for one provisioned slot under its
/// container store dir (`/jackin/<agent>` for primary slots,
/// `/jackin/<agent>-<suffix>` for secondary same-agent slots).
///
/// File-credential agents mount each provisioned file by file name;
/// Kimi/Hermes mount their credential directory. Claude keeps its
/// per-file `exists()` guards: `forward_auth = true` covers `Sync`
/// (host-derived credentials) and `OAuthToken` (the onboarding
/// skeleton), while `ApiKey` and `Ignore` wipe the role-state files —
/// and the guard keeps the `OAuthToken` arm from mounting a stale
/// `credentials.json` if the provision-step removal failed silently.
fn push_slot_auth_mounts(
    mounts: &mut Vec<String>,
    root: &Path,
    slot: &crate::instance::ProvisionedInstanceAuth,
) {
    use jackin_core::Agent;
    if !slot.forward_auth {
        return;
    }
    if matches!(slot.agent, Agent::Kimi | Agent::Hermes) {
        let store = root.join(&slot.container_store_rel);
        mounts.push(format!(
            "{}:/jackin/{}:ro",
            store.display(),
            slot.container_store_rel
        ));
        return;
    }
    for path in &slot.credential_paths {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let guarded = matches!(slot.agent, Agent::Claude) && !path.exists();
        if !guarded {
            mounts.push(format!(
                "{}:/jackin/{}/{}:ro",
                path.display(),
                slot.container_store_rel,
                file_name
            ));
        }
    }
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
pub(crate) fn agent_mounts(state: &crate::instance::RoleState) -> Vec<String> {
    use jackin_core::Agent;
    let mut mounts = vec![format!(
        "{}:/jackin/state",
        state.root.join("state").display()
    )];

    for agent in Agent::ALL {
        let mut slots: Vec<(&String, &crate::instance::ProvisionedInstanceAuth)> = state
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
            if credential.is_file() {
                mounts.push(format!(
                    "{}:{}:ro",
                    credential.display(),
                    jackin_protocol::account_credentials_container_path(instance)
                ));
            }
            push_slot_home_mounts(&mut mounts, &state.root, *agent, slot);
            push_slot_auth_mounts(&mut mounts, &state.root, slot);
        }
    }

    mounts
}

/// Build the directory-only equivalent of [`agent_mounts`] for
/// apple/container. That backend rejects single-file bind sources, so each
/// slot's already-unique auth store is mounted read-only as a directory. The
/// credentials transport is likewise a root-only directory mount; session
/// Landlock rules contain no access rule for it.
pub(crate) fn apple_agent_mounts(
    state: &crate::instance::RoleState,
) -> anyhow::Result<Vec<AppleContainerMount>> {
    use jackin_core::Agent;

    let credentials = state.root.join("credentials");
    anyhow::ensure!(
        credentials.is_dir(),
        "per-instance credential directory is missing before apple/container launch"
    );
    let mut mounts = vec![
        AppleContainerMount::new(state.root.join("state"), "/jackin/state", false),
        AppleContainerMount::new(credentials, jackin_protocol::ACCOUNT_CREDENTIALS_DIR, true),
    ];

    for agent in Agent::ALL {
        let mut slots: Vec<(&String, &crate::instance::ProvisionedInstanceAuth)> = state
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
                let rel = crate::instance::slot_home_rel(entry, slot.slot_suffix.as_deref());
                mounts.push(AppleContainerMount::new(
                    home.join(&rel),
                    format!("/home/agent/{rel}"),
                    false,
                ));
            }
            if slot.forward_auth {
                let store = state.root.join(&slot.container_store_rel);
                anyhow::ensure!(
                    store.is_dir(),
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
    Ok(mounts)
}

pub(crate) fn github_config_mount(state: &crate::instance::RoleState) -> Option<String> {
    if matches!(
        state.gh_provision_outcome,
        crate::instance::GithubProvisionOutcome::Skipped
    ) && !state.gh_config_dir.exists()
    {
        None
    } else {
        Some(format!(
            "{}:/home/agent/.config/gh",
            state.gh_config_dir.display()
        ))
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
pub(crate) fn build_workspace_mount_strings(workspace: &MaterializedWorkspace) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for mount in crate::isolation::materialize::mount_order_for_docker(workspace) {
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
pub(crate) enum Backend {
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
pub(crate) fn resolve_backend(
    config: &AppConfig,
    workspace_name: Option<&str>,
) -> anyhow::Result<Backend> {
    let selected = workspace_name
        .and_then(|name| config.workspaces.get(name))
        .and_then(|ws| ws.runtime.backend.as_deref())
        .or(config.runtime.default_backend.as_deref());
    match selected {
        None | Some(crate::apple_container_client::DOCKER_BACKEND_NAME) => Ok(Backend::Docker),
        Some(crate::apple_container_client::BACKEND_NAME) => Ok(Backend::AppleContainer),
        Some(other) => anyhow::bail!(
            "unknown runtime backend {other:?}: expected `{}` or `{}`",
            crate::apple_container_client::DOCKER_BACKEND_NAME,
            crate::apple_container_client::BACKEND_NAME,
        ),
    }
}

/// Translate a [`MaterializedWorkspace`] into typed apple-container mounts.
/// Apple `container` v0.11.0+ accepts Docker-compatible `:ro` options on `-v`
/// directory mounts but rejects single-file bind sources. Shared mounts retain
/// their configured permissions; worktree isolation fails closed because its
/// two read-only pointer-file overlays cannot be represented safely.
pub(crate) fn build_workspace_mounts(
    workspace: &MaterializedWorkspace,
) -> Result<Vec<AppleContainerMount>, AppleContainerMountError> {
    let mut out = Vec::new();
    for mount in crate::isolation::materialize::mount_order_for_docker(workspace) {
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
