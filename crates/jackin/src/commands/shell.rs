// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::ShellRequest;

use crate::app::App;
use crate::cli::SandboxArg;
use crate::error::CommandError;
use crate::term;

pub(crate) async fn run(app: &mut App, arg: SandboxArg) -> Result<(), CommandError> {
    let size = app
        .terminal
        .as_ref()
        .ok_or(CommandError::NoTerminal)?
        .size();
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    let identity = session.managed_identity(&arg.sandbox, &app.cancel).await?;
    term::shell(app, &session, ShellRequest { identity, size }).await
}
