// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{
    InspectView, LaunchId, Phase, RoleName, SandboxName, SupervisorReading, Timestamp,
    WorkspaceName,
};
use serde::Serialize;

use crate::app::App;
use crate::cli::InspectArgs;
use crate::commands::AgentMetadataReport;
use crate::error::CommandError;
use crate::views::SANDBOX_VIEW_SCHEMA;

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum InspectOutput<'a> {
    Readable {
        #[serde(flatten)]
        view: &'a InspectView,
        agent_metadata: AgentMetadataReport,
    },
    Unreadable {
        schema: u32,
        name: SandboxName,
        phase: Phase,
        created_at: Option<Timestamp>,
        workspace: Option<WorkspaceName>,
        role: Option<RoleName>,
        launch: Option<LaunchId>,
        reason: String,
    },
}

pub(crate) async fn run(app: &mut App, args: InspectArgs) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    match session
        .inspect(&args.sandbox, &app.cancel)
        .await
        .map_err(CommandError::Lifecycle)?
    {
        jackin_session::SandboxInspection::Readable(view) => {
            let metadata = session.agent_metadata(&view.summary, &app.cancel).await;
            let report = AgentMetadataReport::from(metadata);
            if args.json {
                return app.io.json(&InspectOutput::Readable {
                    view: &view,
                    agent_metadata: report,
                });
            }
            human_readable(app, &view, &report)
        }
        jackin_session::SandboxInspection::Unreadable(sandbox) => {
            let reason = sandbox.reason();
            if args.json {
                return app.io.json(&InspectOutput::Unreadable {
                    schema: SANDBOX_VIEW_SCHEMA,
                    name: sandbox.name,
                    phase: sandbox.phase,
                    created_at: sandbox.created_at,
                    workspace: sandbox.workspace,
                    role: sandbox.role,
                    launch: sandbox.launch,
                    reason,
                });
            }
            human_unreadable(app, &sandbox, &reason)
        }
    }
}

fn supervisor_text(reading: &SupervisorReading) -> String {
    match reading {
        SupervisorReading::Unreadable => "unreadable".to_owned(),
        SupervisorReading::Reported(state) => state.render(),
        SupervisorReading::Invalid(text) => format!("invalid ({text})"),
    }
}

fn human_readable(
    app: &mut App,
    view: &InspectView,
    metadata: &AgentMetadataReport,
) -> Result<(), CommandError> {
    let labels = &view.summary.labels.planned;
    let providers: Vec<String> = view.providers.iter().map(ToString::to_string).collect();
    let lines = [
        format!("name\t{}", view.summary.name),
        format!("phase\t{}", view.summary.phase),
        format!("workspace\t{}", labels.workspace),
        format!("role\t{}", labels.role),
        format!("agent hash\t{}", labels.agents_hash),
        format!("lead agent\t{}", labels.lead_agent.as_str()),
        format!("agent metadata\t{}", metadata.human()),
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

fn human_unreadable(
    app: &mut App,
    sandbox: &jackin_session::UnreadableSandbox,
    reason: &str,
) -> Result<(), CommandError> {
    app.io.line(&format!("name\t{}", sandbox.name))?;
    app.io.line(&format!("phase\t{}", sandbox.phase))?;
    app.io.line(&format!(
        "workspace\t{}",
        sandbox
            .workspace
            .as_ref()
            .map_or_else(|| "unavailable".to_owned(), ToString::to_string)
    ))?;
    app.io.line(&format!(
        "role\t{}",
        sandbox
            .role
            .as_ref()
            .map_or_else(|| "unavailable".to_owned(), ToString::to_string)
    ))?;
    app.io.line(&format!("metadata\tunavailable: {reason}"))?;
    Ok(())
}
