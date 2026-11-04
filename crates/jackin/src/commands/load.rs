// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{EnvName, LaunchId, ProjectRoot, TerminalText};
use jackin_image::Rebuild;
use jackin_session::{
    AttachRequest, EnvQuestion, LaunchOutcome, LaunchPlan, LaunchRequest, LineDelivery, PlanError,
    Prompting, RestoreCandidate, launch_events, lookup_workspace, plan_launch,
};

use super::progress::launch_event;
use crate::app::App;
use crate::cli::{EnvAssignment, LoadArgs};
use crate::error::CommandError;
use crate::paths::existing_dir;
use crate::term;

async fn answer(app: &mut App, question: &EnvQuestion) -> Result<String, CommandError> {
    let printable = |text: &str| TerminalText::sanitize(text.as_bytes());
    let label = printable(
        question
            .prompt
            .as_deref()
            .unwrap_or_else(|| question.name.as_str()),
    );
    let shown = question
        .default
        .as_deref()
        .map(|default| format!(" [{}]", printable(default).as_str()))
        .unwrap_or_default();
    loop {
        app.io.ask(&format!("{}{shown}: ", label.as_str()));
        let terminal = app.terminal.as_mut().ok_or(CommandError::NoTerminal)?;
        let line = tokio::select! {
            biased;
            () = app.cancel.cancelled() => return Err(CommandError::Cancelled(format!(
                "cancelled while asking for {}",
                question.name
            ))),
            line = terminal.read_line() => line.map_err(CommandError::Terminal)?,
        };
        match (line, &question.default) {
            (None, _) => {
                return Err(CommandError::Cancelled(format!(
                    "cancelled; the input closed while asking for {}",
                    question.name
                )));
            }
            (Some(text), _) if !text.is_empty() => return Ok(text),
            (Some(_), Some(default)) => return Ok(default.clone()),
            (Some(_), None) => app.io.note(&format!("{} needs a value", question.name)),
        }
    }
}

async fn plan(
    app: &mut App,
    config: &jackin_core::Config,
    role: &jackin_session::LoadedRole,
    request: &LaunchRequest,
    values: &mut BTreeMap<EnvName, String>,
) -> Result<LaunchPlan, CommandError> {
    match plan_launch(config, role, request.clone().env(values.clone())) {
        Err(PlanError::EnvNeeded(questions)) if app.terminal.is_some() => {
            for question in &questions {
                let value = answer(app, question).await?;
                values.insert(question.name.clone(), value);
            }
            Ok(plan_launch(
                config,
                role,
                request.clone().env(values.clone()),
            )?)
        }
        other => Ok(other?),
    }
}

fn cancelled(candidate: &RestoreCandidate) -> CommandError {
    let launch = &candidate.launch_id;
    CommandError::Cancelled(match &candidate.sandbox {
        Some(sandbox) => format!(
            "cancelled; sandbox {sandbox} remains as a restore candidate (launch {launch}); rerun with: jackin load --launch-id {launch}"
        ),
        None => format!(
            "cancelled before a sandbox existed (launch {launch}); rerun with: jackin load --launch-id {launch}"
        ),
    })
}

pub(crate) async fn run(app: &mut App, args: LoadArgs) -> Result<(), CommandError> {
    let mut values = BTreeMap::new();
    for text in &args.env {
        let EnvAssignment { name, value } = EnvAssignment::parse(text)?;
        values.insert(name, value);
    }
    let config = app.store.load().await?;
    let path = existing_dir(app, &args.path).await?;
    let (_, workspace) = lookup_workspace(&config, &path).require_saved()?;
    let project = ProjectRoot::open(app.deps.host.fs.as_ref(), &path).await?;
    let (gateway, _) = config.resolve_gateway(app.gateway.as_ref())?;
    let role = app
        .catalog()
        .load(&config, args.role.as_ref(), &workspace, &app.cancel)
        .await?;
    let launch = match args.launch_id {
        Some(launch) => launch,
        None => LaunchId::generate()?,
    };
    let request = LaunchRequest::new(gateway.clone(), project, role.name().clone(), launch)
        .account(args.account)
        .rebuild(Rebuild::from(args.rebuild))
        .prompting(Prompting::for_terminal(app.terminal.is_some()));
    let plan = plan(app, &config, &role, &request, &mut values).await?;
    let launch = plan.launch_id().clone();
    let session = app.session(&config).await?;
    let (events, mut feed) = launch_events(LineDelivery::Reliable);
    let cancel = app.cancel.clone();
    let show = async {
        while let Some(event) = feed.recv().await {
            launch_event(&mut app.io, &event);
        }
    };
    let (outcome, ()) = tokio::join!(session.launch(plan, events, cancel), show);
    match outcome {
        Err(failure) => Err(CommandError::Launch(failure)),
        Ok(LaunchOutcome::Cancelled(candidate)) => Err(cancelled(&candidate)),
        Ok(LaunchOutcome::Ready(target)) => match &app.terminal {
            Some(terminal) => {
                let request = AttachRequest::owner(target, terminal.size());
                term::attach(app, &session, request).await
            }
            None => app.io.line(&format!("{}\t{launch}", target.sandbox)),
        },
    }
}
