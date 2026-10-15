// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::unwrap_used,
    reason = "integration tests: fail-fast fixtures and host-side blocking helpers"
)]

//! K3: table-driven workflow plan checks.
//!
//! Rows cover one valid plan, an invalid repair limit, denied reads,
//! a changed input hash, post-compile edit immutability, plus one
//! regression row per defect found during implementation:
//! - the evaluator skips `isBetween` on amendments (Rust re-checks),
//! - a missing amended module is silently dropped (Rust requires
//!   every field),
//! - union members need a declared default (schema carries defaults,
//!   Rust still enforces the role pairing).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use jackin_workflow_spec::digest::canonical_plan_bytes;
use jackin_workflow_spec::error::ErrorCode;
use jackin_workflow_spec::eval::{EvalLimits, FrozenBundle, evaluate_bounded};
use jackin_workflow_spec::model::RepositoryInput;
use jackin_workflow_spec::resolve::{
    FrozenInput, LiveAgentFacts, LiveSelectionEnv, ResolvedInputs, resolve_live_selections,
    resolve_repository, resolve_workflow_files, resolve_workspace_repository,
};
use jackin_workflow_spec::rules::{
    CheckedDefinition, HostGrant, Selections, compile_plan, repair_nodes, validate_definition,
};
use jackin_workflow_spec::store::InputStore;

const SCHEMA_VIRTUAL: &str = "/bundle/Workflow.pkl";
const REPAIR_VIRTUAL: &str = "/bundle/repair.pkl";

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../examples/workflows")
}

fn read_example(name: &str) -> Vec<u8> {
    std::fs::read(examples_dir().join(name)).unwrap()
}

fn helper_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_jackin-workflow-eval"))
}

fn fixture_bundle(repair_text: &str) -> FrozenBundle {
    let schema = String::from_utf8(read_example("Workflow.pkl")).unwrap();
    FrozenBundle::from_texts(
        REPAIR_VIRTUAL,
        [
            (SCHEMA_VIRTUAL, schema),
            (REPAIR_VIRTUAL, repair_text.to_owned()),
        ],
    )
    .unwrap()
}

fn fixture_repair_text() -> String {
    String::from_utf8(read_example("repair.pkl")).unwrap()
}

fn fixture_grant() -> HostGrant {
    HostGrant {
        profile: "workflow-local".to_owned(),
        backend: "docker".to_owned(),
        max_cpu_millicores: 16_000,
        max_memory_bytes: 68_719_476_736,
        max_pids: 4096,
        max_run_seconds: 86_400,
        max_disk_monitor_bytes: 1_099_511_627_776,
        authorized_repository_ids: vec!["workspace:current".to_owned()],
        authorized_remote_ids: vec!["test-remote".to_owned()],
    }
}

fn fixture_selections() -> Selections {
    use jackin_workflow_spec::model::AgentSelection;
    Selections {
        implementer: AgentSelection {
            runtime: "claude".to_owned(),
            agent_slug: "claude".to_owned(),
            role_id: "agent-smith".to_owned(),
            role_revision: "rev-1".to_owned(),
            account_id: "test-account-claude".to_owned(),
            auth_mode: "api_key".to_owned(),
            requested_model: "claude-test".to_owned(),
            actual_model: None,
            binary_digest: "sha256:fixture-claude-bin".to_owned(),
            image_digest: "sha256:fixture-claude-img".to_owned(),
        },
        reviewer: AgentSelection {
            runtime: "codex".to_owned(),
            agent_slug: "codex".to_owned(),
            role_id: "default-codex".to_owned(),
            role_revision: "rev-1".to_owned(),
            account_id: "test-account-codex".to_owned(),
            auth_mode: "api_key".to_owned(),
            requested_model: "codex-test".to_owned(),
            actual_model: None,
            binary_digest: "sha256:fixture-codex-bin".to_owned(),
            image_digest: "sha256:fixture-codex-img".to_owned(),
        },
    }
}

/// Copy the example workflow layout into a scratch dir and init a git
/// repo with one commit, so `HEAD` resolution has something exact.
fn scratch_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    for name in [
        "repair.pkl",
        "Workflow.pkl",
        "task.md",
        "requirements.md",
        "calculator.py",
    ] {
        std::fs::write(root.join(name), read_example(name)).unwrap();
    }
    std::fs::create_dir(root.join("checks")).unwrap();
    std::fs::write(
        root.join("checks/check_add.py"),
        read_example("checks/check_add.py"),
    )
    .unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "fixture"]);
    dir
}

fn git_request(dir: &Path, args: &[&str]) -> jackin_process::ExecRequest {
    jackin_process::ExecRequest::new("git", args)
        .cwd(dir)
        .timeout(Duration::from_secs(30))
        .envs([
            ("GIT_AUTHOR_NAME", "workflow-test"),
            ("GIT_AUTHOR_EMAIL", "workflow-test@example.invalid"),
            ("GIT_COMMITTER_NAME", "workflow-test"),
            ("GIT_COMMITTER_EMAIL", "workflow-test@example.invalid"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
        ])
}

fn git(dir: &Path, args: &[&str]) {
    let result = jackin_process::exec_sync(&git_request(dir, args));
    let success = result.is_ok_and(|outcome| outcome.success);
    assert!(success, "git {args:?} failed in test setup");
}

fn git_head(dir: &Path) -> String {
    let result = jackin_process::exec_sync(&git_request(dir, &["rev-parse", "HEAD"]));
    match result {
        Ok(outcome) if outcome.success => {
            String::from_utf8_lossy(&outcome.stdout).trim().to_owned()
        }
        _ => {
            assert_eq!("git-head", "ok", "git rev-parse HEAD failed in test setup");
            String::new()
        }
    }
}

struct Pipeline {
    checked: CheckedDefinition,
    inputs: ResolvedInputs,
    plan_hash: String,
}

/// Full pipeline from fixture texts: evaluate, validate, resolve,
/// compile. The repository lives in a scratch git repo.
fn evaluate_fixture(
    repair_text: &str,
    limits: &EvalLimits,
) -> jackin_workflow_spec::error::WorkflowResult<jackin_workflow_spec::eval::EvaluatedDefinition> {
    let bundle = fixture_bundle(repair_text);
    evaluate_bounded(&bundle, "repair.pkl", &helper_binary(), limits)
}

fn compile_inputs(
    repo: &Path,
    evaluated: &jackin_workflow_spec::eval::EvaluatedDefinition,
    selections: &Selections,
) -> (CheckedDefinition, ResolvedInputs) {
    use jackin_workflow_spec::resolve::FileResolution;
    let checked = validate_definition(&evaluated.json, "repair.pkl").unwrap();
    let repository = resolve_repository(repo, "repair.pkl", "workspace:current").unwrap();
    let resolution = FileResolution {
        workflow_file: repo.join("repair.pkl"),
        file_label: "repair.pkl".to_owned(),
        check_bundle_dir: repo.join("checks"),
        check_image_digest: "sha256:fixture-check-image".to_owned(),
        module_digests: evaluated.module_digests.clone(),
        repository,
        selections: selections.clone(),
    };
    let inputs = resolve_workflow_files(&resolution, &checked).unwrap();
    (checked, inputs)
}

fn compile_in_repo(
    repo: &Path,
    repair_text: &str,
    grant: &HostGrant,
    selections: &Selections,
) -> (
    CheckedDefinition,
    ResolvedInputs,
    jackin_workflow_spec::model::ExecutionPlan,
) {
    let evaluated = evaluate_fixture(repair_text, &EvalLimits::defaults()).unwrap();
    assert!(evaluated.env_reads.is_empty());
    let (checked, inputs) = compile_inputs(repo, &evaluated, selections);
    let plan = compile_plan(&checked, &inputs, grant, "repair.pkl").unwrap();
    (checked, inputs, plan)
}

/// Full pipeline from fixture texts: evaluate, validate, resolve,
/// compile. The repository lives in a scratch git repo.
fn run_pipeline(
    repair_text: &str,
    grant: &HostGrant,
    selections: &Selections,
    task_bytes: Option<&[u8]>,
) -> (Pipeline, tempfile::TempDir) {
    let scratch = scratch_repo();
    let repo = scratch.path();
    if let Some(bytes) = task_bytes {
        std::fs::write(repo.join("task.md"), bytes).unwrap();
    }
    let (checked, inputs, plan) = compile_in_repo(repo, repair_text, grant, selections);
    (
        Pipeline {
            plan_hash: plan.plan_hash.clone(),
            checked,
            inputs,
        },
        scratch,
    )
}

fn expect_error<T>(
    result: jackin_workflow_spec::error::WorkflowResult<T>,
    code: ErrorCode,
    field: &str,
    message: &str,
) -> jackin_workflow_spec::error::WorkflowError {
    let Err(error) = result else {
        assert_eq!(
            code.as_str(),
            "unexpected-success",
            "expected {code} at {field}, got success"
        );
        return jackin_workflow_spec::error::WorkflowError::single(
            jackin_workflow_spec::error::Diagnostic::new(
                "",
                "",
                code,
                "unreachable",
                "unreachable",
            ),
        );
    };
    assert_eq!(error.primary_code(), code);
    assert!(
        error
            .diagnostics
            .iter()
            .any(|found| found.field.contains(field)),
        "field {field} missing in {error:?}"
    );
    if !message.is_empty() {
        assert!(
            error
                .diagnostics
                .iter()
                .any(|found| found.message.contains(message)),
            "message `{message}` missing in {error:?}"
        );
    }
    error
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Eval,
    Validate,
    Compile,
}

struct InvalidCase {
    name: &'static str,
    repair_extra: &'static str,
    stage: Stage,
    code: ErrorCode,
    field: &'static str,
    message: &'static str,
    grant: fn() -> HostGrant,
}

const INVALID_CASES: &[InvalidCase] = &[
    InvalidCase {
        name: "repair-limit",
        repair_extra: "maxRepairTurns = 5\n",
        stage: Stage::Validate,
        code: ErrorCode::InvalidDefinition,
        field: "maxRepairTurns",
        message: "outside 0..=2",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "denied-file-read",
        repair_extra: "task = read(\"file:///etc/hostname\")\n",
        stage: Stage::Eval,
        code: ErrorCode::InvalidDefinition,
        field: "",
        message: "Refusing to read resource `file:///etc/hostname`",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "denied-env-read",
        repair_extra: "task = read(\"env:HOME\")\n",
        stage: Stage::Eval,
        code: ErrorCode::InvalidDefinition,
        field: "",
        message: "Refusing to read resource `env:HOME`",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "denied-remote-import",
        repair_extra: "",
        stage: Stage::Eval,
        code: ErrorCode::InvalidDefinition,
        field: "",
        message: "offline mode",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "missing-base-module",
        repair_extra: "",
        stage: Stage::Validate,
        code: ErrorCode::InvalidDefinition,
        field: "schemaVersion",
        message: "missing required field",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "implementer-role",
        repair_extra: "implementer { runtime = \"codex\" }\n",
        stage: Stage::Validate,
        code: ErrorCode::InvalidDefinition,
        field: "implementer.runtime",
        message: "must be claude",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "reviewer-role",
        repair_extra: "reviewer { runtime = \"claude\" }\n",
        stage: Stage::Validate,
        code: ErrorCode::InvalidDefinition,
        field: "reviewer.runtime",
        message: "must be codex",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "empty-check-argv",
        repair_extra: "checks { bundle = \"x\" argv = new {} }\n",
        stage: Stage::Validate,
        code: ErrorCode::InvalidDefinition,
        field: "checks.argv",
        message: "empty",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "unknown-template",
        repair_extra: "template = \"graph-v9\"\n",
        stage: Stage::Validate,
        code: ErrorCode::InvalidDefinition,
        field: "template",
        message: "unknown template",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "extra-field",
        repair_extra: "frobnicate = 1\n",
        stage: Stage::Validate,
        code: ErrorCode::InvalidDefinition,
        field: "frobnicate",
        message: "unknown field",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "hard-disk-quota",
        repair_extra: "sandbox { requireHardDiskLimit = true }\n",
        stage: Stage::Compile,
        code: ErrorCode::UnsupportedCapability,
        field: "requireHardDiskLimit",
        message: "not enforced",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "forbid-delegation",
        repair_extra: "implementer { vendorSubagents = \"forbid\" }\n",
        stage: Stage::Compile,
        code: ErrorCode::UnsupportedCapability,
        field: "vendorSubagents",
        message: "no adapter enforcement",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "backup-without-remote",
        repair_extra: "backup { mode = \"required\" }\n",
        stage: Stage::Compile,
        code: ErrorCode::InvalidDefinition,
        field: "backup.remote",
        message: "needs a remote",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "backup-remote-unauthorized",
        repair_extra: "backup { mode = \"required\" remote = \"evil\" }\n",
        stage: Stage::Compile,
        code: ErrorCode::AuthorizationDenied,
        field: "backup.remote",
        message: "not authorized",
        grant: fixture_grant,
    },
    InvalidCase {
        name: "grant-maxima",
        repair_extra: "",
        stage: Stage::Compile,
        code: ErrorCode::InvalidDefinition,
        field: "sandbox.memoryBytes",
        message: "exceeds the host grant",
        grant: low_memory_grant,
    },
];

fn low_memory_grant() -> HostGrant {
    let mut grant = fixture_grant();
    grant.max_memory_bytes = 268_435_456;
    grant
}

/// Replace one top-level `name { ... }` block, matching braces so the
/// replacement keeps the module valid.
fn replace_block(base: &str, name: &str, replacement: &str) -> String {
    let marker = format!("\n{name} ");
    let Some(start) = base.find(marker.as_str()) else {
        return format!("{base}{replacement}");
    };
    let mut end = base.len();
    let mut level = 0;
    for (offset, byte) in base
        .char_indices()
        .skip_while(|(offset, _)| *offset < start)
    {
        if byte == '{' {
            level += 1;
        }
        if byte == '}' {
            level -= 1;
            if level == 0 {
                end = offset + 1;
                break;
            }
        }
    }
    let mut out = base[..start].to_owned();
    out.push('\n');
    out.push_str(replacement.trim_end());
    out.push('\n');
    out.push_str(base[end..].trim_start_matches('\n'));
    out
}

fn repair_for(case: &InvalidCase) -> String {
    if case.name == "denied-remote-import" {
        return "amends \"https://example.com/Workflow.pkl\"\n\ntask = \"task.md\"\nreferences = new {}\nchecks { bundle = \"x\" argv = new { \"a\" } }\n"
            .to_owned();
    }
    if case.name == "missing-base-module" {
        return "amends \"Nope.pkl\"\n\ntask = \"task.md\"\nreferences = new {}\nchecks { bundle = \"x\" argv = new { \"a\" } }\n"
            .to_owned();
    }
    if case.name == "denied-file-read" || case.name == "denied-env-read" {
        let base = fixture_repair_text();
        let task_line = "task = \"task.md\"\n";
        let replacement = case.repair_extra;
        return base.replacen(task_line, replacement, 1);
    }
    let base = fixture_repair_text();
    for block in ["checks", "backup", "sandbox", "implementer", "reviewer"] {
        if case.repair_extra.starts_with(&format!("{block} ")) {
            return replace_block(&base, block, case.repair_extra);
        }
    }
    format!("{base}{}", case.repair_extra)
}

#[test]
fn workflow_invalid_table() {
    for case in INVALID_CASES {
        let repair = repair_for(case);
        let evaluated = evaluate_fixture(&repair, &EvalLimits::defaults());
        if case.stage == Stage::Eval {
            expect_error(evaluated, case.code, case.field, case.message);
            continue;
        }
        let json = evaluated.unwrap().json;
        let checked = validate_definition(&json, "repair.pkl");
        if case.stage == Stage::Validate {
            expect_error(checked, case.code, case.field, case.message);
            continue;
        }
        let checked = checked.unwrap();
        let scratch = scratch_repo();
        let repository =
            resolve_repository(scratch.path(), "repair.pkl", "workspace:current").unwrap();
        let resolution = jackin_workflow_spec::resolve::FileResolution {
            workflow_file: scratch.path().join("repair.pkl"),
            file_label: "repair.pkl".to_owned(),
            check_bundle_dir: scratch.path().join("checks"),
            check_image_digest: "sha256:fixture-check-image".to_owned(),
            module_digests: BTreeMap::new(),
            repository,
            selections: fixture_selections(),
        };
        let inputs = resolve_workflow_files(&resolution, &checked).unwrap();
        expect_error(
            compile_plan(&checked, &inputs, &(case.grant)(), "repair.pkl"),
            case.code,
            case.field,
            case.message,
        );
    }
}

#[test]
fn workflow_valid_plan() {
    let (pipeline, _scratch) = run_pipeline(
        &fixture_repair_text(),
        &fixture_grant(),
        &fixture_selections(),
        None,
    );
    assert_eq!(pipeline.checked.max_repair_turns, 2);
    assert_eq!(pipeline.inputs.task.logical_path, "task.md");
    assert_eq!(pipeline.inputs.references.len(), 1);
    assert_eq!(pipeline.inputs.test_bundle_name, "fixture-checks");
    assert!(!pipeline.plan_hash.is_empty());

    let expected_nodes = repair_nodes(2);
    assert_eq!(expected_nodes.len(), 16);
    assert_eq!(
        expected_nodes.first().map(|node| node.id.as_str()),
        Some("prepare@r0")
    );
    assert_eq!(
        expected_nodes.get(1).map(|node| node.id.as_str()),
        Some("implement@r0")
    );
    assert_eq!(
        expected_nodes.last().map(|node| node.id.as_str()),
        Some("decide@r2")
    );

    let task_bytes = read_example("task.md");
    assert_eq!(pipeline.inputs.task.bytes, task_bytes);
    assert_eq!(
        pipeline.inputs.task.digest,
        jackin_workflow_spec::digest::sha256_hex(&task_bytes)
    );

    let evaluated = evaluate_fixture(&fixture_repair_text(), &EvalLimits::defaults()).unwrap();
    for name in evaluated.module_digests.keys() {
        assert!(
            pipeline.inputs.module_digests.contains_key(name),
            "module {name} missing from plan inputs"
        );
    }

    let store_dir = tempfile::tempdir().unwrap();
    let store = InputStore::open(store_dir.path()).unwrap();
    let stored: Vec<&FrozenInput> = std::iter::once(&pipeline.inputs.task)
        .chain(pipeline.inputs.references.iter())
        .chain(pipeline.inputs.test_bundle.iter())
        .collect();
    let scratch = scratch_repo();
    let (_checked, _inputs, plan) = compile_in_repo(
        scratch.path(),
        &fixture_repair_text(),
        &fixture_grant(),
        &fixture_selections(),
    );
    let (_checked_again, _, plan_again) = compile_in_repo(
        scratch.path(),
        &fixture_repair_text(),
        &fixture_grant(),
        &fixture_selections(),
    );
    assert_eq!(plan.plan_hash, plan_again.plan_hash);
    assert_eq!(
        canonical_plan_bytes(&plan).unwrap(),
        canonical_plan_bytes(&plan_again).unwrap()
    );
    let stored_hash = store.persist(&plan, &stored).unwrap();
    assert_eq!(stored_hash, plan.plan_hash);
    let reread = store.read_plan(&stored_hash).unwrap();
    assert_eq!(reread.plan_hash, plan.plan_hash);
    let canonical = canonical_plan_bytes(&plan).unwrap();
    let canonical_again = canonical_plan_bytes(&reread).unwrap();
    assert_eq!(canonical, canonical_again);
    let task_copy = store.read_input(&pipeline.inputs.task.digest).unwrap();
    assert_eq!(task_copy, task_bytes);
}

#[test]
fn workflow_changed_input_hash() {
    let (first, _kept) = run_pipeline(
        &fixture_repair_text(),
        &fixture_grant(),
        &fixture_selections(),
        None,
    );
    let (second, _kept) = run_pipeline(
        &fixture_repair_text(),
        &fixture_grant(),
        &fixture_selections(),
        Some(b"changed task bytes"),
    );
    assert_ne!(first.plan_hash, second.plan_hash);
    assert_ne!(first.inputs.task.digest, second.inputs.task.digest);
}

#[test]
fn workflow_post_compile_edit_unchanged() {
    let scratch = scratch_repo();
    let repo = scratch.path();
    let (_checked, inputs, plan) = compile_in_repo(
        repo,
        &fixture_repair_text(),
        &fixture_grant(),
        &fixture_selections(),
    );
    let store_dir = tempfile::tempdir().unwrap();
    let store = InputStore::open(store_dir.path()).unwrap();
    let stored: Vec<&FrozenInput> = std::iter::once(&inputs.task)
        .chain(inputs.references.iter())
        .chain(inputs.test_bundle.iter())
        .collect();
    let hash = store.persist(&plan, &stored).unwrap();

    let edited_repair = format!("{}{}", fixture_repair_text(), "maxRepairTurns = 1\n");
    let (_edited_checked, _edited_inputs, edited_plan) = compile_in_repo(
        repo,
        &edited_repair,
        &fixture_grant(),
        &fixture_selections(),
    );
    assert_ne!(edited_plan.plan_hash, hash);

    let reread = store.read_plan(&hash).unwrap();
    assert_eq!(reread.plan_hash, hash);
    assert_eq!(reread.max_repair_turns, 2);
    assert_eq!(reread.task.content_digest, inputs.task.digest);
    assert_eq!(
        canonical_plan_bytes(&reread).unwrap(),
        canonical_plan_bytes(&plan).unwrap()
    );
}

/// The plan pins the per-file bundle manifest the manifest digest
/// covers: a two-file bundle lists both entries sorted, and the
/// stored digest recomputes from them with the shared helper.
#[test]
fn workflow_test_bundle_manifest_lists_every_file() {
    use jackin_workflow_spec::digest::bundle_manifest_digest;

    let scratch = scratch_repo();
    std::fs::write(scratch.path().join("checks/helper.py"), "x = 2\n").unwrap();
    let (_checked, inputs, plan) = compile_in_repo(
        scratch.path(),
        &fixture_repair_text(),
        &fixture_grant(),
        &fixture_selections(),
    );
    assert_eq!(inputs.test_bundle.len(), 2);
    let files = &plan.verification.test_bundle_files;
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].logical_path, "check_add.py");
    assert_eq!(files[1].logical_path, "helper.py");
    for (frozen, pinned) in inputs.test_bundle.iter().zip(files.iter()) {
        assert_eq!(frozen.logical_path, pinned.logical_path);
        assert_eq!(frozen.digest, pinned.digest);
    }
    let recomputed = bundle_manifest_digest(
        files
            .iter()
            .map(|file| (file.logical_path.as_str(), file.digest.as_str())),
    );
    assert_eq!(recomputed, plan.verification.test_bundle_digest);
    assert_eq!(recomputed, inputs.test_bundle_digest);
}

/// Stored plans compiled before the per-file manifest existed
/// refuse as legacy: without `test_bundle_files` the bytes are not
/// a valid plan, and `read_plan` reports `plan_digest_mismatch`.
#[test]
fn workflow_legacy_plan_without_bundle_manifest_refuses() {
    let scratch = scratch_repo();
    let (_checked, _inputs, plan) = compile_in_repo(
        scratch.path(),
        &fixture_repair_text(),
        &fixture_grant(),
        &fixture_selections(),
    );
    let store_dir = tempfile::tempdir().unwrap();
    let store = InputStore::open(store_dir.path()).unwrap();
    let mut value = serde_json::to_value(&plan).unwrap();
    value
        .get_mut("verification")
        .expect("plan has verification")
        .as_object_mut()
        .expect("verification is an object")
        .remove("test_bundle_files");
    let bytes = serde_json::to_vec(&value).unwrap();
    let path = store_dir
        .path()
        .join("plans")
        .join(format!("{}.plan.json", plan.plan_hash));
    std::fs::write(&path, &bytes).unwrap();
    let error = store
        .read_plan(&plan.plan_hash)
        .expect_err("legacy plan must refuse");
    assert_eq!(error.primary_code(), ErrorCode::PlanDigestMismatch);
}

#[test]
fn workflow_evaluation_deadline() {
    let mut limits = EvalLimits::defaults();
    limits.deadline = Duration::from_nanos(1);
    let error = expect_error(
        evaluate_fixture(&fixture_repair_text(), &limits),
        ErrorCode::EvaluationLimit,
        "",
        "deadline",
    );
    let first = error
        .diagnostics
        .first()
        .map(|found| found.action.contains("simplify"));
    assert_eq!(first, Some(true));
}

#[test]
fn workflow_evaluation_memory_cap() {
    let mut limits = EvalLimits::defaults();
    limits.memory_bytes = 1;
    expect_error(
        evaluate_fixture(&fixture_repair_text(), &limits),
        ErrorCode::EvaluationLimit,
        "",
        "memory cap",
    );
}

#[test]
fn workflow_repository_head_exact() {
    let scratch = scratch_repo();
    let repo = scratch.path();
    let first: RepositoryInput =
        resolve_repository(repo, "repair.pkl", "workspace:current").unwrap();
    assert_eq!(first.repository_id, "workspace:current");
    assert_eq!(first.base_commit, git_head(repo));
    assert_eq!(first.base_commit.len(), 40);

    std::fs::write(
        repo.join("calculator.py"),
        "def add(a, b):\n    return a + b\n",
    )
    .unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "fix"]);
    let second: RepositoryInput =
        resolve_repository(repo, "repair.pkl", "workspace:current").unwrap();
    assert_eq!(second.base_commit, git_head(repo));
    assert_ne!(first.base_commit, second.base_commit);
    assert_ne!(first.manifest_digest, second.manifest_digest);

    let plain = tempfile::tempdir().unwrap();
    let missing = resolve_repository(plain.path(), "repair.pkl", "workspace:current");
    expect_error(
        missing,
        ErrorCode::MissingInput,
        "repository.revision",
        "not a git checkout",
    );
}

#[test]
fn workflow_grant_maxima_exact_boundary() {
    let repair = fixture_repair_text();
    let evaluated = evaluate_fixture(&repair, &EvalLimits::defaults()).unwrap();
    let scratch = scratch_repo();
    let (checked, inputs) = compile_inputs(scratch.path(), &evaluated, &fixture_selections());
    let mut grant = fixture_grant();
    grant.max_cpu_millicores = checked.resources.cpu_millicores;
    grant.max_memory_bytes = checked.resources.memory_bytes;
    grant.max_pids = checked.resources.max_pids;
    grant.max_run_seconds = checked.resources.max_run_seconds;
    grant.max_disk_monitor_bytes = checked.resources.disk_monitor_bytes;
    let plan = compile_plan(&checked, &inputs, &grant, "repair.pkl").unwrap();
    assert!(!plan.plan_hash.is_empty());
}

#[test]
fn workflow_repair_zero_turns() {
    let repair = format!("{}{}", fixture_repair_text(), "maxRepairTurns = 0\n");
    let evaluated = evaluate_fixture(&repair, &EvalLimits::defaults()).unwrap();
    let scratch = scratch_repo();
    let (checked, inputs) = compile_inputs(scratch.path(), &evaluated, &fixture_selections());
    assert_eq!(checked.max_repair_turns, 0);
    let plan = compile_plan(&checked, &inputs, &fixture_grant(), "repair.pkl").unwrap();
    assert_eq!(plan.nodes.len(), 6);
    assert_eq!(
        plan.nodes.first().map(|node| node.id.as_str()),
        Some("prepare@r0")
    );
    assert_eq!(
        plan.nodes.last().map(|node| node.id.as_str()),
        Some("decide@r0")
    );
}

#[test]
fn workflow_backup_authorized_remote_ok() {
    let repair = replace_block(
        &fixture_repair_text(),
        "backup",
        "backup { mode = \"required\" remote = \"test-remote\" }\n",
    );
    let evaluated = evaluate_fixture(&repair, &EvalLimits::defaults()).unwrap();
    let scratch = scratch_repo();
    let (checked, inputs) = compile_inputs(scratch.path(), &evaluated, &fixture_selections());
    let plan = compile_plan(&checked, &inputs, &fixture_grant(), "repair.pkl").unwrap();
    assert_eq!(plan.backup.mode, "required");
    assert_eq!(plan.backup.remote_id.as_deref(), Some("test-remote"));
}

#[test]
fn workflow_frozen_bundle_digest_stable() {
    let left = fixture_bundle(&fixture_repair_text());
    let right = fixture_bundle(&fixture_repair_text());
    assert_eq!(left.digest(), right.digest());
    let other = fixture_bundle(&format!("{}{}", fixture_repair_text(), "\n"));
    assert_ne!(left.digest(), other.digest());
}

/// Build a live-selection env from temp fixtures only (a fake role
/// manifest and fake agent binaries under a scratch dir, never
/// `~/.jackin`), hashing the files the way the CLI hashes the real
/// role manifest and agent binaries.
fn live_env_fixture() -> (tempfile::TempDir, LiveSelectionEnv) {
    use jackin_workflow_spec::digest::sha256_hex;
    let dir = tempfile::tempdir().unwrap();
    let manifest = b"role = \"temp-fixture\"\n";
    std::fs::write(dir.path().join("jackin.role.toml"), manifest).unwrap();
    let claude_bin = b"fake-claude-binary";
    let codex_bin = b"fake-codex-binary";
    std::fs::write(dir.path().join("claude"), claude_bin).unwrap();
    std::fs::write(dir.path().join("codex"), codex_bin).unwrap();
    let agent = |runtime: &str, account: &str, binary: &[u8]| LiveAgentFacts {
        runtime: runtime.to_owned(),
        agent_slug: runtime.to_owned(),
        account_id: account.to_owned(),
        auth_mode: "profile".to_owned(),
        requested_model: Some(format!("{runtime}-requested")),
        actual_model: None,
        binary_digest: format!("sha256:{}", sha256_hex(binary)),
        image_digest: "sha256:temp-role-image".to_owned(),
    };
    let env = LiveSelectionEnv {
        workspace_name: "temp-workspace".to_owned(),
        role_id: "temp-role".to_owned(),
        role_revision: sha256_hex(manifest),
        implementer: agent("claude", "temp-claude", claude_bin),
        reviewer: agent("codex", "temp-codex", codex_bin),
    };
    (dir, env)
}

#[test]
fn workflow_live_resolution_ok() {
    let (_dir, env) = live_env_fixture();
    let selections = resolve_live_selections("repair.pkl", &env).unwrap();
    assert_eq!(selections.implementer.runtime, "claude");
    assert_eq!(selections.reviewer.runtime, "codex");
    assert_eq!(selections.implementer.role_id, "temp-role");
    assert_eq!(selections.reviewer.role_id, "temp-role");
    assert_eq!(selections.implementer.account_id, "temp-claude");
    assert_eq!(selections.reviewer.account_id, "temp-codex");
    // C1 keeps requested and actual apart: the request is recorded,
    // the vendor-reported model stays unknown until a run reports it.
    assert_eq!(selections.implementer.requested_model, "claude-requested");
    assert_eq!(selections.implementer.actual_model, None);
    assert_eq!(selections.reviewer.requested_model, "codex-requested");
    assert_eq!(selections.reviewer.actual_model, None);
    assert!(selections.implementer.binary_digest.starts_with("sha256:"));
    assert!(selections.reviewer.binary_digest.starts_with("sha256:"));
    assert_ne!(
        selections.implementer.binary_digest,
        selections.reviewer.binary_digest
    );
}

#[test]
fn workflow_live_resolution_missing_table() {
    let (_dir, base) = live_env_fixture();
    let cases: Vec<(&str, &str, LiveSelectionEnv)> = vec![
        (
            "workspace",
            "workspace",
            LiveSelectionEnv {
                workspace_name: String::new(),
                ..base.clone()
            },
        ),
        (
            "role",
            "role",
            LiveSelectionEnv {
                role_id: String::new(),
                ..base.clone()
            },
        ),
        (
            "role.revision",
            "role.revision",
            LiveSelectionEnv {
                role_revision: String::new(),
                ..base.clone()
            },
        ),
        (
            "implementer.account",
            "implementer.account",
            LiveSelectionEnv {
                implementer: LiveAgentFacts {
                    account_id: String::new(),
                    ..base.implementer.clone()
                },
                ..base.clone()
            },
        ),
        (
            "implementer.model",
            "implementer.model",
            LiveSelectionEnv {
                implementer: LiveAgentFacts {
                    requested_model: None,
                    ..base.implementer.clone()
                },
                ..base.clone()
            },
        ),
        (
            "implementer.binary",
            "implementer.binary",
            LiveSelectionEnv {
                implementer: LiveAgentFacts {
                    binary_digest: String::new(),
                    ..base.implementer.clone()
                },
                ..base.clone()
            },
        ),
        (
            "implementer.image",
            "implementer.image",
            LiveSelectionEnv {
                implementer: LiveAgentFacts {
                    image_digest: String::new(),
                    ..base.implementer.clone()
                },
                ..base.clone()
            },
        ),
        (
            "reviewer.account",
            "reviewer.account",
            LiveSelectionEnv {
                reviewer: LiveAgentFacts {
                    account_id: String::new(),
                    ..base.reviewer.clone()
                },
                ..base.clone()
            },
        ),
    ];
    for (name, field, env) in cases {
        let error = expect_error(
            resolve_live_selections("repair.pkl", &env),
            ErrorCode::MissingInput,
            field,
            "",
        );
        assert!(
            error.diagnostics.iter().all(|diagnostic| {
                !diagnostic.message.contains("token")
                    && !diagnostic.message.contains("secret")
                    && !diagnostic.message.contains("key")
            }),
            "case {name}: diagnostic must not carry credential material"
        );
    }
}

#[test]
fn workflow_live_resolution_runtime_mismatch() {
    let (_dir, mut env) = live_env_fixture();
    env.implementer.runtime = "codex".to_owned();
    expect_error(
        resolve_live_selections("repair.pkl", &env),
        ErrorCode::InvalidDefinition,
        "implementer.runtime",
        "expected `claude`",
    );
}

#[test]
fn workflow_live_resolution_plan_hash_stable() {
    let (_dir, env) = live_env_fixture();
    let selections = resolve_live_selections("repair.pkl", &env).unwrap();
    let repair = fixture_repair_text();
    let evaluated = evaluate_fixture(&repair, &EvalLimits::defaults()).unwrap();
    let scratch = scratch_repo();
    let (checked, inputs) = compile_inputs(scratch.path(), &evaluated, &selections);
    let first = compile_plan(&checked, &inputs, &fixture_grant(), "repair.pkl").unwrap();
    let second = compile_plan(&checked, &inputs, &fixture_grant(), "repair.pkl").unwrap();
    assert_eq!(first.plan_hash, second.plan_hash);
    assert_eq!(
        canonical_plan_bytes(&first).unwrap(),
        canonical_plan_bytes(&second).unwrap()
    );
    // Requested-vs-actual survives compilation into the frozen plan.
    assert_eq!(first.implementer.requested_model, "claude-requested");
    assert_eq!(first.implementer.actual_model, None);
    // The frozen plan carries identifiers and digests only.
    let canonical = String::from_utf8(canonical_plan_bytes(&first).unwrap()).unwrap();
    assert!(canonical.contains("temp-claude"));
    assert!(!canonical.contains("fake-claude-binary"));
}

/// Regression: the Pkl definition file may live in a different checkout
/// than the workspace repository (the definition ships with the jackin
/// checkout while the workspace mounts its own repository). The frozen
/// base commit must be the workspace repository HEAD, never the Pkl
/// directory's HEAD. The first candidate without a HEAD is skipped.
#[test]
fn workflow_workspace_repository_uses_workspace_repo_not_pkl_dir() {
    let pkl_checkout = scratch_repo();
    let workspace_checkout = scratch_repo();
    // Diverge the two HEADs so a mix-up is observable.
    std::fs::write(
        workspace_checkout.path().join("task.md"),
        "workspace task\n",
    )
    .unwrap();
    git(workspace_checkout.path(), &["add", "."]);
    git(
        workspace_checkout.path(),
        &["commit", "-q", "-m", "workspace change"],
    );
    let workspace_head = git_head(workspace_checkout.path());
    assert_ne!(workspace_head, git_head(pkl_checkout.path()));

    let not_a_repo = tempfile::tempdir().unwrap();
    let candidates = vec![
        not_a_repo.path().to_path_buf(),
        workspace_checkout.path().to_path_buf(),
    ];
    let resolved = resolve_workspace_repository(
        &candidates,
        "repair.pkl",
        "workspace:current",
        "workflows-mvp",
    )
    .unwrap();
    assert_eq!(resolved.repository_id, "workspace:current");
    assert_eq!(resolved.base_commit, workspace_head);
    assert_eq!(resolved.base_commit.len(), 40);
}

/// Regression: with no qualifying candidate (empty list, or paths
/// without a git checkout HEAD) the error is a typed `missing_input`
/// on `repository.revision` naming the workspace.
#[test]
fn workflow_workspace_repository_none_qualifies() {
    let plain = tempfile::tempdir().unwrap();
    for candidates in [Vec::new(), vec![plain.path().to_path_buf()]] {
        expect_error(
            resolve_workspace_repository(
                &candidates,
                "repair.pkl",
                "workspace:current",
                "workflows-mvp",
            ),
            ErrorCode::MissingInput,
            "repository.revision",
            "workflows-mvp",
        );
    }
}
