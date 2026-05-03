pub mod context;

use anyhow::{Context, Result};
use std::io::ErrorKind;
use std::path::Path;

use crate::cli::agent::{ConsoleArgs, HardlineArgs, LoadArgs};
use crate::cli::cleanup::{EjectArgs, PurgeArgs};
use crate::cli::{self, Cli, Command, WorkspaceCommand};
use crate::config::{self, AppConfig};
use crate::console;
use crate::docker::ShellRunner;
use crate::instance;
use crate::paths::JackinPaths;
use crate::runtime;
use crate::selector::{ClassSelector, Selector};
use crate::tui;
use crate::workspace::{
    self, LoadWorkspaceInput, WorkspaceConfig, WorkspaceEdit, parse_mount_spec_resolved,
    resolve_path,
};

use self::context::{
    TargetKind, classify_target, remember_last_agent, resolve_agent_from_context,
    resolve_running_container_from_context, resolve_target_name,
};

/// Parse an `auth_forward` mode value as it arrived from the CLI.
///
/// Returns the resolved mode and a boolean indicating whether the operator
/// passed the deprecated `copy` alias — the caller is responsible for
/// emitting a user-facing warning in that case.
fn parse_auth_forward_mode_from_cli(raw: &str) -> anyhow::Result<(config::AuthForwardMode, bool)> {
    let mode: config::AuthForwardMode = raw.parse().map_err(|e: String| anyhow::anyhow!("{e}"))?;
    let was_deprecated = raw == "copy";
    Ok((mode, was_deprecated))
}

#[allow(clippy::too_many_lines)]
pub fn run(cli: Cli) -> Result<()> {
    let paths = JackinPaths::detect()?;
    let mut config = AppConfig::load_or_init(&paths)?;
    let mut runner = ShellRunner::default();

    // Resolve the subcommand. Bare `jackin` currently routes to the same
    // console handler as `jackin console`; the TTY-capability fallback and
    // the deprecation warning for `launch` land in a follow-up commit.
    let command = match cli.command {
        Some(cmd) => cmd,
        None => Command::Console(cli.console_args),
    };

    match command {
        Command::Load(LoadArgs {
            selector,
            target,
            mounts,
            rebuild,
            no_intro,
            debug,
            force,
            harness,
        }) => {
            // Harness resolution wires up in Task 16; accept-and-ignore for now.
            let _ = harness;
            runner.debug = debug;
            tui::set_debug_mode(debug);
            let cwd = std::env::current_dir()?;

            let (class, workspace_input) = if let Some(sel) = selector {
                let class = ClassSelector::parse(&sel)?;
                let input = match target {
                    None => LoadWorkspaceInput::CurrentDir,
                    Some(t) => match classify_target(&t) {
                        TargetKind::Path { src, dst } => LoadWorkspaceInput::Path { src, dst },
                        TargetKind::Name(name) => resolve_target_name(&name, &config, &cwd)?,
                    },
                };
                (class, input)
            } else {
                // No selector — resolve agent from workspace context
                resolve_agent_from_context(&config, &cwd)?
            };

            let saved_workspace_name = if let LoadWorkspaceInput::Saved(ref name) = workspace_input
            {
                Some(name.clone())
            } else {
                None
            };

            let ad_hoc_mounts = mounts
                .iter()
                .map(|value| parse_mount_spec_resolved(value))
                .collect::<Result<Vec<_>>>()?;

            let resolved_workspace = crate::workspace::resolve_load_workspace(
                &config,
                &class,
                &cwd,
                workspace_input,
                &ad_hoc_mounts,
            )?;

            let sensitive = crate::workspace::find_sensitive_mounts(&resolved_workspace.mounts);
            if !sensitive.is_empty() && !crate::workspace::confirm_sensitive_mounts(&sensitive)? {
                anyhow::bail!("aborted — sensitive mount paths were not confirmed");
            }

            let mut opts = runtime::LoadOptions::for_load(no_intro, debug, rebuild);
            opts.force = force;
            // Pre-launch reconcile: if a previous agent in a keep_awake
            // workspace already runs, ensure caffeinate is up before we
            // build/launch (so a long Docker build doesn't see the host
            // sleep). Post-launch reconcile below catches the new agent.
            runtime::reconcile_keep_awake(&paths, &mut runner);
            let result = runtime::load_agent(
                &paths,
                &mut config,
                &class,
                &resolved_workspace,
                &mut runner,
                &opts,
            );
            remember_last_agent(
                &paths,
                &mut config,
                saved_workspace_name.as_deref(),
                &class,
                &result,
            );
            runtime::reconcile_keep_awake(&paths, &mut runner);
            result
        }
        Command::Console(ConsoleArgs { debug }) | Command::Launch(ConsoleArgs { debug }) => {
            runner.debug = debug;
            tui::set_debug_mode(debug);
            let cwd = std::env::current_dir()?;
            let Some((class, workspace)) = console::run_console(config, &paths, &cwd)? else {
                return Ok(());
            };

            // config was consumed by run_console (the manager may have written to
            // disk). Reload so the post-console path sees the latest state.
            let mut config = AppConfig::load_or_init(&paths)?;

            let sensitive = crate::workspace::find_sensitive_mounts(&workspace.mounts);
            if !sensitive.is_empty() && !crate::workspace::confirm_sensitive_mounts(&sensitive)? {
                anyhow::bail!("aborted — sensitive mount paths were not confirmed");
            }

            let opts = runtime::LoadOptions::for_launch(debug);
            runtime::reconcile_keep_awake(&paths, &mut runner);
            let result =
                runtime::load_agent(&paths, &mut config, &class, &workspace, &mut runner, &opts);
            remember_last_agent(&paths, &mut config, Some(&workspace.label), &class, &result);
            runtime::reconcile_keep_awake(&paths, &mut runner);
            result
        }
        Command::Hardline(HardlineArgs { selector }) => {
            let container = if let Some(sel) = selector {
                match Selector::parse(&sel)? {
                    Selector::Container(name) => name,
                    Selector::Class(class) => instance::primary_container_name(&class),
                }
            } else {
                let cwd = std::env::current_dir()?;
                resolve_running_container_from_context(&config, &cwd, &mut runner)?
            };
            runtime::reconcile_keep_awake(&paths, &mut runner);
            let result = runtime::hardline_agent(&paths, &container, &mut runner);
            runtime::reconcile_keep_awake(&paths, &mut runner);
            result
        }
        Command::Eject(EjectArgs {
            selector,
            all,
            purge,
        }) => {
            let containers = match Selector::parse(&selector)? {
                Selector::Container(container) => vec![container],
                Selector::Class(class) => {
                    if all {
                        runtime::matching_family(
                            &class,
                            &runtime::list_managed_agent_names(&mut runner)?,
                        )
                    } else {
                        vec![instance::primary_container_name(&class)]
                    }
                }
            };
            // Wrap the loop so a partial failure still hits the trailing
            // reconcile — otherwise a `--all` eject that errors on
            // container N+1 would leave caffeinate running even though
            // earlier containers were already removed.
            let result: anyhow::Result<()> = (|| {
                if containers.is_empty() {
                    println!("No matching agents found.");
                } else {
                    for container in &containers {
                        runtime::eject_agent(container, &mut runner)
                            .with_context(|| format!("ejecting {container}"))?;
                        if purge {
                            crate::isolation::cleanup::purge_isolated_for_container(
                                &paths.data_dir.join(container),
                                &mut runner,
                            )
                            .with_context(|| format!("purging isolated state for {container}"))?;
                            remove_data_dir_if_exists(&paths.data_dir.join(container))
                                .with_context(|| format!("removing data dir for {container}"))?;
                            println!("Ejected and purged {container}.");
                        } else {
                            println!("Ejected {container}.");
                        }
                    }
                }
                Ok(())
            })();
            runtime::reconcile_keep_awake(&paths, &mut runner);
            result
        }
        Command::Exile => {
            let names = runtime::list_managed_agent_names(&mut runner)?;
            let result: anyhow::Result<()> = (|| {
                if names.is_empty() {
                    println!("No agents running.");
                } else {
                    for name in &names {
                        runtime::eject_agent(name, &mut runner)
                            .with_context(|| format!("ejecting {name}"))?;
                        println!("Ejected {name}.");
                    }
                }
                Ok(())
            })();
            runtime::reconcile_keep_awake(&paths, &mut runner);
            result
        }
        Command::Config(config_cmd) => match config_cmd {
            cli::ConfigCommand::Mount(mount_cmd) => match mount_cmd {
                cli::MountCommand::Add {
                    name,
                    src,
                    dst,
                    readonly,
                    scope,
                } => {
                    let ro = if readonly { " (read-only)" } else { "" };
                    let scope_label = scope.as_deref().unwrap_or("global");
                    let resolved_src = resolve_path(&src);
                    let mount = config::MountConfig {
                        src: resolved_src,
                        dst: dst.clone(),
                        readonly,
                        isolation: crate::isolation::MountIsolation::Shared,
                    };
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
                    editor.add_mount(&name, mount, scope.as_deref());
                    editor.save()?;
                    println!("Added mount {name:?} ({scope_label}): {src} -> {dst}{ro}");
                    Ok(())
                }
                cli::MountCommand::Remove { name, scope } => {
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
                    if editor.remove_mount(&name, scope.as_deref()) {
                        editor.save()?;
                        println!("Removed mount {name:?}.");
                    } else {
                        drop(editor);
                        println!("Mount {name:?} not found.");
                    }
                    Ok(())
                }
                cli::MountCommand::List => {
                    let mounts = config.list_mounts();
                    if mounts.is_empty() {
                        println!("No mounts configured.");
                    } else {
                        use tabled::settings::Style;
                        use tabled::{Table, Tabled};
                        #[derive(Tabled)]
                        struct Row {
                            #[tabled(rename = "Scope")]
                            scope: String,
                            #[tabled(rename = "Name")]
                            name: String,
                            #[tabled(rename = "Source")]
                            src: String,
                            #[tabled(rename = "Destination")]
                            dst: String,
                            #[tabled(rename = "Mode")]
                            mode: String,
                        }
                        let rows: Vec<Row> = mounts
                            .iter()
                            .map(|(scope, name, m)| Row {
                                scope: scope.clone(),
                                name: name.clone(),
                                src: tui::shorten_home(&m.src),
                                dst: m.dst.clone(),
                                mode: if m.readonly {
                                    "read-only".to_string()
                                } else {
                                    "read-write".to_string()
                                },
                            })
                            .collect();
                        let mut table = Table::new(rows);
                        table.with(Style::modern_rounded());
                        println!("{table}");
                    }
                    Ok(())
                }
            },
            cli::ConfigCommand::Trust(trust_cmd) => match trust_cmd {
                cli::TrustCommand::Grant { selector } => {
                    let class = ClassSelector::parse(&selector)?;
                    config.resolve_agent_source(&class)?;
                    let was_trusted = config.agents.get(&class.key()).is_some_and(|a| a.trusted);
                    if was_trusted {
                        println!("{} is already trusted.", class.key());
                    } else {
                        let mut editor = crate::config::ConfigEditor::open(&paths)?;
                        if let Some(source) = config.agents.get(&class.key()) {
                            editor.upsert_agent_source(&class.key(), source);
                        }
                        editor.set_agent_trust(&class.key(), true);
                        editor.save()?;
                        println!("Trusted {}.", class.key());
                    }
                    Ok(())
                }
                cli::TrustCommand::Revoke { selector } => {
                    let class = ClassSelector::parse(&selector)?;
                    if AppConfig::is_builtin_agent(&class.key()) {
                        anyhow::bail!("{} is a built-in agent and is always trusted.", class.key());
                    }
                    let was_trusted = config.agents.get(&class.key()).is_some_and(|a| a.trusted);
                    if was_trusted {
                        let mut editor = crate::config::ConfigEditor::open(&paths)?;
                        editor.set_agent_trust(&class.key(), false);
                        editor.save()?;
                        println!("Revoked trust for {}.", class.key());
                    } else {
                        println!("{} is not currently trusted.", class.key());
                    }
                    Ok(())
                }
                cli::TrustCommand::List => {
                    let agents: Vec<_> = config
                        .agents
                        .iter()
                        .filter(|(_, source)| source.trusted)
                        .map(|(key, _)| key.clone())
                        .collect();
                    if agents.is_empty() {
                        println!("No trusted agents.");
                    } else {
                        for key in agents {
                            println!("{key}");
                        }
                    }
                    Ok(())
                }
            },
            cli::ConfigCommand::Auth(auth_cmd) => match auth_cmd {
                cli::AuthCommand::Set { mode, agent } => {
                    let (parsed_mode, was_deprecated) = parse_auth_forward_mode_from_cli(&mode)?;
                    if was_deprecated {
                        tui::deprecation_warning(
                            "auth_forward \"copy\" is deprecated; saving as \"sync\"",
                        );
                    }
                    if let Some(agent_selector) = agent {
                        let class = ClassSelector::parse(&agent_selector)?;
                        config.resolve_agent_source(&class)?;
                        let mut editor = crate::config::ConfigEditor::open(&paths)?;
                        if let Some(source) = config.agents.get(&class.key()) {
                            editor.upsert_agent_source(&class.key(), source);
                        }
                        editor.set_agent_auth_forward(&class.key(), parsed_mode);
                        editor.save()?;
                        println!("Set auth forwarding for {} to {parsed_mode}.", class.key());
                    } else {
                        let mut editor = crate::config::ConfigEditor::open(&paths)?;
                        editor.set_global_auth_forward(parsed_mode);
                        editor.save()?;
                        println!("Set global auth forwarding to {parsed_mode}.");
                    }
                    Ok(())
                }
                cli::AuthCommand::Show { agent } => {
                    if let Some(agent_selector) = agent {
                        let class = ClassSelector::parse(&agent_selector)?;
                        let effective = config.resolve_auth_forward_mode(&class.key());
                        println!("{effective}");
                    } else {
                        println!("{}", config.claude.auth_forward);
                    }
                    Ok(())
                }
            },
            cli::ConfigCommand::Env(env_cmd) => match env_cmd {
                cli::EnvCommand::Set {
                    key,
                    value,
                    agent,
                    comment,
                } => {
                    if key.is_empty() {
                        anyhow::bail!("env var key cannot be empty");
                    }
                    if crate::env_model::is_reserved(&key) {
                        anyhow::bail!(
                            "env name {key:?} is reserved by the jackin runtime and cannot be set"
                        );
                    }
                    if let Some(ref agent_key) = agent
                        && !config.agents.contains_key(agent_key)
                    {
                        anyhow::bail!(
                            "agent {agent_key:?} is not registered; register it with \
                             `jackin agent register` (or run a command that resolves it) \
                             before setting agent-scoped env vars"
                        );
                    }
                    let env_value = resolve_env_value_for_cli(&value)?;
                    let scope = agent.map_or(config::EnvScope::Global, config::EnvScope::Agent);
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
                    editor.set_env_var(&scope, &key, env_value)?;
                    if let Some(ref c) = comment {
                        editor.set_env_comment(&scope, &key, Some(c));
                    }
                    editor.save()?;
                    println!("Set {key}.");
                    Ok(())
                }
                cli::EnvCommand::Unset { key, agent } => {
                    if key.is_empty() {
                        anyhow::bail!("env var key cannot be empty");
                    }
                    let scope = agent.map_or(config::EnvScope::Global, config::EnvScope::Agent);
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
                    if editor.remove_env_var(&scope, &key) {
                        editor.save()?;
                        println!("Removed {key}.");
                    } else {
                        drop(editor);
                        println!("{key} not set.");
                    }
                    Ok(())
                }
                cli::EnvCommand::List { agent } => {
                    let vars: Vec<(String, String)> = agent.as_ref().map_or_else(
                        || {
                            config
                                .env
                                .iter()
                                .map(|(k, v)| (k.clone(), v.as_display_str().to_string()))
                                .collect()
                        },
                        |a| {
                            config.agents.get(a).map_or_else(Vec::new, |src| {
                                src.env
                                    .iter()
                                    .map(|(k, v)| (k.clone(), v.as_display_str().to_string()))
                                    .collect()
                            })
                        },
                    );
                    print_env_table(&vars);
                    Ok(())
                }
            },
        },
        Command::Workspace(command) => match command {
            WorkspaceCommand::Create {
                name,
                workdir,
                mounts,
                no_workdir_mount,
                allowed_agents,
                default_agent,
                mount_isolation,
                keep_awake,
            } => {
                let expanded_workdir = workspace::resolve_path(&workdir);
                let parsed_mounts = mounts
                    .iter()
                    .map(|value| parse_mount_spec_resolved(value))
                    .collect::<Result<Vec<_>>>()?;
                let mut plan = workspace::planner::plan_create(
                    &expanded_workdir,
                    parsed_mounts,
                    no_workdir_mount,
                )?;
                workspace::planner::apply_isolation_overrides(
                    &mut plan.final_mounts,
                    &mount_isolation,
                )?;
                if !plan.collapsed.is_empty() {
                    let removed_list: Vec<String> = plan
                        .collapsed
                        .iter()
                        .map(|r| tui::shorten_home(&r.child.src))
                        .collect();
                    // Parent paths in a single create are all the same set; pick
                    // the first for the summary headline.
                    let parent = tui::shorten_home(&plan.collapsed[0].covered_by.src);
                    eprintln!(
                        "collapsed {} redundant mount(s) under {parent}: {}",
                        plan.collapsed.len(),
                        removed_list.join(", ")
                    );
                }
                let mount_count = plan.final_mounts.len();
                let ws = WorkspaceConfig {
                    workdir: expanded_workdir,
                    mounts: plan.final_mounts,
                    allowed_agents,
                    default_agent,
                    harness: None,
                    last_agent: None,
                    env: std::collections::BTreeMap::new(),
                    agents: std::collections::BTreeMap::new(),
                    keep_awake: crate::workspace::KeepAwakeConfig {
                        enabled: keep_awake,
                    },
                };
                let mut editor = crate::config::ConfigEditor::open(&paths)?;
                editor.create_workspace(&name, ws)?;
                editor.save()?;
                println!(
                    "Created workspace {name:?} (workdir: {}, {mount_count} mount(s)).",
                    tui::shorten_home(&workdir)
                );
                Ok(())
            }
            WorkspaceCommand::List => {
                let workspaces = config.list_workspaces();
                if workspaces.is_empty() {
                    println!("No workspaces configured.");
                    println!();
                    println!("Add one with:");
                    println!(
                        "  jackin workspace create <name> --workdir /path/to/project --mount /path/to/project"
                    );
                } else {
                    use tabled::settings::Style;
                    use tabled::{Table, Tabled};
                    #[derive(Tabled)]
                    struct Row {
                        #[tabled(rename = "Name")]
                        name: String,
                        #[tabled(rename = "Workdir")]
                        workdir: String,
                        #[tabled(rename = "Mounts")]
                        mounts: usize,
                        #[tabled(rename = "Allowed Agents")]
                        allowed: String,
                        #[tabled(rename = "Default Agent")]
                        default_agent: String,
                    }
                    let rows: Vec<Row> = workspaces
                        .iter()
                        .map(|(name, ws)| Row {
                            name: (*name).to_string(),
                            workdir: tui::shorten_home(&ws.workdir),
                            mounts: ws.mounts.len(),
                            allowed: if ws.allowed_agents.is_empty() {
                                "any agent".to_string()
                            } else {
                                ws.allowed_agents.join(", ")
                            },
                            default_agent: ws
                                .default_agent
                                .as_deref()
                                .unwrap_or("none")
                                .to_string(),
                        })
                        .collect();
                    let mut table = Table::new(rows);
                    table.with(Style::modern_rounded());
                    println!("{table}");
                    println!();
                    tui::hint("Run ", "jackin workspace show <name>", " for details.");
                }
                Ok(())
            }
            WorkspaceCommand::Show { name } => {
                let workspace = config.require_workspace(&name)?;
                print!("{}", render_workspace_show(&name, workspace));
                Ok(())
            }
            WorkspaceCommand::Edit {
                name,
                workdir,
                mounts,
                remove_destinations,
                no_workdir_mount,
                allowed_agents,
                remove_allowed_agents,
                default_agent,
                clear_default_agent,
                assume_yes,
                prune,
                mount_isolation,
                delete_isolated_state,
                keep_awake,
                no_keep_awake,
            } => {
                // Map paired flags to Option<bool>: None = no change.
                // Mutual exclusion is enforced at parse time by clap's
                // `conflicts_with`, so at most one of the two is true.
                let keep_awake_change = if keep_awake {
                    Some(true)
                } else if no_keep_awake {
                    Some(false)
                } else {
                    None
                };
                let upsert_mounts = mounts
                    .iter()
                    .map(|value| parse_mount_spec_resolved(value))
                    .collect::<Result<Vec<_>>>()?;

                let current_ws = config.require_workspace(&name)?.clone();

                let plan = workspace::planner::plan_edit(
                    &current_ws,
                    &upsert_mounts,
                    &remove_destinations,
                    no_workdir_mount,
                )?;

                // Reject pre-existing violations unless --prune.
                if !plan.pre_existing_collapses.is_empty() && !prune {
                    let details: Vec<String> = plan
                        .pre_existing_collapses
                        .iter()
                        .map(|r| {
                            format!(
                                "{} covered by {}",
                                tui::shorten_home(&r.child.src),
                                tui::shorten_home(&r.covered_by.src),
                            )
                        })
                        .collect();
                    anyhow::bail!(
                        "workspace {name:?} already contains redundant mounts:\n  - {}\n\
                         run `jackin workspace prune {name}` to clean up, or pass --prune to this edit",
                        details.join("\n  - ")
                    );
                }

                let all_collapses: Vec<&workspace::Removal> = plan
                    .edit_driven_collapses
                    .iter()
                    .chain(plan.pre_existing_collapses.iter())
                    .collect();

                // If there are any collapses to apply, prompt (or bail on
                // non-TTY without --yes).
                if !all_collapses.is_empty() && !assume_yes {
                    tui::require_interactive_stdin(
                        "refusing to collapse mounts without confirmation; pass --yes to proceed non-interactively",
                    )?;

                    if !plan.edit_driven_collapses.is_empty() {
                        eprintln!(
                            "Adding mount(s) will subsume {} existing mount(s):",
                            plan.edit_driven_collapses.len()
                        );
                        for r in &plan.edit_driven_collapses {
                            eprintln!("  • {}", tui::shorten_home(&r.child.src));
                        }
                    }
                    if !plan.pre_existing_collapses.is_empty() {
                        eprintln!(
                            "Cleaning up {} pre-existing redundant mount(s):",
                            plan.pre_existing_collapses.len()
                        );
                        for r in &plan.pre_existing_collapses {
                            eprintln!("  • {}", tui::shorten_home(&r.child.src));
                        }
                    }
                    eprintln!("These will be removed from the workspace.");

                    let confirmed = dialoguer::Confirm::new()
                        .with_prompt("Proceed?")
                        .default(false)
                        .interact()?;
                    if !confirmed {
                        anyhow::bail!("aborted by operator");
                    }
                }

                // Collect what changed for the summary (preserves the existing
                // summary output, plus collapse lines).
                let mut changes: Vec<String> = Vec::new();
                if let Some(ref w) = workdir {
                    changes.push(format!("workdir → {}", tui::shorten_home(w)));
                }
                for m in &upsert_mounts {
                    if all_collapses.iter().any(|r| r.child.dst == m.dst) {
                        continue;
                    }
                    if m.src == m.dst {
                        changes.push(format!("added mount {}", tui::shorten_home(&m.src)));
                    } else {
                        changes.push(format!(
                            "added mount {} → {}",
                            tui::shorten_home(&m.src),
                            tui::shorten_home(&m.dst)
                        ));
                    }
                }
                for dst in &remove_destinations {
                    changes.push(format!("removed mount {}", tui::shorten_home(dst)));
                }
                for r in &all_collapses {
                    changes.push(format!(
                        "collapsed {} under {}",
                        tui::shorten_home(&r.child.src),
                        tui::shorten_home(&r.covered_by.src)
                    ));
                }
                if no_workdir_mount {
                    changes.push("removed workdir auto-mount".to_string());
                }
                for agent in &allowed_agents {
                    changes.push(format!("allowed agent {agent}"));
                }
                for agent in &remove_allowed_agents {
                    changes.push(format!("removed agent {agent}"));
                }
                if clear_default_agent {
                    changes.push("cleared default agent".to_string());
                } else if let Some(ref agent) = default_agent {
                    changes.push(format!("default agent → {agent}"));
                }
                if let Some(v) = keep_awake_change {
                    changes.push(format!(
                        "keep_awake → {}",
                        if v { "enabled" } else { "disabled" }
                    ));
                }

                // Build the prospective mount list (mirrors edit_workspace's
                // merge order) so we can check for source drift on any mount
                // that has preserved isolated state on disk.
                let mut prospective_mounts: Vec<workspace::MountConfig> = current_ws
                    .mounts
                    .iter()
                    .filter(|m| !plan.effective_removals.iter().any(|d| d == &m.dst))
                    .cloned()
                    .collect();
                if no_workdir_mount {
                    let workdir = &current_ws.workdir;
                    prospective_mounts.retain(|m| !(m.src == *workdir && m.dst == *workdir));
                }
                for upsert in &upsert_mounts {
                    if let Some(existing) = prospective_mounts
                        .iter_mut()
                        .find(|existing| existing.dst == upsert.dst)
                    {
                        *existing = upsert.clone();
                    } else {
                        prospective_mounts.push(upsert.clone());
                    }
                }
                let detection = crate::config::detect_workspace_edit_drift(
                    &paths,
                    &name,
                    &prospective_mounts,
                    &mut runner,
                )?;
                if !detection.running_containers.is_empty() {
                    anyhow::bail!(
                        "cannot edit workspace `{name}` while these containers are running with isolated state: {}; eject them first",
                        detection.running_containers.join(", ")
                    );
                }
                if !detection.stopped_records.is_empty() {
                    if !delete_isolated_state {
                        let names: Vec<String> = detection
                            .stopped_records
                            .iter()
                            .map(|r| r.container_name.clone())
                            .collect();
                        anyhow::bail!(
                            "edit affects preserved isolated state for {} container(s): {}; pass --delete-isolated-state to remove and apply, or restore the previous src",
                            detection.stopped_records.len(),
                            names.join(", ")
                        );
                    }
                    for rec in &detection.stopped_records {
                        let container_dir = paths.data_dir.join(&rec.container_name);
                        crate::isolation::cleanup::force_cleanup_isolated(
                            rec,
                            &container_dir,
                            &mut runner,
                        )?;
                    }
                }

                let mut editor = crate::config::ConfigEditor::open(&paths)?;
                editor.edit_workspace(
                    &name,
                    WorkspaceEdit {
                        workdir: workdir.map(|w| resolve_path(&w)),
                        upsert_mounts,
                        remove_destinations: plan.effective_removals,
                        no_workdir_mount,
                        allowed_agents_to_add: allowed_agents,
                        allowed_agents_to_remove: remove_allowed_agents,
                        default_agent: if clear_default_agent {
                            Some(None)
                        } else {
                            default_agent.map(Some)
                        },
                        mount_isolation_overrides: mount_isolation,
                        keep_awake_enabled: keep_awake_change,
                    },
                )?;
                editor.save()?;
                println!("Updated workspace {name:?}:");
                for change in &changes {
                    println!("  - {change}");
                }
                Ok(())
            }
            WorkspaceCommand::Prune { name, assume_yes } => {
                let current_ws = config.require_workspace(&name)?.clone();

                // All existing mounts; nothing new.
                let plan = workspace::plan_collapse(&current_ws.mounts, &[])?;
                if plan.removed.is_empty() {
                    println!("Workspace {name:?} has no redundant mounts.");
                    return Ok(());
                }

                if !assume_yes {
                    tui::require_interactive_stdin(
                        "refusing to collapse mounts without confirmation; pass --yes to proceed non-interactively",
                    )?;
                    eprintln!(
                        "Will remove {} redundant mount(s) from workspace {name:?}:",
                        plan.removed.len()
                    );
                    for r in &plan.removed {
                        eprintln!(
                            "  • {} (covered by {})",
                            tui::shorten_home(&r.child.src),
                            tui::shorten_home(&r.covered_by.src),
                        );
                    }
                    let confirmed = dialoguer::Confirm::new()
                        .with_prompt("Proceed?")
                        .default(false)
                        .interact()?;
                    if !confirmed {
                        anyhow::bail!("aborted by operator");
                    }
                }

                let remove_dsts: Vec<String> =
                    plan.removed.iter().map(|r| r.child.dst.clone()).collect();
                let mut editor = crate::config::ConfigEditor::open(&paths)?;
                editor.edit_workspace(
                    &name,
                    WorkspaceEdit {
                        remove_destinations: remove_dsts,
                        ..WorkspaceEdit::default()
                    },
                )?;
                editor.save()?;
                println!(
                    "Pruned {} redundant mount(s) from workspace {name:?}.",
                    plan.removed.len()
                );
                Ok(())
            }
            WorkspaceCommand::Remove { name } => {
                let mut editor = crate::config::ConfigEditor::open(&paths)?;
                editor.remove_workspace(&name)?;
                editor.save()?;
                println!("Removed workspace {name:?}.");
                Ok(())
            }
            WorkspaceCommand::Env(env_cmd) => match env_cmd {
                cli::WorkspaceEnvCommand::Set {
                    workspace,
                    key,
                    value,
                    agent,
                    comment,
                } => {
                    if key.is_empty() {
                        anyhow::bail!("env var key cannot be empty");
                    }
                    if crate::env_model::is_reserved(&key) {
                        anyhow::bail!(
                            "env name {key:?} is reserved by the jackin runtime and cannot be set"
                        );
                    }
                    config.require_workspace(&workspace)?;
                    if let Some(ref agent_key) = agent
                        && !config.agents.contains_key(agent_key)
                    {
                        anyhow::bail!(
                            "agent {agent_key:?} is not registered; register it with \
                             `jackin agent register` (or run a command that resolves it) \
                             before setting agent-scoped env vars"
                        );
                    }
                    let env_value = resolve_env_value_for_cli(&value)?;
                    let scope = workspace_env_scope(workspace, agent);
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
                    editor.set_env_var(&scope, &key, env_value)?;
                    if let Some(ref c) = comment {
                        editor.set_env_comment(&scope, &key, Some(c));
                    }
                    editor.save()?;
                    println!("Set {key}.");
                    Ok(())
                }
                cli::WorkspaceEnvCommand::Unset {
                    workspace,
                    key,
                    agent,
                } => {
                    if key.is_empty() {
                        anyhow::bail!("env var key cannot be empty");
                    }
                    config.require_workspace(&workspace)?;
                    let scope = workspace_env_scope(workspace, agent);
                    let mut editor = crate::config::ConfigEditor::open(&paths)?;
                    if editor.remove_env_var(&scope, &key) {
                        editor.save()?;
                        println!("Removed {key}.");
                    } else {
                        drop(editor);
                        println!("{key} not set.");
                    }
                    Ok(())
                }
                cli::WorkspaceEnvCommand::List { workspace, agent } => {
                    let ws = config.require_workspace(&workspace)?;
                    let vars: Vec<(String, String)> = agent.as_ref().map_or_else(
                        || {
                            ws.env
                                .iter()
                                .map(|(k, v)| (k.clone(), v.as_display_str().to_string()))
                                .collect()
                        },
                        |a| {
                            ws.agents.get(a).map_or_else(Vec::new, |ov| {
                                ov.env
                                    .iter()
                                    .map(|(k, v)| (k.clone(), v.as_display_str().to_string()))
                                    .collect()
                            })
                        },
                    );
                    print_env_table(&vars);
                    Ok(())
                }
            },
        },
        Command::Purge(PurgeArgs { selector, all }) => match Selector::parse(&selector)? {
            Selector::Container(container) => {
                let short_name = container.trim_start_matches("jackin-");
                runtime::ensure_agent_not_running(&mut runner, short_name)?;
                crate::isolation::cleanup::purge_isolated_for_container(
                    &paths.data_dir.join(&container),
                    &mut runner,
                )?;
                remove_data_dir_if_exists(&paths.data_dir.join(&container))?;
                println!("Purged state for {container}.");
                Ok(())
            }
            Selector::Class(class) => {
                if all {
                    runtime::purge_class_data(&paths, &class)?;
                    println!("Purged all state for {}.", class.key());
                } else {
                    let container = instance::primary_container_name(&class);
                    let short_name = container.trim_start_matches("jackin-");
                    runtime::ensure_agent_not_running(&mut runner, short_name)?;
                    crate::isolation::cleanup::purge_isolated_for_container(
                        &paths.data_dir.join(&container),
                        &mut runner,
                    )?;
                    remove_data_dir_if_exists(&paths.data_dir.join(&container))?;
                    println!("Purged state for {container}.");
                }
                Ok(())
            }
        },
        Command::Help { .. } => {
            // Handled upstream in dispatch before reaching this function.
            unreachable!("Command::Help is dispatched to Action::PrintHelp before run() is called")
        }
    }
}

/// Resolve a CLI-supplied env value string into the appropriate [`EnvValue`]
/// variant. Values starting with `op://` trigger headless resolution via the
/// 1Password CLI; all other values are stored as [`EnvValue::Plain`] unchanged.
///
/// Errors when:
/// - The value starts with `op://` but the `op` CLI is unavailable.
/// - The value starts with `op://` and contains `${VAR}` substitution syntax.
/// - Resolution fails (vault/item/field not found, ambiguity, etc.).
fn resolve_env_value_for_cli(value: &str) -> anyhow::Result<crate::operator_env::EnvValue> {
    if !value.starts_with("op://") {
        return Ok(crate::operator_env::EnvValue::Plain(value.to_string()));
    }

    // Probe op CLI availability before attempting structural queries.
    let op_cli = crate::operator_env::OpCli::new();
    crate::operator_env::OpRunner::probe(&op_cli).map_err(|e| {
        anyhow::anyhow!(
            "`op` CLI not available; cannot resolve `op://...` reference. \
             Install 1Password CLI, or use a non-op:// value.\n\
             Probe error: {e}"
        )
    })?;

    let op_ref = crate::operator_env::resolve_op_uri_to_ref(value, &op_cli)?;
    Ok(crate::operator_env::EnvValue::OpRef(op_ref))
}

fn workspace_env_scope(workspace: String, agent: Option<String>) -> config::EnvScope {
    match agent {
        Some(a) => config::EnvScope::WorkspaceAgent {
            workspace,
            agent: a,
        },
        None => config::EnvScope::Workspace(workspace),
    }
}

#[derive(tabled::Tabled)]
struct EnvRow {
    #[tabled(rename = "Key")]
    key: String,
    #[tabled(rename = "Value")]
    value: String,
}

fn print_env_table(vars: &[(String, String)]) {
    use tabled::Table;
    use tabled::settings::Style;
    if vars.is_empty() {
        println!("No env vars set.");
        return;
    }
    let rows: Vec<EnvRow> = vars
        .iter()
        .map(|(k, v)| EnvRow {
            key: k.clone(),
            value: v.clone(),
        })
        .collect();
    let mut table = Table::new(rows);
    table.with(Style::modern_rounded());
    println!("{table}");
}

fn remove_data_dir_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Render the `workspace show <name>` output as a string. Includes the info
/// table (name/workdir/allowed/default-agent), and, when there are mounts, a
/// trailing mounts table with one row per mount. The mounts table renders the
/// canonical lowercase isolation name (`shared`/`worktree`) so the output
/// matches TOML/CLI input verbatim.
fn render_workspace_show(name: &str, workspace: &WorkspaceConfig) -> String {
    use std::fmt::Write as _;
    use tabled::settings::Style;
    use tabled::{Table, Tabled};

    #[derive(Tabled)]
    struct MountRow {
        #[tabled(rename = "Source")]
        src: String,
        #[tabled(rename = "Destination")]
        dst: String,
        #[tabled(rename = "Mode")]
        mode: String,
        #[tabled(rename = "Isolation")]
        isolation: String,
    }

    let allowed = if workspace.allowed_agents.is_empty() {
        "any agent".to_string()
    } else {
        workspace.allowed_agents.join(", ")
    };
    let default_agent = workspace.default_agent.as_deref().unwrap_or("none");

    let short_workdir = tui::shorten_home(&workspace.workdir);
    let mut info: Vec<(&str, &str)> = vec![
        ("Name", name),
        ("Workdir", short_workdir.as_str()),
        ("Allowed Agents", allowed.as_str()),
        ("Default Agent", default_agent),
    ];
    // Only surface keep_awake when opted in — disabled is the default and
    // shouldn't add noise. When enabled, the operator sees it here so a
    // mysteriously sleepless Mac traces back to the workspace.
    if workspace.keep_awake.enabled {
        info.push(("Keep Awake", "enabled (macOS only)"));
    }
    let mut info_table = Table::builder(info.iter().map(|(k, v)| [*k, *v])).build();
    info_table
        .with(Style::modern_rounded())
        .with(tabled::settings::Remove::row(
            tabled::settings::object::Rows::first(),
        ));

    let mut out = String::new();
    let _ = writeln!(out, "{info_table}");

    if !workspace.mounts.is_empty() {
        let mount_rows: Vec<MountRow> = workspace
            .mounts
            .iter()
            .map(|m| MountRow {
                src: tui::shorten_home(&m.src),
                dst: tui::shorten_home(&m.dst),
                mode: if m.readonly {
                    "read-only".to_string()
                } else {
                    "read-write".to_string()
                },
                isolation: m.isolation.as_str().to_string(),
            })
            .collect();
        let mut mount_table = Table::new(mount_rows);
        mount_table.with(Style::modern_rounded());
        let _ = writeln!(out);
        let _ = writeln!(out, "Mounts:");
        let _ = writeln!(out, "{mount_table}");
    }

    out
}

#[cfg(test)]
mod auth_set_tests {
    use super::*;

    #[test]
    fn parse_auth_forward_mode_from_cli_accepts_copy_as_deprecated() {
        let (mode, was_deprecated) = parse_auth_forward_mode_from_cli("copy").unwrap();
        assert_eq!(mode, crate::config::AuthForwardMode::Sync);
        assert!(was_deprecated);
    }

    #[test]
    fn parse_auth_forward_mode_from_cli_accepts_sync_non_deprecated() {
        let (mode, was_deprecated) = parse_auth_forward_mode_from_cli("sync").unwrap();
        assert_eq!(mode, crate::config::AuthForwardMode::Sync);
        assert!(!was_deprecated);
    }

    #[test]
    fn parse_auth_forward_mode_from_cli_rejects_bogus() {
        assert!(parse_auth_forward_mode_from_cli("bogus").is_err());
    }

    #[test]
    fn workspace_show_includes_isolation_column() {
        let ws = crate::workspace::WorkspaceConfig {
            workdir: "/workspace/jackin".into(),
            mounts: vec![
                crate::workspace::MountConfig {
                    src: "/tmp/x".into(),
                    dst: "/workspace/jackin".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Worktree,
                },
                crate::workspace::MountConfig {
                    src: "/tmp/cache".into(),
                    dst: "/workspace/cache".into(),
                    readonly: false,
                    isolation: crate::isolation::MountIsolation::Shared,
                },
            ],
            allowed_agents: vec![],
            default_agent: None,
            harness: None,
            last_agent: None,
            env: std::collections::BTreeMap::new(),
            agents: std::collections::BTreeMap::new(),
            keep_awake: crate::workspace::KeepAwakeConfig::default(),
        };
        let out = render_workspace_show("jackin", &ws);
        assert!(out.contains("Isolation"));
        assert!(out.contains("worktree"));
        assert!(out.contains("shared"));
    }
}
