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
                },
                MountConfig {
                    src: "/tmp/b".to_string(),
                    dst: "/workspace/project".to_string(),
                    readonly: false,
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
}
