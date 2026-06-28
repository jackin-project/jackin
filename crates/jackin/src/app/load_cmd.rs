//! Load, Console, Hardline, Eject, and Exile command handlers — extracted from `app::run`.

use anyhow::{Context, Result};

use crate::cli::cleanup::EjectArgs;
use crate::cli::role::{HardlineArgs, LoadArgs};
use crate::config::AppConfig;
use crate::console;
use crate::docker::ShellRunner;
use crate::docker_client::BollardDockerClient;
use crate::instance;
use crate::paths::JackinPaths;
use crate::runtime;
use crate::selector::{RoleSelector, Selector};
use crate::workspace::{LoadWorkspaceInput, parse_mount_spec_resolved, resolve_load_workspace};

use super::{
    ConsoleInPlaceHandler, HardlineAction,
    context::{
        TargetKind, classify_target, remember_last_agent, resolve_agent_from_context_with_choice,
        resolve_running_container_from_context, resolve_target_name_with_choice,
    },
    handle_console_instance_action, play_construct_intro_if_needed,
    prompt_explicit_hardline_action_if_multiple_sessions, prompt_hardline_action,
    resolve_instance_reference, resolve_new_session_agent, resolve_role_to_container,
    restore_candidate_for_hardline, restore_hardline_instance, rich_prelaunch_choice,
};

pub(super) async fn handle_load(
    args: LoadArgs,
    config: &mut AppConfig,
    paths: &JackinPaths,
    debug: bool,
    runner: &mut ShellRunner,
    connect_docker: impl FnOnce() -> Result<BollardDockerClient>,
) -> Result<()> {
    let LoadArgs {
        selector,
        target,
        mounts,
        rebuild,
        force,
        agent,
        role_branch,
        docker_profile,
        dry_run,
        format,
    } = args;
    crate::preflight::preflight(crate::preflight::CheckName::preflight_required(), paths).await?;
    let docker = connect_docker()?;
    let cwd = std::env::current_dir()?;

    let (class, workspace_input) = if let Some(sel) = selector {
        let class = RoleSelector::parse(&sel)?;
        let input = match target {
            None => LoadWorkspaceInput::CurrentDir,
            Some(t) => match classify_target(&t) {
                TargetKind::Path { src, dst } => LoadWorkspaceInput::Path { src, dst },
                TargetKind::Name(name) => {
                    resolve_target_name_with_choice(&name, config, &cwd, rich_prelaunch_choice)?
                }
            },
        };
        (class, input)
    } else {
        // No selector — resolve role from workspace context
        resolve_agent_from_context_with_choice(config, &cwd, rich_prelaunch_choice)?
    };

    let saved_workspace_name = if let LoadWorkspaceInput::Saved(ref name) = workspace_input {
        Some(name.clone())
    } else {
        None
    };

    let ad_hoc_mounts = mounts
        .iter()
        .map(|value| parse_mount_spec_resolved(value))
        .collect::<Result<Vec<_>>>()?;

    let resolved_workspace =
        resolve_load_workspace(config, &class, &cwd, workspace_input, &ad_hoc_mounts)?;

    if dry_run {
        return print_dry_run_plan(
            &class,
            &resolved_workspace,
            agent.as_ref(),
            role_branch.as_deref(),
            rebuild,
            &format,
        );
    }

    let mut opts = runtime::LoadOptions::for_load(debug, rebuild);
    opts.force = force;
    opts.agent = agent;
    opts.role_branch = role_branch;
    opts.docker_profile = docker_profile;
    // Pre-launch reconcile: if a previous role in a keep_awake
    // workspace already runs, ensure caffeinate is up before we
    // build/launch (so a long Docker build doesn't see the host
    // sleep). Post-launch reconcile below catches the new role.
    let entry_claim = play_construct_intro_if_needed(paths, &docker).await;
    runtime::reconcile_keep_awake(paths, &docker, runner).await;
    let agent_slug = opts.agent.map(crate::agent::Agent::slug);
    let result = jackin_diagnostics::launch_trace(
        Some(&resolved_workspace.label),
        agent_slug,
        None,
        runtime::load_role(
            paths,
            config,
            &class,
            &resolved_workspace,
            &docker,
            runner,
            &opts,
        ),
    )
    .await;
    remember_last_agent(
        paths,
        config,
        saved_workspace_name.as_deref(),
        &class,
        &result,
    );
    if result.is_err() {
        runtime::release_entry_if_idle(paths, &docker, &entry_claim).await;
    }
    runtime::reconcile_keep_awake(paths, &docker, runner).await;
    result
}

pub(super) async fn handle_console(
    config: AppConfig,
    paths: JackinPaths,
    debug: bool,
) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let mut runner = ShellRunner { debug };
    let mut in_place = ConsoleInPlaceHandler {
        paths: paths.clone(),
        debug,
    };

    // One alternate screen owns the entire console → loading → capsule
    // → exit flow so transitions never flash back to the cooked
    // terminal. Sub-surfaces detect this and skip their own
    // enter/leave; the guard tears the terminal down once, on drop.
    let screen = console::TerminalSession::enter(console::terminal::host_console_terminal())?;

    let connect_docker = || BollardDockerClient::connect();

    let (mut console_entry, startup_error) = match connect_docker() {
        Ok(docker) => {
            let claim = play_construct_intro_if_needed(&paths, &docker).await;
            // D22: while the operator browses the console (off the attach path),
            // refresh any stale baked workspace images in the background so a
            // later launch hits the valid-image fast path. Reuses valid images;
            // only rebuilds stale ones. Launch never waits on this.
            runtime::spawn_background_image_prewarm(
                &paths,
                runtime::background_prewarm_targets(&config),
                debug,
            );
            (Some((docker, claim)), None)
        }
        Err(error) => (None, Some(docker_startup_error(&error))),
    };
    let startup_error_exit = startup_error
        .as_ref()
        .map(|(_, message)| anyhow::anyhow!(message.clone()));

    let op_available = console::effects::op_cli_available();
    let Some(outcome) = console::run_console(
        config,
        &paths,
        &cwd,
        console::tui::run::ConsoleRunOptions {
            op_available,
            startup_error,
            parent_session: Some(&screen),
        },
        &mut in_place,
        &mut runner,
    )
    .await?
    else {
        if let Some((docker, claim)) = &console_entry {
            runtime::release_entry_if_idle(&paths, docker, claim).await;
        }
        if let Some(error) = startup_error_exit {
            return Err(error);
        }
        return Ok(());
    };

    // config was consumed by run_console (the manager may have written to
    // disk). Reload so the post-console path sees the latest state.
    let mut config = AppConfig::load_or_init(&paths)?;
    let docker = connect_docker()?;
    let (class, workspace, selected_agent) = match outcome {
        console::ConsoleOutcome::Launch(class, workspace, selected_agent) => {
            (class, workspace, selected_agent)
        }
        console::ConsoleOutcome::PrewarmNamed(name) => {
            if let Some((docker, claim)) = &console_entry {
                runtime::release_entry_if_idle(&paths, docker, claim).await;
            }
            drop(screen);
            let args = crate::cli::PrewarmArgs {
                agents: Vec::new(),
                image: true,
                roles: false,
                sidecar: false,
                sidecar_container: false,
                keep_sidecar_container: false,
                daemon: false,
                role: None,
                workspace: Some(name),
                all_workspaces: false,
                all_roles: false,
                role_git: None,
                role_branch: None,
            };
            return crate::cli::prewarm::run(&args, &paths, &config, debug).await;
        }
        outcome @ console::ConsoleOutcome::InstanceAction { .. } => {
            // The action owns the terminal with its own foreground
            // process; hand it back the cooked screen.
            if let Some((docker, claim)) = &console_entry {
                runtime::release_entry_if_idle(&paths, docker, claim).await;
            }
            drop(screen);
            return handle_console_instance_action(
                &paths,
                &mut config,
                outcome,
                &docker,
                &mut runner,
            )
            .await;
        }
        console::ConsoleOutcome::NewSessionWithProvider {
            container,
            agent,
            provider,
        } => {
            let manifest =
                instance::InstanceManifest::read(&paths.data_dir.join(&container))
                    .with_context(|| {
                        format!(
                            "cannot start a new agent session in `{container}` because its instance manifest is missing"
                        )
                    })?;
            runtime::reconcile_keep_awake(&paths, &docker, &mut runner).await;
            // The token is backfilled inside the container by the
            // daemon from `ZAI_API_KEY`, so pass overrides without it.
            let result = runtime::spawn_agent_session(
                &paths,
                &container,
                Some(&manifest),
                agent,
                Some(provider.label()),
                &provider.env_overrides(None),
                config.git.coauthor_trailer,
                config.git.dco,
                &docker,
                &mut runner,
            )
            .await;
            runtime::reconcile_keep_awake(&paths, &docker, &mut runner).await;
            if let Some((docker, claim)) = &console_entry {
                runtime::release_entry_if_idle(&paths, docker, claim).await;
            }
            return result;
        }
        console::ConsoleOutcome::LaunchWithProvider {
            selector,
            workspace,
            agent,
            provider,
        } => {
            let provider_label = provider.label();
            let agent_slug = agent.slug();
            let mut opts = runtime::LoadOptions::for_launch(debug);
            opts.agent = Some(agent);
            opts.provider = Some(provider);
            runtime::reconcile_keep_awake(&paths, &docker, &mut runner).await;
            let result = jackin_diagnostics::launch_trace(
                Some(&workspace.label),
                Some(agent_slug),
                Some(provider_label),
                runtime::load_role(
                    &paths,
                    &mut config,
                    &selector,
                    &workspace,
                    &docker,
                    &mut runner,
                    &opts,
                ),
            )
            .await;
            remember_last_agent(
                &paths,
                &mut config,
                Some(&workspace.label),
                &selector,
                &result,
            );
            runtime::reconcile_keep_awake(&paths, &docker, &mut runner).await;
            if let Some((docker, claim)) = &console_entry {
                runtime::release_entry_if_idle(&paths, docker, claim).await;
            }
            return result;
        }
    };

    let mut opts = runtime::LoadOptions::for_launch(debug);
    opts.agent = selected_agent;
    let entry_claim = if let Some((_entry_docker, claim)) = console_entry.take() {
        claim
    } else {
        play_construct_intro_if_needed(&paths, &docker).await
    };
    runtime::reconcile_keep_awake(&paths, &docker, &mut runner).await;
    let agent_slug = opts.agent.map(crate::agent::Agent::slug);
    let result = jackin_diagnostics::launch_trace(
        Some(&workspace.label),
        agent_slug,
        None,
        runtime::load_role(
            &paths,
            &mut config,
            &class,
            &workspace,
            &docker,
            &mut runner,
            &opts,
        ),
    )
    .await;
    remember_last_agent(&paths, &mut config, Some(&workspace.label), &class, &result);
    if result.is_err() {
        runtime::release_entry_if_idle(&paths, &docker, &entry_claim).await;
    }
    runtime::reconcile_keep_awake(&paths, &docker, &mut runner).await;
    // `screen` drops here, after any exit outro, restoring the
    // terminal exactly once.
    result
}

fn docker_startup_error(error: &anyhow::Error) -> (String, String) {
    let detail = error_chain_message(error);
    (
        "Docker daemon not reachable".to_owned(),
        format!(
            "jackin could not connect to the Docker daemon.\n\nError:\n{detail}\n\nStart Docker or switch to a reachable Docker context, then run jackin again."
        ),
    )
}

fn error_chain_message(error: &anyhow::Error) -> String {
    let message = error
        .chain()
        .map(ToString::to_string)
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\nCaused by: ");
    if message.is_empty() {
        "unknown Docker connection error".to_owned()
    } else {
        message
    }
}

pub(super) async fn handle_hardline(
    args: HardlineArgs,
    config: AppConfig,
    paths: JackinPaths,
    debug: bool,
    connect_docker: impl FnOnce() -> Result<BollardDockerClient>,
) -> Result<()> {
    let HardlineArgs {
        selector,
        inspect,
        new,
        agent,
        shell,
    } = args;
    let mut runner = ShellRunner { debug };
    crate::preflight::preflight(crate::preflight::CheckName::preflight_required(), &paths).await?;
    let docker = connect_docker()?;
    // `--inspect` / `--new` / `--shell` mutual exclusion is enforced by
    // clap `conflicts_with_all` on `HardlineArgs`; no runtime guard needed.
    let explicit_selector = selector.is_some();
    let container = if let Some(sel) = selector {
        if let Some(container) = resolve_instance_reference(&paths, &sel)? {
            container
        } else {
            match Selector::parse(&sel)? {
                Selector::Container(name) => name,
                Selector::Role(class) => resolve_role_to_container(&class, &docker).await?,
            }
        }
    } else {
        let cwd = std::env::current_dir()?;
        resolve_running_container_from_context(&paths, &config, &cwd, &docker).await?
    };
    if shell {
        return runtime::spawn_shell_session(&paths, &container, &docker, &mut runner).await;
    }
    let action = if inspect {
        HardlineAction::Inspect
    } else if new {
        HardlineAction::NewSession
    } else if explicit_selector {
        prompt_explicit_hardline_action_if_multiple_sessions(&container, &docker).await?
    } else {
        prompt_hardline_action(&container)?
    };
    if action == HardlineAction::Inspect {
        println!(
            "{}",
            runtime::inspect_hardline_instance(&paths, &container, &docker).await?
        );
        return Ok(());
    }
    if action == HardlineAction::Cancel {
        return Ok(());
    }
    if action == HardlineAction::NewSession {
        let manifest = instance::InstanceManifest::read(&paths.data_dir.join(&container))
            .with_context(|| {
                format!(
                    "cannot start a new agent session in `{container}` because its instance manifest is missing"
                )
            })?;
        let selected_agent = if let Some(agent) = agent {
            agent
        } else {
            resolve_new_session_agent(&paths, &config, &manifest)?
        };
        runtime::reconcile_keep_awake(&paths, &docker, &mut runner).await;
        let result = runtime::spawn_agent_session(
            &paths,
            &container,
            Some(&manifest),
            selected_agent,
            None,
            &[],
            config.git.coauthor_trailer,
            config.git.dco,
            &docker,
            &mut runner,
        )
        .await;
        runtime::reconcile_keep_awake(&paths, &docker, &mut runner).await;
        return result;
    }
    runtime::reconcile_keep_awake(&paths, &docker, &mut runner).await;
    let mut config = config;
    let result = if let Some(manifest) =
        restore_candidate_for_hardline(&paths, &container, &docker).await?
    {
        restore_hardline_instance(&paths, &mut config, &manifest, &docker, &mut runner).await
    } else {
        runtime::hardline_agent(&paths, &container, &docker, &mut runner).await
    };
    runtime::reconcile_keep_awake(&paths, &docker, &mut runner).await;
    result
}

pub(super) async fn handle_eject(
    args: EjectArgs,
    paths: &JackinPaths,
    debug: bool,
    connect_docker: impl FnOnce() -> Result<BollardDockerClient>,
) -> Result<()> {
    let EjectArgs {
        selector,
        all,
        purge,
    } = args;
    let mut runner = ShellRunner { debug };
    crate::preflight::preflight(crate::preflight::CheckName::preflight_required(), paths).await?;
    let docker = connect_docker()?;
    let containers = if let Some(container) = resolve_instance_reference(paths, &selector)? {
        if all {
            anyhow::bail!("--all applies only to role selectors, not instance IDs");
        }
        vec![container]
    } else {
        match Selector::parse(&selector)? {
            Selector::Container(container) => vec![container],
            Selector::Role(class) => {
                if all {
                    runtime::matching_family(
                        &class,
                        &runtime::list_managed_role_names(&docker).await?,
                    )
                } else {
                    vec![resolve_role_to_container(&class, &docker).await?]
                }
            }
        }
    };
    // Wrap the loop so a partial failure still hits the trailing
    // reconcile — otherwise a `--all` eject that errors on
    // container N+1 would leave caffeinate running even though
    // earlier containers were already removed.
    let result: Result<()> = async {
        if containers.is_empty() {
            println!("No matching roles found.");
        } else {
            for container in &containers {
                runtime::eject_role(paths, container, &docker)
                    .await
                    .with_context(|| format!("ejecting {container}"))?;
                if purge {
                    runtime::purge_container_state(paths, container, &docker, &mut runner)
                        .await
                        .with_context(|| format!("purging local state for {container}"))?;
                    println!("Ejected and purged {container}.");
                } else {
                    println!("Ejected {container}.");
                }
            }
        }
        Ok(())
    }
    .await;
    runtime::reconcile_keep_awake(paths, &docker, &mut runner).await;
    result
}

/// Print the resolved load plan for `--dry-run` and exit without launching.
fn print_dry_run_plan(
    class: &RoleSelector,
    workspace: &crate::workspace::ResolvedWorkspace,
    agent: Option<&crate::agent::Agent>,
    role_branch: Option<&str>,
    rebuild: bool,
    format: &str,
) -> Result<()> {
    let agent_slug = agent
        .map(|a| a.slug().to_owned())
        .or_else(|| workspace.default_agent.map(|a| a.slug().to_owned()))
        .unwrap_or_else(|| "claude".to_owned());

    let mount_lines: Vec<String> = workspace
        .mounts
        .iter()
        .map(|m| format!("  {}  <-  {}  ({})", m.dst, m.src, m.isolation))
        .collect();

    if format == "json" {
        let mounts: Vec<serde_json::Value> = workspace
            .mounts
            .iter()
            .map(|m| {
                serde_json::json!({
                    "host_src": m.src,
                    "container_dest": m.dst,
                    "isolation": m.isolation.to_string(),
                })
            })
            .collect();
        let plan = serde_json::json!({
            "schema_version": "v1",
            "data": {
                "workspace": workspace.label,
                "workdir": workspace.workdir,
                "role": class.to_string(),
                "role_branch": role_branch,
                "agent": agent_slug,
                "rebuild": rebuild,
                "mounts": mounts,
            }
        });
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        println!("Workspace:  {} ({})", workspace.label, workspace.workdir);
        let role_display = role_branch.map_or_else(
            || class.to_string(),
            |branch| format!("{class} (branch: {branch})"),
        );
        println!("Role:       {role_display}");
        println!("Agent:      {agent_slug}");
        if rebuild {
            println!("Rebuild:    yes");
        }
        if mount_lines.is_empty() {
            println!("Mounts:     none");
        } else {
            println!("Mounts ({}):", mount_lines.len());
            for line in &mount_lines {
                println!("{line}");
            }
        }
        println!();
        println!("No changes made. Use `jackin load` to execute.");
    }
    Ok(())
}

pub(super) async fn handle_exile(
    paths: &JackinPaths,
    debug: bool,
    connect_docker: impl FnOnce() -> Result<BollardDockerClient>,
) -> Result<()> {
    let mut runner = ShellRunner { debug };
    crate::preflight::preflight(crate::preflight::CheckName::preflight_required(), paths).await?;
    let docker = connect_docker()?;
    let names = runtime::list_managed_role_names(&docker).await?;
    let result: Result<()> = async {
        if names.is_empty() {
            println!("No roles running.");
        } else {
            for name in &names {
                runtime::eject_role(paths, name, &docker)
                    .await
                    .with_context(|| format!("ejecting {name}"))?;
                println!("Ejected {name}.");
            }
        }
        Ok(())
    }
    .await;
    runtime::reconcile_keep_awake(paths, &docker, &mut runner).await;
    result
}

#[cfg(test)]
mod tests;
