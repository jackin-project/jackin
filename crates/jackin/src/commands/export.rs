// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::app::App;
use crate::cli::SandboxArg;
use crate::error::CommandError;

pub(crate) async fn run(app: &mut App, arg: SandboxArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    let report = session
        .export(&config, &arg.sandbox, app.cancel.clone())
        .await?;
    for branch in &report.branches {
        app.io
            .line(&format!("{}\t{}", branch.host_ref, branch.tip))?;
    }
    if let Some(detached) = &report.detached {
        app.io
            .line(&format!("{}\t{}", detached.host_ref, detached.tip))?;
    }
    if report.branches.is_empty() && report.detached.is_none() {
        app.io
            .note(&format!("sandbox {} has nothing to export", report.sandbox));
    }
    Ok(())
}
