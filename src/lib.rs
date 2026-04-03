pub mod cli;
pub mod config;
pub mod derived_image;
pub mod docker;
pub mod instance;
pub mod launch;
pub mod manifest;
pub mod paths;
pub mod repo;
pub mod repo_contract;
pub mod runtime;
pub mod selector;
pub mod tui;
pub mod workspace;

use anyhow::Result;
use cli::{Cli, Command, WorkspaceCommand};
use config::AppConfig;
use docker::ShellRunner;
use paths::JackinPaths;
use selector::{ClassSelector, Selector};
use std::io::ErrorKind;
use std::path::Path;
use workspace::{WorkspaceConfig, WorkspaceEdit, parse_mount_spec};

#[allow(clippy::too_many_lines)]
pub fn run(cli: Cli) -> Result<()> {
    let paths = JackinPaths::detect()?;
    let mut config = AppConfig::load_or_init(&paths)?;
    let mut runner = ShellRunner;

    match cli.command {
        Command::Load {
            selector,
            path,
            workspace,
            mounts,
            workdir,
            no_intro,
            debug,
        } => {
            let class = ClassSelector::parse(&selector)?;
            let cwd = std::env::current_dir()?;
            let workspace_input = if let Some(name) = workspace {
                crate::workspace::LoadWorkspaceInput::Saved(name)
            } else if !mounts.is_empty() {
                let mounts = mounts
                    .iter()
                    .map(|value| crate::workspace::parse_mount_spec(value))
                    .collect::<Result<Vec<_>>>()?;
                crate::workspace::LoadWorkspaceInput::Custom {
                    mounts,
                    workdir: workdir.ok_or_else(|| {
                        anyhow::anyhow!("--workdir is required when using --mount")
                    })?,
                }
            } else if let Some(path) = path {
                crate::workspace::LoadWorkspaceInput::Path(std::path::PathBuf::from(path))
            } else {
                crate::workspace::LoadWorkspaceInput::CurrentDir
            };
            let resolved_workspace =
                crate::workspace::resolve_load_workspace(&config, &class, &cwd, workspace_input)?;
            let opts = runtime::LoadOptions { no_intro, debug };
            runtime::load_agent(
                &paths,
                &mut config,
                &class,
                &resolved_workspace,
                &mut runner,
                &opts,
            )
        }
        Command::Launch => {
            let cwd = std::env::current_dir()?;
            let (class, workspace) = crate::launch::run_launch(&config, &cwd)?;
            let opts = runtime::LoadOptions {
                no_intro: false,
                debug: false,
            };
            runtime::load_agent(&paths, &mut config, &class, &workspace, &mut runner, &opts)
        }
        Command::Hardline { container } => runtime::hardline_agent(&container, &mut runner),
        Command::Eject {
            selector,
            all,
            purge,
        } => {
            let containers = match Selector::parse(&selector)? {
                Selector::Container(container) => vec![container],
                Selector::Class(class) => {
                    if all {
                        runtime::matching_family(
                            &class,
                            &runtime::list_managed_agent_names(&mut runner)?,
                        )
                    } else {
                        vec![crate::instance::primary_container_name(&class)]
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
        Command::Exile => runtime::exile_all(&mut runner),
        Command::Config {
            command: config_cmd,
        } => match config_cmd {
            cli::ConfigCommand::Mount { command: mount_cmd } => match mount_cmd {
                cli::MountCommand::Add {
                    name,
                    src,
                    dst,
                    readonly,
                    scope,
                } => {
                    let ro = if readonly { " (read-only)" } else { "" };
                    let scope_label = scope.as_deref().unwrap_or("global");
                    let mount = config::MountConfig { src: src.clone(), dst: dst.clone(), readonly };
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
                                src: m.src.clone(),
                                dst: m.dst.clone(),
                                mode: if m.readonly { "ro".to_string() } else { "rw".to_string() },
                            })
                            .collect();
                        println!("{}", Table::new(rows));
                    }
                    Ok(())
                }
            },
        },
        Command::Workspace { command } => match command {
            WorkspaceCommand::Add {
                name,
                workdir,
                mounts,
                allowed_agents,
                default_agent,
            } => {
                let mounts = mounts
                    .iter()
                    .map(|value| parse_mount_spec(value))
                    .collect::<Result<Vec<_>>>()?;
                let mount_count = mounts.len();
                config.add_workspace(
                    &name,
                    WorkspaceConfig {
                        workdir: workdir.clone(),
                        mounts,
                        allowed_agents,
                        default_agent,
                    },
                )?;
                config.save(&paths)?;
                println!("Added workspace {name:?} (workdir: {workdir}, {mount_count} mount(s)).");
                Ok(())
            }
            WorkspaceCommand::List => {
                let workspaces = config.list_workspaces();
                if workspaces.is_empty() {
                    println!("No workspaces configured.");
                    println!();
                    println!("Add one with:");
                    println!(
                        "  jackin workspace add <name> --workdir /path/to/project --mount /path/to/project"
                    );
                } else {
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
                            workdir: ws.workdir.clone(),
                            mounts: ws.mounts.len(),
                            allowed: if ws.allowed_agents.is_empty() {
                                "all".to_string()
                            } else {
                                ws.allowed_agents.join(", ")
                            },
                            default_agent: ws.default_agent.as_deref().unwrap_or("-").to_string(),
                        })
                        .collect();
                    println!("{}", Table::new(rows));
                }
                Ok(())
            }
            WorkspaceCommand::Show { name } => {
                let workspace = config
                    .workspaces
                    .get(&name)
                    .ok_or_else(|| anyhow::anyhow!("unknown workspace {name}"))?;
                println!("name: {name}");
                println!("workdir: {}", workspace.workdir);
                println!(
                    "allowed_agents: {}",
                    if workspace.allowed_agents.is_empty() {
                        "all".to_string()
                    } else {
                        workspace.allowed_agents.join(", ")
                    }
                );
                println!(
                    "default_agent: {}",
                    workspace.default_agent.as_deref().unwrap_or("-")
                );
                println!("mounts:");
                for mount in &workspace.mounts {
                    let ro = if mount.readonly { " (ro)" } else { "" };
                    println!("  {} -> {}{ro}", mount.src, mount.dst);
                }
                Ok(())
            }
            WorkspaceCommand::Edit {
                name,
                workdir,
                mounts,
                remove_destinations,
                allowed_agents,
                remove_allowed_agents,
                default_agent,
                clear_default_agent,
            } => {
                let upsert_mounts = mounts
                    .iter()
                    .map(|value| parse_mount_spec(value))
                    .collect::<Result<Vec<_>>>()?;
                config.edit_workspace(
                    &name,
                    WorkspaceEdit {
                        workdir,
                        upsert_mounts,
                        remove_destinations,
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
                println!("Updated workspace {name:?}.");
                Ok(())
            }
            WorkspaceCommand::Remove { name } => {
                config.remove_workspace(&name)?;
                config.save(&paths)?;
                println!("Removed workspace {name:?}.");
                Ok(())
            }
        },
        Command::Purge { selector, all } => match Selector::parse(&selector)? {
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
                    let container = crate::instance::primary_container_name(&class);
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
