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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_agent: Option<String>,
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

fn home_dir() -> Option<String> {
    directories::BaseDirs::new().map(|b| b.home_dir().display().to_string())
}

pub fn expand_tilde(path: &str) -> String {
    if (path == "~" || path.starts_with("~/"))
        && let Some(home) = home_dir()
    {
        return path.replacen('~', &home, 1);
    }

    path.to_string()
}

pub fn parse_mount_spec(spec: &str) -> anyhow::Result<MountConfig> {
    let (raw, readonly) = spec
        .strip_suffix(":ro")
        .map_or((spec, false), |value| (value, true));
    let (src, dst) = raw
        .split_once(':')
        .map_or_else(|| (raw, raw), |(s, d)| (s, d));
    let expanded_src = expand_tilde(src);
    let dst = if src == dst { expanded_src.clone() } else { dst.to_string() };

    Ok(MountConfig {
        src: expanded_src,
        dst,
        readonly,
    })
}

/// Structural validation: absolute paths, no duplicate destinations.
/// Safe to call at config-save time — does not touch the filesystem.
pub fn validate_mount_specs(mounts: &[MountConfig]) -> anyhow::Result<()> {
    let mut seen_dst = std::collections::HashSet::new();

    for mount in mounts {
        if !Path::new(&mount.src).is_absolute() {
            anyhow::bail!("mount source must be absolute: {}", mount.src);
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

/// Filesystem validation: checks that mount sources exist on disk.
/// Call at load/resolve time, not at config-save time.
pub fn validate_mount_paths(mounts: &[MountConfig]) -> anyhow::Result<()> {
    for mount in mounts {
        if !Path::new(&mount.src).exists() {
            anyhow::bail!("mount source does not exist: {}", mount.src);
        }
    }

    Ok(())
}

/// Full validation: structural + filesystem checks combined.
pub fn validate_mounts(mounts: &[MountConfig]) -> anyhow::Result<()> {
    validate_mount_specs(mounts)?;
    validate_mount_paths(mounts)
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
        last_agent: None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadWorkspaceInput {
    CurrentDir,
    Path { src: String, dst: String },
    Saved(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorkspace {
    pub label: String,
    pub workdir: String,
    pub mounts: Vec<MountConfig>,
}

pub fn resolve_load_workspace(
    config: &crate::config::AppConfig,
    selector: &crate::selector::ClassSelector,
    cwd: &Path,
    input: LoadWorkspaceInput,
    ad_hoc_mounts: &[MountConfig],
) -> anyhow::Result<ResolvedWorkspace> {
    let (mut workspace, label) = match input {
        LoadWorkspaceInput::CurrentDir => {
            let ws = current_dir_workspace(cwd)?;
            let label = ws.workdir.clone();
            (ws, label)
        }
        LoadWorkspaceInput::Path { src, dst } => {
            let expanded_src = expand_tilde(&src);
            let canonical_src = Path::new(&expanded_src)
                .canonicalize()
                .map_err(|e| anyhow::anyhow!("cannot resolve path {expanded_src}: {e}"))?;
            let src_str = canonical_src.display().to_string();
            let workdir = if dst == src || dst == expanded_src {
                src_str.clone()
            } else {
                dst.clone()
            };
            let ws = WorkspaceConfig {
                workdir,
                mounts: vec![MountConfig {
                    src: src_str,
                    dst: if dst == src || dst == expanded_src {
                        canonical_src.display().to_string()
                    } else {
                        dst
                    },
                    readonly: false,
                }],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
            };
            let label = ws.workdir.clone();
            (ws, label)
        }
        LoadWorkspaceInput::Saved(name) => {
            let workspace = config
                .workspaces
                .get(&name)
                .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))?
                .clone();
            if !workspace.allowed_agents.is_empty()
                && !workspace
                    .allowed_agents
                    .iter()
                    .any(|agent| agent == &selector.key())
            {
                anyhow::bail!(
                    "agent {} is not allowed by workspace {name}",
                    selector.key()
                );
            }
            (workspace, name)
        }
    };

    // Merge ad-hoc mounts after workspace mounts, checking for dst conflicts.
    for ad_hoc in ad_hoc_mounts {
        if workspace.mounts.iter().any(|existing| existing.dst == ad_hoc.dst) {
            anyhow::bail!(
                "ad-hoc mount destination conflicts with workspace mount destination: {}",
                ad_hoc.dst
            );
        }
        workspace.mounts.push(ad_hoc.clone());
    }

    validate_workspace_config("runtime", &workspace)?;
    validate_mount_paths(&workspace.mounts)?;

    let mut mounts = workspace.mounts.clone();
    let global_mounts =
        crate::config::AppConfig::expand_and_validate_named_mounts(&config.resolve_mounts(selector))?;

    for mount in global_mounts {
        if mounts.iter().any(|existing| existing.dst == mount.dst) {
            anyhow::bail!(
                "global mount destination conflicts with workspace destination: {}",
                mount.dst
            );
        }
        mounts.push(mount);
    }

    Ok(ResolvedWorkspace {
        label,
        workdir: workspace.workdir,
        mounts,
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
    fn parses_mount_spec_with_src_only() {
        let mount = parse_mount_spec("/tmp/project").unwrap();

        assert_eq!(mount.src, "/tmp/project");
        assert_eq!(mount.dst, "/tmp/project");
        assert!(!mount.readonly);
    }

    #[test]
    fn parses_mount_spec_with_src_only_readonly() {
        let mount = parse_mount_spec("/tmp/project:ro").unwrap();

        assert_eq!(mount.src, "/tmp/project");
        assert_eq!(mount.dst, "/tmp/project");
        assert!(mount.readonly);
    }

    #[test]
    fn parses_mount_spec_with_tilde_src_only() {
        let home = std::env::var("HOME").unwrap();
        let mount = parse_mount_spec("~/projects").unwrap();

        assert_eq!(mount.src, format!("{home}/projects"));
        assert_eq!(mount.dst, format!("{home}/projects"));
        assert!(!mount.readonly);
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

    #[test]
    fn resolves_saved_workspace_and_rejects_disallowed_agent() {
        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource {
                git: "git@github.com:donbeave/jackin-agent-smith.git".to_string(),
            },
        );
        config.workspaces.insert(
            "big-monorepo".to_string(),
            WorkspaceConfig {
                workdir: "/workspace/project".to_string(),
                mounts: vec![MountConfig {
                    src: "/tmp/project".to_string(),
                    dst: "/workspace/project".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
                last_agent: None,
            },
        );

        let cwd = std::env::temp_dir();
        let error = resolve_load_workspace(
            &config,
            &crate::selector::ClassSelector::new(None, "neo"),
            &cwd,
            LoadWorkspaceInput::Saved("big-monorepo".to_string()),
            &[],
        )
        .unwrap_err();

        assert!(error.to_string().contains("is not allowed by workspace"));
    }

    #[test]
    fn saved_workspace_label_uses_workspace_name() {
        let temp = tempdir().unwrap();
        let mount_src = temp.path().join("project");
        std::fs::create_dir_all(&mount_src).unwrap();

        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource {
                git: "git@github.com:donbeave/jackin-agent-smith.git".to_string(),
            },
        );
        config.workspaces.insert(
            "big-monorepo".to_string(),
            WorkspaceConfig {
                workdir: "/workspace/project".to_string(),
                mounts: vec![MountConfig {
                    src: mount_src.display().to_string(),
                    dst: "/workspace/project".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
            },
        );

        let cwd = std::env::temp_dir();
        let resolved = resolve_load_workspace(
            &config,
            &crate::selector::ClassSelector::new(None, "agent-smith"),
            &cwd,
            LoadWorkspaceInput::Saved("big-monorepo".to_string()),
            &[],
        )
        .unwrap();

        assert_eq!(resolved.label, "big-monorepo");
        assert_eq!(resolved.workdir, "/workspace/project");
    }

    #[test]
    fn resolves_same_path_relative_target_to_absolute_workdir() {
        let temp = tempdir().unwrap();
        let original_cwd = std::env::current_dir().unwrap();
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::env::set_current_dir(temp.path()).unwrap();

        let result = resolve_load_workspace(
            &crate::config::AppConfig::default(),
            &crate::selector::ClassSelector::new(None, "agent-smith"),
            temp.path(),
            LoadWorkspaceInput::Path {
                src: "./project".to_string(),
                dst: "./project".to_string(),
            },
            &[],
        );

        std::env::set_current_dir(original_cwd).unwrap();

        let resolved = result.unwrap();
        let expected = project_dir.canonicalize().unwrap().display().to_string();
        assert_eq!(resolved.workdir, expected);
        assert_eq!(resolved.mounts[0].dst, expected);
    }

    #[test]
    fn resolves_global_mounts_with_tilde_sources() {
        let home = std::env::var("HOME").unwrap();
        let cwd = tempdir().unwrap();
        let mut config = crate::config::AppConfig::default();
        config.add_mount(
            "home",
            MountConfig {
                src: "~".to_string(),
                dst: "/home/claude/home".to_string(),
                readonly: true,
            },
            None,
        );

        let resolved = resolve_load_workspace(
            &config,
            &crate::selector::ClassSelector::new(None, "agent-smith"),
            cwd.path(),
            LoadWorkspaceInput::CurrentDir,
            &[],
        )
        .unwrap();

        assert!(
            resolved
                .mounts
                .iter()
                .any(|mount| mount.dst == "/home/claude/home"
                    && mount.src == home
                    && mount.readonly)
        );
    }

    #[test]
    fn resolve_with_ad_hoc_mounts_merges_correctly() {
        let temp = tempdir().unwrap();
        let mount_src = temp.path().join("project");
        let extra_src = temp.path().join("extra");
        std::fs::create_dir_all(&mount_src).unwrap();
        std::fs::create_dir_all(&extra_src).unwrap();

        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource {
                git: "git@github.com:donbeave/jackin-agent-smith.git".to_string(),
            },
        );
        config.workspaces.insert(
            "my-ws".to_string(),
            WorkspaceConfig {
                workdir: "/workspace/project".to_string(),
                mounts: vec![MountConfig {
                    src: mount_src.display().to_string(),
                    dst: "/workspace/project".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
            },
        );

        let cwd = std::env::temp_dir();
        let resolved = resolve_load_workspace(
            &config,
            &crate::selector::ClassSelector::new(None, "agent-smith"),
            &cwd,
            LoadWorkspaceInput::Saved("my-ws".to_string()),
            &[MountConfig {
                src: extra_src.display().to_string(),
                dst: "/extra".to_string(),
                readonly: true,
            }],
        )
        .unwrap();

        assert_eq!(resolved.mounts.len(), 2);
        assert!(resolved.mounts.iter().any(|m| m.dst == "/extra" && m.readonly));
    }

    #[test]
    fn resolve_with_ad_hoc_mount_dst_conflict_errors() {
        let temp = tempdir().unwrap();
        let mount_src = temp.path().join("project");
        std::fs::create_dir_all(&mount_src).unwrap();

        let mut config = crate::config::AppConfig::default();
        config.workspaces.insert(
            "my-ws".to_string(),
            WorkspaceConfig {
                workdir: "/workspace/project".to_string(),
                mounts: vec![MountConfig {
                    src: mount_src.display().to_string(),
                    dst: "/workspace/project".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec![],
                default_agent: None,
                last_agent: None,
            },
        );

        let cwd = std::env::temp_dir();
        let error = resolve_load_workspace(
            &config,
            &crate::selector::ClassSelector::new(None, "agent-smith"),
            &cwd,
            LoadWorkspaceInput::Saved("my-ws".to_string()),
            &[MountConfig {
                src: mount_src.display().to_string(),
                dst: "/workspace/project".to_string(),
                readonly: false,
            }],
        )
        .unwrap_err();

        assert!(error.to_string().contains("ad-hoc mount destination conflicts"));
    }
}
