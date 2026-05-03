use anyhow::Result;
use std::path::Path;

use crate::config::AppConfig;
use crate::docker;
use crate::instance;
use crate::paths::JackinPaths;
use crate::runtime;
use crate::selector::ClassSelector;
use crate::tui;
use crate::workspace::{LoadWorkspaceInput, WorkspaceConfig, expand_tilde};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetKind {
    Path { src: String, dst: String },
    Name(String),
}

/// Classify a target string as either a path or a plain name.
///
/// Contains `/`, or starts with `.` or `~` => always a path.
/// Otherwise => a plain name (workspace or directory name).
pub fn classify_target(target: &str) -> TargetKind {
    if target.contains('/') || target.starts_with('.') || target.starts_with('~') {
        // Parse optional :dst — but be careful with src:dst vs path-only.
        // A target like ~/Projects/my-app:/app has the pattern host:container.
        // We split on the LAST colon that is followed by a `/` at position 0
        // (i.e., an absolute container path), to distinguish from :ro suffix.
        //
        // Strategy: if there's a colon where the right side starts with `/`,
        // treat it as src:dst.
        let (src, dst) = if let Some(pos) = find_dst_separator(target) {
            (&target[..pos], &target[pos + 1..])
        } else {
            // Same path for both src and dst — expand tilde for dst too.
            let expanded = expand_tilde(target);
            return TargetKind::Path {
                src: target.to_string(),
                dst: expanded,
            };
        };
        TargetKind::Path {
            src: src.to_string(),
            dst: dst.to_string(),
        }
    } else {
        TargetKind::Name(target.to_string())
    }
}

/// Find the colon that separates src:dst in a target spec.
/// The dst part must start with `/` (absolute container path).
fn find_dst_separator(target: &str) -> Option<usize> {
    // Search for `:` followed by `/`
    for (i, _) in target.match_indices(':') {
        if target[i + 1..].starts_with('/') {
            return Some(i);
        }
    }
    None
}

pub(crate) fn resolve_target_name(
    name: &str,
    config: &AppConfig,
    cwd: &Path,
) -> Result<LoadWorkspaceInput> {
    let workspace_exists = config.workspaces.contains_key(name);
    let dir_exists = cwd.join(name).is_dir();

    match (workspace_exists, dir_exists) {
        (true, true) => {
            let choice = tui::prompt_choice(
                &format!("\"{name}\" matches both a saved workspace and a directory."),
                &[
                    &format!("Use workspace \"{name}\""),
                    &format!("Use directory ./{name}"),
                ],
            )?;
            if choice == 0 {
                Ok(LoadWorkspaceInput::Saved(name.to_string()))
            } else {
                let full_path = cwd.join(name);
                let canonical = full_path.display().to_string();
                Ok(LoadWorkspaceInput::Path {
                    src: canonical.clone(),
                    dst: canonical,
                })
            }
        }
        (true, false) => Ok(LoadWorkspaceInput::Saved(name.to_string())),
        (false, true) => {
            let full_path = cwd.join(name);
            let canonical = full_path.display().to_string();
            Ok(LoadWorkspaceInput::Path {
                src: canonical.clone(),
                dst: canonical,
            })
        }
        (false, false) => {
            anyhow::bail!(
                "\"{name}\" is neither a saved workspace nor a directory in the current path.\n\
                 Saved workspaces: {}\n\
                 Hint: use a path (e.g. ./{name}) to mount a directory.",
                if config.workspaces.is_empty() {
                    "(none)".to_string()
                } else {
                    config
                        .workspaces
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            );
        }
    }
}

/// Find the saved workspace whose host workdir or mounted host path best
/// matches `cwd`. Returns `None` when no saved workspace covers the path.
///
/// Deepest mount-root match wins; ties go to iteration order (`BTreeMap`
/// alphabetical by workspace name). Shared by both the non-interactive
/// CLI resolvers (`jackin load`, `jackin hardline`) and the interactive
/// TUI workspace preselection in `console/`.
pub(crate) fn find_saved_workspace_for_cwd<'a>(
    config: &'a AppConfig,
    cwd: &Path,
) -> Option<(&'a str, &'a WorkspaceConfig)> {
    config
        .workspaces
        .iter()
        .filter_map(|(name, ws)| {
            crate::workspace::saved_workspace_match_depth(ws, cwd).map(|depth| (name, ws, depth))
        })
        .max_by_key(|(_, _, depth)| *depth)
        .map(|(name, ws, _)| (name.as_str(), ws))
}

/// Return the configured agents permitted by a workspace's `allowed_agents`.
///
/// An empty `allowed_agents` list means "any configured agent" — that is
/// the historical TUI and CLI contract, pinned by Phase 0 characterization
/// tests in `console/`. Agents named in `allowed_agents` but absent from
/// `config.agents` are silently dropped (no fabricated selectors).
pub(crate) fn eligible_agents_for_workspace(
    config: &AppConfig,
    workspace: &WorkspaceConfig,
) -> Vec<ClassSelector> {
    config
        .agents
        .keys()
        .filter_map(|key| ClassSelector::parse(key).ok())
        .filter(|agent| {
            workspace.allowed_agents.is_empty()
                || workspace
                    .allowed_agents
                    .iter()
                    .any(|allowed| allowed == &agent.key())
        })
        .collect()
}

/// Return the index of the preferred agent within `eligible`.
///
/// Priority: `last_agent` first, then `default_agent`. Returns `None` when
/// neither is set or when the named agent is not in `eligible`. The TUI's
/// preselection and the CLI's context resolver both go through this
/// helper so the ordering cannot silently diverge.
pub(crate) fn preferred_agent_index(
    eligible: &[ClassSelector],
    last_agent: Option<&str>,
    default_agent: Option<&str>,
) -> Option<usize> {
    last_agent
        .and_then(|last| eligible.iter().position(|agent| agent.key() == last))
        .or_else(|| {
            default_agent
                .and_then(|default| eligible.iter().position(|agent| agent.key() == default))
        })
}

/// Resolve the agent and workspace from the current directory context.
///
/// Finds the saved workspace whose host workdir or mounted host path best
/// matches `cwd`, then picks the agent:
/// 1. `last_agent` (most recently used)
/// 2. `default_agent` (explicitly configured)
/// 3. If multiple agents available — prompt
/// 4. If exactly one agent — use it
/// 5. No match — error with guidance
pub(crate) fn resolve_agent_from_context(
    config: &AppConfig,
    cwd: &Path,
) -> Result<(ClassSelector, LoadWorkspaceInput)> {
    if let Some((name, ws)) = find_saved_workspace_for_cwd(config, cwd) {
        let eligible = eligible_agents_for_workspace(config, ws);

        // Preferred-agent shortcut: last_agent, then default_agent.
        if let Some(preferred_idx) = preferred_agent_index(
            &eligible,
            ws.last_agent.as_deref(),
            ws.default_agent.as_deref(),
        ) {
            return Ok((
                eligible[preferred_idx].clone(),
                LoadWorkspaceInput::Saved(name.to_string()),
            ));
        }

        let chosen = match eligible.as_slice() {
            [] => anyhow::bail!("no agents configured; add one with jackin load <agent>"),
            [only] => only.clone(),
            _ => {
                let options: Vec<String> = eligible.iter().map(ClassSelector::key).collect();
                let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
                let choice = tui::prompt_choice(
                    &format!("Workspace {name:?} has multiple agents. Select one:"),
                    &option_refs,
                )?;
                eligible[choice].clone()
            }
        };
        return Ok((chosen, LoadWorkspaceInput::Saved(name.to_string())));
    }

    anyhow::bail!(
        "no saved workspace matches the current directory.\n\
         Run `jackin load <agent>` to use the current directory, or\n\
         run `jackin console` for the interactive operator console."
    );
}

/// Resolve a running agent container from the current directory context.
///
/// Finds the saved workspace whose host workdir or mounted host path best
/// matches `cwd`, then picks a currently-running container whose class is
/// permitted by the workspace:
/// 1. If the workspace's `last_agent` has a running container — prefer it
/// 2. If exactly one running candidate — use it
/// 3. If multiple — prompt
/// 4. If zero — error with guidance to run `jackin load`
/// 5. No workspace match — error with guidance to pass an explicit selector
pub(crate) fn resolve_running_container_from_context(
    config: &AppConfig,
    cwd: &Path,
    runner: &mut impl docker::CommandRunner,
) -> Result<String> {
    let Some((name, ws)) = find_saved_workspace_for_cwd(config, cwd) else {
        anyhow::bail!(
            "no saved workspace matches the current directory.\n\
             Run jackin hardline <agent> to target explicitly, or\n\
             run jackin load <agent> to start a new session."
        );
    };

    let allowed_classes: Vec<ClassSelector> = if ws.allowed_agents.is_empty() {
        config
            .agents
            .keys()
            .filter_map(|k| ClassSelector::parse(k).ok())
            .collect()
    } else {
        ws.allowed_agents
            .iter()
            .filter_map(|k| ClassSelector::parse(k).ok())
            .collect()
    };

    let running = runtime::list_running_agent_names(runner)?;
    let mut candidates: Vec<String> = allowed_classes
        .iter()
        .flat_map(|class| runtime::matching_family(class, &running))
        .collect();
    candidates.sort();
    candidates.dedup();

    if let Some(last) = ws.last_agent.as_deref()
        && let Ok(last_class) = ClassSelector::parse(last)
    {
        let primary = instance::primary_container_name(&last_class);
        if candidates.contains(&primary) {
            return Ok(primary);
        }
    }

    match candidates.as_slice() {
        [] => anyhow::bail!(
            "no running agents found for workspace {name:?}.\n\
             Start one with jackin load, or run jackin hardline <agent> to target explicitly."
        ),
        [only] => Ok(only.clone()),
        _ => {
            let option_refs: Vec<&str> = candidates.iter().map(String::as_str).collect();
            let choice = tui::prompt_choice(
                &format!("Workspace {name:?} has multiple running agents. Select one:"),
                &option_refs,
            )?;
            Ok(candidates.swap_remove(choice))
        }
    }
}

pub(crate) fn remember_last_agent(
    paths: &JackinPaths,
    config: &mut AppConfig,
    workspace_name: Option<&str>,
    class: &ClassSelector,
    load_result: &Result<()>,
) {
    if load_result.is_err() {
        return;
    }

    let Some(workspace_name) = workspace_name else {
        return;
    };
    if !config.workspaces.contains_key(workspace_name) {
        return;
    }
    // Production callers always reach this point with the config already
    // persisted on disk (it was loaded from disk at startup, and every
    // mutation flows through ConfigEditor). Tests that construct an
    // AppConfig purely in memory must persist it before calling this
    // function — see `remember_last_agent_persists_successful_loads`.
    let mut editor = match crate::config::ConfigEditor::open(paths) {
        Ok(editor) => editor,
        Err(error) => {
            eprintln!("warning: failed to open config for last-used-agent save: {error}");
            return;
        }
    };
    editor.set_last_agent(workspace_name, &class.key());
    match editor.save() {
        Ok(reloaded) => *config = reloaded,
        Err(error) => eprintln!("warning: failed to save last-used agent: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::paths;
    use crate::workspace;

    #[test]
    fn classify_target_tilde_path() {
        let result = classify_target("~/Projects/my-app");
        assert!(matches!(
            result,
            TargetKind::Path { ref src, .. } if src == "~/Projects/my-app"
        ));
    }

    #[test]
    fn classify_target_tilde_path_with_dst() {
        let result = classify_target("~/Projects/my-app:/app");
        assert!(matches!(
            result,
            TargetKind::Path { ref src, ref dst } if src == "~/Projects/my-app" && dst == "/app"
        ));
    }

    #[test]
    fn classify_target_dot_relative_path() {
        let result = classify_target("./my-app");
        assert!(matches!(result, TargetKind::Path { .. }));
    }

    #[test]
    fn classify_target_absolute_path() {
        let result = classify_target("/tmp/my-app");
        assert!(matches!(
            result,
            TargetKind::Path { ref src, ref dst } if src == "/tmp/my-app" && dst == "/tmp/my-app"
        ));
    }

    #[test]
    fn classify_target_absolute_path_with_dst() {
        let result = classify_target("/tmp/my-app:/workspace");
        assert!(matches!(
            result,
            TargetKind::Path { ref src, ref dst } if src == "/tmp/my-app" && dst == "/workspace"
        ));
    }

    #[test]
    fn classify_target_plain_name() {
        let result = classify_target("big-monorepo");
        assert!(matches!(
            result,
            TargetKind::Name(ref name) if name == "big-monorepo"
        ));
    }

    #[test]
    fn classify_target_name_with_no_slash() {
        let result = classify_target("my-workspace");
        assert!(matches!(result, TargetKind::Name(_)));
    }

    #[test]
    fn classify_target_relative_with_slash() {
        // Contains `/` so treated as path
        let result = classify_target("sub/dir");
        assert!(matches!(result, TargetKind::Path { .. }));
    }

    #[test]
    fn resolve_target_name_workspace_only() {
        let mut config = config::AppConfig::default();
        config.workspaces.insert(
            "my-ws".to_string(),
            workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                ..Default::default()
            },
        );
        let cwd = std::env::temp_dir();
        let result = resolve_target_name("my-ws", &config, &cwd).unwrap();
        assert!(matches!(result, LoadWorkspaceInput::Saved(ref name) if name == "my-ws"));
    }

    #[test]
    fn resolve_target_name_directory_only() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("my-dir");
        std::fs::create_dir_all(&dir).unwrap();

        let config = config::AppConfig::default();
        let result = resolve_target_name("my-dir", &config, temp.path()).unwrap();
        assert!(matches!(result, LoadWorkspaceInput::Path { .. }));
    }

    #[test]
    fn resolve_target_name_neither_errors() {
        let config = config::AppConfig::default();
        let cwd = std::env::temp_dir();
        let result = resolve_target_name("nonexistent-thing", &config, &cwd);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("neither a saved workspace nor a directory"));
    }

    #[test]
    fn resolve_agent_from_context_matches_workspace_from_nested_mount_path() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let nested_dir = project_dir.join("src/bin");
        std::fs::create_dir_all(&nested_dir).unwrap();

        let mut config = config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: project_dir.display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
                keep_awake: workspace::KeepAwakeConfig::default(),
            },
        );

        let resolved = resolve_agent_from_context(&config, &nested_dir).unwrap();

        assert_eq!(resolved.0.key(), "agent-smith");
        assert_eq!(resolved.1, LoadWorkspaceInput::Saved("my-app".to_string()));
    }

    #[test]
    fn resolve_agent_from_context_matches_workspace_from_host_workdir_root() {
        let temp = tempfile::tempdir().unwrap();
        let workspace_root = temp.path().join("monorepo");
        let repo_dir = workspace_root.join("jackin");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let workspace_root = workspace_root.canonicalize().unwrap();

        let mut config = config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                workdir: workspace_root.display().to_string(),
                mounts: vec![workspace::MountConfig {
                    src: repo_dir.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace/jackin".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
                last_agent: None,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
                keep_awake: workspace::KeepAwakeConfig::default(),
            },
        );

        let resolved = resolve_agent_from_context(&config, &workspace_root).unwrap();

        assert_eq!(resolved.0.key(), "agent-smith");
        assert_eq!(resolved.1, LoadWorkspaceInput::Saved("my-app".to_string()));
    }

    #[test]
    fn resolve_agent_from_context_ignores_stale_last_agent() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let nested_dir = project_dir.join("src/bin");
        std::fs::create_dir_all(&nested_dir).unwrap();

        let mut config = config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: project_dir.display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: None,
                last_agent: Some("ghost-agent".to_string()),
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
                keep_awake: workspace::KeepAwakeConfig::default(),
            },
        );

        let resolved = resolve_agent_from_context(&config, &nested_dir).unwrap();

        assert_eq!(resolved.0.key(), "agent-smith");
        assert_eq!(resolved.1, LoadWorkspaceInput::Saved("my-app".to_string()));
    }

    /// Build an `AppConfig` pre-populated with an `agent-smith` agent and a
    /// single workspace rooted at `project_dir`.
    fn config_with_workspace(
        project_dir: &Path,
        allowed_agents: Vec<String>,
        last_agent: Option<String>,
    ) -> config::AppConfig {
        let mut config = config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.agents.insert(
            "the-architect".to_string(),
            config::AgentSource {
                git: "https://github.com/jackin-project/jackin-the-architect.git".to_string(),
                trusted: true,
                claude: None,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: project_dir.display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                allowed_agents,
                default_agent: None,
                last_agent,
                env: std::collections::BTreeMap::new(),
                agents: std::collections::BTreeMap::new(),
                keep_awake: workspace::KeepAwakeConfig::default(),
            },
        );
        config
    }

    /// `list_running_agent_names` issues two docker captures (role filter +
    /// legacy filter); supply running-agent output on the first, nothing on
    /// the second.
    fn fake_runner_with_running_agents(names: &[&str]) -> runtime::FakeRunner {
        let mut runner = runtime::FakeRunner::default();
        runner.capture_queue.push_back(names.join("\n"));
        runner.capture_queue.push_back(String::new());
        runner
    }

    #[test]
    fn resolve_running_container_from_context_picks_lone_running_agent() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let nested_dir = project_dir.join("src");
        std::fs::create_dir_all(&nested_dir).unwrap();

        let config = config_with_workspace(&project_dir, vec!["agent-smith".to_string()], None);
        let mut runner = fake_runner_with_running_agents(&["jackin-agent-smith"]);

        let container =
            resolve_running_container_from_context(&config, &nested_dir, &mut runner).unwrap();

        assert_eq!(container, "jackin-agent-smith");
    }

    #[test]
    fn resolve_running_container_from_context_prefers_last_agent() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let config = config_with_workspace(
            &project_dir,
            vec!["agent-smith".to_string(), "the-architect".to_string()],
            Some("the-architect".to_string()),
        );
        let mut runner =
            fake_runner_with_running_agents(&["jackin-agent-smith", "jackin-the-architect"]);

        let container =
            resolve_running_container_from_context(&config, &project_dir, &mut runner).unwrap();

        assert_eq!(container, "jackin-the-architect");
    }

    #[test]
    fn resolve_running_container_from_context_errors_when_nothing_running() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let config = config_with_workspace(&project_dir, vec!["agent-smith".to_string()], None);
        let mut runner = fake_runner_with_running_agents(&[]);

        let err = resolve_running_container_from_context(&config, &project_dir, &mut runner)
            .unwrap_err()
            .to_string();

        assert!(err.contains("no running agents"), "got: {err}");
        assert!(err.contains("my-app"), "got: {err}");
    }

    #[test]
    fn resolve_running_container_from_context_ignores_disallowed_running_agents() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let config = config_with_workspace(&project_dir, vec!["agent-smith".to_string()], None);
        // the-architect is running but not allowed in this workspace.
        let mut runner = fake_runner_with_running_agents(&["jackin-the-architect"]);

        let err = resolve_running_container_from_context(&config, &project_dir, &mut runner)
            .unwrap_err()
            .to_string();

        assert!(err.contains("no running agents"), "got: {err}");
    }

    #[test]
    fn resolve_running_container_from_context_errors_when_no_workspace_matches() {
        let temp = tempfile::tempdir().unwrap();
        let unrelated = temp.path().join("unrelated");
        std::fs::create_dir_all(&unrelated).unwrap();

        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();
        let config = config_with_workspace(&project_dir, vec!["agent-smith".to_string()], None);
        let mut runner = fake_runner_with_running_agents(&["jackin-agent-smith"]);

        let err = resolve_running_container_from_context(&config, &unrelated, &mut runner)
            .unwrap_err()
            .to_string();

        assert!(err.contains("no saved workspace matches"), "got: {err}");
    }

    /// Test helper: construct a minimal workspace-containing `AppConfig`,
    /// persist it to disk at the expected config path, and return the
    /// live in-memory copy. Matches the production invariant that
    /// `remember_last_agent` observes: the config is already on disk.
    fn persisted_config_with_workspace(
        paths: &paths::JackinPaths,
        temp_path: &std::path::Path,
    ) -> config::AppConfig {
        paths.ensure_base_dirs().unwrap();
        let mut config = config::AppConfig::default();
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: temp_path.display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        );
        let serialized = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&paths.config_file, serialized).unwrap();
        config
    }

    #[test]
    fn remember_last_agent_persists_successful_loads() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let mut config = persisted_config_with_workspace(&paths, temp.path());

        remember_last_agent(
            &paths,
            &mut config,
            Some("my-app"),
            &ClassSelector::new(None, "agent-smith"),
            &Ok(()),
        );

        assert_eq!(
            config
                .workspaces
                .get("my-app")
                .and_then(|workspace| workspace.last_agent.as_deref()),
            Some("agent-smith")
        );
    }

    #[test]
    fn remember_last_agent_skips_failed_loads() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let mut config = persisted_config_with_workspace(&paths, temp.path());

        remember_last_agent(
            &paths,
            &mut config,
            Some("my-app"),
            &ClassSelector::new(None, "agent-smith"),
            &Err(anyhow::anyhow!("load failed")),
        );

        assert_eq!(
            config
                .workspaces
                .get("my-app")
                .and_then(|workspace| workspace.last_agent.as_deref()),
            None
        );
    }

    /// Regression: a workspace whose workdir is a broad parent directory must not
    /// match when cwd is an unrelated subdirectory not covered by any mount source.
    #[test]
    fn broad_workdir_does_not_match_unrelated_subdirectory() {
        let temp = tempfile::tempdir().unwrap();
        let broad_workdir = temp.path().join("Projects");
        let agent_repo = broad_workdir.join("agent-repo");
        let unrelated = broad_workdir.join("jackin4");
        std::fs::create_dir_all(&agent_repo).unwrap();
        std::fs::create_dir_all(&unrelated).unwrap();

        let mut config = config::AppConfig::default();
        config.workspaces.insert(
            "jackin-agents".to_string(),
            workspace::WorkspaceConfig {
                workdir: broad_workdir.canonicalize().unwrap().display().to_string(),
                mounts: vec![workspace::MountConfig {
                    src: agent_repo.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace/agent-repo".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        );

        let result = find_saved_workspace_for_cwd(&config, &unrelated);
        assert!(
            result.is_none(),
            "broad workdir must not preselect for an unrelated subdirectory"
        );
    }

    /// Complement: workspace still matches when cwd IS under a mount source.
    #[test]
    fn workspace_matches_when_cwd_is_under_mount_src() {
        let temp = tempfile::tempdir().unwrap();
        let broad_workdir = temp.path().join("Projects");
        let agent_repo = broad_workdir.join("agent-repo");
        let inside_repo = agent_repo.join("src");
        std::fs::create_dir_all(&inside_repo).unwrap();

        let mut config = config::AppConfig::default();
        config.workspaces.insert(
            "jackin-agents".to_string(),
            workspace::WorkspaceConfig {
                workdir: broad_workdir.canonicalize().unwrap().display().to_string(),
                mounts: vec![workspace::MountConfig {
                    src: agent_repo.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace/agent-repo".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                ..Default::default()
            },
        );

        let result = find_saved_workspace_for_cwd(&config, &inside_repo);
        assert!(
            result.is_some(),
            "cwd inside a mount source must still preselect the workspace"
        );
        assert_eq!(result.unwrap().0, "jackin-agents");
    }
}
