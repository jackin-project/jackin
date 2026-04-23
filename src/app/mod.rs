pub mod context;

use anyhow::Result;
use std::io::ErrorKind;
use std::path::Path;

use crate::cli::agent::{HardlineArgs, LaunchArgs, LoadArgs};
use crate::cli::cleanup::{EjectArgs, PurgeArgs};
use crate::cli::{self, Cli, Command, WorkspaceCommand};
use crate::config::{self, AppConfig};
use crate::docker::ShellRunner;
use crate::instance;
use crate::launch;
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

#[allow(clippy::too_many_lines)]
pub fn run(cli: Cli) -> Result<()> {
    let paths = JackinPaths::detect()?;
    let mut config = AppConfig::load_or_init(&paths)?;
    let mut runner = ShellRunner::default();

    match cli.command {
        Command::Load(LoadArgs {
            selector,
            target,
            mounts,
            rebuild,
            no_intro,
            debug,
        }) => {
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

            let opts = runtime::LoadOptions::for_load(no_intro, debug, rebuild);
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
            result
        }
        Command::Launch(LaunchArgs { debug }) => {
            runner.debug = debug;
            tui::set_debug_mode(debug);
            let cwd = std::env::current_dir()?;
            let (class, workspace) = launch::run_launch(&config, &cwd)?;

            let sensitive = crate::workspace::find_sensitive_mounts(&workspace.mounts);
            if !sensitive.is_empty() && !crate::workspace::confirm_sensitive_mounts(&sensitive)? {
                anyhow::bail!("aborted — sensitive mount paths were not confirmed");
            }

            let opts = runtime::LoadOptions::for_launch(debug);
            let result =
                runtime::load_agent(&paths, &mut config, &class, &workspace, &mut runner, &opts);
            remember_last_agent(&paths, &mut config, Some(&workspace.label), &class, &result);
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
            runtime::hardline_agent(&container, &mut runner)
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
            if containers.is_empty() {
                println!("No matching agents found.");
            } else {
                for container in &containers {
                    runtime::eject_agent(container, &mut runner)?;
                    if purge {
                        remove_data_dir_if_exists(&paths.data_dir.join(container))?;
                        println!("Ejected and purged {container}.");
                    } else {
                        println!("Ejected {container}.");
                    }
                }
            }
            Ok(())
        }
        Command::Exile => {
            let names = runtime::list_managed_agent_names(&mut runner)?;
            if names.is_empty() {
                println!("No agents running.");
            } else {
                for name in &names {
                    runtime::eject_agent(name, &mut runner)?;
                    println!("Ejected {name}.");
                }
            }
            Ok(())
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
                    };
                    config.add_mount(&name, mount, scope.as_deref());
                    config.save(&paths)?;
                    println!("Added mount {name:?} ({scope_label}): {src} -> {dst}{ro}");
                    Ok(())
                }
                cli::MountCommand::Remove { name, scope } => {
                    if config.remove_mount(&name, scope.as_deref()) {
                        config.save(&paths)?;
                        println!("Removed mount {name:?}.");
                    } else {
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
                    if config.trust_agent(&class.key()) {
                        config.save(&paths)?;
                        println!("Trusted {}.", class.key());
                    } else {
                        println!("{} is already trusted.", class.key());
                    }
                    Ok(())
                }
                cli::TrustCommand::Revoke { selector } => {
                    let class = ClassSelector::parse(&selector)?;
                    if AppConfig::is_builtin_agent(&class.key()) {
                        anyhow::bail!("{} is a built-in agent and is always trusted.", class.key());
                    }
                    if config.untrust_agent(&class.key()) {
                        config.save(&paths)?;
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
                    let parsed_mode: config::AuthForwardMode =
                        mode.parse().map_err(|e: String| anyhow::anyhow!("{e}"))?;
                    if let Some(agent_selector) = agent {
                        let class = ClassSelector::parse(&agent_selector)?;
                        config.resolve_agent_source(&class)?;
                        config.set_agent_auth_forward(&class.key(), parsed_mode);
                        config.save(&paths)?;
                        println!("Set auth forwarding for {} to {parsed_mode}.", class.key());
                    } else {
                        config.claude.auth_forward = parsed_mode;
                        config.save(&paths)?;
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
        },
        Command::Workspace(command) => match command {
            WorkspaceCommand::Create {
                name,
                workdir,
                mounts,
                no_workdir_mount,
                allowed_agents,
                default_agent,
            } => {
                let expanded_workdir = workspace::resolve_path(&workdir);
                let parsed_mounts = mounts
                    .iter()
                    .map(|value| parse_mount_spec_resolved(value))
                    .collect::<Result<Vec<_>>>()?;
                let plan = workspace::planner::plan_create(
                    &expanded_workdir,
                    parsed_mounts,
                    no_workdir_mount,
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
                config.create_workspace(
                    &name,
                    WorkspaceConfig {
                        workdir: expanded_workdir,
                        mounts: plan.final_mounts,
                        allowed_agents,
                        default_agent,
                        last_agent: None,
                    },
                )?;
                config.save(&paths)?;
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
                }

                let workspace = config.require_workspace(&name)?;

                let allowed = if workspace.allowed_agents.is_empty() {
                    "any agent".to_string()
                } else {
                    workspace.allowed_agents.join(", ")
                };
                let default_agent = workspace.default_agent.as_deref().unwrap_or("none");

                let short_workdir = tui::shorten_home(&workspace.workdir);
                let info = [
                    ("Name", name.as_str()),
                    ("Workdir", short_workdir.as_str()),
                    ("Allowed Agents", &allowed),
                    ("Default Agent", default_agent),
                ];
                let mut info_table = Table::builder(info.iter().map(|(k, v)| [*k, *v])).build();
                info_table
                    .with(Style::modern_rounded())
                    .with(tabled::settings::Remove::row(
                        tabled::settings::object::Rows::first(),
                    ));
                println!("{info_table}");

                if !workspace.mounts.is_empty() {
                    println!();
                    println!("Mounts:");
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
                        })
                        .collect();
                    let mut mount_table = Table::new(mount_rows);
                    mount_table.with(Style::modern_rounded());
                    println!("{mount_table}");
                }

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
            } => {
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

                config.edit_workspace(
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
                    },
                )?;
                config.save(&paths)?;
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
                config.edit_workspace(
                    &name,
                    WorkspaceEdit {
                        remove_destinations: remove_dsts,
                        ..WorkspaceEdit::default()
                    },
                )?;
                config.save(&paths)?;
                println!(
                    "Pruned {} redundant mount(s) from workspace {name:?}.",
                    plan.removed.len()
                );
                Ok(())
            }
            WorkspaceCommand::Remove { name } => {
                config.remove_workspace(&name)?;
                config.save(&paths)?;
                println!("Removed workspace {name:?}.");
                Ok(())
            }
        },
        Command::Purge(PurgeArgs { selector, all }) => match Selector::parse(&selector)? {
            Selector::Container(container) => {
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
                    remove_data_dir_if_exists(&paths.data_dir.join(&container))?;
                    println!("Purged state for {container}.");
                }
                Ok(())
            }
        },
    }
}

fn remove_data_dir_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
