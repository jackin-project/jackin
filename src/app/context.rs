use anyhow::Result;
use std::path::Path;

use crate::config::AppConfig;
use crate::docker;
use crate::instance;
use crate::paths::JackinPaths;
use crate::runtime;
use crate::selector::RoleSelector;
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

/// Return the configured roles permitted by a workspace's `allowed_roles`.
///
/// An empty `allowed_roles` list means "any configured role" — that is
/// the historical TUI and CLI contract, pinned by Phase 0 characterization
/// tests in `console/`. Roles named in `allowed_roles` but absent from
/// `config.roles` are silently dropped (no fabricated selectors).
pub(crate) fn eligible_roles_for_workspace(
    config: &AppConfig,
    workspace: &WorkspaceConfig,
) -> Vec<RoleSelector> {
    config
        .roles
        .keys()
        .filter_map(|key| RoleSelector::parse(key).ok())
        .filter(|role| {
            workspace.allowed_roles.is_empty()
                || workspace
                    .allowed_roles
                    .iter()
                    .any(|allowed| allowed == &role.key())
        })
        .collect()
}

/// Return the index of the preferred role within `eligible`.
///
/// Priority: `last_role` first, then `default_role`. Returns `None` when
/// neither is set or when the named role is not in `eligible`. The TUI's
/// preselection and the CLI's context resolver both go through this
/// helper so the ordering cannot silently diverge.
pub(crate) fn preferred_agent_index(
    eligible: &[RoleSelector],
    last_role: Option<&str>,
    default_role: Option<&str>,
) -> Option<usize> {
    last_role
        .and_then(|last| eligible.iter().position(|role| role.key() == last))
        .or_else(|| {
            default_role.and_then(|default| eligible.iter().position(|role| role.key() == default))
        })
}

/// Resolve the role and workspace from the current directory context.
///
/// Finds the saved workspace whose host workdir or mounted host path best
/// matches `cwd`, then picks the role:
/// 1. `last_role` (most recently used)
/// 2. `default_role` (explicitly configured)
/// 3. If multiple roles available — prompt
/// 4. If exactly one role — use it
/// 5. No match — error with guidance
pub(crate) fn resolve_agent_from_context(
    config: &AppConfig,
    cwd: &Path,
) -> Result<(RoleSelector, LoadWorkspaceInput)> {
    if let Some((name, ws)) = find_saved_workspace_for_cwd(config, cwd) {
        let eligible = eligible_roles_for_workspace(config, ws);

        // Preferred-role shortcut: last_role, then default_role.
        if let Some(preferred_idx) = preferred_agent_index(
            &eligible,
            ws.last_role.as_deref(),
            ws.default_role.as_deref(),
        ) {
            return Ok((
                eligible[preferred_idx].clone(),
                LoadWorkspaceInput::Saved(name.to_string()),
            ));
        }

        let chosen = match eligible.as_slice() {
            [] => anyhow::bail!("no roles configured; add one with jackin load <role>"),
            [only] => only.clone(),
            _ => {
                let options: Vec<String> = eligible.iter().map(RoleSelector::key).collect();
                let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
                let choice = tui::prompt_choice(
                    &format!("Workspace {name:?} has multiple roles. Select one:"),
                    &option_refs,
                )?;
                eligible[choice].clone()
            }
        };
        return Ok((chosen, LoadWorkspaceInput::Saved(name.to_string())));
    }

    anyhow::bail!(
        "no saved workspace matches the current directory.\n\
         Run `jackin load <role>` to use the current directory, or\n\
         run `jackin console` for the interactive operator console."
    );
}

/// Resolve a hardline target from the current directory context.
///
/// Finds the saved workspace whose host workdir or mounted host path best
/// matches `cwd`, then picks an indexed or currently-running container whose
/// class is permitted by the workspace:
/// 1. If the workspace's `last_role` has a candidate — prefer it
/// 2. If exactly one candidate — use it
/// 3. If multiple — prompt
/// 4. If zero — error with guidance to run `jackin load`
/// 5. No workspace match — error with guidance to pass an explicit selector
pub(crate) fn resolve_running_container_from_context(
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &Path,
    runner: &mut impl docker::CommandRunner,
) -> Result<String> {
    let Some((name, ws)) = find_saved_workspace_for_cwd(config, cwd) else {
        return resolve_ad_hoc_container_from_context(paths, cwd, runner).or_else(|err| {
            anyhow::bail!(
                "no saved workspace matches the current directory, and no ad-hoc instance matches it: {err}\n\
                 Run jackin hardline <role> to target explicitly, or\n\
                 run jackin load <role> to start a new session."
            )
        });
    };

    let allowed_classes: Vec<RoleSelector> = if ws.allowed_roles.is_empty() {
        config
            .roles
            .keys()
            .filter_map(|k| RoleSelector::parse(k).ok())
            .collect()
    } else {
        ws.allowed_roles
            .iter()
            .filter_map(|k| RoleSelector::parse(k).ok())
            .collect()
    };

    let mut candidates = indexed_hardline_candidates(paths, name, ws, &allowed_classes, runner)?;
    if candidates.is_empty() {
        let running = runtime::list_running_agent_names(runner)?;
        candidates = allowed_classes
            .iter()
            .flat_map(|class| runtime::matching_family(class, &running))
            .map(|name| HardlineCandidate {
                name,
                state: runtime::ContainerState::Running,
            })
            .collect();
    }
    candidates.sort_by(|a, b| a.name.cmp(&b.name));
    candidates.dedup_by(|a, b| a.name == b.name);
    let names: Vec<String> = candidates.iter().map(|c| c.name.clone()).collect();

    if let Some(last) = ws.last_role.as_deref()
        && let Some(preferred) =
            preferred_indexed_container(paths, name, ws, last, &names).or_else(|| {
                // Random instance IDs leave no deterministic primary
                // name; match by the role component inside container_base.
                let last_class = RoleSelector::parse(last).ok()?;
                let role_slug = instance::naming::compact_component(&last_class.name, "role");
                let mut family = names
                    .iter()
                    .filter(|n| instance::naming::class_family_matches_with_slug(&role_slug, n));
                let first = family.next()?.clone();
                // Commit only when unambiguous — multiple matches must
                // still reach the prompt branch below.
                if family.next().is_some() {
                    return None;
                }
                Some(first)
            })
    {
        return Ok(preferred);
    }

    match candidates.as_slice() {
        [] => anyhow::bail!(
            "no running roles found for workspace {name:?}.\n\
             Start one with jackin load, or run jackin hardline <role> to target explicitly."
        ),
        [only] => Ok(only.name.clone()),
        _ => {
            let options = hardline_candidate_prompt_options(paths, &candidates, runner);
            let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
            let choice = tui::prompt_choice(
                &format!("Workspace {name:?} has multiple matching instances. Select one:"),
                &option_refs,
            )?;
            Ok(candidates.swap_remove(choice).name)
        }
    }
}

fn resolve_ad_hoc_container_from_context(
    paths: &JackinPaths,
    cwd: &Path,
    runner: &mut impl docker::CommandRunner,
) -> Result<String> {
    let mut candidates = ad_hoc_hardline_candidates(paths, cwd, runner)?;
    candidates.sort_by(|a, b| a.name.cmp(&b.name));
    candidates.dedup_by(|a, b| a.name == b.name);

    match candidates.as_slice() {
        [] => anyhow::bail!("no matching ad-hoc instances found"),
        [only] => Ok(only.name.clone()),
        _ => {
            let options = hardline_candidate_prompt_options(paths, &candidates, runner);
            let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
            let choice = tui::prompt_choice(
                "Current directory has multiple ad-hoc instances. Select one:",
                &option_refs,
            )?;
            Ok(candidates.swap_remove(choice).name)
        }
    }
}

/// Hardline-prompt row: container name plus the docker-inspect state
/// captured during candidate collection. Carrying state through avoids
/// re-running `docker inspect` for every prompt row.
#[derive(Debug, Clone)]
struct HardlineCandidate {
    name: String,
    state: runtime::ContainerState,
}

fn hardline_candidate_prompt_options(
    paths: &JackinPaths,
    candidates: &[HardlineCandidate],
    runner: &mut impl docker::CommandRunner,
) -> Vec<String> {
    candidates
        .iter()
        .map(|candidate| hardline_candidate_prompt_label(paths, candidate, runner))
        .collect()
}

fn hardline_candidate_prompt_label(
    paths: &JackinPaths,
    candidate: &HardlineCandidate,
    runner: &mut impl docker::CommandRunner,
) -> String {
    let container = candidate.name.as_str();
    let sessions = runtime::inspect_agent_sessions(runner, container, &candidate.state);
    let docker_state = format!("docker:{}", candidate.state.short_label());
    let session_summary = runtime::describe_agent_session_count(&sessions);

    let state_dir = paths.data_dir.join(container);
    let Ok(manifest) = instance::InstanceManifest::read(&state_dir) else {
        return format!("{container} - {docker_state} - {session_summary}");
    };
    let isolation =
        crate::isolation::state::MountSummary::prompt_label_for_state_dir(&state_dir);
    format!(
        "{} - {} - {} - agent:{} - status:{} - {} - {} - {}",
        manifest.container_base,
        manifest.workspace_label,
        manifest.role_key,
        manifest.agent_runtime,
        manifest.status.label(),
        docker_state,
        session_summary,
        isolation
    )
}

fn ad_hoc_hardline_candidates(
    paths: &JackinPaths,
    cwd: &Path,
    runner: &mut impl docker::CommandRunner,
) -> Result<Vec<HardlineCandidate>> {
    let index = instance::InstanceIndex::read_or_rebuild(&paths.data_dir)?;
    let canonical_cwd = cwd.canonicalize()?;
    let cwd_fingerprint =
        instance::manifest::host_path_fingerprint(&canonical_cwd.display().to_string());
    let mut candidates = Vec::new();

    for entry in index.instances {
        let state_dir = paths.data_dir.join(&entry.container_base);
        let Some(manifest) =
            instance::InstanceManifest::read_or_log(&state_dir, "ad_hoc_hardline_candidates")
        else {
            continue;
        };
        if !ad_hoc_manifest_matches_cwd(&manifest, &canonical_cwd, &cwd_fingerprint) {
            continue;
        }
        let state = runtime::inspect_container_state(runner, &manifest.container_base);
        let docker_live = matches!(
            state,
            runtime::ContainerState::Running
                | runtime::ContainerState::Stopped { .. }
                | runtime::ContainerState::InspectUnavailable(_)
        );
        if docker_live || manifest.is_restore_candidate() {
            candidates.push(HardlineCandidate {
                name: manifest.container_base,
                state,
            });
        }
    }

    Ok(candidates)
}

fn ad_hoc_manifest_matches_cwd(
    manifest: &instance::InstanceManifest,
    canonical_cwd: &Path,
    cwd_fingerprint: &str,
) -> bool {
    if manifest.workspace_name.is_some() {
        return false;
    }
    if manifest.host_workdir_fingerprint == cwd_fingerprint {
        return true;
    }
    let label = std::path::PathBuf::from(&manifest.workspace_label);
    let workdir = std::path::PathBuf::from(&manifest.workdir);
    (label.is_absolute() && canonical_cwd.starts_with(&label))
        || (workdir.is_absolute() && canonical_cwd.starts_with(&workdir))
}

fn indexed_hardline_candidates(
    paths: &JackinPaths,
    workspace_name: &str,
    workspace: &WorkspaceConfig,
    allowed_classes: &[RoleSelector],
    runner: &mut impl docker::CommandRunner,
) -> Result<Vec<HardlineCandidate>> {
    let manifests = instance::InstanceIndex::matching_manifests(
        &paths.data_dir,
        instance::InstanceQuery {
            workspace_name: Some(workspace_name),
            workspace_label: workspace_name,
            workdir: &workspace.workdir,
            role_key: None,
            agent_runtime: None,
        },
    )?;
    Ok(manifests
        .into_iter()
        .filter(|manifest| {
            allowed_classes
                .iter()
                .any(|class| class.key() == manifest.role_key)
        })
        .filter_map(|manifest| {
            let state = runtime::inspect_container_state(runner, &manifest.container_base);
            let docker_live = matches!(
                state,
                runtime::ContainerState::Running
                    | runtime::ContainerState::Stopped { .. }
                    | runtime::ContainerState::InspectUnavailable(_)
            );
            (docker_live || manifest.is_restore_candidate()).then_some(HardlineCandidate {
                name: manifest.container_base,
                state,
            })
        })
        .collect())
}

fn preferred_indexed_container(
    paths: &JackinPaths,
    workspace_name: &str,
    workspace: &WorkspaceConfig,
    last_role: &str,
    candidates: &[String],
) -> Option<String> {
    let manifests = instance::InstanceIndex::matching_manifests(
        &paths.data_dir,
        instance::InstanceQuery {
            workspace_name: Some(workspace_name),
            workspace_label: workspace_name,
            workdir: &workspace.workdir,
            role_key: Some(last_role),
            agent_runtime: None,
        },
    )
    .ok()?;
    manifests
        .into_iter()
        .map(|manifest| manifest.container_base)
        .find(|container| candidates.contains(container))
}

/// Resolve which agent to launch when the operator hasn't explicitly
/// chosen one (no `--agent` flag, no workspace `default_agent`).
///
/// Reads the role's cached `jackin.role.toml` to discover its
/// `supported_agents` list. When the role allows multiple agents and
/// stdin is interactive, prompts the operator with a `dialoguer::Select`
/// menu. Single-agent roles, headless invocations, or a missing/invalid
/// cached manifest all return `Ok(None)` so the caller falls back to
/// the workspace `default_agent` → `Agent::Claude` resolution chain in
/// `runtime::launch::resolve_agent`.
///
/// Loading the manifest from the local cache (no git fetch) keeps this
/// fast and TUI-tear-down-safe: the prompt fires after `run_console`
/// has already restored the terminal but before the heavy `load_role`
/// pipeline begins.
pub(crate) fn prompt_agent_choice_if_needed(
    paths: &JackinPaths,
    selector: &RoleSelector,
    workspace_default: Option<crate::agent::Agent>,
) -> Result<Option<crate::agent::Agent>> {
    use std::io::IsTerminal;

    let Some(supported) = supported_agents_requiring_prompt(paths, selector, workspace_default)
    else {
        return Ok(None);
    };

    if !std::io::stdin().is_terminal() {
        return Ok(None);
    }

    let labels: Vec<String> = supported.iter().map(|a| a.slug().to_string()).collect();
    let selection = dialoguer::Select::new()
        .with_prompt(format!(
            "Role \"{}\" supports multiple agents. Choose one",
            selector.key()
        ))
        .items(&labels)
        .default(0)
        .interact()?;

    Ok(Some(supported[selection]))
}

/// Returns `Some(supported)` when the operator should be asked to pick
/// an agent, `None` when the choice is already determined (workspace
/// default set, single-agent role, or unreadable manifest). Split from
/// the prompting wrapper so the gating logic is unit-testable without
/// stdin / dialoguer scaffolding.
pub(crate) fn supported_agents_requiring_prompt(
    paths: &JackinPaths,
    selector: &RoleSelector,
    workspace_default: Option<crate::agent::Agent>,
) -> Option<Vec<crate::agent::Agent>> {
    if workspace_default.is_some() {
        return None;
    }
    let cached = crate::repo::CachedRepo::new(paths, selector);
    let supported = crate::manifest::RoleManifest::load(&cached.repo_dir)
        .ok()?
        .supported_agents();
    (supported.len() >= 2).then_some(supported)
}

pub(crate) fn remember_last_agent(
    paths: &JackinPaths,
    config: &mut AppConfig,
    workspace_name: Option<&str>,
    class: &RoleSelector,
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
            eprintln!("warning: failed to open config for last-used-role save: {error}");
            return;
        }
    };
    editor.set_last_agent(workspace_name, &class.key());
    match editor.save() {
        Ok(reloaded) => *config = reloaded,
        Err(error) => eprintln!("warning: failed to save last-used role: {error}"),
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
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
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
        config.roles.insert(
            "agent-smith".to_string(),
            config::RoleSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: project_dir.display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                allowed_roles: vec!["agent-smith".to_string()],
                default_role: Some("agent-smith".to_string()),
                default_agent: None,
                last_role: None,
                env: std::collections::BTreeMap::new(),
                roles: std::collections::BTreeMap::new(),
                keep_awake: workspace::KeepAwakeConfig::default(),
                op_account: None,
                claude: None,
                codex: None,
                amp: None,
                github: None,
                git_pull_on_entry: false,
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
        config.roles.insert(
            "agent-smith".to_string(),
            config::RoleSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                workdir: workspace_root.display().to_string(),
                mounts: vec![workspace::MountConfig {
                    src: repo_dir.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace/jackin".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                allowed_roles: vec!["agent-smith".to_string()],
                default_role: Some("agent-smith".to_string()),
                default_agent: None,
                last_role: None,
                env: std::collections::BTreeMap::new(),
                roles: std::collections::BTreeMap::new(),
                keep_awake: workspace::KeepAwakeConfig::default(),
                op_account: None,
                claude: None,
                codex: None,
                amp: None,
                github: None,
                git_pull_on_entry: false,
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
        config.roles.insert(
            "agent-smith".to_string(),
            config::RoleSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: project_dir.display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                allowed_roles: vec!["agent-smith".to_string()],
                default_role: None,
                default_agent: None,
                last_role: Some("ghost-role".to_string()),
                env: std::collections::BTreeMap::new(),
                roles: std::collections::BTreeMap::new(),
                keep_awake: workspace::KeepAwakeConfig::default(),
                op_account: None,
                claude: None,
                codex: None,
                amp: None,
                github: None,
                git_pull_on_entry: false,
            },
        );

        let resolved = resolve_agent_from_context(&config, &nested_dir).unwrap();

        assert_eq!(resolved.0.key(), "agent-smith");
        assert_eq!(resolved.1, LoadWorkspaceInput::Saved("my-app".to_string()));
    }

    /// Build an `AppConfig` pre-populated with an `agent-smith` role and a
    /// single workspace rooted at `project_dir`.
    fn config_with_workspace(
        project_dir: &Path,
        allowed_roles: Vec<String>,
        last_role: Option<String>,
    ) -> config::AppConfig {
        let mut config = config::AppConfig::default();
        config.roles.insert(
            "agent-smith".to_string(),
            config::RoleSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.roles.insert(
            "the-architect".to_string(),
            config::RoleSource {
                git: "https://github.com/jackin-project/jackin-the-architect.git".to_string(),
                trusted: true,
                env: std::collections::BTreeMap::new(),
            },
        );
        config.workspaces.insert(
            "my-app".to_string(),
            workspace::WorkspaceConfig {
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                workdir: "/workspace".to_string(),
                mounts: vec![workspace::MountConfig {
                    src: project_dir.display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                }],
                allowed_roles,
                default_role: None,
                default_agent: None,
                last_role,
                env: std::collections::BTreeMap::new(),
                roles: std::collections::BTreeMap::new(),
                keep_awake: workspace::KeepAwakeConfig::default(),
                op_account: None,
                claude: None,
                codex: None,
                amp: None,
                github: None,
                git_pull_on_entry: false,
            },
        );
        config
    }

    /// `list_running_agent_names` issues one `docker ps` capture; queue
    /// the running-role list as its response.
    fn fake_runner_with_running_agents(names: &[&str]) -> runtime::FakeRunner {
        let mut runner = runtime::FakeRunner::default();
        runner.capture_queue.push_back(names.join("\n"));
        runner
    }

    #[test]
    fn resolve_running_container_from_context_picks_lone_running_agent() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let nested_dir = project_dir.join("src");
        std::fs::create_dir_all(&nested_dir).unwrap();

        let config = config_with_workspace(&project_dir, vec!["agent-smith".to_string()], None);
        let running = "jackin-agentsmith-k7p9m2xq";
        let mut runner = fake_runner_with_running_agents(&[running]);

        let paths = paths::JackinPaths::for_tests(temp.path());
        let container =
            resolve_running_container_from_context(&paths, &config, &nested_dir, &mut runner)
                .unwrap();

        assert_eq!(container, running);
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
        let smith = "jackin-agentsmith-k7p9m2xq";
        let architect = "jackin-thearchitect-a1b2c3d4";
        let mut runner = fake_runner_with_running_agents(&[smith, architect]);

        let paths = paths::JackinPaths::for_tests(temp.path());
        let container =
            resolve_running_container_from_context(&paths, &config, &project_dir, &mut runner)
                .unwrap();

        assert_eq!(container, architect);
    }

    #[test]
    fn resolve_running_container_from_context_uses_indexed_unique_instance() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let config = config_with_workspace(&project_dir, vec!["agent-smith".to_string()], None);
        let manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
            container_base: "jackin-myapp-agentsmith-k7p9m2xq",
            workspace_name: Some("my-app"),
            workspace_label: "my-app",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jackin-agent-smith",
            docker: instance::DockerResources {
                role_container: "jackin-myapp-agentsmith-k7p9m2xq".to_string(),
                dind_container: "jackin-myapp-agentsmith-k7p9m2xq-dind".to_string(),
                network: "jackin-myapp-agentsmith-k7p9m2xq-net".to_string(),
                certs_volume: "jackin-myapp-agentsmith-k7p9m2xq-dind-certs".to_string(),
            },
        });
        let state_dir = paths.data_dir.join(&manifest.container_base);
        manifest.write(&state_dir).unwrap();
        instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
        let mut runner = runtime::FakeRunner::default();
        runner.capture_queue.push_back("true 0 false".to_string());

        let container =
            resolve_running_container_from_context(&paths, &config, &project_dir, &mut runner)
                .unwrap();

        assert_eq!(container, "jackin-myapp-agentsmith-k7p9m2xq");
    }

    #[test]
    fn resolve_running_container_from_context_uses_ad_hoc_indexed_instance() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();
        let canonical_project = project_dir.canonicalize().unwrap();
        let project = canonical_project.display().to_string();

        let config = AppConfig::default();
        let manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
            container_base: "jackin-agentsmith-k7p9m2xq",
            workspace_name: None,
            workspace_label: &project,
            workdir: &project,
            host_workdir_fingerprint: &instance::manifest::host_path_fingerprint(&project),
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jackin-agent-smith",
            docker: instance::DockerResources {
                role_container: "jackin-agentsmith-k7p9m2xq".to_string(),
                dind_container: "jackin-agentsmith-k7p9m2xq-dind".to_string(),
                network: "jackin-agentsmith-k7p9m2xq-net".to_string(),
                certs_volume: "jackin-agentsmith-k7p9m2xq-dind-certs".to_string(),
            },
        });
        let state_dir = paths.data_dir.join(&manifest.container_base);
        manifest.write(&state_dir).unwrap();
        instance::InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
        let mut runner = runtime::FakeRunner::default();

        let container =
            resolve_running_container_from_context(&paths, &config, &project_dir, &mut runner)
                .unwrap();

        assert_eq!(container, "jackin-agentsmith-k7p9m2xq");
    }

    #[test]
    fn hardline_candidate_prompt_label_includes_manifest_and_docker_state() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let container = "jackin-myapp-agentsmith-k7p9m2xq";
        let mut manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
            container_base: container,
            workspace_name: Some("my-app"),
            workspace_label: "my-app",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jackin-agent-smith",
            docker: instance::DockerResources {
                role_container: container.to_string(),
                dind_container: format!("{container}-dind"),
                network: format!("{container}-net"),
                certs_volume: format!("{container}-dind-certs"),
            },
        });
        manifest.mark_status(instance::InstanceStatus::RestoreAvailable);
        manifest.write(&paths.data_dir.join(container)).unwrap();
        let mut runner = runtime::FakeRunner::default();
        let candidate = HardlineCandidate {
            name: container.to_string(),
            state: runtime::ContainerState::Stopped {
                exit_code: 137,
                oom_killed: false,
            },
        };

        let label = hardline_candidate_prompt_label(&paths, &candidate, &mut runner);

        assert!(label.contains(container), "{label}");
        assert!(label.contains("my-app"), "{label}");
        assert!(label.contains("agent-smith"), "{label}");
        assert!(label.contains("agent:claude"), "{label}");
        assert!(label.contains("status:restore_available"), "{label}");
        assert!(label.contains("docker:stopped exit:137"), "{label}");
        assert!(label.contains("sessions:not_running"), "{label}");
    }

    #[test]
    fn hardline_candidate_prompt_label_counts_running_agent_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let container = "jackin-myapp-agentsmith-k7p9m2xq";
        let manifest = instance::InstanceManifest::new(instance::NewInstanceManifest {
            container_base: container,
            workspace_name: Some("my-app"),
            workspace_label: "my-app",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Codex,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jackin-agent-smith",
            docker: instance::DockerResources {
                role_container: container.to_string(),
                dind_container: format!("{container}-dind"),
                network: format!("{container}-net"),
                certs_volume: format!("{container}-dind-certs"),
            },
        });
        manifest.write(&paths.data_dir.join(container)).unwrap();
        let mut runner = runtime::FakeRunner::default();
        runner
            .capture_queue
            .push_back("PID COMMAND\n1 /jackin/runtime/entrypoint.sh\n42 codex exec".to_string());
        let candidate = HardlineCandidate {
            name: container.to_string(),
            state: runtime::ContainerState::Running,
        };

        let label = hardline_candidate_prompt_label(&paths, &candidate, &mut runner);

        assert!(label.contains("docker:running"), "{label}");
        assert!(label.contains("sessions:2"), "{label}");
    }

    #[test]
    fn resolve_running_container_from_context_errors_when_nothing_running() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        std::fs::create_dir_all(&project_dir).unwrap();

        let config = config_with_workspace(&project_dir, vec!["agent-smith".to_string()], None);
        let mut runner = fake_runner_with_running_agents(&[]);

        let paths = paths::JackinPaths::for_tests(temp.path());
        let err =
            resolve_running_container_from_context(&paths, &config, &project_dir, &mut runner)
                .unwrap_err()
                .to_string();

        assert!(err.contains("no running roles"), "got: {err}");
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

        let paths = paths::JackinPaths::for_tests(temp.path());
        let err =
            resolve_running_container_from_context(&paths, &config, &project_dir, &mut runner)
                .unwrap_err()
                .to_string();

        assert!(err.contains("no running roles"), "got: {err}");
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

        let paths = paths::JackinPaths::for_tests(temp.path());
        let err = resolve_running_container_from_context(&paths, &config, &unrelated, &mut runner)
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
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
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
            &RoleSelector::new(None, "agent-smith"),
            &Ok(()),
        );

        assert_eq!(
            config
                .workspaces
                .get("my-app")
                .and_then(|workspace| workspace.last_role.as_deref()),
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
            &RoleSelector::new(None, "agent-smith"),
            &Err(anyhow::anyhow!("load failed")),
        );

        assert_eq!(
            config
                .workspaces
                .get("my-app")
                .and_then(|workspace| workspace.last_role.as_deref()),
            None
        );
    }

    /// Regression: a workspace whose workdir is a broad parent directory must not
    /// match when cwd is an unrelated subdirectory not covered by any mount source.
    #[test]
    fn broad_workdir_does_not_match_unrelated_subdirectory() {
        let temp = tempfile::tempdir().unwrap();
        let broad_workdir = temp.path().join("Projects");
        let agent_repo = broad_workdir.join("role-repo");
        let unrelated = broad_workdir.join("jackin4");
        std::fs::create_dir_all(&agent_repo).unwrap();
        std::fs::create_dir_all(&unrelated).unwrap();

        let mut config = config::AppConfig::default();
        config.workspaces.insert(
            "jackin-roles".to_string(),
            workspace::WorkspaceConfig {
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                workdir: broad_workdir.canonicalize().unwrap().display().to_string(),
                mounts: vec![workspace::MountConfig {
                    src: agent_repo.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace/role-repo".to_string(),
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
        let agent_repo = broad_workdir.join("role-repo");
        let inside_repo = agent_repo.join("src");
        std::fs::create_dir_all(&inside_repo).unwrap();

        let mut config = config::AppConfig::default();
        config.workspaces.insert(
            "jackin-roles".to_string(),
            workspace::WorkspaceConfig {
                version: crate::config::CURRENT_WORKSPACE_VERSION.to_string(),
                workdir: broad_workdir.canonicalize().unwrap().display().to_string(),
                mounts: vec![workspace::MountConfig {
                    src: agent_repo.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace/role-repo".to_string(),
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
        assert_eq!(result.unwrap().0, "jackin-roles");
    }

    // ── supported_agents_requiring_prompt gating ─────────────────────

    fn write_role_manifest(role_dir: &std::path::Path, body: &str) {
        std::fs::create_dir_all(role_dir).unwrap();
        std::fs::write(role_dir.join("jackin.role.toml"), body).unwrap();
    }

    #[test]
    fn requires_prompt_when_role_supports_two_agents_and_no_workspace_default() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let selector = crate::selector::RoleSelector::parse("the-architect").unwrap();
        write_role_manifest(
            &crate::repo::CachedRepo::new(&paths, &selector).repo_dir,
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#,
        );

        let agents = supported_agents_requiring_prompt(&paths, &selector, None)
            .expect("multi-agent role with no workspace default must trigger a prompt");
        assert_eq!(
            agents,
            vec![crate::agent::Agent::Claude, crate::agent::Agent::Codex]
        );
    }

    #[test]
    fn requires_prompt_includes_amp_when_role_supports_three_agents() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let selector = crate::selector::RoleSelector::parse("the-architect").unwrap();
        write_role_manifest(
            &crate::repo::CachedRepo::new(&paths, &selector).repo_dir,
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"
agents = ["claude", "codex", "amp"]

[claude]
plugins = []

[codex]

[amp]
"#,
        );

        let agents = supported_agents_requiring_prompt(&paths, &selector, None)
            .expect("three-agent role with no workspace default must trigger a prompt");
        assert_eq!(
            agents,
            vec![
                crate::agent::Agent::Claude,
                crate::agent::Agent::Codex,
                crate::agent::Agent::Amp,
            ]
        );
    }

    #[test]
    fn skips_prompt_when_workspace_default_agent_is_set() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let selector = crate::selector::RoleSelector::parse("the-architect").unwrap();
        write_role_manifest(
            &crate::repo::CachedRepo::new(&paths, &selector).repo_dir,
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#,
        );

        let result =
            supported_agents_requiring_prompt(&paths, &selector, Some(crate::agent::Agent::Codex));
        assert!(
            result.is_none(),
            "explicit workspace default_agent must short-circuit the prompt"
        );
    }

    #[test]
    fn skips_prompt_when_role_supports_a_single_agent() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let selector = crate::selector::RoleSelector::parse("solo").unwrap();
        write_role_manifest(
            &crate::repo::CachedRepo::new(&paths, &selector).repo_dir,
            r#"version = "v1alpha2"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
"#,
        );

        assert!(
            supported_agents_requiring_prompt(&paths, &selector, None).is_none(),
            "single-agent roles have nothing to disambiguate"
        );
    }

    #[test]
    fn skips_prompt_when_manifest_is_missing_or_unreadable() {
        let temp = tempfile::tempdir().unwrap();
        let paths = paths::JackinPaths::for_tests(temp.path());
        let selector = crate::selector::RoleSelector::parse("ghost").unwrap();
        // No manifest written — load_role will fetch and validate later.
        assert!(supported_agents_requiring_prompt(&paths, &selector, None).is_none());
    }
}
