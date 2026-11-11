// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{InspectView, SupervisorReading};

use crate::app::App;
use crate::cli::InspectArgs;
use crate::error::CommandError;

pub(crate) async fn run(app: &mut App, args: InspectArgs) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    let view = session
        .inspect(&args.sandbox)
        .await
        .map_err(CommandError::Lifecycle)?;
    if args.json {
        return app.io.json(&view);
    }
    human(app, &view)
}

fn supervisor_text(reading: &SupervisorReading) -> String {
    match reading {
        SupervisorReading::Unreadable => "unreadable".to_owned(),
        SupervisorReading::Reported(state) => state.render(),
        SupervisorReading::Invalid(text) => format!("invalid ({text})"),
    }
}

fn human(app: &mut App, view: &InspectView) -> Result<(), CommandError> {
    let labels = &view.summary.labels.planned;
    let providers: Vec<String> = view.providers.iter().map(ToString::to_string).collect();
    let lines = [
        format!("name\t{}", view.summary.name),
        format!("phase\t{}", view.summary.phase),
        format!("workspace\t{}", labels.workspace),
        format!("role\t{}", labels.role),
        format!("account\t{}", labels.agents.agents_label()),
        format!("launch\t{}", labels.launch),
        format!("providers\t{}", providers.join(", ")),
        format!("supervisor\t{}", supervisor_text(&view.supervisor)),
        format!("policy loaded\t{}", view.policy.loaded),
    ];
    for line in lines {
        app.io.line(&line)?;
    }
    for violation in &view.closure_violations {
        app.io.line(&format!("violation\t{violation}"))?;
    }
    Ok(())
}
