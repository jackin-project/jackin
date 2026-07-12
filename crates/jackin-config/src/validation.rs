//! Pure workspace validation helpers: isolation layout and workspace config.

use crate::ConfigError;
use jackin_core::MountIsolation;
use jackin_core::WorkspaceName;

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
        let Some(rest) = isolated.get(i + 1..) else {
            break;
        };
        for (_, mb, b) in rest {
            if is_strict_ancestor(a, b) || is_strict_ancestor(b, a) {
                let (outer, inner) = if is_strict_ancestor(a, b) {
                    (*a, *b)
                } else {
                    (*b, *a)
                };
                return Err(ConfigError::msg(format!(
                    "isolated mount `{inner}` cannot be nested inside isolated mount `{outer}`; \
                     either make the inner mount `shared` or move the inner mount outside \
                     the parent's path"
                ))
                .into());
            }
            if matches!(ma.isolation, MountIsolation::Worktree)
                && matches!(mb.isolation, MountIsolation::Worktree)
                && same_host_repo(&ma.src, &mb.src)
            {
                return Err(ConfigError::msg(format!(
                    "isolated mounts `{}` and `{}` cannot share the same host repository `{}`; \
                     remove one of them or change one to `shared` (V1 limitation — see roadmap)",
                    ma.dst, mb.dst, ma.src,
                ))
                .into());
            }
        }
    }
    Ok(())
}

/// Validate a saved workspace: workdir, mounts, isolation, auth modes, roles.
pub fn validate_workspace_config(
    name: &WorkspaceName,
    workspace: &WorkspaceConfig,
) -> anyhow::Result<()> {
    // Use Debug of the stem string (not Debug of the newtype) so operator
    // messages stay `workspace "foo"` rather than `WorkspaceName("foo")`.
    let name = name.as_str();
    if workspace.workdir.is_empty() {
        return Err(ConfigError::WorkdirRequired(name.to_owned()).into());
    }
    if !workspace.workdir.starts_with('/') {
        return Err(ConfigError::WorkdirNotAbsolute(name.to_owned()).into());
    }
    if workspace.mounts.is_empty() {
        return Err(ConfigError::MountsRequired(name.to_owned()).into());
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
    if !covers_workdir {
        return Err(ConfigError::msg(format!(
            "workspace {name:?} workdir must be equal to, inside, or a parent of one of the workspace mount destinations"
        ))
        .into());
    }

    if let Some(default_role) = &workspace.default_role
        && !workspace.allowed_roles.is_empty()
        && !workspace
            .allowed_roles
            .iter()
            .any(|role| role == default_role)
    {
        return Err(ConfigError::msg(format!(
            "workspace {name:?} default_role must be a member of allowed_roles when allowed_roles is set"
        ))
        .into());
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
