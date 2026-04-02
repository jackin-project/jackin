use serde::{Deserialize, Serialize};
use std::path::Path;

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
}

#[derive(Debug, Clone, Default)]
pub struct WorkspaceEdit {
    pub workdir: Option<String>,
    pub upsert_mounts: Vec<MountConfig>,
    pub remove_destinations: Vec<String>,
    pub allowed_agents_to_add: Vec<String>,
    pub allowed_agents_to_remove: Vec<String>,
    pub default_agent: Option<Option<String>>,
}

pub fn expand_tilde(path: &str) -> String {
    if (path == "~" || path.starts_with("~/")) && let Ok(home) = std::env::var("HOME") {
        return path.replacen('~', &home, 1);
    }

    path.to_string()
}

pub fn parse_mount_spec(spec: &str) -> anyhow::Result<MountConfig> {
    let (raw, readonly) = match spec.strip_suffix(":ro") {
        Some(value) => (value, true),
        None => (spec, false),
    };
    let (src, dst) = raw
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid mount spec {spec:?}; expected src:dst[:ro]"))?;

    Ok(MountConfig {
        src: expand_tilde(src),
        dst: dst.to_string(),
        readonly,
    })
}

pub fn validate_mounts(mounts: &[MountConfig]) -> anyhow::Result<()> {
    let mut seen_dst = std::collections::HashSet::new();

    for mount in mounts {
        if !Path::new(&mount.src).is_absolute() {
            anyhow::bail!("mount source must be absolute: {}", mount.src);
        }
        if !Path::new(&mount.src).exists() {
            anyhow::bail!("mount source does not exist: {}", mount.src);
        }
        if !mount.dst.starts_with('/') {
            anyhow::bail!("mount destination must be an absolute path: {}", mount.dst);
        }
        if !seen_dst.insert(mount.dst.clone()) {
            anyhow::bail!("duplicate mount destination: {}", mount.dst);
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

    validate_mounts(&workspace.mounts)?;

    let within_mount = workspace.mounts.iter().any(|mount| {
        workspace.workdir == mount.dst
            || workspace
                .workdir
                .starts_with(&format!("{}/", mount.dst.trim_end_matches('/')))
    });
    anyhow::ensure!(
        within_mount,
        "workspace {name:?} workdir must be equal to or inside one of the workspace mount destinations"
    );

    if let Some(default_agent) = &workspace.default_agent
        && !workspace.allowed_agents.is_empty()
        && !workspace.allowed_agents.iter().any(|agent| agent == default_agent)
    {
        anyhow::bail!(
            "workspace {name:?} default_agent must be a member of allowed_agents when allowed_agents is set"
        );
    }

    Ok(())
}

pub fn current_dir_workspace(cwd: &Path) -> anyhow::Result<WorkspaceConfig> {
    let cwd = cwd.canonicalize()?;
    let path = cwd.display().to_string();

    Ok(WorkspaceConfig {
        workdir: path.clone(),
        mounts: vec![MountConfig {
            src: path.clone(),
            dst: path,
            readonly: false,
        }],
        allowed_agents: vec![],
        default_agent: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_mount_spec_with_optional_readonly_suffix() {
        let mount = parse_mount_spec("/tmp/cache:/workspace/cache:ro").unwrap();

        assert_eq!(mount.src, "/tmp/cache");
        assert_eq!(mount.dst, "/workspace/cache");
        assert!(mount.readonly);
    }

    #[test]
    fn current_dir_workspace_uses_same_host_and_container_path() {
        let dir = tempdir().unwrap();
        let workspace = current_dir_workspace(dir.path()).unwrap();

        assert_eq!(
            workspace.workdir,
            dir.path().canonicalize().unwrap().display().to_string()
        );
        assert_eq!(workspace.mounts.len(), 1);
        assert_eq!(workspace.mounts[0].src, workspace.mounts[0].dst);
    }
}
