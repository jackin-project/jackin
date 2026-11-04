// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ConfigEdit, Workspace, plain};

use crate::app::App;
use crate::cli::{JsonArg, WorkspaceAddArgs, WorkspaceArg, WorkspaceCommand};
use crate::error::CommandError;
use crate::paths::existing_dir;
use crate::views::WorkspaceLs;

pub(crate) async fn run(app: &mut App, command: WorkspaceCommand) -> Result<(), CommandError> {
    match command {
        WorkspaceCommand::Add(args) => add(app, args).await,
        WorkspaceCommand::Ls(arg) => ls(app, &arg).await,
        WorkspaceCommand::Rm(arg) => rm(app, arg).await,
    }
}

async fn add(app: &App, args: WorkspaceAddArgs) -> Result<(), CommandError> {
    let workspace = Workspace::new(existing_dir(app, &args.path).await?, args.docker);
    app.store
        .edit(vec![ConfigEdit::AddWorkspace {
            name: args.name,
            workspace,
        }])
        .await?;
    Ok(())
}

async fn ls(app: &mut App, arg: &JsonArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let view = WorkspaceLs::of(&config);
    if arg.json {
        return app.io.json(&view);
    }
    for row in &view.workspaces {
        let docker = if row.docker { "\tdocker" } else { "" };
        app.io
            .line(&format!("{}\t{}{docker}", row.name, plain(&row.path)))?;
    }
    Ok(())
}

async fn rm(app: &App, arg: WorkspaceArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    if config.workspace(&arg.name).is_some() && config.gateways().next().is_some() {
        app.gateways(&config)
            .await?
            .ensure_workspace_unused(&config, &arg.name)
            .await?;
    }
    app.store
        .edit(vec![ConfigEdit::RemoveWorkspace(arg.name)])
        .await?;
    Ok(())
}
