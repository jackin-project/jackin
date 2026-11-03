// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::ExportReport;

use crate::app::App;
use crate::cli::SandboxArg;
use crate::error::CommandError;

pub(crate) async fn run(app: &mut App, arg: SandboxArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    let report = session
        .export(&config, &arg.sandbox, app.cancel.clone())
        .await?;
    let ExportReport {
        sandbox,
        bundle: _,
        branches,
        detached,
        worktrees,
    } = report;
    let published = branches
        .iter()
        .map(|branch| (&branch.host_ref, &branch.tip))
        .chain(detached.iter().map(|head| (&head.host_ref, &head.tip)))
        .chain(
            worktrees
                .iter()
                .map(|worktree| (&worktree.host_ref, &worktree.tip)),
        );
    let mut any = false;
    for (host_ref, tip) in published {
        app.io.line(&format!("{host_ref}\t{tip}"))?;
        any = true;
    }
    if !any {
        app.io
            .note(&format!("sandbox {sandbox} has nothing to export"));
    }
    Ok(())
}
