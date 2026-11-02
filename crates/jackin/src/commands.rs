// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod account;
mod attach;
mod build;
mod doctor;
mod gateway;
mod inspect;
mod load;
mod ls;
mod progress;
mod role;
mod workspace;

use crate::app::App;
use crate::cli::Command;
use crate::error::CommandError;

pub(crate) async fn dispatch(app: &mut App, command: Command) -> Result<(), CommandError> {
    match command {
        Command::Gateway(command) => gateway::run(app, command).await,
        Command::Account(command) => account::run(app, command).await,
        Command::Doctor => doctor::run(app).await,
        Command::Build(args) => build::run(app, args).await,
        Command::Load(args) => load::run(app, args).await,
        Command::Attach(args) => attach::run(app, args).await,
        Command::Ls(arg) => ls::run(app, &arg).await,
        Command::Inspect(args) => inspect::run(app, args).await,
        Command::Role(command) => role::run(app, command).await,
        Command::Workspace(command) => workspace::run(app, command).await,
    }
}
