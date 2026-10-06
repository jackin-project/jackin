const PUSH_HEAD_LEDGER_SCHEMA: u32 = 1;

#[derive(Clone, Debug, Deserialize)]
struct PushHeadLedgerArtifact {
    schema: u32,
    repository: String,
    branch: String,
    event: String,
    workflow_path: String,
    run_id: u64,
    head_sha: String,
    before_sha: String,
    tree_sha: String,
    committed_at: String,
    pushed_commits: Vec<String>,
    raw_event_sha256: String,
}

fn expected_from_push_head_ledger(
    root: &Path,
    repository: &str,
    branch: &str,
    window: &TimeWindow,
    ledger_workflow_ids: &BTreeSet<u64>,
) -> Result<ExpectedDenominator> {
    if ledger_workflow_ids.is_empty() {
        bail!(
            "durable push-head ledger workflow {DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW} was not found"
        );
    }
    let shallow = cmd::output_string(
        Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "--is-shallow-repository"]),
    )?;
    let shallow = match shallow.trim() {
        "true" => true,
        "false" => false,
        value => bail!("git returned invalid shallow-repository proof `{value}`"),
    };
    let fetch_args = if shallow {
        vec!["fetch", "--no-tags", "--unshallow", "origin", branch]
    } else {
        vec!["fetch", "--no-tags", "origin", branch]
    };
    cmd::run(Command::new("git").current_dir(root).args(fetch_args)).with_context(|| {
        format!("fetching complete {branch} history required to verify durable push-head coverage")
    })?;

    let runs = list_push_head_runs(repository, branch, window, ledger_workflow_ids)?;
    if runs.is_empty() {
        bail!(
            "no durable push-head ledger runs cover {branch} in {}..{}",
            window.since,
            window.until
        );
    }
    let predecessor_run = list_push_head_predecessor_runs(
        repository,
        branch,
        &window.since,
        ledger_workflow_ids,
    )?
    .pop()
    .context("no durable push-head ledger predecessor before the collection window")?;
    let predecessor_workflow_id =
        validate_push_head_run(&predecessor_run, branch, ledger_workflow_ids)?;
    let boundary_predecessor = validate_push_head_artifact(
        root,
        repository,
        branch,
        &predecessor_run,
        predecessor_workflow_id,
        download_push_head_artifact(repository, predecessor_run.id)?,
    )?;
    let mut observations = Vec::with_capacity(runs.len());
    for run in runs {
        let workflow_id = validate_push_head_run(&run, branch, ledger_workflow_ids)?;
        let artifact = download_push_head_artifact(repository, run.id)?;
        observations.push(validate_push_head_artifact(
            root,
            repository,
            branch,
            &run,
            workflow_id,
            artifact,
        )?);
    }
    observations.sort_by_key(|observation| (observation.created_at.clone(), observation.run_id));
    validate_push_head_chain(root, branch, &observations, &boundary_predecessor)?;
    let history = observations
        .iter()
        .map(|observation| HistoryCommitObservation {
            sha: observation.head_sha.clone(),
            base_sha: Some(observation.before_sha.clone()),
            tree_sha: observation.tree_sha.clone(),
            committed_at: observation.committed_at.clone(),
        })
        .collect::<Vec<_>>();
    let expected = expected_from_history(&history, DenominatorSource::PushHeadLedger)?;
    Ok(ExpectedDenominator {
        expected,
        denominator: DenominatorProof {
            source: DenominatorSource::PushHeadLedger,
            branch: branch.to_owned(),
            window: window.clone(),
            fetch_succeeded: true,
            commit_count: history.len(),
            source_workflow: Some(DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned()),
            source_run_count: observations.len(),
            boundary: DenominatorBoundary::PushHead {
                predecessor: Box::new(boundary_predecessor),
            },
        },
        history,
        push_heads: observations,
    })
}

fn list_push_head_runs(
    repository: &str,
    branch: &str,
    window: &TimeWindow,
    workflow_ids: &BTreeSet<u64>,
) -> Result<Vec<ApiRun>> {
    let mut runs: Vec<ApiRun> = Vec::new();
    for workflow_id in workflow_ids {
        let endpoint = format!(
            "repos/{repository}/actions/workflows/{workflow_id}/runs?branch={branch}&event=push&per_page=100&created={since}..{until}",
            since = api_timestamp(&window.since),
            until = api_timestamp(&window.until)
        );
        runs.extend(decode_pages(&api_pages(&endpoint)?, "workflow_runs")?);
    }
    runs.sort_by_key(|run: &ApiRun| (run.created_at.clone(), run.id));
    runs.dedup_by_key(|run| run.id);
    let since = parse_timestamp(&window.since)?;
    let until = parse_timestamp(&window.until)?;
    for run in &runs {
        let created_at = parse_timestamp(&run.created_at)
            .with_context(|| format!("parsing push-head run {} creation time", run.id))?;
        if created_at < since || created_at > until {
            bail!(
                "push-head ledger run {} is outside the requested window",
                run.id
            );
        }
    }
    Ok(runs)
}

fn list_push_head_predecessor_runs(
    repository: &str,
    branch: &str,
    since: &str,
    workflow_ids: &BTreeSet<u64>,
) -> Result<Vec<ApiRun>> {
    let mut runs: Vec<ApiRun> = Vec::new();
    for workflow_id in workflow_ids {
        let endpoint = format!(
            "repos/{repository}/actions/workflows/{workflow_id}/runs?branch={branch}&event=push&per_page=100&created=1970-01-01T00:00:00Z..{since}",
            since = api_timestamp(since)
        );
        runs.extend(decode_pages(&api_pages(&endpoint)?, "workflow_runs")?);
    }
    let since = parse_timestamp(since)?;
    runs.sort_by_key(|run: &ApiRun| (run.created_at.clone(), run.id));
    runs.dedup_by_key(|run| run.id);
    let mut predecessors = Vec::new();
    for run in runs {
        let created_at = parse_timestamp(&run.created_at).with_context(|| {
            format!("parsing push-head predecessor run {} creation time", run.id)
        })?;
        if created_at < since {
            predecessors.push(run);
        }
    }
    Ok(predecessors)
}

fn validate_push_head_run(
    run: &ApiRun,
    branch: &str,
    ledger_workflow_ids: &BTreeSet<u64>,
) -> Result<u64> {
    let workflow_id = run
        .workflow_id
        .context("push-head ledger run has no workflow identity")?;
    if !ledger_workflow_ids.contains(&workflow_id) {
        bail!(
            "push-head ledger run {} has an unconfigured workflow ID",
            run.id
        );
    }
    if run.event.as_deref() != Some("push")
        || run.head_branch.as_deref() != Some(branch)
        || !run.path.as_deref().is_some_and(|path| {
            workflow_path_matches(path, &[DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned()])
        })
        || !run.status.eq_ignore_ascii_case("completed")
        || run.conclusion.as_deref() != Some("success")
    {
        bail!(
            "push-head ledger run {} is not a successful main push: event={:?}, branch={:?}, status={}, conclusion={:?}",
            run.id,
            run.event,
            run.head_branch,
            run.status,
            run.conclusion
        );
    }
    Ok(workflow_id)
}

fn download_push_head_artifact(repository: &str, run_id: u64) -> Result<(Vec<u8>, Vec<u8>)> {
    let temp = tempfile::tempdir().context("creating push-head artifact staging directory")?;
    cmd::run(Command::new("gh").args([
        "run",
        "download",
        &run_id.to_string(),
        "--repo",
        repository,
        "--name",
        DEFAULT_PUSH_HEAD_LEDGER_ARTIFACT,
        "--dir",
        temp.path().to_string_lossy().as_ref(),
    ]))
    .with_context(|| format!("downloading push-head ledger artifact for run {run_id}"))?;
    read_push_head_artifact(temp.path(), run_id)
}

fn read_push_head_artifact(directory: &Path, run_id: u64) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut names = BTreeSet::new();
    for entry in crate::fs_util::read_dir_sorted(directory)
        .context("reading push-head artifact contents")?
    {
        if !entry.file_type()?.is_file() {
            bail!("push-head ledger artifact contains a non-file entry");
        }
        names.insert(entry.file_name());
    }
    let expected_names = BTreeSet::from([
        std::ffi::OsString::from("event.json"),
        std::ffi::OsString::from("push-head.json"),
    ]);
    if names != expected_names {
        bail!("push-head ledger artifact for run {run_id} has unexpected files: {names:?}");
    }
    Ok((
        fs::read(directory.join("push-head.json")).context("reading push-head manifest")?,
        fs::read(directory.join("event.json")).context("reading raw push event")?,
    ))
}

fn validate_push_head_artifact(
    root: &Path,
    repository: &str,
    branch: &str,
    run: &ApiRun,
    workflow_id: u64,
    artifact: (Vec<u8>, Vec<u8>),
) -> Result<PushHeadObservation> {
    let (manifest_bytes, raw_event_bytes) = artifact;
    let manifest_text = String::from_utf8(manifest_bytes.clone())
        .with_context(|| format!("push-head ledger manifest for run {} is not UTF-8", run.id))?;
    let event_text = String::from_utf8(raw_event_bytes.clone())
        .with_context(|| format!("raw push event for run {} is not UTF-8", run.id))?;
    let manifest: PushHeadLedgerArtifact = serde_json::from_slice(&manifest_bytes)
        .with_context(|| format!("parsing push-head ledger manifest for run {}", run.id))?;
    if manifest.schema != PUSH_HEAD_LEDGER_SCHEMA {
        bail!(
            "push-head ledger run {} has unsupported artifact schema {}",
            run.id,
            manifest.schema
        );
    }
    if manifest.repository != repository
        || manifest.branch != branch
        || manifest.event != "push"
        || manifest.workflow_path != DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW
        || manifest.run_id != run.id
        || manifest.head_sha != run.head_sha
    {
        bail!(
            "push-head ledger manifest for run {} has mismatched provenance",
            run.id
        );
    }
    if !is_git_sha(&manifest.before_sha)
        || !is_git_sha(&manifest.head_sha)
        || manifest.before_sha == manifest.head_sha
        || !is_git_sha(&manifest.tree_sha)
        || !valid_commit_list(&manifest.pushed_commits)
        || !is_hex_digest(&manifest.raw_event_sha256)
    {
        bail!("push-head ledger manifest for run {} is incomplete", run.id);
    }
    let raw_digest = sha256_hex(&raw_event_bytes);
    if manifest.raw_event_sha256 != raw_digest {
        bail!(
            "push-head ledger raw event digest mismatch for run {}",
            run.id
        );
    }
    let raw_event: serde_json::Value = serde_json::from_slice(&raw_event_bytes)
        .with_context(|| format!("parsing raw push event for run {}", run.id))?;
    let raw_repository = raw_event
        .get("repository")
        .and_then(|value| value.get("full_name"))
        .and_then(serde_json::Value::as_str);
    let raw_ref = raw_event.get("ref").and_then(serde_json::Value::as_str);
    let raw_before = raw_event.get("before").and_then(serde_json::Value::as_str);
    let raw_after = raw_event.get("after").and_then(serde_json::Value::as_str);
    let raw_commits = raw_event
        .get("commits")
        .and_then(serde_json::Value::as_array)
        .context("raw push event has no commits array")?
        .iter()
        .map(|commit| {
            commit
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .context("raw push event contains a commit without an ID")
        })
        .collect::<Result<Vec<_>>>()?;
    let expected_ref = format!("refs/heads/{branch}");
    if raw_repository != Some(repository)
        || raw_ref != Some(expected_ref.as_str())
        || raw_before != Some(manifest.before_sha.as_str())
        || raw_after != Some(manifest.head_sha.as_str())
        || !valid_commit_list(&raw_commits)
        || raw_commits != manifest.pushed_commits
        || raw_commits.last() != Some(&manifest.head_sha)
    {
        bail!(
            "raw push event does not match push-head ledger manifest for run {}",
            run.id
        );
    }
    let tree_sha = required_tree_sha(root, &manifest.head_sha)?;
    if tree_sha != manifest.tree_sha {
        bail!("push-head ledger tree identity mismatch for run {}", run.id);
    }
    let committed_at = required_commit_time(root, &manifest.head_sha)?;
    if api_timestamp(&committed_at) != api_timestamp(&manifest.committed_at) {
        bail!(
            "push-head ledger commit timestamp mismatch for run {}",
            run.id
        );
    }
    parse_timestamp(&run.created_at)?;
    parse_timestamp(&manifest.committed_at)?;
    Ok(PushHeadObservation {
        repository: manifest.repository,
        branch: manifest.branch,
        event: manifest.event,
        workflow_id,
        workflow_path: manifest.workflow_path,
        run_id: manifest.run_id,
        head_sha: manifest.head_sha,
        before_sha: manifest.before_sha,
        tree_sha: manifest.tree_sha,
        committed_at: manifest.committed_at,
        created_at: run.created_at.clone(),
        pushed_commits: manifest.pushed_commits,
        raw_event_sha256: manifest.raw_event_sha256,
        artifact: PushHeadArtifactProof {
            manifest: manifest_text,
            event: event_text,
            manifest_sha256: sha256_hex(&manifest_bytes),
        },
    })
}

fn required_commit_time(root: &Path, sha: &str) -> Result<String> {
    let committed_at = cmd::output_string(Command::new("git").current_dir(root).args([
        "show",
        "-s",
        "--format=%cI",
        sha,
    ]))?;
    let committed_at = committed_at.trim();
    if committed_at.is_empty() {
        bail!("commit {sha} has no committed-at identity");
    }
    Ok(committed_at.to_owned())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}

fn is_hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_git_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_commit_list(commits: &[String]) -> bool {
    !commits.is_empty()
        && commits.iter().all(|commit| is_git_sha(commit))
        && commits.iter().collect::<BTreeSet<_>>().len() == commits.len()
}

fn validate_push_head_chain(
    root: &Path,
    branch: &str,
    observations: &[PushHeadObservation],
    boundary_predecessor: &PushHeadObservation,
) -> Result<()> {
    let mut seen_runs = BTreeSet::new();
    let mut seen_heads = BTreeSet::new();
    let first = observations
        .first()
        .context("push-head ledger has no in-window first entry")?;
    if boundary_predecessor.head_sha != first.before_sha {
        bail!(
            "push-head ledger boundary predecessor {} does not match first before SHA {}",
            boundary_predecessor.head_sha,
            first.before_sha
        );
    }
    if boundary_predecessor.run_id == first.run_id
        || parse_timestamp(&boundary_predecessor.created_at)? >= parse_timestamp(&first.created_at)?
    {
        bail!("push-head ledger boundary predecessor is not strictly earlier");
    }
    for (index, observation) in observations.iter().enumerate() {
        if !seen_runs.insert(observation.run_id) || !seen_heads.insert(observation.head_sha.clone())
        {
            bail!("push-head ledger contains a duplicate run or head");
        }
        if index > 0 && observations[index - 1].head_sha != observation.before_sha {
            bail!(
                "push-head ledger has a coverage gap before {}: expected {}, got {}",
                observation.head_sha,
                observations[index - 1].head_sha,
                observation.before_sha
            );
        }
        if is_zero_sha(&observation.before_sha) {
            bail!("push-head ledger has an all-zero before SHA for {branch}");
        }
        for pushed_commit in &observation.pushed_commits {
            cmd::run(Command::new("git").current_dir(root).args([
                "cat-file",
                "-e",
                &format!("{pushed_commit}^{{commit}}"),
            ]))
            .with_context(|| {
                format!(
                    "verifying push-head ledger commit {pushed_commit} for run {}",
                    observation.run_id
                )
            })?;
        }
        cmd::run(Command::new("git").current_dir(root).args([
            "merge-base",
            "--is-ancestor",
            &observation.before_sha,
            &observation.head_sha,
        ]))
        .with_context(|| {
            format!(
                "verifying push-head range {}..{}",
                observation.before_sha, observation.head_sha
            )
        })?;
        let range = cmd::output_string(Command::new("git").current_dir(root).args([
            "rev-list",
            "--first-parent",
            &format!("{}..{}", observation.before_sha, observation.head_sha),
        ]))?;
        let range = range.lines().map(str::trim).collect::<BTreeSet<_>>();
        if range.is_empty()
            || !range.iter().all(|sha| {
                observation
                    .pushed_commits
                    .iter()
                    .any(|pushed| pushed == sha)
            })
        {
            bail!(
                "push-head ledger entry {} does not cover its first-parent commit range",
                observation.run_id
            );
        }
    }
    let remote = format!("refs/remotes/origin/{branch}");
    let tip = cmd::output_string(
        Command::new("git")
            .current_dir(root)
            .args(["rev-parse", &remote]),
    )?;
    if observations
        .last()
        .is_none_or(|observation| observation.head_sha != tip.trim())
    {
        bail!("push-head ledger does not reach the current origin/{branch} tip");
    }
    Ok(())
}

fn is_zero_sha(value: &str) -> bool {
    !value.is_empty() && value.chars().all(|character| character == '0')
}

fn expected_from_history(
    history: &[HistoryCommitObservation],
    source: DenominatorSource,
) -> Result<Vec<ExpectedObligation>> {
    let mut seen = BTreeSet::new();
    let mut expected = Vec::with_capacity(history.len() * Cohort::ALL.len());
    for history_commit in history {
        if history_commit.sha.is_empty() || history_commit.committed_at.is_empty() {
            bail!("history commit observation has incomplete identity");
        }
        if !seen.insert(history_commit.sha.clone()) {
            bail!("duplicate denominator commit {}", history_commit.sha);
        }
        let commit = ExpectedCommit {
            sha: history_commit.sha.clone(),
            base_sha: history_commit.base_sha.clone(),
            tree_sha: history_commit.tree_sha.clone(),
            committed_at: Some(history_commit.committed_at.clone()),
            source,
        };
        for cohort in Cohort::ALL {
            expected.push(ExpectedObligation {
                commit: commit.clone(),
                cohort,
                provenance: match source {
                    DenominatorSource::PushHeadLedger => ObligationProvenance::PushHeadLedger,
                    DenominatorSource::Fixture => ObligationProvenance::Fixture,
                },
            });
        }
    }
    validate_expected(&expected)?;
    Ok(expected)
}
