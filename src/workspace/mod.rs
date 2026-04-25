pub mod mounts;
pub mod paths;
pub(crate) mod planner;
pub mod resolve;
pub mod sensitive;

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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MountConfig {
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub readonly: bool,
    #[serde(
        default,
        skip_serializing_if = "crate::isolation::MountIsolation::is_shared"
    )]
    pub isolation: crate::isolation::MountIsolation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceConfig {
    pub workdir: String,
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
    #[serde(default)]
    pub allowed_agents: Vec<String>,
    #[serde(default)]
    pub default_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_agent: Option<String>,
    /// Workspace-level operator env map. Keys are env var names;
    /// values use the `operator_env` dispatch syntax
    /// (`op://...` | `$NAME` | `${NAME}` | literal).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: std::collections::BTreeMap<String, String>,
    /// Per-(workspace × agent) env overrides, keyed by the agent
    /// selector (e.g. `"agent-smith"` or `"chainargos/agent-brown"`).
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub agents: std::collections::BTreeMap<String, WorkspaceAgentOverride>,
}

/// Per-(workspace × agent) operator overrides.
///
/// Currently only `env` is supported; the struct exists as a named type
/// so future overrides (e.g. `auth_forward`) can be added without a
/// TOML schema break.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceAgentOverride {
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub env: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceEdit {
    pub workdir: Option<String>,
    pub upsert_mounts: Vec<MountConfig>,
    pub remove_destinations: Vec<String>,
    pub no_workdir_mount: bool,
    pub allowed_agents_to_add: Vec<String>,
    pub allowed_agents_to_remove: Vec<String>,
    pub default_agent: Option<Option<String>>,
    pub mount_isolation_overrides: Vec<(String, crate::isolation::MountIsolation)>,
}

/// Reject two `Worktree` mounts where one's `dst` is a strict ancestor of
/// the other's. Sibling isolated mounts and isolated-parent-with-shared-child
/// remain allowed.
pub fn validate_isolation_layout(mounts: &[MountConfig]) -> anyhow::Result<()> {
    use crate::isolation::MountIsolation;

    let isolated: Vec<(usize, &str)> = mounts
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m.isolation, MountIsolation::Worktree))
        .map(|(i, m)| (i, m.dst.trim_end_matches('/')))
        .collect();

    for (i, (_, a)) in isolated.iter().enumerate() {
        for (_, b) in &isolated[i + 1..] {
            if is_strict_ancestor(a, b) || is_strict_ancestor(b, a) {
                anyhow::bail!(
                    "isolated mount `{b}` cannot be nested inside isolated mount `{a}`; \
                     either make the inner mount `shared` or move the inner mount outside \
                     the parent's path",
                    a = if is_strict_ancestor(a, b) { a } else { b },
                    b = if is_strict_ancestor(a, b) { b } else { a },
                );
            }
        }
    }
    Ok(())
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

    if let Some(default_agent) = &workspace.default_agent
        && !workspace.allowed_agents.is_empty()
        && !workspace
            .allowed_agents
            .iter()
            .any(|agent| agent == default_agent)
    {
        anyhow::bail!(
            "workspace {name:?} default_agent must be a member of allowed_agents when allowed_agents is set"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- validate_workspace_config: workdir vs mount destination coverage ------

    fn workspace_with_workdir_and_dst(workdir: &str, dst: &str) -> WorkspaceConfig {
        WorkspaceConfig {
            workdir: workdir.to_string(),
            mounts: vec![MountConfig {
                src: "/tmp/src".to_string(),
                dst: dst.to_string(),
                readonly: false,
                isolation: crate::isolation::MountIsolation::Shared,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn validate_workdir_equal_to_mount_dst() {
        let ws = workspace_with_workdir_and_dst("/workspace/project", "/workspace/project");
        validate_workspace_config("test", &ws).unwrap();
    }

    #[test]
    fn validate_workdir_inside_mount_dst() {
        let ws = workspace_with_workdir_and_dst("/workspace/project/src", "/workspace/project");
        validate_workspace_config("test", &ws).unwrap();
    }

    #[test]
    fn validate_workdir_deeply_nested_inside_mount_dst() {
        let ws =
            workspace_with_workdir_and_dst("/workspace/project/src/main", "/workspace/project");
        validate_workspace_config("test", &ws).unwrap();
    }

    #[test]
    fn validate_workdir_parent_of_mount_dst() {
        let ws = workspace_with_workdir_and_dst("/workspace", "/workspace/project");
        validate_workspace_config("test", &ws).unwrap();
    }

    #[test]
    fn validate_workdir_grandparent_of_mount_dst() {
        let ws = workspace_with_workdir_and_dst("/workspace", "/workspace/project/src");
        validate_workspace_config("test", &ws).unwrap();
    }

    #[test]
    fn validate_workdir_parent_with_trailing_slash_on_dst() {
        let ws = workspace_with_workdir_and_dst("/workspace", "/workspace/project/");
        validate_workspace_config("test", &ws).unwrap();
    }

    #[test]
    fn validate_rejects_workdir_sibling_of_mount_dst() {
        let ws = workspace_with_workdir_and_dst("/workspace/other", "/workspace/project");
        let err = validate_workspace_config("test", &ws).unwrap_err();
        assert!(err.to_string().contains(
            "must be equal to, inside, or a parent of one of the workspace mount destinations"
        ));
    }

    #[test]
    fn validate_rejects_workdir_with_prefix_overlap_but_not_parent() {
        // /workspace/project-v2 is NOT inside /workspace/project
        let ws = workspace_with_workdir_and_dst("/workspace/project-v2", "/workspace/project");
        let err = validate_workspace_config("test", &ws).unwrap_err();
        assert!(err.to_string().contains(
            "must be equal to, inside, or a parent of one of the workspace mount destinations"
        ));
    }

    #[test]
    fn validate_rejects_mount_dst_with_prefix_overlap_but_not_child() {
        // /workspace/project is NOT a parent of /workspace/project-v2
        let ws = workspace_with_workdir_and_dst("/workspace/project", "/workspace/project-v2");
        let err = validate_workspace_config("test", &ws).unwrap_err();
        assert!(err.to_string().contains(
            "must be equal to, inside, or a parent of one of the workspace mount destinations"
        ));
    }

    #[test]
    fn validate_rejects_completely_unrelated_workdir() {
        let ws = workspace_with_workdir_and_dst("/home/user", "/workspace/project");
        let err = validate_workspace_config("test", &ws).unwrap_err();
        assert!(err.to_string().contains(
            "must be equal to, inside, or a parent of one of the workspace mount destinations"
        ));
    }

    #[test]
    fn validate_workdir_parent_of_any_mount_dst() {
        let ws = WorkspaceConfig {
            workdir: "/workspace".to_string(),
            mounts: vec![
                MountConfig {
                    src: "/tmp/a".to_string(),
                    dst: "/other/path".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
                MountConfig {
                    src: "/tmp/b".to_string(),
                    dst: "/workspace/project".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            ],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
        };
        validate_workspace_config("test", &ws).unwrap();
    }

    use crate::isolation::MountIsolation;

    #[test]
    fn mount_config_defaults_isolation_to_shared() {
        let toml = r#"src = "/tmp/src"
dst = "/workspace/x"
"#;
        let mount: MountConfig = toml::from_str(toml).unwrap();
        assert_eq!(mount.isolation, MountIsolation::Shared);
    }

    #[test]
    fn mount_config_parses_worktree_isolation() {
        let toml = r#"src = "/tmp/src"
dst = "/workspace/x"
isolation = "worktree"
"#;
        let mount: MountConfig = toml::from_str(toml).unwrap();
        assert_eq!(mount.isolation, MountIsolation::Worktree);
    }

    #[test]
    fn mount_config_omits_isolation_field_when_shared_on_serialize() {
        let mount = MountConfig {
            src: "/tmp/src".into(),
            dst: "/workspace/x".into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        };
        let serialized = toml::to_string(&mount).unwrap();
        assert!(!serialized.contains("isolation"));
    }

    #[test]
    fn mount_config_emits_isolation_field_when_non_shared_on_serialize() {
        let mount = MountConfig {
            src: "/tmp/src".into(),
            dst: "/workspace/x".into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        };
        let serialized = toml::to_string(&mount).unwrap();
        assert!(serialized.contains(r#"isolation = "worktree""#));
    }

    fn worktree_mount(src: &str, dst: &str) -> MountConfig {
        MountConfig {
            src: src.into(),
            dst: dst.into(),
            readonly: false,
            isolation: MountIsolation::Worktree,
        }
    }

    fn shared_mount(src: &str, dst: &str) -> MountConfig {
        MountConfig {
            src: src.into(),
            dst: dst.into(),
            readonly: false,
            isolation: MountIsolation::Shared,
        }
    }

    #[test]
    fn isolation_layout_allows_one_worktree_plus_n_shared() {
        let mounts = vec![
            worktree_mount("/tmp/a", "/workspace/a"),
            shared_mount("/tmp/cache", "/workspace/cache"),
        ];
        validate_isolation_layout(&mounts).unwrap();
    }

    #[test]
    fn isolation_layout_allows_sibling_worktrees() {
        let mounts = vec![
            worktree_mount("/tmp/a", "/workspace/a"),
            worktree_mount("/tmp/b", "/workspace/b"),
        ];
        validate_isolation_layout(&mounts).unwrap();
    }

    #[test]
    fn isolation_layout_allows_isolated_parent_with_shared_child() {
        let mounts = vec![
            worktree_mount("/tmp/proj", "/workspace/proj"),
            shared_mount("/tmp/proj-target", "/workspace/proj/target"),
        ];
        validate_isolation_layout(&mounts).unwrap();
    }

    #[test]
    fn isolation_layout_rejects_nested_worktrees_parent_child() {
        let mounts = vec![
            worktree_mount("/tmp/proj", "/workspace/proj"),
            worktree_mount("/tmp/sub", "/workspace/proj/sub"),
        ];
        let err = validate_isolation_layout(&mounts).unwrap_err().to_string();
        assert!(err.contains("/workspace/proj"), "missing parent dst: {err}");
        assert!(
            err.contains("/workspace/proj/sub"),
            "missing child dst: {err}"
        );
    }

    #[test]
    fn isolation_layout_rejects_nested_worktrees_grandparent() {
        let mounts = vec![
            worktree_mount("/tmp/a", "/workspace"),
            worktree_mount("/tmp/b", "/workspace/proj/sub"),
        ];
        let err = validate_isolation_layout(&mounts).unwrap_err().to_string();
        assert!(err.contains("/workspace") && err.contains("/workspace/proj/sub"));
    }

    #[test]
    fn isolation_layout_ignores_trailing_slashes() {
        let mounts = vec![
            worktree_mount("/tmp/a", "/workspace/proj/"),
            worktree_mount("/tmp/b", "/workspace/proj/sub/"),
        ];
        let err = validate_isolation_layout(&mounts).unwrap_err().to_string();
        assert!(err.contains("/workspace/proj"));
    }
}
