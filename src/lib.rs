pub mod cli;
pub mod config;
pub mod paths;
pub mod selector;

use anyhow::Result;
use cli::Cli;

pub fn run(_cli: Cli) -> Result<()> {
    Ok(())
}
