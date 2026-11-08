// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{LaunchId, Phase, RoleName, SandboxSummary, Timestamp, WorkspaceName};
use jackin_session::UnreadableSandbox;
use serde::Serialize;

use crate::app::App;
use crate::cli::JsonArg;
use crate::commands::AgentMetadataReport;
use crate::error::CommandError;
use crate::views::SANDBOX_VIEW_SCHEMA;

#[derive(Serialize)]
struct LsOutput {
    schema: u32,
    sandboxes: Vec<LsEntry>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum LsEntry {
    Readable {
        sandbox: SandboxSummary,
        agent_metadata: AgentMetadataReport,
    },
    Unreadable {
        name: jackin_core::SandboxName,
        phase: Phase,
        created_at: Option<Timestamp>,
        workspace: Option<WorkspaceName>,
        role: Option<RoleName>,
        launch: Option<LaunchId>,
        reason: String,
    },
}

impl From<UnreadableSandbox> for LsEntry {
    fn from(sandbox: UnreadableSandbox) -> Self {
        let reason = sandbox.reason();
        LsEntry::Unreadable {
            name: sandbox.name,
            phase: sandbox.phase,
            created_at: sandbox.created_at,
            workspace: sandbox.workspace,
            role: sandbox.role,
            launch: sandbox.launch,
            reason,
        }
    }
}

pub(crate) async fn run(app: &mut App, arg: &JsonArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    let listing = session
        .list(&app.cancel)
        .await
        .map_err(CommandError::Lifecycle)?;
    let mut sandboxes = Vec::with_capacity(listing.sandboxes.len() + listing.unreadable.len());
    for sandbox in listing.sandboxes {
        let metadata = session.agent_metadata(&sandbox, &app.cancel).await;
        sandboxes.push(LsEntry::Readable {
            sandbox,
            agent_metadata: metadata.into(),
        });
    }
    sandboxes.extend(listing.unreadable.into_iter().map(LsEntry::from));
    let output = LsOutput {
        schema: SANDBOX_VIEW_SCHEMA,
        sandboxes,
    };
    if arg.json {
        return app.io.json(&output);
    }
    if output.sandboxes.is_empty() {
        app.io.note("no sandboxes");
    }
    for sandbox in &output.sandboxes {
        match sandbox {
            LsEntry::Readable {
                sandbox,
                agent_metadata,
            } => {
                let labels = &sandbox.labels.planned;
                app.io.line(&format!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    sandbox.name,
                    labels.workspace,
                    labels.role,
                    labels.agents_hash,
                    sandbox.phase,
                    agent_metadata.human(),
                ))?;
            }
            LsEntry::Unreadable {
                name,
                phase,
                workspace,
                role,
                reason,
                ..
            } => {
                app.io.line(&format!(
                    "{name}\t{}\t{}\t{phase}\tmetadata unavailable: {reason}",
                    workspace
                        .as_ref()
                        .map_or_else(|| "workspace unavailable".to_owned(), ToString::to_string),
                    role.as_ref()
                        .map_or_else(|| "role unavailable".to_owned(), ToString::to_string),
                ))?;
            }
        }
    }
    Ok(())
}
