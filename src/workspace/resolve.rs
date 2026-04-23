use std::path::{Path, PathBuf};

use crate::workspace::mounts::validate_mount_paths;
use crate::workspace::paths::expand_tilde;
use crate::workspace::{MountConfig, WorkspaceConfig, validate_workspace_config};

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

fn host_path_match_depth(path: &str, canonical_cwd: &Path) -> Option<usize> {
    let expanded = expand_tilde(path);
    let canonical_path = Path::new(&expanded).canonicalize().ok()?;

    if canonical_cwd == canonical_path || canonical_cwd.starts_with(&canonical_path) {
        Some(canonical_path.components().count())
    } else {
        None
    }
}

pub fn saved_workspace_match_depth(workspace: &WorkspaceConfig, cwd: &Path) -> Option<usize> {
    let canonical_cwd = cwd.canonicalize().ok()?;

    std::iter::once(host_path_match_depth(&workspace.workdir, &canonical_cwd))
        .chain(
            workspace
                .mounts
                .iter()
                .map(|mount| host_path_match_depth(&mount.src, &canonical_cwd)),
        )
        .flatten()
        .max()
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
            let abs_src = if Path::new(&expanded_src).is_absolute() {
                PathBuf::from(&expanded_src)
            } else {
                cwd.join(&expanded_src)
            };
            let canonical_src = abs_src
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
            let workspace = config.require_workspace(&name)?.clone();
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
        if workspace
            .mounts
            .iter()
            .any(|existing| existing.dst == ad_hoc.dst)
        {
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
    let global_mounts = crate::config::AppConfig::expand_and_validate_named_mounts(
        &config.resolve_mounts(selector),
    )?;

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
    fn saved_workspace_match_depth_matches_host_workdir_parent_of_mounts() {
        let temp = tempdir().unwrap();
        let workspace_root = temp.path().join("monorepo");
        let repo_a = workspace_root.join("jackin");
        let repo_b = workspace_root.join("jackin-dev");
        std::fs::create_dir_all(&repo_a).unwrap();
        std::fs::create_dir_all(&repo_b).unwrap();

        let canonical_root = workspace_root.canonicalize().unwrap();
        let workspace = WorkspaceConfig {
            workdir: canonical_root.display().to_string(),
            mounts: vec![
                MountConfig {
                    src: repo_a.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace/jackin".to_string(),
                    readonly: false,
                },
                MountConfig {
                    src: repo_b.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace/jackin-dev".to_string(),
                    readonly: false,
                },
            ],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
        };

        assert_eq!(
            saved_workspace_match_depth(&workspace, &canonical_root),
            Some(canonical_root.components().count())
        );
    }

    #[test]
    fn saved_workspace_match_depth_still_matches_nested_path_under_mount_root() {
        let temp = tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let nested_dir = project_dir.join("src/bin");
        std::fs::create_dir_all(&nested_dir).unwrap();

        let workspace = WorkspaceConfig {
            workdir: "/workspace".to_string(),
            mounts: vec![MountConfig {
                src: project_dir.canonicalize().unwrap().display().to_string(),
                dst: "/workspace".to_string(),
                readonly: false,
            }],
            allowed_agents: vec![],
            default_agent: None,
            last_agent: None,
        };

        assert_eq!(
            saved_workspace_match_depth(&workspace, &nested_dir),
            Some(project_dir.canonicalize().unwrap().components().count())
        );
    }

    #[test]
    fn resolves_saved_workspace_and_rejects_disallowed_agent() {
        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
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
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
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
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();

        // The cwd parameter is used to resolve relative paths — no need
        // to mutate the global process CWD.
        let resolved = resolve_load_workspace(
            &crate::config::AppConfig::default(),
            &crate::selector::ClassSelector::new(None, "agent-smith"),
            temp.path(),
            LoadWorkspaceInput::Path {
                src: "./project".to_string(),
                dst: "./project".to_string(),
            },
            &[],
        )
        .unwrap();

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
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
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
        assert!(
            resolved
                .mounts
                .iter()
                .any(|m| m.dst == "/extra" && m.readonly)
        );
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

        assert!(
            error
                .to_string()
                .contains("ad-hoc mount destination conflicts")
        );
    }
}
