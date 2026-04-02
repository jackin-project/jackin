pub mod cli;
pub mod config;
pub mod derived_image;
pub mod docker;
pub mod instance;
pub mod manifest;
pub mod paths;
pub mod repo;
pub mod repo_contract;
pub mod runtime;
pub mod selector;
pub mod tui;

use anyhow::Result;
use cli::{Cli, Command};
use config::AppConfig;
use docker::ShellRunner;
use paths::JackinPaths;
use selector::{ClassSelector, Selector};
use std::io::ErrorKind;
use std::path::Path;

pub fn run(cli: Cli) -> Result<()> {
    let paths = JackinPaths::detect()?;
    let mut config = AppConfig::load_or_init(&paths)?;
    let mut runner = ShellRunner;

    match cli.command {
        Command::Load {
            selector,
            no_intro,
            debug,
        } => {
            let class = ClassSelector::parse(&selector)?;
            let opts = runtime::LoadOptions { no_intro, debug };
            runtime::load_agent(&paths, &mut config, &class, &mut runner, &opts)
        }
        Command::Hardline { container } => runtime::hardline_agent(&container, &mut runner),
        Command::Eject {
            selector,
            all,
            purge,
        } => match Selector::parse(&selector)? {
            Selector::Container(container) => {
                runtime::eject_agent(&container, &mut runner)?;
                if purge {
                    remove_data_dir_if_exists(&paths.data_dir.join(&container))?;
                }
                Ok(())
            }
            Selector::Class(class) => {
                if all {
                    for container in
                        runtime::matching_family(&class, &runtime::list_managed_agent_names(&mut runner)?)
                    {
                        runtime::eject_agent(&container, &mut runner)?;
                        if purge {
                            remove_data_dir_if_exists(&paths.data_dir.join(&container))?;
                        }
                    }
                    Ok(())
                } else {
                    let container = crate::instance::primary_container_name(&class);
                    runtime::eject_agent(&container, &mut runner)?;
                    if purge {
                        remove_data_dir_if_exists(&paths.data_dir.join(&container))?;
                    }
                    Ok(())
                }
            }
        },
        Command::Exile => runtime::exile_all(&mut runner),
        Command::Purge { selector, all } => match Selector::parse(&selector)? {
            Selector::Container(container) => {
                remove_data_dir_if_exists(&paths.data_dir.join(container))?;
                Ok(())
            }
            Selector::Class(class) => {
                if all {
                    runtime::purge_class_data(&paths, &class)
                } else {
                    remove_data_dir_if_exists(
                        &paths
                            .data_dir
                            .join(crate::instance::primary_container_name(&class)),
                    )?;
                    Ok(())
                }
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
