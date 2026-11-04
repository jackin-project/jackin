// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::BuildOutput;
use jackin_image::{BuiltImage, Rebuild};
use jackin_session::{BuildRequest, build_observed};

use super::progress::image_event;
use crate::app::App;
use crate::cli::BuildArgs;
use crate::error::CommandError;
use crate::views::VIEW_SCHEMA;

pub(crate) async fn run(app: &mut App, args: BuildArgs) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let catalog = app.catalog();
    let (gateway, path) = (app.gateway.clone(), app.cwd.clone());
    let request = BuildRequest {
        gateway: gateway.as_ref(),
        role: args.role.as_ref(),
        path: &path,
        rebuild: Rebuild::from(args.rebuild),
    };
    let built = show_build(app, &config, &catalog, &request).await?;
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

async fn show_build(
    app: &mut App,
    config: &jackin_core::Config,
    catalog: &jackin_session::RoleCatalog,
    request: &BuildRequest<'_>,
) -> Result<BuiltImage, CommandError> {
    let engine = std::sync::Arc::clone(&app.deps.engine);
    let cancel = app.cancel.clone();
    let built = build_observed(
        engine.as_ref(),
        catalog,
        config,
        request,
        &cancel,
        |event| image_event(&mut app.io, event),
    )
    .await?;
    Ok(built)
}
