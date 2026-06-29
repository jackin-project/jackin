//! Target classification and context-aware resolution helpers for CLI commands.
//!
//! Resolves a raw target string (workspace name, directory path, or
//! `<src>:<dst>` mount spec) into a `TargetKind`, then maps it to a running
//! container, workspace config, or agent choice. Used by `app/mod.rs` to
//! drive `jackin load`, `jackin console`, and related subcommands.
//!
//! Not responsible for: launch or attach mechanics (`runtime/`), or config
//! persistence (`config/`).

use anyhow::Result;
use std::path::Path;

use crate::runtime;
use crate::workspace::{LoadWorkspaceInput, WorkspaceConfig, expand_tilde};
use jackin_config::AppConfig;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::docker_client::DockerApi;
use jackin_runtime::instance;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TargetKind {
    Path { src: String, dst: String },
    Name(String),
}

/// Classify a target string as either a path or a plain name.
///
/// Contains `/`, or starts with `.` or `~` => always a path.
/// Otherwise => a plain name (workspace or directory name).
pub(crate) fn classify_target(target: &str) -> TargetKind {
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
                src: target.to_owned(),
                dst: expanded,
            };
        };
        TargetKind::Path {
            src: src.to_owned(),
            dst: dst.to_owned(),
        }
    } else {
        TargetKind::Name(target.to_owned())
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

#[cfg(test)]
pub(crate) fn resolve_target_name(
    name: &str,
    config: &AppConfig,
    cwd: &Path,
) -> Result<LoadWorkspaceInput> {
    resolve_target_name_with_choice(name, config, cwd, |message, options| {
        let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
        crate::prompt::prompt_choice(message, &option_refs)
    })
}

pub(crate) fn resolve_target_name_with_choice(
    name: &str,
    config: &AppConfig,
    cwd: &Path,
    mut choose: impl FnMut(&str, Vec<String>) -> Result<usize>,
) -> Result<LoadWorkspaceInput> {
    let workspace_exists = config.workspaces.contains_key(name);
    let dir_exists = cwd.join(name).is_dir();

    match (workspace_exists, dir_exists) {
        (true, true) => {
            let choice = choose(
                &format!("\"{name}\" matches both a saved workspace and a directory."),
                vec![
                    format!("Use workspace \"{name}\""),
                    format!("Use directory ./{name}"),
                ],
            )?;
            if choice == 0 {
                Ok(LoadWorkspaceInput::Saved(name.to_owned()))
            } else {
                let full_path = cwd.join(name);
                let canonical = full_path.display().to_string();
                Ok(LoadWorkspaceInput::Path {
                    src: canonical.clone(),
                    dst: canonical,
                })
            }
        }
        (true, false) => Ok(LoadWorkspaceInput::Saved(name.to_owned())),
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
                    "(none)".to_owned()
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

/// Resolve the role and workspace from the current directory context.
///
/// Finds the saved workspace whose host workdir or mounted host path best
/// matches `cwd`, then picks the role:
/// 1. `last_role` (most recently used)
/// 2. `default_role` (explicitly configured)
/// 3. If multiple roles available — prompt
/// 4. If exactly one role — use it
/// 5. No match — error with guidance
#[cfg(test)]
pub(crate) fn resolve_agent_from_context(
    config: &AppConfig,
    cwd: &Path,
) -> Result<(RoleSelector, LoadWorkspaceInput)> {
    resolve_agent_from_context_with_choice(config, cwd, |message, options| {
        let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
        crate::prompt::prompt_choice(message, &option_refs)
    })
}

pub(crate) fn resolve_agent_from_context_with_choice(
    config: &AppConfig,
    cwd: &Path,
    mut choose: impl FnMut(&str, Vec<String>) -> Result<usize>,
) -> Result<(RoleSelector, LoadWorkspaceInput)> {
    if let Some((name, ws)) = jackin_config::find_saved_workspace_for_cwd(config, cwd) {
        let eligible =
            jackin_console::workspace::eligible_roles_for_workspace(config.roles.keys(), ws);

        // Preferred-role shortcut: last_role, then default_role.
        if let Some(preferred_idx) = jackin_console::workspace::preferred_role_index(
            &eligible,
            ws.last_role.as_deref(),
            ws.default_role.as_deref(),
        ) {
            return Ok((
                eligible[preferred_idx].clone(),
                LoadWorkspaceInput::Saved(name.to_owned()),
            ));
        }

        let chosen = match eligible.as_slice() {
            [] => anyhow::bail!("no roles configured; add one with jackin load <role>"),
            [only] => only.clone(),
            _ => {
                let options: Vec<String> = eligible.iter().map(RoleSelector::key).collect();
                let choice = choose(
                    &format!("Workspace {name:?} has multiple roles. Select one:"),
                    options,
                )?;
                eligible[choice].clone()
            }
        };
        return Ok((chosen, LoadWorkspaceInput::Saved(name.to_owned())));
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
pub(crate) async fn resolve_running_container_from_context(
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &Path,
    docker: &impl DockerApi,
) -> Result<String> {
    let Some((name, ws)) = jackin_config::find_saved_workspace_for_cwd(config, cwd) else {
        return resolve_ad_hoc_container_from_context(paths, cwd, docker).await.or_else(|err| {
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

    let mut candidates =
        indexed_hardline_candidates(paths, name, ws, &allowed_classes, docker).await?;
    if candidates.is_empty() {
        let running = runtime::list_running_agent_names(docker).await?;
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
            let options = hardline_candidate_prompt_options(paths, &candidates);
            let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
            let choice = crate::prompt::prompt_choice(
                &format!("Workspace {name:?} has multiple matching instances. Select one:"),
                &option_refs,
            )?;
            Ok(candidates.swap_remove(choice).name)
        }
    }
}

async fn resolve_ad_hoc_container_from_context(
    paths: &JackinPaths,
    cwd: &Path,
    docker: &impl DockerApi,
) -> Result<String> {
    let mut candidates = ad_hoc_hardline_candidates(paths, cwd, docker).await?;
    candidates.sort_by(|a, b| a.name.cmp(&b.name));
    candidates.dedup_by(|a, b| a.name == b.name);

    match candidates.as_slice() {
        [] => anyhow::bail!("no matching ad-hoc instances found"),
        [only] => Ok(only.name.clone()),
        _ => {
            let options = hardline_candidate_prompt_options(paths, &candidates);
            let option_refs: Vec<&str> = options.iter().map(String::as_str).collect();
            let choice = crate::prompt::prompt_choice(
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
) -> Vec<String> {
    let mut options = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        options.push(hardline_candidate_prompt_label(paths, candidate));
    }
    options
}

fn hardline_candidate_prompt_label(paths: &JackinPaths, candidate: &HardlineCandidate) -> String {
    let container = candidate.name.as_str();
    // Use state from candidate — session inventory not fetched here
    // to avoid blocking async in a sync context
    let docker_state = format!("docker:{}", candidate.state.short_label());
    let session_summary = "sessions:unknown".to_owned();

    let state_dir = paths.data_dir.join(container);
    let Ok(manifest) = instance::InstanceManifest::read(&state_dir) else {
        return format!("{container} - {docker_state} - {session_summary}");
    };
    let isolation =
        jackin_runtime::isolation::state::MountSummary::prompt_label_for_state_dir(&state_dir);
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

async fn ad_hoc_hardline_candidates(
    paths: &JackinPaths,
    cwd: &Path,
    docker: &impl DockerApi,
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
        let state = docker
            .inspect_container_state(&manifest.container_base)
            .await;
        let docker_live = state.is_present();
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

async fn indexed_hardline_candidates(
    paths: &JackinPaths,
    workspace_name: &str,
    workspace: &WorkspaceConfig,
    allowed_classes: &[RoleSelector],
    docker: &impl DockerApi,
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
    let filtered: Vec<_> = manifests
        .into_iter()
        .filter(|manifest| {
            allowed_classes
                .iter()
                .any(|class| class.key() == manifest.role_key)
        })
        .collect();
    let mut candidates = Vec::new();
    for manifest in filtered {
        let state = docker
            .inspect_container_state(&manifest.container_base)
            .await;
        let docker_live = state.is_present();
        if docker_live || manifest.is_restore_candidate() {
            candidates.push(HardlineCandidate {
                name: manifest.container_base,
                state,
            });
        }
    }
    Ok(candidates)
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
    workspace_default: Option<jackin_core::Agent>,
) -> Result<Option<jackin_core::Agent>> {
    use std::io::IsTerminal;

    let Some(supported) = supported_agents_requiring_prompt(paths, selector, workspace_default)
    else {
        return Ok(None);
    };

    if !std::io::stdin().is_terminal() {
        return Ok(None);
    }

    let labels: Vec<String> = supported.iter().map(|a| a.slug().to_owned()).collect();
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
    workspace_default: Option<jackin_core::Agent>,
) -> Option<Vec<jackin_core::Agent>> {
    if workspace_default.is_some() {
        return None;
    }
    let cached = jackin_manifest::repo::CachedRepo::new(paths, selector);
    let supported = jackin_manifest::load_role_manifest(&cached.repo_dir)
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
    let mut editor = match jackin_config::ConfigEditor::open(paths) {
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
mod tests;
