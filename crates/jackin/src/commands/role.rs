// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::app::App;
use crate::cli::{RoleCheckArgs, RoleCommand};
use crate::error::CommandError;
use crate::paths::absolutize;

pub(crate) async fn run(app: &mut App, command: RoleCommand) -> Result<(), CommandError> {
    match command {
        RoleCommand::Check(args) => check(app, &args).await,
    }
}

async fn check(app: &App, args: &RoleCheckArgs) -> Result<(), CommandError> {
    let root = absolutize(&args.path, &app.cwd)?;
    let findings = app.catalog().findings(&root).await;
    if findings.is_empty() {
        Ok(())
    } else {
        Err(CommandError::RoleFindings(findings))
    }
}
