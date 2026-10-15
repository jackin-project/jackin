// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Thin `jackin workflow` handlers: validate a definition, or compile
//! it into an immutable plan. `plan` resolves the existing workspace
//! selections (workspace, role manifest, account bindings, agent
//! binaries, models, role image, repository HEAD), compiles the plan,
//! stores it immutably, and prints the plan hash. A missing or
//! ambiguous selection is a typed `missing_input` diagnostic, never
//! an interactive prompt.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

use crate::cli::workflow::{WorkflowCommand, WorkflowFileArgs, WorkflowPlanArgs};
use jackin_workflow_spec::error::{Diagnostic, ErrorCode, WorkflowError};
use jackin_workflow_spec::resolve::{LiveAgentFacts, LiveSelectionEnv};

/// Requested-model marker frozen into the plan when neither the agent
/// configuration nor the account names a model (subscription/profile
/// accounts carry none). The vendor picks the live model at launch;
/// the plan keeps `actual_model` unknown (`None`) per C1.
const VENDOR_DEFAULT_MODEL: &str = "vendor-default";

/// Repository ID for the workspace repository source.
const WORKSPACE_REPOSITORY_ID: &str = "workspace:current";

pub(super) fn handle(
    command: WorkflowCommand,
    config: &jackin_config::AppConfig,
    paths: &jackin_core::JackinPaths,
) -> Result<()> {
    match command {
        WorkflowCommand::Validate(args) => validate(&args),
        WorkflowCommand::Plan(args) => plan(&args, config, paths),
        run_control => run_control_command(run_control, config, paths),
    }
}

/// Durable run control: the CLI verifies the stored plan for
/// `run`, then drives the host daemon's workflow service over the
/// daemon socket. Every command supports stable `--json` output
/// (`OutputEnvelope::v1`) and typed exit codes (0/2/3/4/1); bare
/// `status` lists runs (C7).
#[cfg(unix)]
fn run_control_command(
    command: WorkflowCommand,
    _config: &jackin_config::AppConfig,
    paths: &jackin_core::JackinPaths,
) -> Result<()> {
    use crate::cli::workflow::{
        WorkflowCancelArgs, WorkflowGraphArgs, WorkflowPauseArgs, WorkflowResolveInputArgs,
        WorkflowResumeArgs,
    };
    use jackin_runtime::host_daemon::{DaemonLayout, DaemonRequestKind};

    let layout = DaemonLayout::new(paths);
    match command {
        WorkflowCommand::Validate(_) | WorkflowCommand::Plan(_) => {
            anyhow::bail!("not a run-control command")
        }
        WorkflowCommand::Run(args) => run_stored_plan(&layout, paths, args),
        WorkflowCommand::Status(args) => run_status(&layout, args),
        WorkflowCommand::Graph(args) => {
            let WorkflowGraphArgs { run_id, json } = args;
            let detail = daemon_run_detail(&layout, &run_id, -1, 0, json)?;
            emit_graph(&detail, json);
            Ok(())
        }
        WorkflowCommand::Events(args) => run_events(&layout, args),
        WorkflowCommand::Pause(args) => {
            let WorkflowPauseArgs {
                run_id,
                reason,
                checkpoint,
                json,
            } = args;
            let summary = daemon_run_summary(
                &layout,
                DaemonRequestKind::WorkflowPause {
                    run_id,
                    reason,
                    checkpoint,
                },
                json,
            )?;
            emit_summary(&summary, json);
            Ok(())
        }
        WorkflowCommand::Resume(args) => {
            let WorkflowResumeArgs { run_id, json } = args;
            let summary =
                daemon_run_summary(&layout, DaemonRequestKind::WorkflowResume { run_id }, json)?;
            emit_summary(&summary, json);
            Ok(())
        }
        WorkflowCommand::Cancel(args) => {
            let WorkflowCancelArgs {
                run_id,
                reason,
                json,
            } = args;
            let summary = daemon_run_summary(
                &layout,
                DaemonRequestKind::WorkflowCancel { run_id, reason },
                json,
            )?;
            emit_summary(&summary, json);
            Ok(())
        }
        WorkflowCommand::Attach(args) => run_attach(&layout, args),
        WorkflowCommand::ResolveInput(args) => {
            let WorkflowResolveInputArgs {
                run_id,
                command,
                action,
                reason: _,
                observed_session_state,
                json,
            } = args;
            let summary = daemon_run_summary(
                &layout,
                DaemonRequestKind::WorkflowResolve {
                    run_id,
                    command_id: command,
                    decision: action,
                    observed_session_state,
                },
                json,
            )?;
            emit_summary(&summary, json);
            Ok(())
        }
        WorkflowCommand::Export(args) => run_export(&layout, args),
    }
}

/// Start a run for a verified stored plan.
#[cfg(unix)]
fn run_stored_plan(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
    paths: &jackin_core::JackinPaths,
    args: crate::cli::workflow::WorkflowRunArgs,
) -> Result<()> {
    use jackin_runtime::host_daemon::DaemonRequestKind;

    let crate::cli::workflow::WorkflowRunArgs {
        plan_hash,
        idempotency_key,
        json,
    } = args;
    let plan = read_stored_plan(paths, &plan_hash)?;
    let summary = daemon_run_summary(
        layout,
        DaemonRequestKind::WorkflowStart {
            plan_hash,
            max_repair_turns: plan.max_repair_turns,
            idempotency_key: idempotency_key.unwrap_or_else(|| plan.plan_hash.clone()),
        },
        json,
    )?;
    emit_summary(&summary, json);
    Ok(())
}

/// Single-run status, or the bounded run list without a run ID.
#[cfg(unix)]
fn run_status(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
    args: crate::cli::workflow::WorkflowStatusArgs,
) -> Result<()> {
    use jackin_runtime::host_daemon::DaemonRequestKind;

    let crate::cli::workflow::WorkflowStatusArgs {
        run_id,
        json,
        limit,
    } = args;
    if let Some(run_id) = run_id {
        let summary = daemon_run_summary(
            layout,
            DaemonRequestKind::WorkflowStatus {
                run_id: Some(run_id),
            },
            json,
        )?;
        emit_summary(&summary, json);
    } else {
        let summaries = daemon_run_list(layout, limit, json)?;
        emit_list(&summaries, json);
    }
    Ok(())
}

/// Bounded event history after a resume cursor.
#[cfg(unix)]
fn run_events(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
    args: crate::cli::workflow::WorkflowEventsArgs,
) -> Result<()> {
    let crate::cli::workflow::WorkflowEventsArgs {
        run_id,
        after,
        limit,
        json,
    } = args;
    let detail = daemon_run_detail(layout, &run_id, after, limit, json)?;
    emit_events(&detail, after, json);
    Ok(())
}

/// Resolve the exact recorded session for a run worker.
#[cfg(unix)]
fn run_attach(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
    args: crate::cli::workflow::WorkflowAttachArgs,
) -> Result<()> {
    let crate::cli::workflow::WorkflowAttachArgs {
        run_id,
        worker,
        observe,
        control,
        json,
    } = args;
    let detail = daemon_run_detail(layout, &run_id, -1, 0, json)?;
    emit_attach(&detail, &worker, observe, control, json)
}

/// Markdown export of the run detail (default content class: no
/// terminal artifacts).
#[cfg(unix)]
fn run_export(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
    args: crate::cli::workflow::WorkflowExportArgs,
) -> Result<()> {
    let crate::cli::workflow::WorkflowExportArgs {
        run_id,
        out,
        include_terminal,
        json,
    } = args;
    let detail = daemon_run_detail(layout, &run_id, -1, 500, json)?;
    if include_terminal {
        // The CLI has no retained spool to read: terminal artifacts
        // need explicit content selection from a retained source,
        // and inventing transcript content would be fabrication.
        return Err(fail_err(
            "invalid_request",
            format!(
                "no retained terminal text is available for run `{run_id}` from the CLI; export without --include-terminal"
            ),
            json,
        ));
    }
    let manifest = crate::app::workflow_export::write_report(&detail, None, &out)?;
    emit_export(&manifest, json);
    Ok(())
}

#[cfg(not(unix))]
fn run_control_command(
    _command: WorkflowCommand,
    _config: &jackin_config::AppConfig,
    _paths: &jackin_core::JackinPaths,
) -> Result<()> {
    anyhow::bail!("workflow run control needs the host daemon socket (unix only)")
}

/// Read and digest-verify a stored plan before starting a run for its
/// hash, so `run` never starts an unknown or edited plan.
#[cfg(unix)]
fn read_stored_plan(
    paths: &jackin_core::JackinPaths,
    plan_hash: &str,
) -> Result<jackin_workflow_spec::model::ExecutionPlan> {
    use jackin_workflow_spec::store::InputStore;

    let store = InputStore::open(&paths.data_dir.join("workflow-plans")).map_err(|error| {
        anyhow::anyhow!("cannot open the workflow plan store: {}", render(error))
    })?;
    store
        .read_plan(plan_hash)
        .map_err(|error| anyhow::anyhow!("unknown plan hash `{plan_hash}`: {}", render(error)))
}

#[cfg(unix)]
fn daemon_request(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
    kind: jackin_runtime::host_daemon::DaemonRequestKind,
    json: bool,
) -> Result<jackin_runtime::host_daemon::DaemonResponseKind> {
    use jackin_runtime::host_daemon::request;

    let response = request(&layout.socket_path, env!("JACKIN_VERSION"), kind).map_err(|error| {
        fail_err(
            "execution_error",
            format!("workflow daemon transport failed: {error:#}"),
            json,
        )
    })?;
    Ok(response.kind)
}

#[cfg(unix)]
fn daemon_run_summary(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
    kind: jackin_runtime::host_daemon::DaemonRequestKind,
    json: bool,
) -> Result<jackin_workflow::RunSummary> {
    use jackin_runtime::host_daemon::DaemonResponseKind;

    match daemon_request(layout, kind, json)? {
        DaemonResponseKind::WorkflowRun { summary } => Ok(summary),
        DaemonResponseKind::Error { message, code } => Err(fail_err(&code, message, json)),
        other => Err(fail_err(
            "execution_error",
            format!("unexpected daemon response: {other:?}"),
            json,
        )),
    }
}

#[cfg(unix)]
fn daemon_run_list(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
    limit: u32,
    json: bool,
) -> Result<Vec<jackin_workflow::RunSummary>> {
    use jackin_runtime::host_daemon::{DaemonRequestKind, DaemonResponseKind};

    match daemon_request(layout, DaemonRequestKind::WorkflowList { limit }, json)? {
        DaemonResponseKind::WorkflowRuns { summaries } => Ok(summaries),
        DaemonResponseKind::Error { message, code } => Err(fail_err(&code, message, json)),
        other => Err(fail_err(
            "execution_error",
            format!("unexpected daemon response: {other:?}"),
            json,
        )),
    }
}

#[cfg(unix)]
fn daemon_run_detail(
    layout: &jackin_runtime::host_daemon::DaemonLayout,
    run_id: &str,
    after_seq: i64,
    event_limit: u32,
    json: bool,
) -> Result<jackin_workflow::RunDetail> {
    use jackin_runtime::host_daemon::{DaemonRequestKind, DaemonResponseKind};

    match daemon_request(
        layout,
        DaemonRequestKind::WorkflowDetail {
            run_id: run_id.to_owned(),
            after_seq,
            event_limit,
        },
        json,
    )? {
        DaemonResponseKind::WorkflowRunDetail { detail } => Ok(detail),
        DaemonResponseKind::Error { message, code } => Err(fail_err(&code, message, json)),
        other => Err(fail_err(
            "execution_error",
            format!("unexpected daemon response: {other:?}"),
            json,
        )),
    }
}

/// Build the typed CLI failure for a daemon error code: the C7 exit
/// code plus a stable JSON error envelope (`--json`) or a human
/// stderr line. Callers return it so `main` exits 0/2/3/4/1 through
/// [`crate::cli::workflow::WorkflowCommandExit`] instead of the
/// generic 1.
#[cfg(unix)]
fn fail_err(code: &str, message: String, json: bool) -> anyhow::Error {
    use crate::cli::workflow::{WorkflowCommandExit, exit_code_for_daemon_code};

    let exit_code = exit_code_for_daemon_code(code);
    if json {
        let payload = crate::cli::format::OutputEnvelope::v1(serde_json::json!({
            "error": { "code": code, "message": message },
        }));
        let text = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_owned());
        WorkflowCommandExit::new(exit_code, text).into()
    } else {
        eprintln!("error [{code}]: {message}");
        WorkflowCommandExit::new(exit_code, String::new()).into()
    }
}

#[cfg(unix)]
fn emit_envelope<T: serde::Serialize>(data: T) {
    let envelope = crate::cli::format::OutputEnvelope::v1(data);
    match serde_json::to_string(&envelope) {
        Ok(text) => println!("{text}"),
        Err(error) => eprintln!("error [execution_error]: cannot render workflow JSON: {error}"),
    }
}

#[cfg(unix)]
fn emit_summary(summary: &jackin_workflow::RunSummary, json: bool) {
    if json {
        emit_envelope(serde_json::json!({ "run": summary }));
        return;
    }
    println!("run: {}", summary.run_id);
    println!("plan: {}", summary.plan_hash);
    if summary.status == "paused" {
        println!(
            "status: paused({})",
            summary.pause_kind.as_deref().unwrap_or("operator")
        );
    } else {
        println!("status: {}", summary.status);
    }
    println!("stage: {}", summary.stage);
    println!("round: {}/{}", summary.round, summary.max_repair_turns);
    println!(
        "candidate: {}",
        summary.candidate.as_deref().unwrap_or("none")
    );
    println!("version: {}", summary.version);
    println!("owner_epoch: {}", summary.owner_epoch);
}

#[cfg(unix)]
fn emit_list(summaries: &[jackin_workflow::RunSummary], json: bool) {
    if json {
        emit_envelope(serde_json::json!({ "count": summaries.len(), "runs": summaries }));
        return;
    }
    if summaries.is_empty() {
        println!("no workflow runs");
        return;
    }
    for summary in summaries {
        println!(
            "{} {} {} {}",
            summary.run_id, summary.status, summary.stage, summary.plan_hash
        );
    }
}

#[cfg(unix)]
fn emit_graph(detail: &jackin_workflow::RunDetail, json: bool) {
    let stages: Vec<serde_json::Value> = jackin_workflow::GRAPH_STAGES
        .iter()
        .map(|stage| {
            serde_json::json!({
                "stage": stage,
                "state": jackin_workflow::graph_node_state(stage, &detail.summary.stage),
            })
        })
        .collect();
    if json {
        emit_envelope(serde_json::json!({
            "run_id": detail.summary.run_id,
            "stage": detail.summary.stage,
            "status": detail.summary.status,
            "stages": stages,
        }));
        return;
    }
    println!("run: {}", detail.summary.run_id);
    for stage in &stages {
        println!(
            "{}: {}",
            stage["stage"].as_str().unwrap_or("?"),
            stage["state"].as_str().unwrap_or("?")
        );
    }
}

#[cfg(unix)]
fn emit_events(detail: &jackin_workflow::RunDetail, after: i64, json: bool) {
    if json {
        emit_envelope(serde_json::json!({
            "run_id": detail.summary.run_id,
            "after": after,
            "events": detail.events,
        }));
        return;
    }
    if detail.events.is_empty() {
        println!("no events after {after} for {}", detail.summary.run_id);
        return;
    }
    for event in &detail.events {
        println!("{} {} by {}", event.seq, event.kind, event.actor);
    }
}

#[cfg(unix)]
fn emit_attach(
    detail: &jackin_workflow::RunDetail,
    worker: &str,
    observe: bool,
    control: bool,
    json: bool,
) -> Result<()> {
    // Observe is the default: it grants no input ownership.
    // `--control` only resolves the same exact session in control
    // mode; ownership itself is taken through the Console action.
    let _ = observe;
    let mode = if control { "control" } else { "observe" };
    let Some(session) = detail.sessions.iter().find(|s| s.worker_id == worker) else {
        return Err(fail_err(
            "not_found",
            format!(
                "run `{}` has no recorded session for worker `{worker}`",
                detail.summary.run_id
            ),
            json,
        ));
    };
    if json {
        emit_envelope(serde_json::json!({
            "run_id": detail.summary.run_id,
            "worker": worker,
            "mode": mode,
            "session": session,
        }));
        return Ok(());
    }
    println!("run: {}", detail.summary.run_id);
    println!("worker: {worker}");
    println!("mode: {mode}");
    println!(
        "instance: {}",
        session.instance_id.as_deref().unwrap_or("none")
    );
    println!(
        "session: {}",
        session
            .session_id
            .map_or_else(|| "none".to_owned(), |id| id.to_string())
    );
    println!(
        "capsule_epoch: {}",
        session
            .capsule_epoch
            .map_or_else(|| "none".to_owned(), |epoch| epoch.to_string())
    );
    println!(
        "process_identity: {}",
        session.process_identity.as_deref().unwrap_or("none")
    );
    if control {
        println!(
            "note: take exclusive input ownership through the Console takeover action, then release it before resuming"
        );
    }
    Ok(())
}

#[cfg(unix)]
fn emit_export(manifest: &crate::app::workflow_export::WorkflowExportManifest, json: bool) {
    if json {
        emit_envelope(manifest);
        return;
    }
    println!("run: {}", manifest.run_id);
    println!("report: {}", manifest.report_path.display());
}

struct LoadedDefinition {
    checked: jackin_workflow_spec::rules::CheckedDefinition,
    file_label: String,
    workspace_field: String,
    module_digests: BTreeMap<String, String>,
}

fn load_definition(file: &Path) -> Result<LoadedDefinition, WorkflowError> {
    use jackin_workflow_spec::eval::{EvalLimits, FrozenBundle, evaluate_bounded};
    use jackin_workflow_spec::rules::validate_definition;

    let file_label = file.display().to_string();
    let dir = file.parent().unwrap_or_else(|| Path::new("."));
    let schema_path = dir.join("Workflow.pkl");
    let schema = std::fs::read_to_string(&schema_path).map_err(|_| {
        missing(
            &file_label,
            "schema",
            format!("cannot read schema `{}`", schema_path.display()),
            "keep Workflow.pkl next to the workflow file",
        )
    })?;
    let workflow = std::fs::read_to_string(file).map_err(|_| {
        missing(
            &file_label,
            "definition",
            format!("cannot read workflow file `{}`", file.display()),
            "pass the repair-workflow definition file",
        )
    })?;
    let bundle = FrozenBundle::from_texts(
        "/bundle/repair.pkl",
        [
            ("/bundle/Workflow.pkl", schema),
            ("/bundle/repair.pkl", workflow),
        ],
    )?;
    let Some(helper) = jackin_workflow_spec::eval::resolve_helper_binary() else {
        return Err(missing(
            &file_label,
            "evaluator",
            "evaluation helper not installed next to this jackin binary".to_owned(),
            "reinstall jackin so jackin-workflow-eval sits next to it",
        ));
    };
    let evaluated = evaluate_bounded(&bundle, &file_label, &helper, &EvalLimits::defaults())?;
    let checked = validate_definition(&evaluated.json, &file_label)?;
    let workspace_field = evaluated
        .json
        .get("workspace")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("current")
        .to_owned();
    Ok(LoadedDefinition {
        checked,
        file_label,
        workspace_field,
        module_digests: evaluated.module_digests,
    })
}

fn missing(file_label: &str, field: &str, message: String, action: &str) -> WorkflowError {
    WorkflowError::single(Diagnostic::new(
        file_label,
        field,
        ErrorCode::MissingInput,
        message,
        action,
    ))
}

fn render(error: WorkflowError) -> anyhow::Error {
    let lines: Vec<String> = error
        .diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{}:{} [{}] {} (action: {})",
                diagnostic.file,
                diagnostic.field,
                diagnostic.code,
                diagnostic.message,
                diagnostic.action
            )
        })
        .collect();
    anyhow::anyhow!("{}", lines.join("\n"))
}

fn validate(args: &WorkflowFileArgs) -> Result<()> {
    match load_definition(&args.file) {
        Ok(_) => {
            println!("valid: {}", args.file.display());
            Ok(())
        }
        Err(error) => {
            println!("invalid: {}", args.file.display());
            Err(render(error))
        }
    }
}

fn plan(
    args: &WorkflowPlanArgs,
    config: &jackin_config::AppConfig,
    paths: &jackin_core::JackinPaths,
) -> Result<()> {
    let outcome = plan_inner(&args.file, config, paths, args.model.as_deref());
    match outcome {
        Ok(summary) => {
            if args.json {
                println!("{}", summary.json);
            } else {
                print!("{}", summary.human);
            }
            Ok(())
        }
        Err(error) => {
            if args.json {
                print_diagnostics_json(&error);
            }
            Err(render(error))
        }
    }
}

struct PlanSummary {
    json: serde_json::Value,
    human: String,
}

fn plan_inner(
    file: &Path,
    config: &jackin_config::AppConfig,
    paths: &jackin_core::JackinPaths,
    model_override: Option<&str>,
) -> Result<PlanSummary, WorkflowError> {
    use jackin_workflow_spec::resolve::{
        FileResolution, resolve_live_selections, resolve_workflow_files,
        resolve_workspace_repository,
    };
    use jackin_workflow_spec::rules::{HostGrant, compile_plan};
    use jackin_workflow_spec::store::InputStore;

    let loaded = load_definition(file)?;
    let file_label = loaded.file_label.as_str();
    let dir = file.parent().unwrap_or_else(|| Path::new("."));

    let (workspace_name, workspace) =
        resolve_workspace(config, &loaded.workspace_field, file_label)?;
    let workspace_key = jackin_core::WorkspaceName::parse(&workspace_name).map_err(|error| {
        missing(
            file_label,
            "workspace",
            format!("saved workspace `{workspace_name}` is not a valid name: {error}"),
            "repair the workspace configuration",
        )
    })?;
    let role_id = workspace
        .default_role
        .clone()
        .or_else(|| workspace.last_role.clone())
        .ok_or_else(|| {
            missing(
                file_label,
                "role",
                format!("workspace `{workspace_name}` has no role selected"),
                "run `jackin load <role>` to select a role for the workspace",
            )
        })?;
    if !config.roles.contains_key(&role_id) {
        return Err(missing(
            file_label,
            "role",
            format!("role `{role_id}` is not in the role registry"),
            "add the role or select a registered one",
        ));
    }
    let role_revision = role_manifest_revision(paths, &role_id, file_label)?;
    let image_digest = role_image_digest(&role_id, file_label)?;

    let mut implementer = live_agent_facts(
        config,
        &workspace_key,
        &role_id,
        jackin_core::Agent::Claude,
        "implementer",
        &image_digest,
        file_label,
    )?;
    implementer.requested_model = apply_model_override(implementer.requested_model, model_override);
    let reviewer = live_agent_facts(
        config,
        &workspace_key,
        &role_id,
        jackin_core::Agent::Codex,
        "reviewer",
        &image_digest,
        file_label,
    )?;
    let env = LiveSelectionEnv {
        workspace_name: workspace_name.clone(),
        role_id: role_id.clone(),
        role_revision,
        implementer,
        reviewer,
    };
    let selections = resolve_live_selections(file_label, &env)?;
    let repository = resolve_workspace_repository(
        &workspace_repository_candidates(workspace),
        file_label,
        WORKSPACE_REPOSITORY_ID,
        &workspace_name,
    )?;
    let resolution = FileResolution {
        workflow_file: file.to_path_buf(),
        file_label: file_label.to_owned(),
        check_bundle_dir: dir.join("checks"),
        check_image_digest: image_digest,
        module_digests: loaded.module_digests,
        repository,
        selections,
    };
    let inputs = resolve_workflow_files(&resolution, &loaded.checked)?;
    let grant = HostGrant {
        profile: loaded.checked.sandbox_profile.clone(),
        backend: loaded.checked.sandbox_backend.clone(),
        max_cpu_millicores: 16_000,
        max_memory_bytes: 68_719_476_736,
        max_pids: 4096,
        max_run_seconds: 86_400,
        max_disk_monitor_bytes: 1_099_511_627_776,
        authorized_repository_ids: vec![WORKSPACE_REPOSITORY_ID.to_owned()],
        authorized_remote_ids: Vec::new(),
    };
    let plan = compile_plan(&loaded.checked, &inputs, &grant, file_label)?;
    let store = InputStore::open(&paths.data_dir.join("workflow-plans")).map_err(|_| {
        missing(
            file_label,
            "store",
            "cannot open the workflow plan store".to_owned(),
            "make the jackin data directory writable",
        )
    })?;
    let frozen: Vec<&jackin_workflow_spec::resolve::FrozenInput> = std::iter::once(&inputs.task)
        .chain(inputs.references.iter())
        .chain(inputs.test_bundle.iter())
        .collect();
    // Re-planning identical inputs must stay stable: the store marks
    // plan files read-only, so re-verify the stored digest instead of
    // rewriting identical bytes.
    let stored_hash = if store_plan_path(paths, &plan.plan_hash).is_file() {
        store.read_plan(&plan.plan_hash)?.plan_hash
    } else {
        store.persist(&plan, &frozen)?
    };
    debug_assert_eq!(stored_hash, plan.plan_hash);
    Ok(summarize(
        &plan,
        &workspace_name,
        &role_id,
        &store_plan_path(paths, &stored_hash),
        &stored_hash,
    ))
}

/// Resolve the workspace named by the definition, defaulting to the
/// saved workspace matching the current directory.
fn resolve_workspace<'config>(
    config: &'config jackin_config::AppConfig,
    workspace_field: &str,
    file_label: &str,
) -> Result<(String, &'config jackin_config::WorkspaceConfig), WorkflowError> {
    if workspace_field != "current" {
        return config
            .workspaces
            .get(workspace_field)
            .map(|workspace| (workspace_field.to_owned(), workspace))
            .ok_or_else(|| {
                missing(
                    file_label,
                    "workspace",
                    format!("workspace `{workspace_field}` is not configured"),
                    "select an existing saved workspace",
                )
            });
    }
    let cwd = std::env::current_dir().map_err(|_| {
        missing(
            file_label,
            "workspace",
            "cannot determine the current directory".to_owned(),
            "run plan from inside the fixture workspace",
        )
    })?;
    jackin_config::find_saved_workspace_for_cwd(config, &cwd)
        .map(|(name, workspace)| (name.to_owned(), workspace))
        .ok_or_else(|| {
            missing(
                file_label,
                "workspace",
                format!("no saved workspace matches {}", cwd.display()),
                "run plan from inside the fixture workspace directory",
            )
        })
}

/// Ordered absolute host-path candidates for the workspace repository:
/// the workspace workdir when it is an absolute host path, then each
/// mount source in order. The workflow file's own directory is never a
/// candidate: Pkl source `workspace` means the workspace repository,
/// not the checkout the definition file happens to live in.
fn workspace_repository_candidates(workspace: &jackin_config::WorkspaceConfig) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let workdir = jackin_config::expand_tilde(&workspace.workdir);
    if Path::new(&workdir).is_absolute() {
        candidates.push(PathBuf::from(workdir));
    }
    for mount in &workspace.mounts {
        let src = jackin_config::expand_tilde(&mount.src);
        if Path::new(&src).is_absolute() {
            candidates.push(PathBuf::from(src));
        }
    }
    candidates
}

/// Read the cached role manifest and hash it into the exact role
/// revision frozen in the plan.
fn role_manifest_revision(
    paths: &jackin_core::JackinPaths,
    role_id: &str,
    file_label: &str,
) -> Result<String, WorkflowError> {
    let selector = jackin_core::RoleSelector::parse(role_id).map_err(|_| {
        missing(
            file_label,
            "role",
            format!("role `{role_id}` is not a valid role selector"),
            "select a registered role for the workspace",
        )
    })?;
    // Same layout as the cached role checkouts the loader populates.
    let mut role_dir = match &selector.namespace {
        Some(namespace) => paths.roles_dir.join(namespace).join(&selector.name),
        None => paths.roles_dir.join(&selector.name),
    };
    role_dir.push("default");
    let manifest_path = role_dir.join(jackin_core::MANIFEST_FILENAME);
    let hex = jackin_image::binary_artifact::hash_file_sha256(&manifest_path).map_err(|_| {
        missing(
            file_label,
            "role.revision",
            format!("cannot read role manifest `{}`", manifest_path.display()),
            "refresh the cached role checkout and retry",
        )
    })?;
    Ok(hex)
}

/// Resolve the single local image digest for the role repository
/// `jk_<role>`. Zero images is a missing input; more than one
/// distinct digest is ambiguous. Both workers share the workspace
/// role image in this release, and the test bundle runs in it too.
fn role_image_digest(role_id: &str, file_label: &str) -> Result<String, WorkflowError> {
    let selector = jackin_core::RoleSelector::parse(role_id).map_err(|_| {
        missing(
            file_label,
            "reviewer.image",
            format!("role `{role_id}` is not a valid role selector"),
            "select a registered role for the workspace",
        )
    })?;
    let repository = format!("jk_{}", selector.name);
    let request = jackin_process::ExecRequest::new(
        "docker",
        [
            "images",
            "--digests",
            "--format",
            "{{.Digest}}",
            repository.as_str(),
        ],
    )
    .timeout(std::time::Duration::from_secs(30));
    let outcome = jackin_process::exec_sync(&request).map_err(|_| {
        missing(
            file_label,
            "reviewer.image",
            "cannot list local docker images".to_owned(),
            "start the docker daemon and retry",
        )
    })?;
    if !outcome.success {
        return Err(missing(
            file_label,
            "reviewer.image",
            format!("cannot list local docker images for `{repository}`"),
            "start the docker daemon and retry",
        ));
    }
    let mut digests: Vec<String> = String::from_utf8_lossy(&outcome.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "<none>")
        .map(str::to_owned)
        .collect();
    digests.sort();
    digests.dedup();
    match digests.as_slice() {
        [digest] => Ok(digest.clone()),
        [] => Err(missing(
            file_label,
            "reviewer.image",
            format!("no local image found for role `{role_id}` ({repository})"),
            "build or pull the role image and retry",
        )),
        _ => Err(missing(
            file_label,
            "reviewer.image",
            format!("multiple image digests found for role `{role_id}` ({repository})"),
            "leave exactly one role image tag and retry",
        )),
    }
}

/// Gather one agent's live facts from the existing selection: the
/// account binding (claude/codex), its auth mode, the requested
/// model, and the agent binary digest. Identifiers and digests only;
/// secret values are never read.
/// Operator `--model` precedence for the frozen requested model: a
/// non-blank plan-time override wins; otherwise the resolved role or
/// account default stands. Only the implementer (Claude) side takes
/// an override in this release; the reviewer keeps its resolved model.
fn apply_model_override(resolved: Option<String>, model_override: Option<&str>) -> Option<String> {
    match model_override.map(str::trim) {
        Some(override_model) if !override_model.is_empty() => Some(override_model.to_owned()),
        _ => resolved,
    }
}

fn live_agent_facts(
    config: &jackin_config::AppConfig,
    workspace: &jackin_core::WorkspaceName,
    role_id: &str,
    agent: jackin_core::Agent,
    side: &str,
    image_digest: &str,
    file_label: &str,
) -> Result<LiveAgentFacts, WorkflowError> {
    let account = jackin_config::resolve_account(config, agent, Some(workspace), role_id)
        .map_err(|error| {
            missing(
                file_label,
                &format!("{side}.account"),
                format!("cannot resolve the {side} account: {error}"),
                &format!("assign one {agent} account to the workspace"),
            )
        })?
        .ok_or_else(|| {
            missing(
                file_label,
                &format!("{side}.account"),
                format!("no {agent} account is selected for this workspace"),
                &format!("assign one {agent} account to the workspace"),
            )
        })?;
    let account_id = config
        .accounts
        .iter()
        .find_map(|(id, candidate)| std::ptr::eq(candidate, account).then(|| id.clone()))
        .with_context(|| format!("resolved {side} account is not in the account registry"))
        .map_err(|error| {
            missing(
                file_label,
                &format!("{side}.account"),
                format!("{error:#}"),
                "repair the account registry",
            )
        })?;
    let auth_mode = match &account.credential {
        jackin_config::AccountCredential::Profile { .. } => "profile",
        jackin_config::AccountCredential::ApiKey { .. } => "api_key",
        jackin_config::AccountCredential::OAuthToken { .. } => "oauth_token",
    }
    .to_owned();
    let resolved_model = config
        .agent_configurations
        .values()
        .filter(|configuration| configuration.agent == agent && configuration.account == account_id)
        .find_map(|configuration| configuration.model.clone())
        .or_else(|| match &account.credential {
            jackin_config::AccountCredential::ApiKey { model, .. } => model.clone(),
            _ => None,
        });
    let requested_model = resolved_model.or(Some(VENDOR_DEFAULT_MODEL.to_owned()));
    let binary_name = agent.slug();
    let binary_digest = agent_binary_digest(binary_name, side, file_label)?;
    Ok(LiveAgentFacts {
        runtime: binary_name.to_owned(),
        agent_slug: binary_name.to_owned(),
        account_id,
        auth_mode,
        requested_model,
        actual_model: None,
        binary_digest,
        image_digest: image_digest.to_owned(),
    })
}

/// Hash the agent binary found on `PATH` into `sha256:<hex>`.
fn agent_binary_digest(
    binary: &str,
    side: &str,
    file_label: &str,
) -> Result<String, WorkflowError> {
    let path = std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(binary))
                .find(|candidate| candidate.is_file())
        })
        .ok_or_else(|| {
            missing(
                file_label,
                &format!("{side}.binary"),
                format!("agent binary `{binary}` not found on PATH"),
                &format!("install `{binary}` and retry"),
            )
        })?;
    join_digest(&path, side, file_label, binary)
}

fn join_digest(
    path: &Path,
    side: &str,
    file_label: &str,
    binary: &str,
) -> Result<String, WorkflowError> {
    let hex = jackin_image::binary_artifact::hash_file_sha256(path).map_err(|_| {
        missing(
            file_label,
            &format!("{side}.binary"),
            format!("cannot hash agent binary `{}`", path.display()),
            &format!("reinstall `{binary}` and retry"),
        )
    })?;
    Ok(format!("sha256:{hex}"))
}

fn store_plan_path(paths: &jackin_core::JackinPaths, hash: &str) -> PathBuf {
    paths
        .data_dir
        .join("workflow-plans")
        .join("plans")
        .join(format!("{hash}.plan.json"))
}

fn summarize(
    plan: &jackin_workflow_spec::model::ExecutionPlan,
    workspace: &str,
    role: &str,
    stored: &Path,
    hash: &str,
) -> PlanSummary {
    let json = serde_json::json!({
        "plan_hash": plan.plan_hash,
        "template": plan.template,
        "schema_version": plan.schema_version,
        "workspace": workspace,
        "role": role,
        "nodes": plan.nodes.len(),
        "max_repair_turns": plan.max_repair_turns,
        "implementer": {
            "account": plan.implementer.account_id,
            "requested_model": plan.implementer.requested_model,
        },
        "reviewer": {
            "account": plan.reviewer.account_id,
            "requested_model": plan.reviewer.requested_model,
        },
        "repository": {
            "id": plan.repository.repository_id,
            "base_commit": plan.repository.base_commit,
        },
        "test_bundle": plan.verification.test_bundle,
        "stored": stored.display().to_string(),
    });
    let human = format!(
        "plan: {hash}\n\
         template: {} ({} nodes, max_repair_turns {})\n\
         workspace: {workspace} role: {role}\n\
         implementer: {} {} ({})\n\
         reviewer: {} {} ({})\n\
         repository: {} {}\n\
         stored: {}\n",
        plan.template,
        plan.nodes.len(),
        plan.max_repair_turns,
        plan.implementer.runtime,
        plan.implementer.account_id,
        plan.implementer.requested_model,
        plan.reviewer.runtime,
        plan.reviewer.account_id,
        plan.reviewer.requested_model,
        plan.repository.repository_id,
        plan.repository.base_commit,
        stored.display(),
    );
    PlanSummary { json, human }
}

fn print_diagnostics_json(error: &WorkflowError) {
    let diagnostics: Vec<serde_json::Value> = error
        .diagnostics
        .iter()
        .map(|diagnostic| {
            serde_json::json!({
                "file": diagnostic.file,
                "field": diagnostic.field,
                "code": diagnostic.code.as_str(),
                "message": diagnostic.message,
                "action": diagnostic.action,
            })
        })
        .collect();
    println!("{}", serde_json::json!({ "diagnostics": diagnostics }));
}

#[cfg(test)]
mod tests;
