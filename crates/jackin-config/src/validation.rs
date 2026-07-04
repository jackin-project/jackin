// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Pure workspace validation helpers: isolation layout and workspace config.

use jackin_core::MountIsolation;

use crate::schema::{MountConfig, WorkspaceConfig, validate_mount_specs};

/// Validate the isolation layout for a workspace's mounts.
///
/// Two rules enforced:
/// 1. No nested isolated mounts (inner dst inside outer dst).
/// 2. No same-host-repo worktree siblings (same src canonicalized).
pub fn validate_isolation_layout(mounts: &[MountConfig]) -> anyhow::Result<()> {
    let isolated: Vec<(usize, &MountConfig, &str)> = mounts
        .iter()
        .enumerate()
        .filter(|(_, m)| !m.isolation.is_shared())
        .map(|(i, m)| (i, m, m.dst.trim_end_matches('/')))
        .collect();

    for (i, (_, ma, a)) in isolated.iter().enumerate() {
        for (_, mb, b) in &isolated[i + 1..] {
            if is_strict_ancestor(a, b) || is_strict_ancestor(b, a) {
                anyhow::bail!(
                    "isolated mount `{b}` cannot be nested inside isolated mount `{a}`; \
                     either make the inner mount `shared` or move the inner mount outside \
                     the parent's path",
                    a = if is_strict_ancestor(a, b) { a } else { b },
                    b = if is_strict_ancestor(a, b) { b } else { a },
                );
            }
            if matches!(ma.isolation, MountIsolation::Worktree)
                && matches!(mb.isolation, MountIsolation::Worktree)
                && same_host_repo(&ma.src, &mb.src)
            {
                anyhow::bail!(
                    "isolated mounts `{}` and `{}` cannot share the same host repository `{}`; \
                     remove one of them or change one to `shared` (V1 limitation — see roadmap)",
                    ma.dst,
                    mb.dst,
                    ma.src,
                );
            }
        }
    }
    Ok(())
}

pub fn validate_workspace_config(name: &str, workspace: &WorkspaceConfig) -> anyhow::Result<()> {
    if workspace.workdir.is_empty() {
        anyhow::bail!("workspace {name:?} must define workdir");
    }
    if !workspace.workdir.starts_with('/') {
        anyhow::bail!("workspace {name:?} workdir must be an absolute container path");
    }
    if workspace.mounts.is_empty() {
        anyhow::bail!("workspace {name:?} must define at least one mount");
    }

    validate_mount_specs(&workspace.mounts)?;
    validate_isolation_layout(&workspace.mounts)?;
    workspace.validate_auth_modes()?;

    let covers_workdir = workspace.mounts.iter().any(|mount| {
        let dst = mount.dst.trim_end_matches('/');
        workspace.workdir == dst
            || workspace.workdir.starts_with(&format!("{dst}/"))
            || dst.starts_with(&format!("{}/", workspace.workdir.trim_end_matches('/')))
    });
    anyhow::ensure!(
        covers_workdir,
        "workspace {name:?} workdir must be equal to, inside, or a parent of one of the workspace mount destinations"
    );

    if let Some(default_role) = &workspace.default_role
        && !workspace.allowed_roles.is_empty()
        && !workspace
            .allowed_roles
            .iter()
            .any(|role| role == default_role)
    {
        anyhow::bail!(
            "workspace {name:?} default_role must be a member of allowed_roles when allowed_roles is set"
        );
    }

    Ok(())
}

fn same_host_repo(a: &str, b: &str) -> bool {
    let ca = std::fs::canonicalize(a).ok();
    let cb = std::fs::canonicalize(b).ok();
    match (ca, cb) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

fn is_strict_ancestor(parent: &str, child: &str) -> bool {
    let parent = parent.trim_end_matches('/');
    let child = child.trim_end_matches('/');
    if parent == child {
        return false;
    }
    let prefix = format!("{parent}/");
    child.starts_with(&prefix)
}
