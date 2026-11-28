// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::{AgentNamesError, AttachRequest, RestartError};

use crate::app::App;
use crate::cli::{AgentCommand, AgentRestartArgs, SandboxArg, parse_agent};
use crate::error::CommandError;
use crate::report::{gateway_command, restart_unsettled};
use crate::term;

pub(crate) async fn run(app: &mut App, command: AgentCommand) -> Result<(), CommandError> {
    match command {
        AgentCommand::Restart(args) => restart(app, args).await,
    }
}

async fn restart(app: &mut App, args: AgentRestartArgs) -> Result<(), CommandError> {
    let agent = parse_agent(&args.agent)?;
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    let names = session
        .sandbox_agent_names(&args.sandbox, &app.cancel)
        .await
        .map_err(|error| match error {
            AgentNamesError::NotManaged(sandbox) => {
                CommandError::Restart(RestartError::NotManaged(sandbox))
            }
            AgentNamesError::Gateway(error) => CommandError::Gateway(error),
        })?;
    let is_lead = names.first().is_none_or(|lead| lead == agent.as_str());
    if is_lead {
        let size = app
            .terminal
            .as_ref()
            .ok_or(CommandError::NoTerminal)?
            .size();
        let (events, mut feed) =
            jackin_session::launch_events(jackin_session::LineDelivery::Reliable);
        app.unsettled(restart_unsettled(&args.sandbox, session.gateway_name()));
        let cancel = app.cancel.clone();
        let show = async {
            while let Some(event) = feed.recv().await {
                crate::commands::progress::launch_event(&mut app.io, &event);
            }
        };
        let (restarted, ()) =
            tokio::join!(session.restart_agent(&args.sandbox, events, cancel), show);
        app.settled();
        let request = AttachRequest::owner(restarted?, size);
        return term::attach(app, &session, request).await;
    }
    app.unsettled(restart_unsettled(&args.sandbox, session.gateway_name()));
    session
        .restart_instance(&args.sandbox, agent, &app.cancel.clone())
        .await?;
    app.io.line(&format!("{} ready", agent.as_str()))?;
    Ok(())
}

pub(crate) async fn integrate(app: &mut App, args: SandboxArg) -> Result<(), CommandError> {
    let config = app.store.load().await?;
    let session = app.session(&config).await?;
    let follow_up = gateway_command("integrate", session.gateway_name());
    let sandbox = &args.sandbox;
    app.unsettled(format!(
        "sandbox {sandbox}: the integrate may have merged some agent branches; run {follow_up} {sandbox} to finish it"
    ));
    let merged = session
        .integrate(&args.sandbox, &app.cancel.clone())
        .await?;
    for branch in merged {
        app.io.line(&format!("merged {branch}"))?;
    }
    Ok(())
}
