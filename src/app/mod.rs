//! CLI dispatch: maps parsed `Cli` commands to runtime, console, workspace,
//! and instance calls.
//!
//! `pub async fn run` is the binary entry point after argument parsing. Not a
//! stable library boundary — callers are `main.rs` and tests only.
//!
//! Not responsible for: argument parsing (`cli/`), runtime mechanics
//! (`runtime/`), or TUI rendering (`console/tui/`). This module is glue.

mod config_cmd;
pub mod context;
mod helpers;
mod load_cmd;
mod prune_cmd;
mod token_cmd;
mod workspace_cmd;

use token_cmd::handle_claude_token;
#[cfg(test)]
pub(crate) use token_cmd::{delete_prior_op_item_with_runner, validate_setup_role_allowed};

use helpers::{
    mount_display, mount_mode, render_workspace_show, resolve_instance_reference,
    resolve_role_to_container,
};

use anyhow::{Context, Result};

use crate::cli::role::ConsoleArgs;
use crate::cli::{Cli, Command};
use crate::config::{self, AppConfig};
use crate::console;
use crate::docker::ShellRunner;
use crate::docker_client::{BollardDockerClient, DockerApi};
use crate::instance;
use crate::paths::JackinPaths;
use crate::runtime;
use crate::selector::RoleSelector;
use crate::tui;
use crate::workspace::{self, LoadWorkspaceInput, resolve_path};

use self::context::prompt_agent_choice_if_needed;

/// Parse an `auth_forward` mode value as it arrived from the CLI.
fn parse_auth_forward_mode_from_cli(raw: &str) -> anyhow::Result<config::AuthForwardMode> {
    raw.parse().map_err(|e: String| anyhow::anyhow!("{e}"))
}

/// Parse an agent slug as it arrived from the CLI.
fn parse_agent_from_cli(raw: &str) -> anyhow::Result<crate::agent::Agent> {
    raw.parse()
        .map_err(|_| anyhow::anyhow!("unknown agent {raw:?}; expected one of: claude, codex, amp"))
}

fn rich_prelaunch_choice(title: &str, items: Vec<String>) -> anyhow::Result<usize> {
    runtime::progress::prelaunch_select_choice(
        std::env::var_os("JACKIN_NO_MOTION").is_some(),
        title,
        items,
    )
}

async fn play_construct_intro_if_needed(
    paths: &JackinPaths,
    docker: &impl DockerApi,
) -> runtime::EntryClaim {
    let claim = runtime::claim_construct_entry(paths, docker).await;
    if (claim.start_kind() == runtime::StartKind::FreshConstruct
        || runtime::force_boundary_intro_enabled())
        && runtime::progress::rich_terminal_supported()
    {
        // The intro is two screens: the opening phrase/brand screen, then the
        // accelerating warp into the Construct.
        crate::tui::warp_intro();
    }
    claim
}

pub async fn run(cli: Cli) -> Result<()> {
    let debug = cli.debug;
    tui::set_debug_mode(debug);

    // Resolve the subcommand. Bare `jackin` currently routes to the same
    // console handler as `jackin console`; the TTY-capability fallback and
    // the deprecation warning for `launch` land in a follow-up commit.
    let command = match cli.command {
        Some(cmd) => cmd,
        None => Command::Console(cli.console_args),
    };

    let paths = JackinPaths::detect()?;
    let command_name = command_name(&command);
    let diagnostics = crate::diagnostics::RunDiagnostics::start(&paths, debug, command_name)?;
    let _diagnostics_guard = diagnostics.activate();
    crate::diagnostics::prune_old_runs(&paths);
    if debug {
        announce_debug_run(&diagnostics);
    }
    let command = match command {
        Command::Role(command) => return crate::role_authoring::run(command),
        command => command,
    };
    let mut config = AppConfig::load_or_init(&paths)?;
    let mut runner = ShellRunner { debug };
    let connect_docker = || BollardDockerClient::connect();

    match command {
        Command::Load(args) => {
            load_cmd::handle_load(
                args,
                &mut config,
                &paths,
                debug,
                &mut runner,
                connect_docker,
            )
            .await
        }
        Command::Console(ConsoleArgs {}) => load_cmd::handle_console(config, paths, debug).await,
        Command::Hardline(args) => {
            load_cmd::handle_hardline(args, config, paths, debug, connect_docker).await
        }
        Command::Eject(args) => load_cmd::handle_eject(args, &paths, debug, connect_docker).await,
        Command::Exile => load_cmd::handle_exile(&paths, debug, connect_docker).await,
        Command::Logs(args) => runtime::logs::run(&paths, args),
        Command::Config(config_cmd) => config_cmd::handle(config_cmd, &mut config, &paths, debug),
        Command::Workspace(command) => {
            workspace_cmd::handle(command, &mut config, &paths, debug).await
        }
        Command::Purge(args) => {
            prune_cmd::handle_purge(args, &paths, &mut runner, connect_docker).await
        }
        Command::Prune(cmd) => {
            prune_cmd::handle_prune(cmd, &paths, &mut runner, connect_docker).await
        }
        Command::Help { .. } => {
            // Handled upstream in dispatch before reaching this function.
            unreachable!("Command::Help is dispatched to Action::PrintHelp before run() is called")
        }
        Command::Role(_) => unreachable!("Command::Role returns before config-backed dispatch"),
    }
}

const fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Load(_) => "load",
        Command::Hardline(_) => "hardline",
        Command::Eject(_) => "eject",
        Command::Exile => "exile",
        Command::Purge(_) => "purge",
        Command::Prune(_) => "prune",
        Command::Console(_) => "console",
        Command::Role(_) => "role",
        Command::Workspace(_) => "workspace",
        Command::Config(_) => "config",
        Command::Logs(_) => "logs",
        Command::Help { .. } => "help",
    }
}

/// In `--debug`, surface the diagnostics run id on the plain CLI before
/// anything else runs — never through a rich TUI. This is identical for
/// every command (CLI or TUI): print the run id the operator must keep to
/// retrieve the run's diagnostics file later, then, on an interactive
/// terminal, gate on Enter so the id is read before the normal flow (rich
/// or CLI, per terminal capability) takes over. Debug evidence itself is
/// written only to the run file, never echoed here.
fn announce_debug_run(diagnostics: &crate::diagnostics::RunDiagnostics) {
    use owo_colors::OwoColorize as _;
    use std::io::{IsTerminal, Write};
    let mut err = std::io::stderr();
    let _ = writeln!(err);
    let _ = writeln!(
        err,
        "{} debug mode — save this run id to retrieve the run later:",
        "[jackin]".bold()
    );
    let _ = writeln!(err, "    {}", diagnostics.run_id());
    if std::io::stdin().is_terminal() {
        let _ = write!(err, "[jackin] press Enter to continue... ");
        let _ = err.flush();
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
    }
}

fn workspace_env_scope(workspace: String, role: Option<String>) -> config::EnvScope {
    match role {
        Some(a) => config::EnvScope::WorkspaceRole { workspace, role: a },
        None => config::EnvScope::Workspace(workspace),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HardlineAction {
    Reconnect,
    NewSession,
    Inspect,
    Cancel,
}

fn prompt_hardline_action(container: &str) -> Result<HardlineAction> {
    prompt_hardline_action_with_prompt(&format!(
        "Instance `{container}` is available. Choose hardline action:"
    ))
}

async fn prompt_explicit_hardline_action_if_multiple_sessions(
    container: &str,
    docker: &impl DockerApi,
) -> Result<HardlineAction> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        return Ok(HardlineAction::Reconnect);
    }
    let state = docker.inspect_container_state(container).await;
    let sessions = runtime::inspect_agent_sessions(docker, container, &state).await;
    if !has_multiple_agent_sessions(&sessions) {
        return Ok(HardlineAction::Reconnect);
    }
    prompt_hardline_action_with_prompt(&format!(
        "Instance `{}` has multiple detected agent sessions ({}). Docker can reconnect the original container TTY or start another foreground session. Choose hardline action:",
        container,
        runtime::describe_agent_session_count(&sessions)
    ))
}

const fn has_multiple_agent_sessions(sessions: &runtime::AgentSessionInventory) -> bool {
    matches!(sessions, runtime::AgentSessionInventory::Sessions(items) if items.len() > 1)
}

fn prompt_hardline_action_with_prompt(prompt: &str) -> Result<HardlineAction> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        return Ok(HardlineAction::Reconnect);
    }

    let options = hardline_action_options();
    let labels: Vec<&str> = options.iter().map(|(label, _)| *label).collect();
    let choice = tui::prompt_choice(prompt, &labels)?;
    Ok(options[choice].1)
}

/// Pick the agent for a new foreground session inside an existing
/// instance, mirroring the `load` / `hardline --new` resolution order:
/// workspace `default_agent` short-circuits the prompt; otherwise
/// `prompt_agent_choice_if_needed` offers the manifest's supported
/// agents; on non-TTY or single-agent roles, fall back to the
/// workspace default or the manifest's recorded agent.
fn resolve_new_session_agent(
    paths: &JackinPaths,
    config: &AppConfig,
    manifest: &instance::InstanceManifest,
) -> Result<crate::agent::Agent> {
    let class = RoleSelector::parse(&manifest.role_key)?;
    let workspace_default_agent = manifest
        .workspace_name
        .as_deref()
        .and_then(|name| config.workspaces.get(name))
        .and_then(|ws| ws.default_agent);
    // Prompt declined to ask → workspace default covers it, role is
    // single-agent, or non-TTY context. Prefer the workspace default;
    // fall back to the manifest's recorded agent.
    prompt_agent_choice_if_needed(paths, &class, workspace_default_agent)?.map_or_else(
        || workspace_default_agent.map_or_else(|| manifest.agent(), Ok),
        Ok,
    )
}

/// Bridge from the TUI event loop to async docker work for Stop/Purge.
/// Now that `run_in_place` is async, the work runs directly on the
/// existing Tokio runtime — no nested runtime or OS thread needed.
struct ConsoleInPlaceHandler {
    paths: JackinPaths,
    debug: bool,
}

impl console::InstanceActionHandler for ConsoleInPlaceHandler {
    async fn run_in_place(
        &mut self,
        container: &str,
        action: console::ConsoleInstanceAction,
    ) -> anyhow::Result<()> {
        let docker = BollardDockerClient::connect()?;
        let mut runner = ShellRunner { debug: self.debug };
        // Wrap the eject + post-condition work in an async block so a
        // partial failure still hits the trailing reconcile +
        // manifest-status update. Without this, an eject that errored
        // after removing the last keep-awake container would leave
        // caffeinate asserted on the host and the on-disk manifest
        // stuck at Active/Running while the container is half-gone.
        let result: anyhow::Result<()> = async {
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
fn mark_instance_restore_available(paths: &JackinPaths, container: &str) {
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

async fn mark_instance_restore_available_after_stop(
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

async fn handle_console_instance_action(
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

const fn hardline_action_options() -> [(&'static str, HardlineAction); 4] {
    [
        (
            "Reconnect or recover this instance",
            HardlineAction::Reconnect,
        ),
        (
            "Start another foreground agent session",
            HardlineAction::NewSession,
        ),
        ("Inspect state without attaching", HardlineAction::Inspect),
        ("Cancel", HardlineAction::Cancel),
    ]
}

async fn restore_candidate_for_hardline(
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

async fn restore_hardline_instance(
    paths: &JackinPaths,
    config: &mut AppConfig,
    manifest: &instance::InstanceManifest,
    docker: &impl DockerApi,
    runner: &mut impl crate::docker::CommandRunner,
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

fn ad_hoc_restore_input_for_current_dir(
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

fn ad_hoc_restore_input_for_moved_path(
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
        "Browse for moved project path".to_string(),
        "Enter another moved project path".to_string(),
        "Cancel restore".to_string(),
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
enum MovedPathBrowserChoice {
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
            Self::Manual => "Enter a path manually".to_string(),
            Self::Cancel => "Cancel restore".to_string(),
        }
    }
}

fn moved_path_browser_choices(cwd: &std::path::Path) -> Vec<MovedPathBrowserChoice> {
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
enum MovedPathEntryStep {
    /// Empty input → operator cancelled.
    Cancel,
    /// Canonical absolute path; entry loop returns this.
    Accepted(std::path::PathBuf),
    /// Operator must retry; carries the message to print before the
    /// next prompt iteration.
    Retry(String),
}

fn classify_moved_path_entry(raw: &str) -> MovedPathEntryStep {
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

/// Render the `config auth show` output as a string. Empty workspace + role
/// names fall through to layer 1 (global), so this prints the global default
/// for each agent. Printing every built-in agent avoids privileging any one
/// runtime in the no-context output until/unless an `--agent` flag is added.
fn render_auth_show(config: &AppConfig) -> String {
    use std::fmt::Write as _;
    let claude_mode = crate::config::resolve_mode(config, crate::agent::Agent::Claude, "", "");
    let codex_mode = crate::config::resolve_mode(config, crate::agent::Agent::Codex, "", "");
    let amp_mode = crate::config::resolve_mode(config, crate::agent::Agent::Amp, "", "");
    let kimi_mode = crate::config::resolve_mode(config, crate::agent::Agent::Kimi, "", "");
    let opencode_mode = crate::config::resolve_mode(config, crate::agent::Agent::Opencode, "", "");
    let mut out = String::new();
    let _ = writeln!(out, "claude: {claude_mode}");
    let _ = writeln!(out, "codex:  {codex_mode}");
    let _ = writeln!(out, "amp:    {amp_mode}");
    let _ = writeln!(out, "kimi:   {kimi_mode}");
    let _ = writeln!(out, "opencode: {opencode_mode}");
    out
}

#[cfg(test)]
mod auth_set_tests;
#[cfg(test)]
mod resolve_role_tests;
