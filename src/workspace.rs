use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

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
    pub no_workdir_mount: bool,
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

/// Normalize an absolute path by resolving `.` and `..` components without
/// touching the filesystem (unlike [`std::fs::canonicalize`]).
fn normalize_path(path: &Path) -> PathBuf {
    let mut parts: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = parts.last() {
                    parts.pop();
                }
            }
            Component::CurDir => {}
            c => parts.push(c),
        }
    }
    parts.iter().collect()
}

/// Expand tilde, resolve relative paths to absolute using the current working
/// directory, and normalize `.` / `..` components.
pub fn resolve_path(path: &str) -> String {
    let expanded = expand_tilde(path);
    let abs = if expanded.starts_with('/') {
        PathBuf::from(&expanded)
    } else if let Ok(cwd) = std::env::current_dir() {
        cwd.join(&expanded)
    } else {
        return expanded;
    };
    normalize_path(&abs).display().to_string()
}

pub fn parse_mount_spec(spec: &str) -> anyhow::Result<MountConfig> {
    Ok(parse_mount_spec_inner(spec, false))
}

/// Like [`parse_mount_spec`] but also resolves relative paths to absolute.
/// Use this for CLI arguments where the user may pass relative paths.
pub fn parse_mount_spec_resolved(spec: &str) -> anyhow::Result<MountConfig> {
    Ok(parse_mount_spec_inner(spec, true))
}

fn parse_mount_spec_inner(spec: &str, resolve: bool) -> MountConfig {
    let (raw, readonly) = spec
        .strip_suffix(":ro")
        .map_or((spec, false), |value| (value, true));
    let (src, dst) = raw
        .split_once(':')
        .map_or_else(|| (raw, raw), |(s, d)| (s, d));
    let expand = if resolve { resolve_path } else { expand_tilde };
    let expanded_src = expand(src);
    let dst = if src == dst {
        expanded_src.clone()
    } else {
        dst.to_string()
    };

    MountConfig {
        src: expanded_src,
        dst,
        readonly,
    }
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

// ── Sensitive mount detection ────────────────────────────────────────────

/// Path suffixes that indicate sensitive host directories. A mount source is
/// considered sensitive when its resolved path ends with one of these suffixes
/// (after tilde expansion).
const SENSITIVE_SUFFIXES: &[(&str, &str)] = &[
    ("/.ssh", "SSH keys and configuration"),
    ("/.aws", "AWS credentials and configuration"),
    ("/.gnupg", "GPG keys and trust database"),
    ("/.config/gcloud", "Google Cloud credentials"),
    ("/.kube", "Kubernetes credentials and configuration"),
    ("/.docker", "Docker credentials and configuration"),
];

/// A mount source that matched a sensitive path pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensitiveMount {
    pub src: String,
    pub reason: String,
}

/// Return any mounts whose source path matches a known sensitive pattern.
pub fn find_sensitive_mounts(mounts: &[MountConfig]) -> Vec<SensitiveMount> {
    let mut hits = Vec::new();
    for mount in mounts {
        let normalized = mount.src.trim_end_matches('/');
        for &(suffix, reason) in SENSITIVE_SUFFIXES {
            if normalized.ends_with(suffix) || normalized == suffix.trim_start_matches('/') {
                hits.push(SensitiveMount {
                    src: mount.src.clone(),
                    reason: reason.to_string(),
                });
                break;
            }
        }
    }
    hits
}

/// Display a warning for sensitive mounts and ask the operator to confirm.
/// Returns `Ok(true)` when the operator confirms, `Ok(false)` when they
/// decline, and `Err` on I/O errors.
pub fn confirm_sensitive_mounts(sensitive: &[SensitiveMount]) -> anyhow::Result<bool> {
    use owo_colors::OwoColorize;
    use std::io::IsTerminal;

    if sensitive.is_empty() {
        return Ok(true);
    }

    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "sensitive mount paths detected but stdin is not a terminal — cannot prompt for confirmation"
        );
    }

    eprintln!(
        "\n{}",
        "⚠  Sensitive host paths detected in mounts:"
            .yellow()
            .bold()
    );
    for hit in sensitive {
        eprintln!("     {} — {}", hit.src.bold(), hit.reason);
    }
    eprintln!(
        "   {}",
        "These paths may expose credentials to the agent container.".dimmed()
    );
    eprintln!();

    Ok(dialoguer::Confirm::new()
        .with_prompt("Continue with these mounts?")
        .default(false)
        .interact()?)
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
    fn resolve_path_resolves_relative_to_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let resolved = resolve_path("my-project");

        assert_eq!(resolved, cwd.join("my-project").display().to_string());
        assert!(resolved.starts_with('/'));
    }

    #[test]
    fn resolve_path_leaves_absolute_unchanged() {
        assert_eq!(resolve_path("/workspace/project"), "/workspace/project");
    }

    #[test]
    fn resolve_path_normalizes_dot_to_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let resolved = resolve_path(".");

        assert_eq!(resolved, cwd.display().to_string());
    }

    #[test]
    fn resolve_path_normalizes_parent_component() {
        let cwd = std::env::current_dir().unwrap();
        let resolved = resolve_path("../sibling");
        let expected = cwd.parent().unwrap().join("sibling");

        assert_eq!(resolved, expected.display().to_string());
        assert!(!resolved.contains(".."));
    }

    #[test]
    fn resolve_path_normalizes_absolute_with_dotdot() {
        assert_eq!(resolve_path("/a/b/../c"), "/a/c");
    }

    #[test]
    fn normalize_path_handles_multiple_parent_refs() {
        let path = Path::new("/a/b/c/../../d");
        assert_eq!(normalize_path(path), PathBuf::from("/a/d"));
    }

    #[test]
    fn normalize_path_preserves_root_on_excessive_parents() {
        let path = Path::new("/a/../../../b");
        assert_eq!(normalize_path(path), PathBuf::from("/b"));
    }

    #[test]
    fn parse_mount_spec_resolved_resolves_relative_src_and_dst() {
        let cwd = std::env::current_dir().unwrap();
        let mount = parse_mount_spec_resolved("my-project").unwrap();
        let expected = cwd.join("my-project").display().to_string();

        assert_eq!(mount.src, expected);
        assert_eq!(mount.dst, expected);
        assert!(!mount.readonly);
    }

    #[test]
    fn parse_mount_spec_resolved_resolves_relative_src_with_explicit_dst() {
        let cwd = std::env::current_dir().unwrap();
        let mount = parse_mount_spec_resolved("my-project:/workspace/project").unwrap();

        assert_eq!(mount.src, cwd.join("my-project").display().to_string());
        assert_eq!(mount.dst, "/workspace/project");
        assert!(!mount.readonly);
    }

    #[test]
    fn parse_mount_spec_resolved_normalizes_dotdot_in_relative_path() {
        let cwd = std::env::current_dir().unwrap();
        let mount = parse_mount_spec_resolved("../sibling-project").unwrap();
        let expected = cwd.parent().unwrap().join("sibling-project");

        assert_eq!(mount.src, expected.display().to_string());
        assert_eq!(mount.dst, expected.display().to_string());
        assert!(!mount.src.contains(".."));
    }

    #[test]
    fn parse_mount_spec_resolved_normalizes_dot_path() {
        let cwd = std::env::current_dir().unwrap();
        let mount = parse_mount_spec_resolved(".").unwrap();

        assert_eq!(mount.src, cwd.display().to_string());
        assert_eq!(mount.dst, cwd.display().to_string());
    }

    #[test]
    fn parse_mount_spec_does_not_resolve_relative_paths() {
        let mount = parse_mount_spec("my-project").unwrap();

        assert_eq!(mount.src, "my-project");
        assert_eq!(mount.dst, "my-project");
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
        };
        validate_workspace_config("test", &ws).unwrap();
    }

    // -- find_sensitive_mounts ------------------------------------------------

    fn mount(src: &str) -> MountConfig {
        MountConfig {
            src: src.to_string(),
            dst: "/container/path".to_string(),
            readonly: false,
        }
    }

    #[test]
    fn detects_ssh_mount() {
        let mounts = vec![mount("/home/user/.ssh")];
        let hits = find_sensitive_mounts(&mounts);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].src, "/home/user/.ssh");
        assert!(hits[0].reason.contains("SSH"));
    }

    #[test]
    fn detects_aws_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.aws")]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].reason.contains("AWS"));
    }

    #[test]
    fn detects_gnupg_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.gnupg")]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].reason.contains("GPG"));
    }

    #[test]
    fn detects_gcloud_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.config/gcloud")]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].reason.contains("Google Cloud"));
    }

    #[test]
    fn detects_kube_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.kube")]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].reason.contains("Kubernetes"));
    }

    #[test]
    fn detects_docker_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.docker")]);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].reason.contains("Docker"));
    }

    #[test]
    fn ignores_safe_mounts() {
        let mounts = vec![
            mount("/home/user/projects"),
            mount("/tmp/workspace"),
            mount("/var/data"),
        ];
        assert!(find_sensitive_mounts(&mounts).is_empty());
    }

    #[test]
    fn detects_multiple_sensitive_mounts() {
        let mounts = vec![
            mount("/home/user/.ssh"),
            mount("/home/user/projects"),
            mount("/home/user/.aws"),
        ];
        let hits = find_sensitive_mounts(&mounts);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn handles_trailing_slash_on_sensitive_mount() {
        let hits = find_sensitive_mounts(&[mount("/home/user/.ssh/")]);
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn does_not_match_partial_name() {
        // ".sshd" should NOT match ".ssh"
        let hits = find_sensitive_mounts(&[mount("/home/user/.sshd")]);
        assert!(hits.is_empty());
    }
}
