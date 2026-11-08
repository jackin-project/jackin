// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::PowerAction;

use crate::app::App;
use crate::cli::SandboxArg;
use crate::error::CommandError;

pub(crate) async fn run(
    app: &mut App,
    arg: SandboxArg,
    action: PowerAction,
) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    let identity = session
        .managed_identity(&arg.sandbox, &app.cancel)
        .await
        .map_err(CommandError::Lifecycle)?;
    let summary = session
        .power(&identity, action, &app.cancel)
        .await
        .map_err(CommandError::Lifecycle)?;
    let word = match action {
        PowerAction::Start => "started",
        PowerAction::Stop => "stopped",
    };
    match summary {
        jackin_session::SandboxObservation::Readable(summary) => {
            app.io.note(&format!("sandbox {} {word}", summary.name));
        }
        jackin_session::SandboxObservation::Unreadable(sandbox) => {
            app.io.note(&format!(
                "sandbox {} {word}; metadata unavailable: {}",
                sandbox.name,
                sandbox.reason()
            ));
        }
    }
    Ok(())
}
