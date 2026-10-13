const MAX_GITHUB_EVENT_BYTES: u64 = 4 * 1024 * 1024;

struct PushContext {
    root: PathBuf,
    repository: String,
    branch: String,
    git_ref: String,
    workflow_path: String,
    workflow_sha: String,
    run_id: u64,
    run_attempt: u32,
    head_sha: String,
    before_sha: String,
}

fn record_push_head() -> Result<()> {
    let event = env::var("GITHUB_EVENT_NAME").context("GITHUB_EVENT_NAME is missing")?;
    if event == "workflow_dispatch" {
        bail!("manual dispatch does not represent a main push and cannot produce a push-head ledger");
    }
    if event != "push" || env::var("GITHUB_ACTIONS").ok().as_deref() != Some("true") {
        bail!("push-head producer only accepts GitHub Actions push events");
    }

    let context = load_push_context()?;
    let workflow_id = validate_live_push_run(&context)?;
    let pushed_commits = validated_push_commits(&context)?;
    let event_proof = PushEventProof {
        repository: context.repository.clone(),
        repository_id: TARGET_REPOSITORY_ID,
        reference: context.git_ref.clone(),
        before: context.before_sha.clone(),
        after: context.head_sha.clone(),
    };
    let event_bytes = serde_json::to_vec_pretty(&event_proof)
        .context("serializing sanitized push event proof")?;
    let manifest = PushHeadLedgerArtifact {
        schema: PUSH_HEAD_LEDGER_SCHEMA,
        repository: context.repository,
        repository_id: TARGET_REPOSITORY_ID,
        branch: context.branch,
        event: "push".to_owned(),
        workflow_path: context.workflow_path,
        workflow_sha: context.workflow_sha,
        workflow_id,
        run_id: context.run_id,
        run_attempt: context.run_attempt,
        head_sha: context.head_sha.clone(),
        before_sha: context.before_sha,
        tree_sha: required_tree_sha(&context.root, &context.head_sha)?,
        committed_at: required_commit_time(&context.root, &context.head_sha)?,
        pushed_commits,
        event_sha256: sha256_hex(&event_bytes),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .context("serializing push-head ledger manifest")?;
    write_push_artifact(&context.root, &manifest_bytes, &event_bytes)
}

fn load_push_context() -> Result<PushContext> {
    let root = docs::repo_root()?;
    let repository = canonical_repository(
        &env::var("GITHUB_REPOSITORY").context("GITHUB_REPOSITORY is missing")?,
    )?;
    validate_git_remote_identity(&root, &repository)?;
    let branch = env::var("GITHUB_REF_NAME").context("GITHUB_REF_NAME is missing")?;
    let git_ref = env::var("GITHUB_REF").context("GITHUB_REF is missing")?;
    let head_sha = env::var("GITHUB_SHA").context("GITHUB_SHA is missing")?;
    let workflow_sha = env::var("GITHUB_WORKFLOW_SHA")
        .context("GITHUB_WORKFLOW_SHA is missing")?;
    let workflow_ref = env::var("GITHUB_WORKFLOW_REF").context("GITHUB_WORKFLOW_REF is missing")?;
    let (workflow_path, workflow_git_ref) = parse_workflow_ref(&workflow_ref, &repository)?;
    let run_id = env::var("GITHUB_RUN_ID")
        .context("GITHUB_RUN_ID is missing")?
        .parse::<u64>()
        .context("GITHUB_RUN_ID is not a number")?;
    let run_attempt = env::var("GITHUB_RUN_ATTEMPT")
        .context("GITHUB_RUN_ATTEMPT is missing")?
        .parse::<u32>()
        .context("GITHUB_RUN_ATTEMPT is not a number")?;
    if branch != "main"
        || git_ref != "refs/heads/main"
        || workflow_git_ref != git_ref
        || workflow_path != DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW
        || !is_git_sha(&head_sha)
        || workflow_sha != head_sha
        || run_id == 0
        || run_attempt == 0
        || run_attempt > MAX_RUN_ATTEMPTS
    {
        bail!("push-head producer is not bound to the generated main push workflow");
    }
    let before_sha = read_push_event_before(&root, &repository, &git_ref, &head_sha)?;
    Ok(PushContext {
        root,
        repository,
        branch,
        git_ref,
        workflow_path,
        workflow_sha,
        run_id,
        run_attempt,
        head_sha,
        before_sha,
    })
}

fn read_push_event_before(
    root: &Path,
    repository: &str,
    git_ref: &str,
    head_sha: &str,
) -> Result<String> {
    let event_path = env::var_os("GITHUB_EVENT_PATH")
        .map(PathBuf::from)
        .context("GITHUB_EVENT_PATH is missing")?;
    let metadata = fs::metadata(&event_path)
        .with_context(|| format!("reading GitHub event metadata {}", event_path.display()))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_GITHUB_EVENT_BYTES {
        bail!("GitHub push event payload is missing or exceeds the size limit");
    }
    let raw_event = fs::read(&event_path)
        .with_context(|| format!("reading GitHub push event {}", event_path.display()))?;
    let raw_event: serde_json::Value = serde_json::from_slice(&raw_event)
        .context("parsing GitHub push event payload")?;
    let event_repository = raw_event
        .get("repository")
        .and_then(|value| value.get("full_name"))
        .and_then(serde_json::Value::as_str);
    let event_repository_id = raw_event
        .get("repository")
        .and_then(|value| value.get("id"))
        .and_then(serde_json::Value::as_u64);
    let event_ref = raw_event.get("ref").and_then(serde_json::Value::as_str);
    let before_sha = raw_event
        .get("before")
        .and_then(serde_json::Value::as_str)
        .context("push event has no before SHA")?
        .to_owned();
    let event_head_sha = raw_event
        .get("after")
        .and_then(serde_json::Value::as_str)
        .context("push event has no after SHA")?;
    if event_repository.is_none_or(|name| !name.eq_ignore_ascii_case(repository))
        || event_repository_id != Some(TARGET_REPOSITORY_ID)
        || event_ref != Some(git_ref)
        || event_head_sha != head_sha
        || !is_git_sha(&before_sha)
        || is_zero_sha(&before_sha)
    {
        bail!("GitHub push event identity does not match the current workflow run");
    }
    let checked_out_head = cmd::output_string(
        Command::new("git").current_dir(root).args(["rev-parse", "HEAD"]),
    )?;
    if checked_out_head.trim() != head_sha {
        bail!("checked-out Git HEAD differs from the push event head");
    }
    Ok(before_sha)
}

fn validate_live_push_run(context: &PushContext) -> Result<u64> {
    let run = api_run(&context.repository, context.run_id)?;
    let workflow_id = active_workflow_id(&context.repository, DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW)?;
    validate_api_run_contract(
        &run,
        context.run_id,
        "main",
        "push",
        DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW,
        Some(workflow_id),
        Some(&context.head_sha),
    )?;
    if run.run_attempt != context.run_attempt
        || !run.status.eq_ignore_ascii_case("in_progress")
        || run.conclusion.is_some()
    {
        bail!("GitHub API run state does not match the active producer attempt");
    }
    Ok(workflow_id)
}

fn validated_push_commits(context: &PushContext) -> Result<Vec<String>> {
    cmd::run(Command::new("git").current_dir(&context.root).args([
        "cat-file",
        "-e",
        &format!("{}^{{commit}}", context.before_sha),
    ]))
    .context("verifying push predecessor commit")?;
    cmd::run(Command::new("git").current_dir(&context.root).args([
        "merge-base",
        "--is-ancestor",
        &context.before_sha,
        &context.head_sha,
    ]))
    .context("verifying pushed commit range")?;
    let commits = cmd::output_string(Command::new("git").current_dir(&context.root).args([
        "rev-list",
        "--first-parent",
        "--reverse",
        &format!("{}..{}", context.before_sha, context.head_sha),
    ]))?
    .lines()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    if !valid_commit_list(&commits) || commits.last() != Some(&context.head_sha) {
        bail!("Git did not produce a complete first-parent push range");
    }
    Ok(commits)
}

fn write_push_artifact(root: &Path, manifest: &[u8], event: &[u8]) -> Result<()> {
    let directory = root.join(CI_PUSH_HEAD_LEDGER_ARTIFACT_PATH);
    let output = OutputLocation::open(&directory, true)?;
    if output.parent.entry_kind(&output.name)? != EntryKind::Missing {
        bail!("push-head artifact output directory already exists");
    }
    let (staged_name, staged) = create_staging_directory(&output.parent, "ci-push-head-ledger")?;
    let write_result = (|| -> Result<()> {
        staged.write_new(OsStr::new("push-head.json"), manifest)?;
        staged.write_new(OsStr::new("event.json"), event)?;
        output
            .parent
            .rename(&staged_name, output.name.as_os_str())
            .context("publishing complete push-head artifact")?;
        Ok(())
    })();
    if write_result.is_err() {
        drop(staged.remove_file(OsStr::new("push-head.json")));
        drop(staged.remove_file(OsStr::new("event.json")));
        drop(output.parent.remove_directory(&staged_name));
    }
    write_result
}
