// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{EnvName, LaunchId, PlainEnvValue, ProjectRoot, TerminalText};
use jackin_image::Rebuild;
use jackin_session::{
    AttachRequest, EnvQuestion, LaunchOutcome, LaunchPlan, LaunchRequest, LineDelivery, PlanError,
    Prompting, RestoreCandidate, launch_events, lookup_workspace, plan_launch,
};

use super::progress::launch_event;
use crate::app::App;
use crate::cli::{EnvAssignment, LoadArgs, parse_agent, parse_provider};
use crate::error::CommandError;
use crate::paths::existing_dir;
use crate::term;

async fn answer(app: &mut App, question: &EnvQuestion) -> Result<PlainEnvValue, CommandError> {
    let printable = |text: &str| TerminalText::sanitize(text.as_bytes());
    let label = printable(
        question
            .prompt
            .as_deref()
            .unwrap_or_else(|| question.name.as_str()),
    );
    let shown = question
        .default
        .as_ref()
        .map(|default| format!(" [{}]", printable(default.as_str()).as_str()))
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
            (Some(text), _) if !text.is_empty() => match PlainEnvValue::parse(&text) {
                Ok(value) => return Ok(value),
                Err(error) => app.io.note(&format!("{}: {error}", question.name)),
            },
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
    values: &mut BTreeMap<EnvName, PlainEnvValue>,
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
    let (workspace_name, workspace) = lookup_workspace(&config, &path).require_saved()?;
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
    let agent_texts: Vec<String> = if args.agent.is_empty() {
        vec!["codex".to_owned()]
    } else {
        args.agent.clone()
    };
    let mut agents = Vec::new();
    for text in &agent_texts {
        agents.push(parse_agent(text)?);
    }
    let agent = agents[0];
    let provider = match &args.provider {
        Some(text) => parse_provider(text)?,
        None => agent.provider(),
    };
    let parse_for = |entries: &[String]| -> Result<BTreeMap<String, String>, CommandError> {
        let mut map = BTreeMap::new();
        for entry in entries {
            let (name, value) = entry.split_once('=').ok_or_else(|| {
                CommandError::Argument(format!("expected <agent>=<value>, got {entry}"))
            })?;
            map.insert(name.to_owned(), value.to_owned());
        }
        Ok(map)
    };
    let providers_for = parse_for(&args.provider_for)?;
    let accounts_for = parse_for(&args.account_for)?;
    let models_for = parse_for(&args.model_for)?;
    let mut instances = Vec::new();
    for instance_agent in &agents {
        let instance_provider = match providers_for.get(instance_agent.as_str()) {
            Some(text) => parse_provider(text)?,
            None if agents.len() == 1 => provider,
            None => instance_agent.provider(),
        };
        let tuple = jackin_core::lookup_tuple(*instance_agent, instance_provider)
            .map_err(|error| CommandError::Argument(error.to_string()))?;
        if tuple.state != jackin_core::TupleState::Supported {
            return Err(CommandError::Argument(format!(
                "tuple {} with provider {} is not supported",
                instance_agent.as_str(),
                instance_provider.as_str()
            )));
        }
        let explicit = match accounts_for.get(instance_agent.as_str()) {
            Some(text) => Some(
                jackin_core::AccountName::parse(text)
                    .map_err(|error| CommandError::Argument(error.to_string()))?,
            ),
            None => None,
        };
        let global = match (&explicit, &args.account) {
            (Some(_), _) => None,
            (None, Some(name)) => match config.account(name) {
                Some(entry) if entry.provider() != instance_provider => None,
                _ => Some(name.clone()),
            },
            (None, None) => None,
        };
        let flag = explicit.as_ref().or(global.as_ref());
        let instance_account = config
            .select_account(&jackin_core::AccountQuery {
                flag,
                workspace: &workspace_name,
                role: role.name(),
                agent: *instance_agent,
                provider: instance_provider,
            })
            .map_err(|error| CommandError::Argument(error.to_string()))?
            .account;
        let instance_model = models_for
            .get(instance_agent.as_str())
            .cloned()
            .or_else(|| {
                if agents.len() == 1 {
                    args.model.clone()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| tuple.default_model.to_owned());
        tuple
            .check_model(&instance_model)
            .map_err(|error| CommandError::Argument(error.to_string()))?;
        instances.push(jackin_core::AgentInstance::new(
            *instance_agent,
            instance_provider,
            instance_account,
            instance_model,
        ));
    }
    let instances = jackin_core::AgentInstances::new(instances)
        .map_err(|error| CommandError::Argument(error.to_string()))?;
    let task = resolve_task(
        app,
        args.task.as_ref(),
        args.task_file.as_ref(),
        args.issue.as_ref(),
    )
    .await?;
    let request = LaunchRequest::new(gateway.clone(), project, role.name().clone(), launch)
        .tuple(agent, provider)
        .instances(instances)
        .task(task)
        .model(args.model)
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

async fn resolve_task(
    app: &mut App,
    task: Option<&String>,
    task_file: Option<&std::path::PathBuf>,
    issue: Option<&String>,
) -> Result<Option<String>, CommandError> {
    match (task, task_file, issue) {
        (Some(text), None, None) => Ok(Some(text.clone())),
        (None, Some(path), None) => {
            Ok(Some(tokio::fs::read_to_string(path).await.map_err(
                |error| CommandError::Argument(format!("cannot read task file: {error}")),
            )?))
        }
        (None, None, Some(url)) => Ok(Some(issue_task(app, url).await?)),
        (None, None, None) => Ok(None),
        _ => Err(CommandError::Argument(
            "pass only one of --task, --task-file, and --issue".to_owned(),
        )),
    }
}

async fn issue_task(app: &mut App, url: &str) -> Result<String, CommandError> {
    let output = tokio::process::Command::new("gh")
        .args(["issue", "view", url, "--json", "title,body"])
        .output()
        .await
        .map_err(|error| CommandError::Argument(format!("cannot run gh: {error}")))?;
    if !output.status.success() {
        return Err(CommandError::Argument("gh issue view failed".to_owned()));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let title = json_string(&text, "title").unwrap_or_default();
    let body = json_string(&text, "body").unwrap_or_default();
    let _ = app;
    Ok(format!("{title}\n\n{body}"))
}

fn json_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = text.find(&needle)? + needle.len();
    let mut out = String::new();
    let mut chars = text[start..].chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => {}
            },
            other => out.push(other),
        }
    }
    Some(out)
}
