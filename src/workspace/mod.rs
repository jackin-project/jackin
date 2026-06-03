//! Workspace configuration types and resolution.
//!
//! Schema types (`WorkspaceConfig`, `MountConfig`, etc.) are defined in
//! `jackin-config` and re-exported here. This module owns:
//! - Path helpers, planner, mount validation, workspace resolution
//! - `WorkspaceEdit` mutation helper (not a schema type)
//! - `validate_workspace_config` (behavior, not schema)
//! - The orphan-rule impls for jackin-console traits on jackin-config types
//!
//! Not responsible for: reading or writing workspace files (`config/editor.rs`
//! via `ConfigEditor`), or container mount materialization
//! (`isolation/materialize.rs`).

pub mod mounts;
pub mod paths;
pub(crate) mod planner;
pub mod resolve;
pub mod sensitive;
pub mod token_setup;

pub use mounts::{
    parse_mount_spec, parse_mount_spec_resolved, validate_mount_paths, validate_mount_specs,
    validate_mounts,
};
pub use paths::{expand_tilde, resolve_path};
pub use planner::{CollapseError, CollapsePlan, Removal, plan_collapse};
pub use resolve::{
    LoadWorkspaceInput, ResolvedWorkspace, current_dir_workspace, resolve_load_workspace,
    saved_workspace_match_depth,
};
pub use sensitive::{SensitiveMount, confirm_sensitive_mounts, find_sensitive_mounts};

/// Re-exported schema types from `jackin-config`.
pub use jackin_config::{
    KeepAwakeConfig, MountConfig, MountIsolation, WorkspaceConfig, WorkspaceRoleOverride,
};

// ─── WorkspaceEdit (mutation helper, not a schema type) ───────────────────────

/// Mutation specification for `AppConfig::edit_workspace`.
///
/// Not serialized — built by CLI and TUI callers, then applied by the editor.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceEdit {
    pub workdir: Option<String>,
    pub upsert_mounts: Vec<MountConfig>,
    pub remove_destinations: Vec<String>,
    pub no_workdir_mount: bool,
    pub allowed_roles_to_add: Vec<String>,
    pub allowed_roles_to_remove: Vec<String>,
    pub default_role: Option<Option<String>>,
    /// `None` = no change, `Some(Some(h))` = set to `h`,
    /// `Some(None)` = clear (fall back to Claude).
    pub default_agent: Option<Option<crate::agent::Agent>>,
    pub mount_isolation_overrides: Vec<(String, crate::isolation::MountIsolation)>,
    pub keep_awake_enabled: Option<bool>,
    pub git_pull_on_entry_enabled: Option<bool>,
}

// ─── Workspace validation ─────────────────────────────────────────────────────

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

#[cfg(test)]
mod tests;
