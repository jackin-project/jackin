pub mod cli;
pub mod config;
pub mod docker;
pub mod instance;
pub mod manifest;
pub mod paths;
pub mod repo;
pub mod runtime;
pub mod selector;

use anyhow::Result;
use cli::{Cli, Command};
use config::AppConfig;
use docker::ShellRunner;
use paths::JackinPaths;
use selector::Selector;

pub fn run(cli: Cli) -> Result<()> {
    let paths = JackinPaths::detect()?;
    let mut config = AppConfig::load_or_init(&paths)?;
    let mut runner = ShellRunner;

    match cli.command {
        Command::Load { selector } => match Selector::parse(&selector)? {
            Selector::Class(class) => runtime::load_agent(&paths, &mut config, &class, &mut runner),
            Selector::Container(_) => anyhow::bail!("load expects a class selector"),
        },
        Command::Hardline { container } => runtime::hardline_agent(&container, &mut runner),
        Command::Eject {
            selector,
            all,
            purge,
        } => match Selector::parse(&selector)? {
            Selector::Container(container) => {
                runtime::eject_agent(&container, &mut runner)?;
                if purge {
                    std::fs::remove_dir_all(paths.data_dir.join(&container)).ok();
                }
                Ok(())
            }
            Selector::Class(class) => {
                if all {
                    for container in
                        runtime::matching_family(&class, &runtime::list_running_agent_names(&mut runner)?)
                    {
                        runtime::eject_agent(&container, &mut runner)?;
                        if purge {
                            std::fs::remove_dir_all(paths.data_dir.join(&container)).ok();
                        }
                    }
                    Ok(())
                } else {
                    let container = crate::instance::primary_container_name(&class);
                    runtime::eject_agent(&container, &mut runner)?;
                    if purge {
                        std::fs::remove_dir_all(paths.data_dir.join(&container)).ok();
                    }
                    Ok(())
                }
            }
        },
        Command::Exile => runtime::exile_all(&mut runner),
        Command::Purge { selector, all } => match Selector::parse(&selector)? {
            Selector::Container(container) => {
                std::fs::remove_dir_all(paths.data_dir.join(container)).ok();
                Ok(())
            }
            Selector::Class(class) => {
                if all {
                    runtime::purge_class_data(&paths, &class)
                } else {
                    std::fs::remove_dir_all(
                        paths
                            .data_dir
                            .join(crate::instance::primary_container_name(&class)),
                    )
                    .ok();
                    Ok(())
                }
            }
        },
    }
}
