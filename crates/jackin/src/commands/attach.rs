// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_openshell::InputMode;
use jackin_session::{AttachRequest, LineDelivery, launch_events};

use super::progress::launch_event;
use crate::app::App;
use crate::cli::{AttachArgs, parse_agent};
use crate::error::CommandError;
use crate::term;

pub(crate) async fn run(app: &mut App, args: AttachArgs) -> Result<(), CommandError> {
    let size = app
        .terminal
        .as_ref()
        .ok_or(CommandError::NoTerminal)?
        .size();
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    let requested_agent = args.agent.as_deref().map(parse_agent).transpose()?;
    let request = if args.restart {
        let listing = session
            .list(&app.cancel)
            .await
            .map_err(CommandError::Lifecycle)?;
        if let Some(unreadable) = listing
            .unreadable
            .iter()
            .find(|entry| entry.name == args.sandbox)
        {
            return Err(CommandError::Argument(format!(
                "sandbox {} has unavailable metadata: {}",
                unreadable.name,
                unreadable.reason()
            )));
        }
        let summary = listing
            .sandboxes
            .iter()
            .find(|summary| summary.name == args.sandbox)
            .ok_or_else(|| {
                CommandError::Argument(format!("sandbox {} is not managed", args.sandbox))
            })?;
        let metadata = session
            .agent_metadata(summary, &app.cancel)
            .await
            .map_err(|source| CommandError::AgentMetadata {
                sandbox: args.sandbox.clone(),
                source,
            })?;
        let agent = requested_agent.unwrap_or_else(|| metadata.lead_agent());
        if !metadata
            .instances
            .iter()
            .any(|instance| instance.agent == agent)
        {
            return Err(CommandError::Argument(
                jackin_session::AgentMetadataError::UnknownAgent { agent }.to_string(),
            ));
        }
        if agent == metadata.lead_agent() {
            let (events, mut feed) = launch_events(LineDelivery::Reliable);
            let cancel = app.cancel.clone();
            let show = async {
                while let Some(event) = feed.recv().await {
                    launch_event(&mut app.io, &event);
                }
            };
            let (restarted, ()) = tokio::join!(
                session.restart_agent(&metadata.identity, events, cancel),
                show
            );
            AttachRequest::owner(restarted?, size)
        } else {
            session
                .restart_instance(&metadata.identity, agent, &app.cancel)
                .await
                .map_err(|error| CommandError::Argument(error.to_string()))?;
            AttachRequest {
                identity: metadata.identity,
                size,
                input: InputMode::Owner,
                agent: Some(agent),
            }
        }
    } else {
        let identity = session
            .agent_metadata_for_sandbox(&args.sandbox, &app.cancel)
            .await
            .map_err(|source| CommandError::AgentMetadata {
                sandbox: args.sandbox.clone(),
                source,
            })?
            .identity;
        AttachRequest {
            identity,
            size,
            input: InputMode::Owner,
            agent: requested_agent,
        }
    };
    term::attach(app, &session, request).await
}
