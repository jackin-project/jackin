// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod role;

use crate::app::App;
use crate::cli::Command;
use crate::error::CommandError;

pub(crate) async fn dispatch(app: &mut App, command: Command) -> Result<(), CommandError> {
    match command {
        Command::Role(command) => role::run(app, command).await,
    }
}
