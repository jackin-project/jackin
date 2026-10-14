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
    }
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
    let outcome = plan_inner(&args.file, config, paths);
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

    let implementer = live_agent_facts(
        config,
        &workspace_key,
        &role_id,
        jackin_core::Agent::Claude,
        "implementer",
        &image_digest,
        file_label,
    )?;
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
    let requested_model = config
        .agent_configurations
        .values()
        .filter(|configuration| configuration.agent == agent && configuration.account == account_id)
        .find_map(|configuration| configuration.model.clone())
        .or_else(|| match &account.credential {
            jackin_config::AccountCredential::ApiKey { model, .. } => model.clone(),
            _ => None,
        })
        .or(Some(VENDOR_DEFAULT_MODEL.to_owned()));
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
