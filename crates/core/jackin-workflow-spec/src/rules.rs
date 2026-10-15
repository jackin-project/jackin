// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Definition validation and plan compilation.
//!
//! [`validate_definition`] turns evaluated JSON into a typed
//! [`CheckedDefinition`]. Pkl enforces declaration-site constraints,
//! but this crate re-checks every range in Rust: the selected
//! evaluator does not re-check constraints on amendments, and a
//! missing amended module is silently dropped, so the Rust layer owns
//! required fields, ranges, the closed template, and the cross-field
//! role rules. [`compile_plan`] freezes those checks together with
//! resolved inputs and host facts into an [`ExecutionPlan`].

use std::collections::BTreeMap;

use serde_json::Value;

use crate::digest::finalize_plan;
use crate::error::{Diagnostic, ErrorCode, WorkflowError, WorkflowResult};
use crate::eval::PKLR_VERSION;
use crate::model::{
    AgentSelection, BackupPolicy, CapabilityFacts, ExecutionPlan, InputFile, PlanNode,
    REPAIR_STAGES, ResourceLimits, SCHEMA_VERSION, TEMPLATE_REPAIR_V1, TestBundleFile,
    VerificationPolicy,
};
use crate::resolve::ResolvedInputs;

/// Schema-scale bounds mirrored from the Pkl schema so amendments are
/// checked in Rust even when the evaluator skips them.
const MAX_CPU_MILLICORES: u64 = 16_000;
const MIN_MEMORY_BYTES: u64 = 268_435_456;
const MAX_MEMORY_BYTES: u64 = 68_719_476_736;
const MAX_PIDS: u64 = 4096;
const MAX_RUN_SECONDS: u64 = 86_400;
const MAX_DISK_MONITOR_BYTES: u64 = 1_099_511_627_776;
const MAX_CHECK_TIMEOUT_SECONDS: u64 = 3600;
const MAX_REPAIR_TURNS: u32 = 2;

/// Top-level properties the repair template accepts. Anything else is
/// an extra field and rejected.
const KNOWN_TOP_LEVEL: &[&str] = &[
    "schemaVersion",
    "template",
    "workspace",
    "repository",
    "task",
    "references",
    "implementer",
    "reviewer",
    "checks",
    "sandbox",
    "backup",
    "maxRepairTurns",
];

/// Checked workflow definition: every field present and in range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedDefinition {
    /// Task file path relative to the workflow file's directory.
    pub task: String,
    /// Reference paths relative to the workflow file's directory.
    pub references: Vec<String>,
    /// Implementer runtime; always `claude` after validation.
    pub implementer_runtime: String,
    /// Implementer selection name; always `workspace-default`.
    pub implementer_selection: String,
    /// Implementer delegation mode.
    pub implementer_vendor_subagents: String,
    /// Reviewer runtime; always `codex` after validation.
    pub reviewer_runtime: String,
    /// Reviewer selection name; always `workspace-default`.
    pub reviewer_selection: String,
    /// Reviewer delegation mode.
    pub reviewer_vendor_subagents: String,
    /// Trusted test bundle name.
    pub checks_bundle: String,
    /// Exact check argv.
    pub checks_argv: Vec<String>,
    /// Check timeout in seconds.
    pub checks_timeout: u64,
    /// Sandbox backend; always `docker` after validation.
    pub sandbox_backend: String,
    /// Sandbox profile name.
    pub sandbox_profile: String,
    /// Requested resource limits.
    pub resources: ResourceLimits,
    /// Whether a hard disk limit is requested (unsupported).
    pub require_hard_disk_limit: bool,
    /// Backup mode.
    pub backup_mode: String,
    /// Backup remote ID, if any.
    pub backup_remote: Option<String>,
    /// Bounded repair turns.
    pub max_repair_turns: u32,
}

/// Host authorization and capability facts for admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostGrant {
    /// Sandbox profile the host grants.
    pub profile: String,
    /// Sandbox backend the host grants.
    pub backend: String,
    /// Granted CPU ceiling in millicores.
    pub max_cpu_millicores: u64,
    /// Granted memory ceiling in bytes.
    pub max_memory_bytes: u64,
    /// Granted process ceiling.
    pub max_pids: u32,
    /// Granted run time ceiling in seconds.
    pub max_run_seconds: u64,
    /// Granted disk monitor ceiling in bytes.
    pub max_disk_monitor_bytes: u64,
    /// Repository IDs the host authorizes.
    pub authorized_repository_ids: Vec<String>,
    /// Backup remote IDs the host authorizes (identifiers only).
    pub authorized_remote_ids: Vec<String>,
}

/// Resolved agent selections for both workers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selections {
    /// Implementer selection.
    pub implementer: AgentSelection,
    /// Reviewer selection.
    pub reviewer: AgentSelection,
}

struct HeadParts {
    task: String,
    references: Vec<String>,
}

struct AgentParts {
    implementer_runtime: String,
    implementer_selection: String,
    implementer_vendor_subagents: String,
    reviewer_runtime: String,
    reviewer_selection: String,
    reviewer_vendor_subagents: String,
}

struct ChecksParts {
    bundle: String,
    argv: Vec<String>,
    timeout: u64,
}

struct SandboxParts {
    backend: String,
    profile: String,
    resources: ResourceLimits,
    require_hard_disk_limit: bool,
}

struct BackupParts {
    mode: String,
    remote: Option<String>,
}

/// Validate evaluated JSON into a [`CheckedDefinition`].
pub fn validate_definition(json: &Value, file_label: &str) -> WorkflowResult<CheckedDefinition> {
    reject_extra_fields(json, file_label)?;
    let head = validate_head(json, file_label)?;
    let agents = validate_agents(json, file_label)?;
    let checks = validate_checks(json, file_label)?;
    let sandbox = validate_sandbox(json, file_label)?;
    let backup = validate_backup(json, file_label)?;
    let max_repair_turns = ranged_u64(
        get(json, file_label, "", "maxRepairTurns")?,
        file_label,
        "maxRepairTurns",
        0,
        u64::from(MAX_REPAIR_TURNS),
        "set maxRepairTurns between 0 and 2",
    )? as u32;
    Ok(CheckedDefinition {
        task: head.task,
        references: head.references,
        implementer_runtime: agents.implementer_runtime,
        implementer_selection: agents.implementer_selection,
        implementer_vendor_subagents: agents.implementer_vendor_subagents,
        reviewer_runtime: agents.reviewer_runtime,
        reviewer_selection: agents.reviewer_selection,
        reviewer_vendor_subagents: agents.reviewer_vendor_subagents,
        checks_bundle: checks.bundle,
        checks_argv: checks.argv,
        checks_timeout: checks.timeout,
        sandbox_backend: sandbox.backend,
        sandbox_profile: sandbox.profile,
        resources: sandbox.resources,
        require_hard_disk_limit: sandbox.require_hard_disk_limit,
        backup_mode: backup.mode,
        backup_remote: backup.remote,
        max_repair_turns,
    })
}

fn reject_extra_fields(json: &Value, file_label: &str) -> WorkflowResult<()> {
    let root = as_object(json, file_label, "")?;
    for key in root.keys() {
        if !KNOWN_TOP_LEVEL.contains(&key.as_str()) {
            return Err(single(
                file_label,
                key,
                ErrorCode::InvalidDefinition,
                format!("unknown field `{key}`"),
                "remove the field or use the repair-v1 template fields only",
            ));
        }
    }
    Ok(())
}

fn validate_head(json: &Value, file_label: &str) -> WorkflowResult<HeadParts> {
    let schema_version = as_u64(
        get(json, file_label, "", "schemaVersion")?,
        file_label,
        "schemaVersion",
    )?;
    if schema_version != u64::from(SCHEMA_VERSION) {
        return Err(single(
            file_label,
            "schemaVersion",
            ErrorCode::InvalidDefinition,
            format!("unsupported schema version {schema_version}"),
            "use schemaVersion 1",
        ));
    }
    let template = as_string(
        get(json, file_label, "", "template")?,
        file_label,
        "template",
    )?;
    if template != TEMPLATE_REPAIR_V1 {
        return Err(single(
            file_label,
            "template",
            ErrorCode::InvalidDefinition,
            format!("unknown template `{template}`"),
            "use the repair-v1 template; it is the only template in this release",
        ));
    }
    let workspace = as_string(
        get(json, file_label, "", "workspace")?,
        file_label,
        "workspace",
    )?;
    if workspace != "current" {
        return Err(single(
            file_label,
            "workspace",
            ErrorCode::InvalidDefinition,
            format!("unsupported workspace selector `{workspace}`"),
            "select the current workspace",
        ));
    }
    let repository = get(json, file_label, "", "repository")?;
    let repo_source = as_string(
        get(repository, file_label, "repository", "source")?,
        file_label,
        "repository.source",
    )?;
    if repo_source != "workspace" {
        return Err(single(
            file_label,
            "repository.source",
            ErrorCode::InvalidDefinition,
            format!("unsupported repository source `{repo_source}`"),
            "use the workspace repository source",
        ));
    }
    let task = as_string(get(json, file_label, "", "task")?, file_label, "task")?;
    if task.is_empty() {
        return Err(single(
            file_label,
            "task",
            ErrorCode::InvalidDefinition,
            "task path is empty",
            "point task at the frozen task file",
        ));
    }
    let references = as_string_list(
        get(json, file_label, "", "references")?,
        file_label,
        "references",
    )?;
    Ok(HeadParts { task, references })
}

fn validate_agents(json: &Value, file_label: &str) -> WorkflowResult<AgentParts> {
    let implementer = get(json, file_label, "", "implementer")?;
    let reviewer = get(json, file_label, "", "reviewer")?;
    Ok(AgentParts {
        implementer_runtime: agent_runtime(implementer, file_label, "implementer", "claude")?,
        implementer_selection: agent_selection_name(implementer, file_label, "implementer")?,
        implementer_vendor_subagents: agent_vendor_mode(implementer, file_label, "implementer")?,
        reviewer_runtime: agent_runtime(reviewer, file_label, "reviewer", "codex")?,
        reviewer_selection: agent_selection_name(reviewer, file_label, "reviewer")?,
        reviewer_vendor_subagents: agent_vendor_mode(reviewer, file_label, "reviewer")?,
    })
}

fn validate_checks(json: &Value, file_label: &str) -> WorkflowResult<ChecksParts> {
    let checks = get(json, file_label, "", "checks")?;
    let bundle = as_string(
        get(checks, file_label, "checks", "bundle")?,
        file_label,
        "checks.bundle",
    )?;
    if bundle.is_empty() {
        return Err(single(
            file_label,
            "checks.bundle",
            ErrorCode::InvalidDefinition,
            "check bundle name is empty",
            "name the frozen trusted test bundle",
        ));
    }
    let argv = as_string_list(
        get(checks, file_label, "checks", "argv")?,
        file_label,
        "checks.argv",
    )?;
    if argv.is_empty() {
        return Err(single(
            file_label,
            "checks.argv",
            ErrorCode::InvalidDefinition,
            "check argv is empty",
            "provide the exact check command argv",
        ));
    }
    let timeout = ranged_u64(
        get(checks, file_label, "checks", "timeoutSeconds")?,
        file_label,
        "checks.timeoutSeconds",
        1,
        MAX_CHECK_TIMEOUT_SECONDS,
        "set checks.timeoutSeconds between 1 and 3600",
    )?;
    Ok(ChecksParts {
        bundle,
        argv,
        timeout,
    })
}

fn validate_sandbox(json: &Value, file_label: &str) -> WorkflowResult<SandboxParts> {
    let sandbox = get(json, file_label, "", "sandbox")?;
    let backend = as_string(
        get(sandbox, file_label, "sandbox", "backend")?,
        file_label,
        "sandbox.backend",
    )?;
    if backend != "docker" {
        return Err(single(
            file_label,
            "sandbox.backend",
            ErrorCode::InvalidDefinition,
            format!("unsupported sandbox backend `{backend}`"),
            "use the docker backend",
        ));
    }
    let profile = as_string(
        get(sandbox, file_label, "sandbox", "profile")?,
        file_label,
        "sandbox.profile",
    )?;
    let resources = ResourceLimits {
        cpu_millicores: ranged_u64(
            get(sandbox, file_label, "sandbox", "cpuMillicores")?,
            file_label,
            "sandbox.cpuMillicores",
            1,
            MAX_CPU_MILLICORES,
            "set sandbox.cpuMillicores between 1 and 16000",
        )?,
        memory_bytes: ranged_u64(
            get(sandbox, file_label, "sandbox", "memoryBytes")?,
            file_label,
            "sandbox.memoryBytes",
            MIN_MEMORY_BYTES,
            MAX_MEMORY_BYTES,
            "set sandbox.memoryBytes between 256MiB and 64GiB",
        )?,
        max_pids: ranged_u64(
            get(sandbox, file_label, "sandbox", "maxPids")?,
            file_label,
            "sandbox.maxPids",
            1,
            MAX_PIDS,
            "set sandbox.maxPids between 1 and 4096",
        )? as u32,
        max_run_seconds: ranged_u64(
            get(sandbox, file_label, "sandbox", "maxRunSeconds")?,
            file_label,
            "sandbox.maxRunSeconds",
            1,
            MAX_RUN_SECONDS,
            "set sandbox.maxRunSeconds between 1 and 86400",
        )?,
        disk_monitor_bytes: ranged_u64(
            get(sandbox, file_label, "sandbox", "diskMonitorBytes")?,
            file_label,
            "sandbox.diskMonitorBytes",
            1,
            MAX_DISK_MONITOR_BYTES,
            "set sandbox.diskMonitorBytes between 1 and 1TiB",
        )?,
    };
    let require_hard_disk_limit = as_bool(
        get(sandbox, file_label, "sandbox", "requireHardDiskLimit")?,
        file_label,
        "sandbox.requireHardDiskLimit",
    )?;
    Ok(SandboxParts {
        backend,
        profile,
        resources,
        require_hard_disk_limit,
    })
}

fn validate_backup(json: &Value, file_label: &str) -> WorkflowResult<BackupParts> {
    let backup = get(json, file_label, "", "backup")?;
    let mode = as_string(
        get(backup, file_label, "backup", "mode")?,
        file_label,
        "backup.mode",
    )?;
    if !["off", "best_effort", "required"].contains(&mode.as_str()) {
        return Err(single(
            file_label,
            "backup.mode",
            ErrorCode::InvalidDefinition,
            format!("unsupported backup mode `{mode}`"),
            "use off, best_effort, or required",
        ));
    }
    let remote = match get(backup, file_label, "backup", "remote")? {
        Value::Null => None,
        Value::String(remote) => Some(remote.clone()),
        _ => {
            return Err(single(
                file_label,
                "backup.remote",
                ErrorCode::InvalidDefinition,
                "backup.remote must be a string or null",
                "set an authorized remote ID or null",
            ));
        }
    };
    Ok(BackupParts { mode, remote })
}

/// Compile a checked definition plus resolved inputs into a hashed plan.
pub fn compile_plan(
    checked: &CheckedDefinition,
    inputs: &ResolvedInputs,
    grant: &HostGrant,
    file_label: &str,
) -> WorkflowResult<ExecutionPlan> {
    if inputs.selections.implementer.runtime != checked.implementer_runtime {
        return Err(single(
            file_label,
            "implementer",
            ErrorCode::InvalidDefinition,
            "implementer selection does not match the definition runtime",
            "resolve the implementer selection for the definition runtime",
        ));
    }
    if inputs.selections.reviewer.runtime != checked.reviewer_runtime {
        return Err(single(
            file_label,
            "reviewer",
            ErrorCode::InvalidDefinition,
            "reviewer selection does not match the definition runtime",
            "resolve the reviewer selection for the definition runtime",
        ));
    }
    for (field, mode) in [
        (
            "implementer.vendorSubagents",
            checked.implementer_vendor_subagents.as_str(),
        ),
        (
            "reviewer.vendorSubagents",
            checked.reviewer_vendor_subagents.as_str(),
        ),
    ] {
        if mode == "forbid" {
            return Err(single(
                file_label,
                field,
                ErrorCode::UnsupportedCapability,
                "forbidding delegation has no adapter enforcement in this release",
                "use within_worker delegation",
            ));
        }
    }
    if checked.require_hard_disk_limit {
        return Err(single(
            file_label,
            "sandbox.requireHardDiskLimit",
            ErrorCode::UnsupportedCapability,
            "hard disk quotas are not enforced by the selected runtime",
            "leave requireHardDiskLimit false and rely on disk monitoring",
        ));
    }
    check_grant_maxima(checked, grant, file_label)?;
    if !grant
        .authorized_repository_ids
        .contains(&inputs.repository.repository_id)
    {
        return Err(single(
            file_label,
            "repository",
            ErrorCode::AuthorizationDenied,
            format!(
                "repository `{}` is not authorized",
                inputs.repository.repository_id
            ),
            "authorize the repository or pick an authorized one",
        ));
    }
    let backup = backup_policy(checked, grant, file_label)?;
    let input_digests = plan_input_digests(inputs);
    let mut remote_ids = grant.authorized_remote_ids.clone();
    remote_ids.sort();
    let plan = ExecutionPlan {
        schema_version: SCHEMA_VERSION,
        template: TEMPLATE_REPAIR_V1.to_owned(),
        nodes: repair_nodes(checked.max_repair_turns),
        implementer: inputs.selections.implementer.clone(),
        reviewer: inputs.selections.reviewer.clone(),
        task: InputFile {
            logical_path: inputs.task.logical_path.clone(),
            content_digest: inputs.task.digest.clone(),
            byte_length: inputs.task.bytes.len() as u64,
            mount_path: format!("/work/task/{}", inputs.task.logical_path),
        },
        references: inputs
            .references
            .iter()
            .map(|reference| InputFile {
                logical_path: reference.logical_path.clone(),
                content_digest: reference.digest.clone(),
                byte_length: reference.bytes.len() as u64,
                mount_path: format!("/work/references/{}", reference.logical_path),
            })
            .collect(),
        repository: inputs.repository.clone(),
        resources: checked.resources.clone(),
        verification: VerificationPolicy {
            test_bundle: inputs.test_bundle_name.clone(),
            test_bundle_digest: inputs.test_bundle_digest.clone(),
            test_bundle_files: inputs
                .test_bundle
                .iter()
                .map(|file| TestBundleFile {
                    logical_path: file.logical_path.clone(),
                    digest: file.digest.clone(),
                })
                .collect(),
            check_argv: checked.checks_argv.clone(),
            check_image_digest: inputs.check_image_digest.clone(),
            timeout_seconds: checked.checks_timeout,
            review_schema_version: 1,
        },
        backup,
        max_repair_turns: checked.max_repair_turns,
        input_digests,
        capability_facts: CapabilityFacts {
            sandbox_backend: checked.sandbox_backend.clone(),
            sandbox_profile: checked.sandbox_profile.clone(),
            authorized_remote_ids: remote_ids,
            evaluator: format!("pklr {PKLR_VERSION}"),
            schema_version: SCHEMA_VERSION,
        },
        plan_hash: String::new(),
    };
    finalize_plan(plan)
}

fn backup_policy(
    checked: &CheckedDefinition,
    grant: &HostGrant,
    file_label: &str,
) -> WorkflowResult<BackupPolicy> {
    match (
        checked.backup_mode.as_str(),
        checked.backup_remote.as_deref(),
    ) {
        ("off", None) => Ok(BackupPolicy {
            mode: "off".to_owned(),
            remote_id: None,
            credential_ref: None,
            ref_prefix: None,
        }),
        ("off", Some(_)) => Err(single(
            file_label,
            "backup.remote",
            ErrorCode::InvalidDefinition,
            "backup is off but a remote is set",
            "remove the remote or set a non-off backup mode",
        )),
        (mode, Some(remote)) => {
            if !grant.authorized_remote_ids.contains(&remote.to_owned()) {
                return Err(single(
                    file_label,
                    "backup.remote",
                    ErrorCode::AuthorizationDenied,
                    format!("backup remote `{remote}` is not authorized"),
                    "authorize the remote or pick an authorized one",
                ));
            }
            Ok(BackupPolicy {
                mode: mode.to_owned(),
                remote_id: Some(remote.to_owned()),
                credential_ref: None,
                ref_prefix: None,
            })
        }
        (_, None) => Err(single(
            file_label,
            "backup.remote",
            ErrorCode::InvalidDefinition,
            "non-off backup mode needs a remote",
            "set an authorized backup remote ID",
        )),
    }
}

fn plan_input_digests(inputs: &ResolvedInputs) -> BTreeMap<String, String> {
    let mut digests = BTreeMap::new();
    digests.insert(
        format!("task/{}", inputs.task.logical_path),
        inputs.task.digest.clone(),
    );
    for reference in &inputs.references {
        digests.insert(
            format!("references/{}", reference.logical_path),
            reference.digest.clone(),
        );
    }
    digests.insert(
        format!("checks/{}", inputs.test_bundle_name),
        inputs.test_bundle_digest.clone(),
    );
    digests.insert(
        "repository/manifest".to_owned(),
        inputs.repository.manifest_digest.clone(),
    );
    for (name, digest) in &inputs.module_digests {
        digests.insert(format!("pkl/{name}"), digest.clone());
    }
    digests
}

/// Closed node sequence for a repair limit: `prepare` once, then every
/// stage per round from zero through the limit.
#[must_use]
pub fn repair_nodes(max_repair_turns: u32) -> Vec<PlanNode> {
    let mut nodes = vec![PlanNode {
        id: "prepare@r0".to_owned(),
        stage: "prepare".to_owned(),
        round: 0,
    }];
    for round in 0..=max_repair_turns {
        for stage in REPAIR_STAGES.iter().skip(1) {
            nodes.push(PlanNode {
                id: format!("{stage}@r{round}"),
                stage: (*stage).to_owned(),
                round,
            });
        }
    }
    nodes
}

fn check_grant_maxima(
    checked: &CheckedDefinition,
    grant: &HostGrant,
    file_label: &str,
) -> WorkflowResult<()> {
    let requested = &checked.resources;
    let over = [
        (
            "sandbox.cpuMillicores",
            requested.cpu_millicores,
            grant.max_cpu_millicores,
        ),
        (
            "sandbox.memoryBytes",
            requested.memory_bytes,
            grant.max_memory_bytes,
        ),
        (
            "sandbox.maxPids",
            u64::from(requested.max_pids),
            u64::from(grant.max_pids),
        ),
        (
            "sandbox.maxRunSeconds",
            requested.max_run_seconds,
            grant.max_run_seconds,
        ),
        (
            "sandbox.diskMonitorBytes",
            requested.disk_monitor_bytes,
            grant.max_disk_monitor_bytes,
        ),
    ]
    .into_iter()
    .find(|(_, want, have)| want > have);
    if let Some((field, want, have)) = over {
        return Err(single(
            file_label,
            field,
            ErrorCode::InvalidDefinition,
            format!("requested {want} exceeds the host grant maximum {have}"),
            "lower the request or broaden the host grant and recompile",
        ));
    }
    Ok(())
}

fn single(
    file: &str,
    field: &str,
    code: ErrorCode,
    message: impl Into<String>,
    action: impl Into<String>,
) -> WorkflowError {
    WorkflowError::single(Diagnostic::new(file, field, code, message, action))
}

fn get<'a>(value: &'a Value, file: &str, parent: &str, key: &str) -> WorkflowResult<&'a Value> {
    let field = if parent.is_empty() {
        key.to_owned()
    } else {
        format!("{parent}.{key}")
    };
    value.get(key).ok_or_else(|| {
        single(
            file,
            &field,
            ErrorCode::InvalidDefinition,
            format!("missing required field `{field}`"),
            format!("add `{field}` to the workflow definition"),
        )
    })
}

fn as_object<'a>(
    value: &'a Value,
    file: &str,
    field: &str,
) -> WorkflowResult<&'a serde_json::Map<String, Value>> {
    value.as_object().ok_or_else(|| {
        single(
            file,
            field,
            ErrorCode::InvalidDefinition,
            format!("field `{field}` must be an object"),
            "fix the field shape",
        )
    })
}

fn as_string(value: &Value, file: &str, field: &str) -> WorkflowResult<String> {
    value.as_str().map(str::to_owned).ok_or_else(|| {
        single(
            file,
            field,
            ErrorCode::InvalidDefinition,
            format!("field `{field}` must be a string"),
            "fix the field shape",
        )
    })
}

fn as_bool(value: &Value, file: &str, field: &str) -> WorkflowResult<bool> {
    value.as_bool().ok_or_else(|| {
        single(
            file,
            field,
            ErrorCode::InvalidDefinition,
            format!("field `{field}` must be a boolean"),
            "fix the field shape",
        )
    })
}

fn as_u64(value: &Value, file: &str, field: &str) -> WorkflowResult<u64> {
    match value {
        Value::Number(number) => number.as_u64().ok_or_else(|| {
            single(
                file,
                field,
                ErrorCode::InvalidDefinition,
                format!("field `{field}` must be a non-negative integer"),
                "fix the field shape",
            )
        }),
        _ => Err(single(
            file,
            field,
            ErrorCode::InvalidDefinition,
            format!("field `{field}` must be a non-negative integer"),
            "fix the field shape",
        )),
    }
}

fn ranged_u64(
    value: &Value,
    file: &str,
    field: &str,
    min: u64,
    max: u64,
    action: &str,
) -> WorkflowResult<u64> {
    let found = as_u64(value, file, field)?;
    if found < min || found > max {
        return Err(single(
            file,
            field,
            ErrorCode::InvalidDefinition,
            format!("field `{field}` value {found} is outside {min}..={max}"),
            action,
        ));
    }
    Ok(found)
}

fn as_string_list(value: &Value, file: &str, field: &str) -> WorkflowResult<Vec<String>> {
    let items = value.as_array().ok_or_else(|| {
        single(
            file,
            field,
            ErrorCode::InvalidDefinition,
            format!("field `{field}` must be a list of strings"),
            "fix the field shape",
        )
    })?;
    items
        .iter()
        .map(|item| as_string(item, file, field))
        .collect()
}

fn agent_runtime(agent: &Value, file: &str, field: &str, want: &str) -> WorkflowResult<String> {
    let runtime = as_string(
        get(agent, file, field, "runtime")?,
        file,
        &format!("{field}.runtime"),
    )?;
    if runtime != want {
        return Err(single(
            file,
            &format!("{field}.runtime"),
            ErrorCode::InvalidDefinition,
            format!("{field} runtime must be {want} in this release"),
            format!("set {field} runtime to {want}"),
        ));
    }
    Ok(runtime)
}

fn agent_selection_name(agent: &Value, file: &str, field: &str) -> WorkflowResult<String> {
    let selection = as_string(
        get(agent, file, field, "selection")?,
        file,
        &format!("{field}.selection"),
    )?;
    if selection != "workspace-default" {
        return Err(single(
            file,
            &format!("{field}.selection"),
            ErrorCode::InvalidDefinition,
            format!("{field} selection must be workspace-default in this release"),
            "resolve the selection from the existing workspace configuration",
        ));
    }
    Ok(selection)
}

fn agent_vendor_mode(agent: &Value, file: &str, field: &str) -> WorkflowResult<String> {
    let mode = as_string(
        get(agent, file, field, "vendorSubagents")?,
        file,
        &format!("{field}.vendorSubagents"),
    )?;
    if !["within_worker", "forbid"].contains(&mode.as_str()) {
        return Err(single(
            file,
            &format!("{field}.vendorSubagents"),
            ErrorCode::InvalidDefinition,
            format!("unknown delegation mode `{mode}`"),
            "use within_worker or forbid",
        ));
    }
    Ok(mode)
}
