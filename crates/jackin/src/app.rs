// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! CLI dispatch: maps parsed `Cli` commands to runtime, console, workspace,
//! and instance calls.
//!
//! `pub async fn run` is the binary entry point after argument parsing. Not a
//! stable library boundary — callers are `main.rs` and tests only.
//!
//! Not responsible for: argument parsing (`cli/`), runtime mechanics
//! (`runtime/`), or TUI rendering (`console/tui/`). This module is glue.

mod config_cmd;
pub(crate) mod context;
#[cfg(unix)]
mod daemon_cmd;
mod helpers;
mod load_cmd;
mod prune_cmd;
mod restore;
mod token_cmd;
mod workspace_cmd;

#[cfg(test)]
pub(crate) use crate::workspace::LoadWorkspaceInput;
use restore::{
    ConsoleInPlaceHandler, handle_console_instance_action, restore_candidate_for_hardline,
    restore_hardline_instance,
};
#[cfg(test)]
pub(crate) use restore::{
    MovedPathBrowserChoice, MovedPathEntryStep, ad_hoc_restore_input_for_current_dir,
    ad_hoc_restore_input_for_moved_path, classify_moved_path_entry,
    mark_instance_restore_available_after_stop, moved_path_browser_choices,
};
use token_cmd::handle_claude_token;
#[cfg(test)]
pub(crate) use token_cmd::{delete_prior_op_item_with_runner, validate_setup_role_allowed};

use helpers::{
    mount_display, mount_mode, render_workspace_show, resolve_instance_reference,
    resolve_role_to_container,
};

use anyhow::Result;

use crate::cli::role::ConsoleArgs;
use crate::cli::{Cli, Command};
use jackin_config::{self, AppConfig};
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use jackin_docker::ShellRunner;
use jackin_docker::docker_client::{BollardDockerClient, DockerApi};
use jackin_runtime::instance;
use jackin_runtime::runtime;

use self::context::prompt_agent_choice_if_needed;

/// Parse an `auth_forward` mode value as it arrived from the CLI.
fn parse_auth_forward_mode_from_cli(raw: &str) -> Result<jackin_config::AuthForwardMode> {
    raw.parse().map_err(|e: String| anyhow::anyhow!("{e}"))
}

/// Parse an agent slug as it arrived from the CLI.
fn parse_agent_from_cli(raw: &str) -> Result<jackin_core::Agent> {
    raw.parse()
        .map_err(|_| anyhow::anyhow!("unknown agent {raw:?}; expected one of: claude, codex, amp"))
}

fn rich_prelaunch_choice(title: &str, items: Vec<String>) -> Result<usize> {
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
        crate::warp::warp_intro(jackin_tui::ownership::host_screen_owned());
    }
    claim
}

pub async fn run(cli: Cli) -> Result<()> {
    let debug = cli.debug;
    jackin_diagnostics::set_debug_mode(debug);
    jackin_diagnostics::install_host_panic_hook();

    // Fail fast and loud on an unsupported OTLP protocol: jackin exports over
    // gRPC only. An OTLP endpoint configured with a non-grpc protocol would
    // otherwise build an exporter that silently never delivers — surface it as a
    // structured fatal error at startup rather than running with broken
    // telemetry the operator believes is working.
    if let Some(requested) = jackin_diagnostics::unsupported_otlp_protocol() {
        return Err(crate::error::JackinError::UnsupportedOtlpProtocol { requested }.into());
    }

    // Resolve the subcommand. Bare `jackin` is classified in `cli/dispatch.rs`
    // (TTY-capable → console; otherwise silent help). When `run` is invoked
    // with an explicit `Command::Console` (tests / direct callers), bare
    // None still maps to console for backward compatibility.
    let command = match cli.command {
        Some(cmd) => cmd,
        None => Command::Console(cli.console_args),
    };
    let invocation_id = jackin_telemetry::identity::InvocationId::mint();
    let _invocation_result = jackin_telemetry::identity::set_current_invocation(invocation_id);
    if let Command::Role(command) = command {
        return crate::role_authoring::run(command);
    }

    let paths = JackinPaths::detect()?;
    let mut config = AppConfig::load_or_init(&paths)?;
    apply_telemetry_config(&config);
    let command_name = command_name(&command);
    // Installs the global tracing subscriber (Defect 47.1 foundation) with
    // the freshly minted run id, so OTLP export (when configured) stamps the
    // id on every span and log record.
    let diagnostics = jackin_diagnostics::RunDiagnostics::start(&paths, debug, command_name)?;
    let _diagnostics_guard = diagnostics.activate();
    let interactive = matches!(
        command,
        Command::Console(_) | Command::Load(_) | Command::Hardline(_)
    );
    let invocation_id_value = invocation_id.to_string();
    let root_attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CLI_COMMAND_NAME,
            value: jackin_telemetry::Value::Str(command_name),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CLI_INVOCATION_ID,
            value: jackin_telemetry::Value::Str(&invocation_id_value),
        },
    ];
    if interactive
        && let Ok(startup) =
            jackin_telemetry::root_operation(&jackin_telemetry::operation::APP_STARTUP, &root_attrs)
    {
        startup.complete(jackin_telemetry::schema::enums::OutcomeValue::Success, None);
    }
    let command_operation = (!interactive)
        .then(|| {
            jackin_telemetry::root_operation(&jackin_telemetry::operation::CLI_COMMAND, &root_attrs)
                .ok()
        })
        .flatten();
    let command_entered = command_operation
        .as_ref()
        .map(|operation| operation.span().enter());
    // Wire the jackin-diagnostics operator-notice sink to the
    // jackin-core::operator_notice port-trait dispatcher so domain
    // crates (L0) can call `jackin_core::emit_compact_line` without
    // depending on the L2 diagnostics layer.
    jackin_diagnostics::operator_notice::install_operator_notice_sink();
    jackin_launch_tui::install_standalone_dialog_sink();
    if debug {
        announce_debug_run(&diagnostics);
    }
    let mut runner = ShellRunner { debug };
    let connect_docker = || BollardDockerClient::connect();

    let result = match command {
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
        Command::Config(config_cmd) => config_cmd::handle(config_cmd, &mut config, &paths, debug),
        #[cfg(unix)]
        Command::Daemon(command) => daemon_cmd::handle(command, &paths).await,
        Command::Workspace(command) => {
            workspace_cmd::handle(command, &mut config, &paths, debug).await
        }
        Command::Purge(args) => {
            prune_cmd::handle_purge(args, &paths, &mut runner, connect_docker).await
        }
        Command::Prewarm(args) => crate::cli::prewarm::run(&args, &paths, &config, debug).await,
        Command::Prune(cmd) => {
            prune_cmd::handle_prune(cmd, &paths, &mut runner, connect_docker).await
        }
        Command::Doctor(args) => crate::cli::doctor::run(&args, &paths).await,
        Command::Diagnostics(command) => crate::cli::diagnostics::run(&command),
        Command::Status(args) => crate::cli::status::run(&args, &paths).await,
        Command::Usage(args) => crate::cli::usage::run(&args, &paths).await,
        Command::Help { .. } => {
            // Handled upstream in dispatch before reaching this function.
            unreachable!("Command::Help is dispatched to Action::PrintHelp before run() is called")
        }
        Command::Role(_) => unreachable!("Command::Role returns before config-backed dispatch"),
    };
    drop(command_entered);
    let success = result.is_ok();
    if let Some(operation) = command_operation {
        operation.complete(
            if success {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            },
            (!success).then_some("command_failed"),
        );
    }
    if interactive
        && let Ok(shutdown) = jackin_telemetry::root_operation(
            &jackin_telemetry::operation::APP_SHUTDOWN,
            &root_attrs,
        )
    {
        shutdown.complete(
            if success {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            },
            (!success).then_some("command_failed"),
        );
    }
    record_run_error(&result);
    // Emit per-stage duration summary before the run guard drops (Defect 47.5).
    // The guard's Drop then flushes OTLP, so the summary makes the export.
    diagnostics.emit_run_summary();
    announce_run_teardown(&diagnostics);
    result
}

fn apply_telemetry_config(config: &AppConfig) {
    let level = config.telemetry.level.map(|level| match level {
        jackin_config::TelemetryLevelConfig::Info => jackin_diagnostics::TelemetryLevel::Info,
        jackin_config::TelemetryLevelConfig::Debug => jackin_diagnostics::TelemetryLevel::Debug,
        jackin_config::TelemetryLevelConfig::Trace => jackin_diagnostics::TelemetryLevel::Trace,
    });
    jackin_diagnostics::set_config_telemetry(level, &config.telemetry.categories);
}

fn record_run_error(result: &Result<()>) {
    let Err(error) = result else {
        return;
    };
    if runtime::progress::LaunchCancelled::is_cancel(error) {
        return;
    }
    let Some(run) = jackin_diagnostics::active_run() else {
        return;
    };
    if let Some(jackin_err) = error.downcast_ref::<crate::error::JackinError>() {
        let code = jackin_err.user_message().code.as_str();
        run.error_typed(code, &jackin_err.to_string(), Some(code));
    } else {
        run.error_typed("error", &format!("{error:#}"), Some("error"));
    }
}

fn announce_run_teardown(diagnostics: &jackin_diagnostics::RunDiagnostics) {
    let backend = jackin_diagnostics::configured_endpoint_summary().map_or_else(
        || "OpenTelemetry export disabled".to_owned(),
        |endpoint| format!("OpenTelemetry endpoint {endpoint}"),
    );
    let line = format!("telemetry: invocation {} - {backend}", diagnostics.run_id());
    jackin_diagnostics::emit_operator_notice(&line);
}

const fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Load(_) => "load",
        Command::Hardline(_) => "hardline",
        Command::Eject(_) => "eject",
        Command::Exile => "exile",
        Command::Purge(_) => "purge",
        Command::Prewarm(_) => "prewarm",
        Command::Prune(_) => "prune",
        Command::Console(_) => "console",
        Command::Role(_) => "role",
        Command::Workspace(_) => "workspace",
        Command::Config(_) => "config",
        #[cfg(unix)]
        Command::Daemon(_) => "daemon",
        Command::Doctor(_) => "doctor",
        Command::Diagnostics(crate::cli::DiagnosticsCommand::Validate) => "diagnostics.validate",
        Command::Status(_) => "status",
        Command::Usage(_) => "usage",
        Command::Help { .. } => "help",
    }
}

/// In `--debug`, surface the invocation id on the plain CLI before
/// anything else runs — never through a rich TUI. This is identical for
/// every command (CLI or TUI): print the invocation id the operator must keep to
/// query the configured backend later, then, on an interactive
/// terminal, gate on Enter so the id is read before the normal flow (rich
/// or CLI, per terminal capability) takes over. Debug evidence itself is
/// exported only through governed OTLP, never echoed here.
fn announce_debug_run(diagnostics: &jackin_diagnostics::RunDiagnostics) {
    use owo_colors::OwoColorize as _;
    use std::io::{IsTerminal, Write};
    let mut err = std::io::stderr();
    let _unused = writeln!(err);
    let _unused = writeln!(err, "{} debug mode — invocation id:", "[jackin]".bold());
    let _unused = writeln!(err, "    {}", diagnostics.run_id());
    match jackin_diagnostics::configured_endpoint_summary() {
        Some(endpoint) => {
            let _unused = writeln!(err, "[jackin] OTLP endpoint: {endpoint}");
        }
        None => {
            let _unused = writeln!(err, "[jackin] OTLP export: disabled");
        }
    }
    if std::io::stdin().is_terminal() {
        let _unused = write!(err, "[jackin] press Enter to continue... ");
        drop(err.flush());
        let mut line = String::new();
        drop(std::io::stdin().read_line(&mut line));
    }
}

fn workspace_env_scope(workspace: String, role: Option<String>) -> jackin_config::EnvScope {
    match role {
        Some(a) => jackin_config::EnvScope::WorkspaceRole { workspace, role: a },
        None => jackin_config::EnvScope::Workspace(workspace),
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
    let choice = crate::prompt::prompt_choice(prompt, &labels)?;
    Ok(options[choice].1)
}

/// Pick the agent for a new foreground session inside an existing
/// instance, mirroring the `load` / `hardline --new` resolution order:
/// workspace `default_agent` short-circuits the prompt; otherwise
/// `prompt_agent_choice_if_needed` offers the manifest's supported
/// agents; on non-TTY or single-agent roles, fall back to the
/// workspace default or the manifest's recorded agent.
pub(super) fn resolve_new_session_agent(
    paths: &JackinPaths,
    config: &AppConfig,
    manifest: &instance::InstanceManifest,
) -> Result<jackin_core::Agent> {
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
        || {
            workspace_default_agent
                .map_or_else(|| manifest.agent().map_err(anyhow::Error::from), Ok)
        },
        Ok,
    )
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

/// Render the `config auth show` output as a string. Empty workspace + role
/// names fall through to layer 1 (global), so this prints the global default
/// for each agent. Printing every built-in agent avoids privileging any one
/// runtime in the no-context output until/unless an `--agent` flag is added.
fn render_auth_show(config: &AppConfig) -> String {
    use std::fmt::Write as _;
    let claude_mode = jackin_config::resolve_mode(config, jackin_core::Agent::Claude, None, "");
    let codex_mode = jackin_config::resolve_mode(config, jackin_core::Agent::Codex, None, "");
    let amp_mode = jackin_config::resolve_mode(config, jackin_core::Agent::Amp, None, "");
    let kimi_mode = jackin_config::resolve_mode(config, jackin_core::Agent::Kimi, None, "");
    let opencode_mode = jackin_config::resolve_mode(config, jackin_core::Agent::Opencode, None, "");
    let mut out = String::new();
    let _unused = writeln!(out, "claude: {claude_mode}");
    let _unused = writeln!(out, "codex:  {codex_mode}");
    let _unused = writeln!(out, "amp:    {amp_mode}");
    let _unused = writeln!(out, "kimi:   {kimi_mode}");
    let _unused = writeln!(out, "opencode: {opencode_mode}");
    out
}

#[cfg(test)]
mod tests;
