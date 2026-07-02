//! Hardline reconnect, restore, and moved-path recovery logic.

use anyhow::{Context, Result};

use crate::workspace::{self, resolve_path};
use jackin_config::{AppConfig, LoadWorkspaceInput};
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::ShellRunner;
use jackin_docker::docker_client::BollardDockerClient;
use jackin_docker::docker_client::DockerApi;
use jackin_runtime::instance;
use jackin_runtime::runtime;

use crate::console;

use super::resolve_new_session_agent;

/// Bridge from the TUI event loop to async docker work for Stop/Purge.
/// Now that `run_in_place` is async, the work runs directly on the
/// existing Tokio runtime — no nested runtime or OS thread needed.
pub(super) struct ConsoleInPlaceHandler {
    pub(super) paths: JackinPaths,
    pub(super) debug: bool,
}

impl console::InstanceActionHandler<jackin_core::Agent> for ConsoleInPlaceHandler {
    async fn run_in_place(
        &mut self,
        container: &str,
        action: console::ConsoleInstanceAction,
    ) -> Result<()> {
        let docker = BollardDockerClient::connect()?;
        let mut runner = ShellRunner { debug: self.debug };
        // Wrap the eject + post-condition work in an async block so a
        // partial failure still hits the trailing reconcile +
        // manifest-status update. Without this, an eject that errored
        // after removing the last keep-awake container would leave
        // caffeinate asserted on the host and the on-disk manifest
        // stuck at Active/Running while the container is half-gone.
        let result: Result<()> = async {
            match action {
                console::ConsoleInstanceAction::Stop => {
                    runtime::eject_role(&self.paths, container, &docker).await
                }
                console::ConsoleInstanceAction::Purge => {
                    runtime::eject_role(&self.paths, container, &docker).await?;
                    runtime::purge_container_state(&self.paths, container, &docker, &mut runner)
                        .await
                }
                _ => Ok(()),
            }
        }
        .await;
        if matches!(action, console::ConsoleInstanceAction::Stop) {
            mark_instance_restore_available_after_stop(
                &self.paths,
                container,
                &docker,
                result.is_ok(),
            )
            .await;
        }
        runtime::reconcile_keep_awake(&self.paths, &docker, &mut runner).await;
        result
    }
}

/// Promote the manifest for `container` to `RestoreAvailable` so the
/// console list reflects "stopped, recoverable on demand" instead of the
/// stale `Active` / `Running` that `eject_role` would otherwise leave
/// behind (eject removes Docker resources but writes nothing to the
/// on-disk index). Logs and proceeds on error — the eject itself
/// succeeded and a stale row is recoverable on next interaction with
/// the container.
pub(crate) fn mark_instance_restore_available(paths: &JackinPaths, container: &str) {
    let state_dir = paths.data_dir.join(container);
    match instance::InstanceManifest::read(&state_dir) {
        Ok(mut manifest) => {
            if let Err(e) = manifest.mark_restore_available(paths) {
                eprintln!("[jackin] failed to mark instance {container} as RestoreAvailable: {e}");
            }
        }
        Err(e) => {
            eprintln!("[jackin] cannot update instance manifest for {container} after stop: {e}");
        }
    }
}

pub(crate) async fn mark_instance_restore_available_after_stop(
    paths: &JackinPaths,
    container: &str,
    docker: &impl DockerApi,
    stop_succeeded: bool,
) {
    if stop_succeeded {
        mark_instance_restore_available(paths, container);
        return;
    }

    if matches!(
        docker.inspect_container_state(container).await,
        runtime::ContainerState::NotFound
    ) {
        mark_instance_restore_available(paths, container);
    }
}

pub(super) async fn handle_console_instance_action(
    paths: &JackinPaths,
    config: &mut AppConfig,
    outcome: console::ConsoleOutcome,
    docker: &impl DockerApi,
    runner: &mut ShellRunner,
) -> Result<()> {
    let console::ConsoleOutcome::InstanceAction { container, action } = outcome else {
        unreachable!("console launch outcomes are handled before instance actions")
    };
    match action {
        console::ConsoleInstanceAction::Reconnect => {
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            let result = if let Some(manifest) =
                restore_candidate_for_hardline(paths, &container, docker).await?
            {
                restore_hardline_instance(paths, config, &manifest, docker, runner).await
            } else {
                runtime::hardline_agent(paths, &container, docker, runner).await
            };
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            result
        }
        console::ConsoleInstanceAction::ReconnectFocus(session_id) => {
            // Same as `Reconnect` but forwards a pane-focus id to the
            // daemon. Only fires for running instances reachable via
            // the bind-mounted socket — `restore_hardline_instance`
            // (cold-restore path) does not surface the snapshot
            // preview that produces a focus id, so we route directly
            // through the focused hardline.
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            let result = runtime::hardline_agent_with_focus(
                paths,
                &container,
                Some(session_id),
                docker,
                runner,
            )
            .await;
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            result
        }
        console::ConsoleInstanceAction::NewSession
        | console::ConsoleInstanceAction::NewSessionWithAgent(_) => {
            let manifest = instance::InstanceManifest::read(&paths.data_dir.join(&container))
                .with_context(|| {
                    format!(
                        "cannot start a new agent session in `{container}` because its instance manifest is missing"
                    )
                })?;
            let selected_agent =
                if let console::ConsoleInstanceAction::NewSessionWithAgent(agent) = action {
                    agent
                } else {
                    resolve_new_session_agent(paths, config, &manifest)?
                };
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            let result = runtime::spawn_agent_session(
                paths,
                &container,
                Some(&manifest),
                selected_agent,
                None,
                &[],
                config.git.coauthor_trailer,
                config.git.dco,
                docker,
                runner,
            )
            .await;
            runtime::reconcile_keep_awake(paths, docker, runner).await;
            result
        }
        console::ConsoleInstanceAction::Shell => {
            runtime::spawn_shell_session(paths, &container, docker, runner).await
        }
        console::ConsoleInstanceAction::Inspect => {
            println!(
                "{}",
                runtime::inspect_hardline_instance(paths, &container, docker).await?
            );
            Ok(())
        }
        // Stop and Purge are dispatched via `ConsoleInPlaceHandler::run_in_place`
        // (see `console::ConsoleInstanceAction::runs_in_place`), so
        // the console event loop never returns
        // `ConsoleOutcome::InstanceAction` for them. Bail with a
        // diagnostic — `unreachable!` would panic in a future caller
        // that bypasses the runs_in_place gate; bail surfaces the
        // dispatch bug without taking the process down.
        console::ConsoleInstanceAction::Stop | console::ConsoleInstanceAction::Purge => {
            anyhow::bail!(
                "{action:?} must run via ConsoleInPlaceHandler::run_in_place; reached handle_console_instance_action by mistake"
            )
        }
    }
}

pub(super) async fn restore_candidate_for_hardline(
    paths: &JackinPaths,
    container: &str,
    docker: &impl DockerApi,
) -> Result<Option<instance::InstanceManifest>> {
    let state_dir = paths.data_dir.join(container);
    let Some(mut manifest) = instance::InstanceManifest::read_optional(&state_dir)? else {
        return Ok(None);
    };
    if !manifest.is_restore_candidate() {
        return Ok(None);
    }

    match docker.inspect_container_state(container).await {
        runtime::ContainerState::NotFound => {
            manifest.mark_restore_available(paths)?;
            Ok(Some(manifest))
        }
        runtime::ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!(
                "{}",
                runtime::docker_unavailable_msg(
                    &format!("inspect container `{container}`"),
                    &reason,
                )
            );
        }
        runtime::ContainerState::Running
        | runtime::ContainerState::Paused
        | runtime::ContainerState::Restarting
        | runtime::ContainerState::Created
        | runtime::ContainerState::Removing
        | runtime::ContainerState::Dead
        | runtime::ContainerState::Stopped { .. } => Ok(None),
    }
}

pub(super) async fn restore_hardline_instance(
    paths: &JackinPaths,
    config: &mut AppConfig,
    manifest: &instance::InstanceManifest,
    docker: &impl DockerApi,
    runner: &mut impl jackin_docker::CommandRunner,
) -> Result<()> {
    let class = RoleSelector::parse(&manifest.role_key)?;
    let cwd = std::env::current_dir()?;
    let workspace = if let Some(workspace_name) = manifest.workspace_name.as_ref() {
        workspace::resolve_load_workspace(
            config,
            &class,
            &cwd,
            LoadWorkspaceInput::Saved(workspace_name.clone()),
            &[],
        )?
    } else {
        let input = resolve_ad_hoc_restore_input(manifest, &cwd)?;
        workspace::resolve_load_workspace(config, &class, &cwd, input, &[])?
    };

    let opts = runtime::LoadOptions {
        agent: Some(manifest.agent()?),
        role_branch: manifest.role_source_ref.clone(),
        restore_container_base: Some(manifest.container_base.clone()),
        restore_role_source_git: Some(manifest.role_source_git.clone()),
        ..runtime::LoadOptions::default()
    };
    runtime::load_role(paths, config, &class, &workspace, docker, runner, &opts).await
}

fn resolve_ad_hoc_restore_input(
    manifest: &instance::InstanceManifest,
    cwd: &std::path::Path,
) -> Result<LoadWorkspaceInput> {
    let cwd = cwd.canonicalize()?;
    if ad_hoc_restore_input_for_current_dir(manifest, &cwd, false).is_some() {
        return Ok(LoadWorkspaceInput::CurrentDir);
    }
    if let Some(path) = prompt_moved_ad_hoc_project_path(manifest, &cwd)? {
        return ad_hoc_restore_input_for_moved_path(manifest, &path).with_context(|| {
            format!(
                "cannot restore ad-hoc instance `{}` from {}",
                manifest.container_base,
                path.display()
            )
        });
    }
    anyhow::bail!(
        "cannot restore ad-hoc instance `{}` from {}; rerun `jackin hardline {}` from its original project directory, select the moved project path interactively, or use `jackin eject {} --purge` to discard it",
        manifest.container_base,
        cwd.display(),
        manifest.container_base,
        manifest.container_base
    )
}

pub(crate) fn ad_hoc_restore_input_for_current_dir(
    manifest: &instance::InstanceManifest,
    cwd: &std::path::Path,
    allow_moved: bool,
) -> Option<LoadWorkspaceInput> {
    let cwd_str = cwd.display().to_string();
    let cwd_fingerprint = instance::manifest::host_path_fingerprint(&cwd_str);
    if cwd_fingerprint == manifest.host_workdir_fingerprint {
        return Some(LoadWorkspaceInput::CurrentDir);
    }
    if allow_moved {
        return Some(LoadWorkspaceInput::Path {
            src: cwd_str,
            dst: manifest.workdir.clone(),
        });
    }
    None
}

pub(crate) fn ad_hoc_restore_input_for_moved_path(
    manifest: &instance::InstanceManifest,
    path: &std::path::Path,
) -> Option<LoadWorkspaceInput> {
    let path = path.canonicalize().ok()?;
    ad_hoc_restore_input_for_current_dir(manifest, &path, true)
}

fn prompt_moved_ad_hoc_project_path(
    manifest: &instance::InstanceManifest,
    cwd: &std::path::Path,
) -> Result<Option<std::path::PathBuf>> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        return Ok(None);
    }
    let choices = [
        format!("Use current directory ({})", cwd.display()),
        "Browse for moved project path".to_owned(),
        "Enter another moved project path".to_owned(),
        "Cancel restore".to_owned(),
    ];
    let selected = dialoguer::Select::new()
        .with_prompt(format!(
            "Ad-hoc instance `{}` was created for `{}`, but the current directory is `{}`. Which host path should be mounted at the original in-container workdir?",
            manifest.container_base,
            manifest.workdir,
            cwd.display()
        ))
        .items(&choices)
        .default(0)
        .interact()?;

    match selected {
        0 => Ok(Some(cwd.to_path_buf())),
        1 => prompt_ad_hoc_moved_path_browser(cwd),
        2 => prompt_ad_hoc_moved_path_entry(),
        _ => Ok(None),
    }
}

fn prompt_ad_hoc_moved_path_browser(start: &std::path::Path) -> Result<Option<std::path::PathBuf>> {
    let mut cwd = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    loop {
        let choices = moved_path_browser_choices(&cwd);
        let labels: Vec<String> = choices.iter().map(MovedPathBrowserChoice::label).collect();
        let selected = dialoguer::Select::new()
            .with_prompt(format!(
                "Browse to the moved project directory from {}",
                cwd.display()
            ))
            .items(&labels)
            .default(0)
            .interact()?;
        match choices
            .get(selected)
            .cloned()
            .unwrap_or(MovedPathBrowserChoice::Cancel)
        {
            MovedPathBrowserChoice::SelectCurrent(path) => return Ok(Some(path)),
            MovedPathBrowserChoice::Parent(path) | MovedPathBrowserChoice::Child(path) => {
                cwd = path;
            }
            MovedPathBrowserChoice::Manual => return prompt_ad_hoc_moved_path_entry(),
            MovedPathBrowserChoice::Cancel => return Ok(None),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MovedPathBrowserChoice {
    SelectCurrent(std::path::PathBuf),
    Parent(std::path::PathBuf),
    Child(std::path::PathBuf),
    Manual,
    Cancel,
}

impl MovedPathBrowserChoice {
    fn label(&self) -> String {
        match self {
            Self::SelectCurrent(path) => format!("Use this directory ({})", path.display()),
            Self::Parent(path) => format!("Go up ({})", path.display()),
            Self::Child(path) => format!(
                "{}/",
                path.file_name().unwrap_or_default().to_string_lossy()
            ),
            Self::Manual => "Enter a path manually".to_owned(),
            Self::Cancel => "Cancel restore".to_owned(),
        }
    }
}

pub(crate) fn moved_path_browser_choices(cwd: &std::path::Path) -> Vec<MovedPathBrowserChoice> {
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    let mut choices = vec![MovedPathBrowserChoice::SelectCurrent(cwd.clone())];
    if let Some(parent) = cwd.parent() {
        choices.push(MovedPathBrowserChoice::Parent(parent.to_path_buf()));
    }
    choices.extend(
        moved_path_browser_child_dirs(&cwd)
            .into_iter()
            .map(MovedPathBrowserChoice::Child),
    );
    choices.push(MovedPathBrowserChoice::Manual);
    choices.push(MovedPathBrowserChoice::Cancel);
    choices
}

fn moved_path_browser_child_dirs(cwd: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(cwd) else {
        return Vec::new();
    };
    let mut dirs: Vec<std::path::PathBuf> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            path.is_dir().then_some(path)
        })
        .collect();
    dirs.sort_by_key(|path| {
        path.file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    dirs
}

/// One step of the moved-path entry loop, factored out of the
/// `dialoguer::Input::interact_text()` call so the four cases (blank /
/// valid dir / not-a-dir / canonicalize-fail) can be unit-tested
/// without an interactive prompt.
pub(crate) enum MovedPathEntryStep {
    /// Empty input → operator cancelled.
    Cancel,
    /// Canonical absolute path; entry loop returns this.
    Accepted(std::path::PathBuf),
    /// Operator must retry; carries the message to print before the
    /// next prompt iteration.
    Retry(String),
}

pub(crate) fn classify_moved_path_entry(raw: &str) -> MovedPathEntryStep {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return MovedPathEntryStep::Cancel;
    }
    let path = std::path::PathBuf::from(resolve_path(trimmed));
    match path.canonicalize() {
        Ok(canonical) if canonical.is_dir() => MovedPathEntryStep::Accepted(canonical),
        Ok(canonical) => MovedPathEntryStep::Retry(format!(
            "path `{}` exists but is not a directory; enter a project directory or leave blank to cancel",
            canonical.display(),
        )),
        Err(err) => MovedPathEntryStep::Retry(format!(
            "cannot use `{}`: {err}; enter an existing project directory or leave blank to cancel",
            path.display(),
        )),
    }
}

fn prompt_ad_hoc_moved_path_entry() -> Result<Option<std::path::PathBuf>> {
    loop {
        let raw: String = dialoguer::Input::new()
            .with_prompt("Moved project path")
            .interact_text()?;
        match classify_moved_path_entry(&raw) {
            MovedPathEntryStep::Cancel => return Ok(None),
            MovedPathEntryStep::Accepted(path) => return Ok(Some(path)),
            MovedPathEntryStep::Retry(msg) => eprintln!("{msg}"),
        }
    }
}
