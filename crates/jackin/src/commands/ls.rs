// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::LsOutput;

use crate::app::App;
use crate::cli::JsonArg;
use crate::error::CommandError;
use crate::views::VIEW_SCHEMA;

pub(crate) async fn run(app: &mut App, arg: &JsonArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    let listing = session.list().await.map_err(CommandError::Lifecycle)?;
    for unreadable in &listing.unreadable {
        app.io.note(&format!(
            "warning: sandbox {} has unreadable labels and is not listed: {}",
            unreadable.name, unreadable.error
        ));
    }
    let output = LsOutput {
        schema: VIEW_SCHEMA,
        sandboxes: listing.sandboxes,
    };
    if arg.json {
        return app.io.json(&output);
    }
    if output.sandboxes.is_empty() {
        app.io.note("no sandboxes");
    }
    for sandbox in &output.sandboxes {
        let labels = &sandbox.labels.planned;
        app.io.line(&format!(
            "{}\t{}\t{}\t{}\t{}",
            sandbox.name, labels.workspace, labels.role, labels.account, sandbox.phase
        ))?;
    }
    Ok(())
}
