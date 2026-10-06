const PUSH_HEAD_LEDGER_SCHEMA: u32 = 2;
const MAX_ARTIFACT_ZIP_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARTIFACT_FILE_BYTES: u64 = 32 * 1024 * 1024;

struct VerifiedPushArtifact {
    manifest: Vec<u8>,
    event: Vec<u8>,
    id: u64,
    digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PushHeadLedgerArtifact {
    schema: u32,
    repository: String,
    repository_id: u64,
    branch: String,
    event: String,
    workflow_path: String,
    workflow_sha: String,
    workflow_id: u64,
    run_id: u64,
    run_attempt: u32,
    head_sha: String,
    before_sha: String,
    tree_sha: String,
    committed_at: String,
    pushed_commits: Vec<String>,
    event_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PushEventProof {
    repository: String,
    repository_id: u64,
    #[serde(rename = "ref")]
    reference: String,
    before: String,
    after: String,
}

fn expected_from_push_head_ledger(
    root: &Path,
    repository: &str,
    branch: &str,
    window: &TimeWindow,
    ledger_workflow_ids: &BTreeSet<u64>,
) -> Result<ExpectedDenominator> {
    if branch != "main" {
        bail!("the durable push-head denominator is restricted to main");
    }
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
    let before_fetch_tip = api_branch_tip(repository, branch)?;
    let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    let mut fetch = Command::new("git");
    fetch.current_dir(root).args(["fetch", "--no-tags"]);
    if shallow {
        fetch.arg("--unshallow");
    }
    fetch.args(["origin", refspec.as_str()]);
    cmd::run(&mut fetch).with_context(|| {
        format!("fetching complete {branch} history required to verify durable push-head coverage")
    })?;
    let fetched_tip = cmd::output_string(
        Command::new("git")
            .current_dir(root)
            .args(["rev-parse", &format!("refs/remotes/origin/{branch}")]),
    )?;
    let after_fetch_tip = api_branch_tip(repository, branch)?;
    if before_fetch_tip != after_fetch_tip || fetched_tip.trim() != after_fetch_tip {
        bail!("authoritative {branch} moved during the denominator fetch");
    }

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
    .pop();
    let boundary_predecessor = predecessor_run
        .as_ref()
        .map(|run| {
            let workflow_id = validate_push_head_run(run, branch, ledger_workflow_ids)?;
            validate_push_head_artifact(
                root,
                repository,
                branch,
                run,
                workflow_id,
                download_push_head_artifact(repository, run)?,
            )
        })
        .transpose()?;
    let mut observations = Vec::with_capacity(runs.len());
    for run in runs {
        let workflow_id = validate_push_head_run(&run, branch, ledger_workflow_ids)?;
        let artifact = download_push_head_artifact(repository, &run)?;
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
    let mut effective_window = window.clone();
    let boundary = if let Some(boundary_predecessor) = boundary_predecessor {
        DenominatorBoundary::PushHead {
            predecessor: Box::new(boundary_predecessor),
        }
    } else {
        let first = observations
            .first()
            .context("push-head ledger has no verifiable predecessor; first in-window push is required")?;
        effective_window.since = first.created_at.clone();
        DenominatorBoundary::Bootstrap {
            first_run_id: first.run_id,
            before_sha: first.before_sha.clone(),
        }
    };
    validate_push_head_chain(root, branch, &observations, &boundary)?;
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
            window: effective_window.clone(),
            fetch_succeeded: true,
            commit_count: history.len(),
            source_workflow: Some(DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned()),
            source_run_count: observations.len(),
            boundary,
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
        let endpoint = format!("repos/{repository}/actions/workflows/{workflow_id}/runs");
        let parameters = vec![
            ("branch", branch.to_owned()),
            ("event", "push".to_owned()),
            (
                "created",
                format!("{}..{}", api_timestamp(&window.since), api_timestamp(&window.until)),
            ),
        ];
        let page_runs: Vec<ApiRun> = decode_pages(
            &api_pages(&endpoint, &parameters, "workflow_runs")?,
            "workflow_runs",
        )?;
        for run in &page_runs {
            validate_api_run_repository(run)?;
        }
        runs.extend(page_runs);
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
        let endpoint = format!("repos/{repository}/actions/workflows/{workflow_id}/runs");
        let parameters = vec![
            ("branch", branch.to_owned()),
            ("event", "push".to_owned()),
            (
                "created",
                format!("1970-01-01T00:00:00Z..{}", api_timestamp(since)),
            ),
            ("sort", "created".to_owned()),
            ("direction", "desc".to_owned()),
        ];
        let page = api_single_page(&endpoint, &parameters, "workflow_runs", 1)?;
        let page_runs: Vec<ApiRun> = decode_pages(&[page], "workflow_runs")?;
        for run in &page_runs {
            validate_api_run_repository(run)?;
        }
        if page_runs.len() > 1 {
            bail!("GitHub predecessor query returned more than one run");
        }
        runs.extend(page_runs);
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
            workflow_ref_matches(path, DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW, branch)
        })
        || !run.status.eq_ignore_ascii_case("completed")
        || run.conclusion.as_deref() != Some("success")
        || run.run_attempt == 0
        || run.run_attempt > MAX_RUN_ATTEMPTS
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

fn download_push_head_artifact(repository: &str, run: &ApiRun) -> Result<VerifiedPushArtifact> {
    let repository_info = api_repository(repository)?;
    if !repository_info.full_name.eq_ignore_ascii_case(repository) {
        bail!("GitHub repository identity changed while collecting artifacts");
    }
    let artifacts = api_artifacts(repository, run.id)?;
    let matches = artifacts
        .into_iter()
        .filter(|artifact| artifact.name == DEFAULT_PUSH_HEAD_LEDGER_ARTIFACT)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        bail!("push-head run {} has {} matching artifacts", run.id, matches.len());
    }
    let artifact = matches
        .into_iter()
        .next()
        .context("push-head artifact disappeared after uniqueness check")?;
    let run_metadata = artifact
        .workflow_run
        .as_ref()
        .context("push-head artifact has no workflow-run identity")?;
    let digest = artifact
        .digest
        .as_deref()
        .context("push-head artifact has no API SHA-256 digest")?;
    if artifact.id == 0
        || artifact.expired
        || artifact.size_in_bytes == 0
        || artifact.size_in_bytes > MAX_ARTIFACT_ZIP_BYTES
        || run_metadata.id != run.id
        || run_metadata.head_branch.as_deref() != Some("main")
        || run_metadata.head_sha.as_deref() != Some(run.head_sha.as_str())
        || run_metadata.repository_id != Some(repository_info.id)
        || run_metadata.head_repository_id != Some(repository_info.id)
        || !digest.starts_with("sha256:")
        || !is_hex_digest(digest.trim_start_matches("sha256:"))
    {
        bail!("push-head artifact metadata is invalid or bound to another run");
    }
    let endpoint = format!("repos/{repository}/actions/artifacts/{}/zip", artifact.id);
    let zip_bytes = cmd::output_timeout_limited(
        Command::new("gh").args(["api", &endpoint]),
        Duration::from_secs(180),
        MAX_ARTIFACT_ZIP_BYTES as usize,
        MAX_API_STDERR_BYTES,
    )
    .with_context(|| format!("downloading push-head artifact {}", artifact.id))?;
    if zip_bytes.len() as u64 != artifact.size_in_bytes
        || sha256_hex(&zip_bytes) != digest.trim_start_matches("sha256:")
    {
        bail!("push-head artifact ZIP differs from its API size or digest");
    }
    let (manifest, event) = read_push_head_zip(&zip_bytes, run.id)?;
    Ok(VerifiedPushArtifact {
        manifest,
        event,
        id: artifact.id,
        digest: digest.to_owned(),
    })
}

fn read_push_head_zip(zip_bytes: &[u8], run_id: u64) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(io::Cursor::new(zip_bytes))
        .context("parsing push-head artifact ZIP")?;
    if archive.len() != 2 {
        bail!("push-head ledger artifact for run {run_id} must contain exactly two files");
    }
    let mut files = BTreeMap::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .context("reading push-head artifact ZIP entry")?;
        let name = file.name().to_owned();
        if file.is_dir()
            || !matches!(name.as_str(), "event.json" | "push-head.json")
            || file.size() > MAX_ARTIFACT_FILE_BYTES
        {
            bail!("push-head ledger artifact for run {run_id} has an unsafe or oversized entry");
        }
        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut bytes)
            .context("decompressing push-head artifact ZIP entry")?;
        if bytes.len() as u64 != file.size() {
            bail!("push-head artifact ZIP entry has an inconsistent size");
        }
        files.insert(name, bytes);
    }
    let names = files.keys().map(String::as_str).collect::<BTreeSet<_>>();
    if names != BTreeSet::from(["event.json", "push-head.json"]) {
        bail!("push-head ledger artifact for run {run_id} has missing or duplicate files");
    }
    let manifest = files
        .remove("push-head.json")
        .context("validated push-head manifest disappeared")?;
    let event = files
        .remove("event.json")
        .context("validated event proof disappeared")?;
    Ok((manifest, event))
}

fn validate_push_head_artifact(
    root: &Path,
    repository: &str,
    branch: &str,
    run: &ApiRun,
    workflow_id: u64,
    artifact: VerifiedPushArtifact,
) -> Result<PushHeadObservation> {
    let manifest_bytes = artifact.manifest;
    let raw_event_bytes = artifact.event;
    let artifact_id = artifact.id;
    let artifact_digest = artifact.digest;
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
        || manifest.repository_id != TARGET_REPOSITORY_ID
        || manifest.branch != branch
        || manifest.event != "push"
        || manifest.workflow_path != DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW
        || manifest.workflow_sha != run.head_sha
        || manifest.workflow_id != workflow_id
        || manifest.run_id != run.id
        || manifest.run_attempt != run.run_attempt
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
        || !is_hex_digest(&manifest.event_sha256)
    {
        bail!("push-head ledger manifest for run {} is incomplete", run.id);
    }
    let event_proof: PushEventProof = serde_json::from_slice(&raw_event_bytes)
        .with_context(|| format!("parsing sanitized push proof for run {}", run.id))?;
    let expected_ref = format!("refs/heads/{branch}");
    if !is_hex_digest(&manifest.event_sha256)
        || manifest.event_sha256 != sha256_hex(&raw_event_bytes)
        || event_proof.repository != repository
        || event_proof.repository_id != TARGET_REPOSITORY_ID
        || event_proof.reference != expected_ref
        || event_proof.before != manifest.before_sha
        || event_proof.after != manifest.head_sha
        || manifest.pushed_commits.last() != Some(&manifest.head_sha)
    {
        bail!(
            "sanitized push event does not match push-head ledger manifest for run {}",
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
        workflow_sha: manifest.workflow_sha,
        run_id: manifest.run_id,
        run_attempt: manifest.run_attempt,
        head_sha: manifest.head_sha,
        before_sha: manifest.before_sha,
        tree_sha: manifest.tree_sha,
        committed_at: manifest.committed_at,
        created_at: run.created_at.clone(),
        pushed_commits: manifest.pushed_commits,
        event_sha256: manifest.event_sha256,
        artifact: PushHeadArtifactProof {
            manifest: manifest_text,
            event: event_text,
            manifest_sha256: sha256_hex(&manifest_bytes),
            artifact_id,
            artifact_digest,
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
    boundary: &DenominatorBoundary,
) -> Result<()> {
    let mut seen_runs = BTreeSet::new();
    let mut seen_heads = BTreeSet::new();
    let first = observations
        .first()
        .context("push-head ledger has no in-window first entry")?;
    match boundary {
        DenominatorBoundary::PushHead { predecessor } => {
            if predecessor.head_sha != first.before_sha {
                bail!(
                    "push-head ledger boundary predecessor {} does not match first before SHA {}",
                    predecessor.head_sha,
                    first.before_sha
                );
            }
            if predecessor.run_id == first.run_id
                || parse_timestamp(&predecessor.created_at)? >= parse_timestamp(&first.created_at)?
            {
                bail!("push-head ledger boundary predecessor is not strictly earlier");
            }
        }
        DenominatorBoundary::Bootstrap { first_run_id, before_sha } => {
            if *first_run_id != first.run_id || before_sha != &first.before_sha {
                bail!("push-head ledger bootstrap boundary does not match its first event");
            }
        }
        DenominatorBoundary::Fixture => {
            bail!("push-head ledger has a fixture boundary");
        }
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
        let pushed = observation
            .pushed_commits
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if range.is_empty() || range != pushed {
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
