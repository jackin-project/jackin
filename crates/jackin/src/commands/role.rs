// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{ConfigEdit, FileKind, RoleSource, RoleUrl};

use crate::app::App;
use crate::cli::{JsonArg, RoleAddArgs, RoleArg, RoleCheckArgs, RoleCommand};
use crate::error::CommandError;
use crate::paths::absolutize;
use crate::views::{RoleLs, RoleSourceRow};

pub(crate) async fn run(app: &mut App, command: RoleCommand) -> Result<(), CommandError> {
    match command {
        RoleCommand::Add(args) => add(app, args).await,
        RoleCommand::Ls(arg) => ls(app, &arg).await,
        RoleCommand::Rm(arg) => rm(app, arg).await,
        RoleCommand::Check(args) => check(app, &args).await,
    }
}

async fn source(app: &App, args: &RoleAddArgs) -> Result<RoleSource, CommandError> {
    let path = absolutize(Path::new(&args.source), &app.cwd)?;
    if app.deps.host.fs.stat(&path).await? == Some(FileKind::Dir) {
        if args.rev.is_some() {
            return Err(CommandError::Argument(
                "--rev applies only to a Git source".to_owned(),
            ));
        }
        return Ok(RoleSource::Path(path));
    }
    let url = RoleUrl::parse(&args.source).map_err(|_| {
        CommandError::Argument(format!(
            "{} is neither an existing directory nor a Git URL",
            args.source
        ))
    })?;
    Ok(RoleSource::Git {
        url,
        rev: args.rev.clone(),
    })
}

async fn add(app: &App, args: RoleAddArgs) -> Result<(), CommandError> {
    let source = source(app, &args).await?;
    app.store
        .edit(vec![ConfigEdit::AddRole {
            name: args.name,
            source,
        }])
        .await?;
    Ok(())
}

async fn ls(app: &mut App, arg: &JsonArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let view = RoleLs::of(&config);
    if arg.json {
        return app.io.json(&view);
    }
    for row in &view.roles {
        let source = match &row.source {
            RoleSourceRow::Path { path } => path.clone(),
            RoleSourceRow::Git { url, rev } => match rev {
                Some(rev) => format!("{url}@{rev}"),
                None => url.to_string(),
            },
        };
        app.io.line(&format!("{}\t{source}", row.name))?;
    }
    Ok(())
}

async fn rm(app: &App, arg: RoleArg) -> Result<(), CommandError> {
    app.store
        .edit(vec![ConfigEdit::RemoveRole(arg.name)])
        .await?;
    Ok(())
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
