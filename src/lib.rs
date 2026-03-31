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
        Command::Eject { .. } | Command::Exile | Command::Purge { .. } => Ok(()),
    }
}
