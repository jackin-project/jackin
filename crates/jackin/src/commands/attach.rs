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
    let request = if args.restart {
        let (events, mut feed) = launch_events(LineDelivery::Reliable);
        let cancel = app.cancel.clone();
        let show = async {
            while let Some(event) = feed.recv().await {
                launch_event(&mut app.io, &event);
            }
        };
        let (restarted, ()) =
            tokio::join!(session.restart_agent(&args.sandbox, events, cancel), show);
        AttachRequest::owner(restarted?, size)
    } else {
        AttachRequest {
            sandbox: args.sandbox,
            size,
            input: InputMode::Owner,
            agent: match &args.agent {
                Some(text) => Some(parse_agent(text)?),
                None => None,
            },
        }
    };
    term::attach(app, &session, request).await
}
