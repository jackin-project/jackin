// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{BuildOutput, Workspace};
use jackin_image::{BuiltImage, ImageRequest, Rebuild, ensure_image};
use jackin_session::{LAUNCH_EVENT_CAPACITY, plan_image};
use tokio::sync::mpsc;

use super::progress::image_event;
use crate::app::App;
use crate::cli::BuildArgs;
use crate::error::CommandError;
use crate::views::VIEW_SCHEMA;

pub(crate) async fn run(app: &mut App, args: BuildArgs) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let (gateway, _) = config.resolve_gateway(app.gateway.as_ref())?;
    let workspace = config.workspace_for(&app.cwd).map_or_else(
        |_| Workspace {
            path: app.cwd.clone(),
            docker: false,
            default_role: None,
            bindings: BTreeMap::new(),
        },
        |(_, found)| found.clone(),
    );
    let role = app
        .catalog()
        .load(&config, args.role.as_ref(), &workspace)
        .await?;
    let request = plan_image(&config, &role, gateway, None, Rebuild::from(args.rebuild))?;
    let built = show_build(app, &request).await?;
    if args.json {
        app.io.json(&BuildOutput {
            schema: VIEW_SCHEMA,
            image: built.tag,
            id: built.id,
        })
    } else {
        app.io.line(&built.id.to_string())
    }
}

async fn show_build(app: &mut App, request: &ImageRequest) -> Result<BuiltImage, CommandError> {
    let (progress, mut events) = mpsc::channel(LAUNCH_EVENT_CAPACITY);
    let engine = std::sync::Arc::clone(&app.deps.engine);
    let cancel = app.cancel.clone();
    let build = async move { ensure_image(engine.as_ref(), request, &progress, &cancel).await };
    let render = async {
        while let Some(event) = events.recv().await {
            image_event(&mut app.io, &event);
        }
    };
    let (built, ()) = tokio::join!(build, render);
    Ok(built?)
}
