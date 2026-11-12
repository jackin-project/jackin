// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use jackin_core::{
    Agent, EnvName, FileSystem, FsError, HostPath, LaunchId, PlainEnvValue, ProcessError,
    ProcessSpec, ProcessStatus, ProcessStdin, Program, ProjectRoot, RegularFileProblem, RepoPath,
    TerminalText,
};
use jackin_image::Rebuild;
use jackin_session::{
    AttachRequest, EnvQuestion, LaunchOutcome, LaunchPlan, LaunchRequest, LineDelivery, PlanError,
    Prompting, RestoreCandidate, launch_events, lookup_workspace, plan_launch,
};
use serde::Deserialize;

use super::progress::launch_event;
use crate::app::App;
use crate::cli::{EnvAssignment, LoadArgs, parse_agent, parse_provider};
use crate::error::CommandError;
use crate::paths::existing_dir;
use crate::term;

const TASK_FILE_MAX_BYTES: usize = 1 << 20;
const ISSUE_TIMEOUT: Duration = Duration::from_secs(30);

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
            "cancelled; sandbox {sandbox} remains as a restore candidate (launch {launch}); repeat the same jackin load command with --launch-id {launch}"
        ),
        None => format!(
            "cancelled before a sandbox existed (launch {launch}); repeat the same jackin load command with --launch-id {launch}"
        ),
    })
}

fn parse_for(
    entries: &[String],
    flag: &str,
    agents: &[Agent],
) -> Result<BTreeMap<String, String>, CommandError> {
    let mut map = BTreeMap::new();
    for entry in entries {
        let (name, value) = entry.split_once('=').ok_or_else(|| {
            CommandError::Argument(format!("expected <agent>=<value>, got {entry}"))
        })?;
        check_agent(flag, name, agents)?;
        map.insert(name.to_owned(), value.to_owned());
    }
    Ok(map)
}

fn check_agent(flag: &str, name: &str, agents: &[Agent]) -> Result<(), CommandError> {
    if agents.iter().filter(|agent| agent.as_str() == name).count() == 1 {
        return Ok(());
    }
    let selected: Vec<&str> = agents.iter().map(|agent| agent.as_str()).collect();
    Err(CommandError::Argument(format!(
        "--{flag} names {name}, which does not name exactly one of the selected agents: {}",
        selected.join(", ")
    )))
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
    let providers_for = parse_for(&args.provider_for, "provider-for", &agents)?;
    let accounts_for = parse_for(&args.account_for, "account-for", &agents)?;
    let models_for = parse_for(&args.model_for, "model-for", &agents)?;
    let mut instances = Vec::new();
    for (index, instance_agent) in agents.iter().enumerate() {
        let instance_provider = match providers_for.get(instance_agent.as_str()) {
            Some(text) => parse_provider(text)?,
            None if index == 0 => provider,
            None => instance_agent.provider(),
        };
        let tuple = jackin_core::lookup_tuple(*instance_agent, instance_provider)
            .map_err(|error| CommandError::Argument(error.to_string()))?;
        if tuple.state != jackin_core::TupleState::Supported {
            return Err(PlanError::TupleNotSupported {
                agent: *instance_agent,
                provider: instance_provider,
                reason: tuple.reason.unwrap_or("not supported").to_owned(),
            }
            .into());
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
            (None, Some(name)) if agents.len() == 1 => Some(name.clone()),
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
    let lead_provider = instances.lead().provider;
    let task = resolve_task(
        app,
        args.task.as_ref(),
        args.task_file.as_ref(),
        args.issue.as_ref(),
    )
    .await?;
    let request = LaunchRequest::new(gateway.clone(), project, role.name().clone(), launch)
        .tuple(agent, lead_provider)
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
    task_file: Option<&PathBuf>,
    issue: Option<&String>,
) -> Result<Option<String>, CommandError> {
    match (task, task_file, issue) {
        (Some(text), None, None) => Ok(Some(text.clone())),
        (None, Some(path), None) => Ok(Some(task_text(app, path).await?)),
        (None, None, Some(url)) => Ok(Some(issue_task(app, url).await?)),
        (None, None, None) => Ok(None),
        _ => Err(CommandError::Argument(
            "pass only one of --task, --task-file, and --issue".to_owned(),
        )),
    }
}

async fn task_text(app: &App, path: &Path) -> Result<String, CommandError> {
    let cancel = app.cancel.clone();
    let read = read_task_file(app.deps.host.fs.as_ref(), &app.cwd, path);
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(CommandError::Cancelled(
            "cancelled while reading the task file".to_owned(),
        )),
        result = read => result,
    }
}

async fn read_task_file(
    fs: &dyn FileSystem,
    base: &HostPath,
    path: &Path,
) -> Result<String, CommandError> {
    let shown = path.display().to_string();
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(CommandError::Argument(format!(
            "the task file {shown} may not contain a parent segment"
        )));
    }
    let absolute = HostPath::resolve(path, base)?;
    let Some(target) = fs.canonicalize(&absolute).await? else {
        return Err(CommandError::Argument(format!(
            "cannot read the task file {shown}: it does not exist"
        )));
    };
    let Some(dir) = target.as_path().parent() else {
        return Err(CommandError::Argument(format!(
            "cannot read the task file {shown}: it has no parent directory"
        )));
    };
    let Some(name) = target
        .as_path()
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| RepoPath::parse(name).ok())
    else {
        return Err(CommandError::Argument(format!(
            "cannot read the task file {shown}: the name is not usable"
        )));
    };
    let dir = HostPath::parse(dir)?;
    match fs.read_regular(&dir, &name, TASK_FILE_MAX_BYTES).await {
        Ok(Ok(file)) if file.bytes.len() > TASK_FILE_MAX_BYTES => Err(CommandError::Argument(
            format!("the task file {shown} is larger than {TASK_FILE_MAX_BYTES} bytes"),
        )),
        Ok(Ok(file)) => String::from_utf8(file.bytes).map_err(|_| {
            CommandError::Argument(format!("the task file {shown} is not valid UTF-8"))
        }),
        Ok(Err(RegularFileProblem::Missing)) => Err(CommandError::Argument(format!(
            "cannot read the task file {shown}: it does not exist"
        ))),
        Ok(Err(RegularFileProblem::Symlink | RegularFileProblem::NotARegularFile)) => Err(
            CommandError::Argument(format!("the task file {shown} is not a regular file")),
        ),
        Err(FsError::Io {
            kind: ErrorKind::NotFound,
            ..
        }) => Err(CommandError::Argument(format!(
            "cannot read the task file {shown}: it does not exist"
        ))),
        Err(other) => Err(other.into()),
    }
}

#[derive(Debug, Deserialize)]
struct IssueView {
    title: String,
    #[serde(default)]
    body: String,
}

async fn issue_task(app: &App, url: &str) -> Result<String, CommandError> {
    let spec = ProcessSpec {
        program: Program::Gh,
        args: vec![
            "issue".to_owned(),
            "view".to_owned(),
            url.to_owned(),
            "--json".to_owned(),
            "title,body".to_owned(),
        ],
        cwd: None,
        env: Vec::new(),
        env_remove_prefixes: Vec::new(),
        stdin: ProcessStdin::Null,
        timeout: ISSUE_TIMEOUT,
    };
    let output = app
        .deps
        .host
        .runner
        .run(&spec, None, &app.cancel)
        .await
        .map_err(|error| match error {
            ProcessError::Cancelled { .. } => {
                CommandError::Cancelled("cancelled while reading the GitHub issue".to_owned())
            }
            other => CommandError::Argument(format!("cannot run gh: {other}")),
        })?;
    if output.status != ProcessStatus::Exited(0) {
        return Err(CommandError::Argument("gh issue view failed".to_owned()));
    }
    let issue: IssueView = serde_json::from_slice(&output.stdout).map_err(|error| {
        CommandError::Argument(format!("gh issue view returned no readable issue: {error}"))
    })?;
    Ok(format!("{}\n\n{}", issue.title, issue.body))
}
