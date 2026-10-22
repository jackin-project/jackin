use std::process::{Command, Output};

use anyhow::Result;

pub(crate) async fn command_output(command: &mut Command) -> Result<Output> {
    let child = command.spawn()?;
    let output = tokio::task::spawn_blocking(move || child.wait_with_output()).await??;
    Ok(output)
}
