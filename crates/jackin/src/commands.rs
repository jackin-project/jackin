// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod account;
mod gateway;
mod role;
mod workspace;

use crate::app::App;
use crate::cli::Command;
use crate::error::CommandError;

pub(crate) async fn dispatch(app: &mut App, command: Command) -> Result<(), CommandError> {
    match command {
        Command::Gateway(command) => gateway::run(app, command).await,
        Command::Account(command) => account::run(app, command).await,
        Command::Role(command) => role::run(app, command).await,
        Command::Workspace(command) => workspace::run(app, command).await,
    }
}
