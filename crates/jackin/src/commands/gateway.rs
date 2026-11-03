// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ConfigEdit, EngineConfig, GatewayEntry, Platform};
use jackin_session::{AccountEditError, UnreachableGateway, gateway_holds_providers};

use crate::app::App;
use crate::cli::{GatewayAddArgs, GatewayCommand, GatewayRmArgs, JsonArg};
use crate::error::CommandError;
use crate::paths::absolutize;
use crate::views::GatewayLs;

pub(crate) async fn run(app: &mut App, command: GatewayCommand) -> Result<(), CommandError> {
    match command {
        GatewayCommand::Add(args) => add(app, args).await,
        GatewayCommand::Ls(arg) => ls(app, &arg).await,
        GatewayCommand::Rm(args) => rm(app, args).await,
    }
}

async fn add(app: &App, args: GatewayAddArgs) -> Result<(), CommandError> {
    let docker_engine = match (args.engine_port, &args.engine_certs) {
        (None, None) => None,
        (Some(port), Some(certs)) => Some(EngineConfig {
            port,
            client_certs: absolutize(certs, &app.cwd)?,
        }),
        _ => {
            return Err(CommandError::Argument(
                "--engine-port and --engine-certs go together".to_owned(),
            ));
        }
    };
    let entry = GatewayEntry {
        openshell_workspace: args.workspace,
        builder: args.builder,
        platform: args
            .platform
            .map_or_else(Platform::host_default, Platform::from),
        docker_engine,
    };
    let mut edits = vec![ConfigEdit::AddGateway {
        name: args.name.clone(),
        entry,
    }];
    if args.default {
        edits.push(ConfigEdit::SetDefaultGateway(Some(args.name)));
    }
    app.store.edit(edits).await?;
    Ok(())
}

async fn ls(app: &mut App, arg: &JsonArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let view = GatewayLs::of(&config);
    if arg.json {
        return app.io.json(&view);
    }
    for row in &view.gateways {
        let default = if row.default { "\tdefault" } else { "" };
        app.io.line(&format!(
            "{}\t{}\t{}\t{}{default}",
            row.name, row.workspace, row.builder, row.platform
        ))?;
    }
    Ok(())
}

async fn rm(app: &mut App, arg: GatewayRmArgs) -> Result<(), CommandError> {
    if arg.offline {
        app.store
            .edit(vec![ConfigEdit::RemoveGateway(arg.name.clone())])
            .await?;
        app.io.note(&format!(
            "warning: gateway {} was removed without contacting it; its OpenShell providers, which hold account credentials, stay on it until that gateway is reset",
            arg.name
        ));
        return Ok(());
    }
    let config = app.store.load().await?;
    let entry = config
        .gateway(&arg.name)
        .filter(|_| gateway_holds_providers(&config));
    let Some(entry) = entry else {
        app.store
            .edit(vec![ConfigEdit::RemoveGateway(arg.name)])
            .await?;
        return Ok(());
    };
    let gateway = match app
        .deps
        .connect
        .connect(&arg.name, entry, &app.cancel)
        .await
    {
        Ok(gateway) => gateway,
        Err(error) if error.is_cancelled() => return Err(error.into()),
        Err(error) => {
            return Err(AccountEditError::Unreachable(vec![UnreachableGateway::new(
                arg.name, error,
            )])
            .into());
        }
    };
    app.session_of(gateway)
        .remove_gateway(&app.store, &app.cancel)
        .await?;
    Ok(())
}
