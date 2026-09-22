#[expect(
    clippy::too_many_lines,
    reason = "the validator keeps the evidence invariants in one fail-closed boundary"
)]
fn validate_evidence(evidence: &EvidenceFile) -> Result<()> {
    if evidence.schema != SCHEMA {
        bail!("unsupported evidence schema {}", evidence.schema);
    }
    validate_window(&evidence.window)?;
    validate_runtime_identity(&evidence.runtime)?;
    validate_collection_provenance(&evidence.provenance, &evidence.repository)?;
    validate_denominator(
        &evidence.repository,
        &evidence.denominator,
        &evidence.history,
        &evidence.push_heads,
        &evidence.window,
    )?;
    if evidence.denominator.source == DenominatorSource::PushHeadLedger {
        let root = docs::repo_root().context("resolving Git state for push-head evidence")?;
        validate_git_state(&root, evidence)?;
    }
    let derived_expected = expected_from_history(&evidence.history, evidence.denominator.source)?;
    if evidence.expected != derived_expected {
        bail!("expected obligations are not derived from raw denominator history");
    }
    validate_expected(&evidence.expected)?;
    let expected = evidence
        .expected
        .iter()
        .map(|obligation| (obligation.commit.sha.as_str(), obligation.cohort))
        .collect::<BTreeSet<_>>();
    let expected_sources = evidence
        .expected
        .iter()
        .map(|obligation| {
            (
                (obligation.commit.sha.as_str(), obligation.cohort),
                obligation.commit.source,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut keys = BTreeSet::new();
    for attempt in &evidence.attempts {
        if !keys.insert((attempt.run_id, attempt.attempt)) {
            bail!(
                "duplicate run attempt {} / {} in evidence",
                attempt.run_id,
                attempt.attempt
            );
        }
        if attempt.attempt == 0 || attempt.is_first_attempt != (attempt.attempt == 1) {
            bail!("invalid first-attempt marker for run {}", attempt.run_id);
        }
        if !expected.contains(&(attempt.head_sha.as_str(), attempt.cohort)) {
            bail!(
                "attempt {} / {} is not bound to an expected {} obligation",
                attempt.run_id,
                attempt.attempt,
                attempt.cohort.label()
            );
        }
        if attempt.denominator_source
            != expected_sources[&(attempt.head_sha.as_str(), attempt.cohort)]
        {
            bail!(
                "attempt {} has stale denominator provenance",
                attempt.run_id
            );
        }
        let expected_commit = evidence
            .expected
            .iter()
            .find(|obligation| {
                obligation.commit.sha == attempt.head_sha && obligation.cohort == attempt.cohort
            })
            .context("attempt expected obligation disappeared during validation")?;
        if attempt.tree_sha.is_empty() || attempt.tree_sha != expected_commit.commit.tree_sha {
            bail!(
                "attempt {} has a missing or stale tree identity",
                attempt.run_id
            );
        }
        if attempt.runtime != evidence.runtime {
            bail!("attempt {} has stale runtime provenance", attempt.run_id);
        }
        let expected_work = attempt
            .cohort
            .expected_work()
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        if attempt.expected_work != expected_work {
            bail!(
                "attempt {} has a stale expected-work contract",
                attempt.run_id
            );
        }
        if attempt.status.is_empty() || attempt.created_at.is_empty() {
            bail!(
                "attempt {} has incomplete status/timestamp evidence",
                attempt.run_id
            );
        }
        parse_timestamp(&attempt.created_at)?;
        parse_timestamp(&attempt.first_observed_at)?;
        if let Some(completed_at) = &attempt.completed_at {
            parse_timestamp(completed_at)?;
        }
        if let Some(started_at) = &attempt.started_at {
            parse_timestamp(started_at)?;
        }
        let recomputed = classify_outcome(
            &attempt.status,
            attempt.conclusion.as_deref(),
            &attempt.jobs,
            &attempt.expected_work,
        );
        let retained_observation = raw_attempt_observation(attempt);
        if attempt.raw_observations.is_empty()
            || !attempt.raw_observations.contains(&retained_observation)
            || attempt
                .raw_observations
                .iter()
                .any(|observation| observation.status.is_empty())
        {
            bail!(
                "attempt {} retained row is not represented by complete raw observations",
                attempt.run_id
            );
        }
        let terminal_observations = terminal_raw_observations(&attempt.raw_observations);
        if let Some(reason) = attempt.data_quality_reason {
            if reason != DataQualityReason::ConflictingTerminalObservation
                || attempt.classification != OutcomeClass::DataQuality
                || terminal_observations.len() < 2
                || attempt.conflicting_observations != terminal_observations
            {
                bail!(
                    "attempt {} has an unproven data-quality conflict",
                    attempt.run_id
                );
            }
            if attempt
                .conflicting_observations
                .iter()
                .any(|observation| !attempt.raw_observations.contains(observation))
                || (is_terminal(&attempt.status, attempt.conclusion.as_deref())
                    && !attempt
                        .conflicting_observations
                        .contains(&retained_observation))
            {
                bail!(
                    "attempt {} conflict marker is not represented by retained raw observations",
                    attempt.run_id
                );
            }
        } else if terminal_observations.len() > 1 || !attempt.conflicting_observations.is_empty() {
            bail!(
                "attempt {} terminal conflict provenance was cleared",
                attempt.run_id
            );
        } else if attempt.classification != recomputed {
            bail!(
                "attempt {} classification does not match its raw status, conclusion, and jobs",
                attempt.run_id
            );
        }
        if let Some(duration) = attempt.duration_seconds {
            if duration < 0 || attempt.within_120_seconds != Some(duration <= 120) {
                bail!(
                    "attempt {} has inconsistent duration evidence",
                    attempt.run_id
                );
            }
        } else if attempt.within_120_seconds.is_some() {
            bail!(
                "attempt {} has a timing verdict without a duration",
                attempt.run_id
            );
        }
        if !is_terminal(&attempt.status, attempt.conclusion.as_deref())
            && (attempt.completed_at.is_some()
                || attempt.duration_seconds.is_some()
                || attempt.within_120_seconds.is_some())
        {
            bail!(
                "active attempt {} has terminal timing evidence",
                attempt.run_id
            );
        }
    }
    let mut unclassified = BTreeSet::new();
    for run in &evidence.unclassified_runs {
        if !unclassified.insert(run.run_id) {
            bail!("duplicate unclassified run {} in evidence", run.run_id);
        }
        if run.head_sha.is_empty() || run.created_at.is_empty() {
            bail!("unclassified run {} has incomplete identity", run.run_id);
        }
        parse_timestamp(&run.created_at)?;
    }
    Ok(())
}

fn validate_collection_provenance(
    provenance: &CollectionProvenance,
    repository: &str,
) -> Result<()> {
    if provenance.repository != repository
        || provenance.branch.is_empty()
        || provenance.event.is_empty()
        || provenance.workflow_path.is_empty()
    {
        bail!("collection provenance is incomplete or bound to another repository");
    }
    match provenance.event.as_str() {
        "schedule" => {
            if provenance.branch != "main"
                || provenance.workflow_path != "ci-evidence.yml"
                || provenance.run_id.is_none()
            {
                bail!("scheduled evidence provenance is not bound to main ci-evidence");
            }
        }
        "local" | "test" => {}
        event => bail!("unsupported CI evidence collection event `{event}`"),
    }
    Ok(())
}

fn validate_push_head_observation(
    repository: &str,
    branch: &str,
    observation: &PushHeadObservation,
) -> Result<()> {
    if observation.repository != repository
        || observation.branch != branch
        || observation.event != "push"
        || observation.workflow_path != DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW
        || observation.workflow_id == 0
        || observation.run_id == 0
        || !is_git_sha(&observation.head_sha)
        || !is_git_sha(&observation.before_sha)
        || is_zero_sha(&observation.before_sha)
        || !is_git_sha(&observation.tree_sha)
        || !valid_commit_list(&observation.pushed_commits)
        || !is_hex_digest(&observation.raw_event_sha256)
        || !observation
            .pushed_commits
            .iter()
            .any(|commit| commit == &observation.head_sha)
        || observation
            .pushed_commits
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != observation.pushed_commits.len()
    {
        bail!("push-head denominator contains invalid or duplicate provenance");
    }
    if observation.artifact.manifest.is_empty() || observation.artifact.event.is_empty() {
        bail!("push-head denominator is missing retained artifact bytes");
    }
    if !is_hex_digest(&observation.artifact.manifest_sha256)
        || sha256_hex(observation.artifact.manifest.as_bytes())
            != observation.artifact.manifest_sha256
    {
        bail!("push-head denominator manifest artifact digest mismatch");
    }
    let manifest: PushHeadLedgerArtifact = serde_json::from_str(&observation.artifact.manifest)
        .context("parsing retained push-head ledger manifest")?;
    if manifest.schema != PUSH_HEAD_LEDGER_SCHEMA
        || manifest.repository != observation.repository
        || manifest.branch != observation.branch
        || manifest.event != observation.event
        || manifest.workflow_path != observation.workflow_path
        || manifest.run_id != observation.run_id
        || manifest.head_sha != observation.head_sha
        || manifest.before_sha != observation.before_sha
        || manifest.tree_sha != observation.tree_sha
        || manifest.committed_at != observation.committed_at
        || manifest.pushed_commits != observation.pushed_commits
        || manifest.raw_event_sha256 != observation.raw_event_sha256
    {
        bail!("retained push-head manifest does not match its observation");
    }
    if sha256_hex(observation.artifact.event.as_bytes()) != observation.raw_event_sha256 {
        bail!("retained push-head event artifact digest mismatch");
    }
    let raw_event: serde_json::Value = serde_json::from_str(&observation.artifact.event)
        .context("parsing retained raw push event")?;
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
        .context("retained raw push event has no commits array")?
        .iter()
        .map(|commit| {
            commit
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .context("retained raw push event contains a commit without an ID")
        })
        .collect::<Result<Vec<_>>>()?;
    let expected_ref = format!("refs/heads/{branch}");
    if raw_repository != Some(repository)
        || raw_ref != Some(expected_ref.as_str())
        || raw_before != Some(observation.before_sha.as_str())
        || raw_after != Some(observation.head_sha.as_str())
        || raw_commits != observation.pushed_commits
    {
        bail!("retained raw push event does not match its observation");
    }
    parse_timestamp(&observation.committed_at)?;
    parse_timestamp(&observation.created_at)?;
    Ok(())
}

fn validate_denominator(
    repository: &str,
    proof: &DenominatorProof,
    history: &[HistoryCommitObservation],
    push_heads: &[PushHeadObservation],
    window: &TimeWindow,
) -> Result<()> {
    if proof.branch.is_empty() || proof.window != *window {
        bail!("denominator proof does not match the evidence window or branch");
    }
    if proof.commit_count != history.len() {
        bail!("denominator proof commit count does not match history");
    }
    match proof.source {
        DenominatorSource::PushHeadLedger => {
            if !proof.fetch_succeeded
                || proof.source_workflow.as_deref() != Some(DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW)
                || proof.source_run_count != push_heads.len()
                || push_heads.is_empty()
                || history.len() != push_heads.len()
            {
                bail!("push-head denominator is missing durable source proof");
            }
            let boundary_predecessor = match &proof.boundary {
                DenominatorBoundary::PushHead { predecessor } => predecessor,
                DenominatorBoundary::Fixture => {
                    bail!("push-head denominator is missing its boundary predecessor proof")
                }
            };
            validate_push_head_observation(repository, &proof.branch, boundary_predecessor)?;
            let mut seen_heads = BTreeSet::new();
            let mut seen_runs = BTreeSet::new();
            for observation in push_heads {
                validate_push_head_observation(repository, &proof.branch, observation)?;
                if !seen_heads.insert(observation.head_sha.clone())
                    || !seen_runs.insert(observation.run_id)
                {
                    bail!("push-head denominator contains duplicate provenance");
                }
            }
            let first = push_heads
                .first()
                .context("push-head denominator has no first in-window entry")?;
            if boundary_predecessor.head_sha != first.before_sha
                || boundary_predecessor.run_id == first.run_id
                || parse_timestamp(&boundary_predecessor.created_at)?
                    >= parse_timestamp(&first.created_at)?
            {
                bail!("push-head denominator boundary predecessor is not adjacent");
            }
            if history.iter().zip(push_heads).any(|(commit, observation)| {
                commit.sha != observation.head_sha
                    || commit.base_sha.as_deref() != Some(observation.before_sha.as_str())
                    || commit.tree_sha != observation.tree_sha
                    || commit.committed_at != observation.committed_at
            }) {
                bail!("push-head denominator history is not the retained ledger");
            }
            if push_heads
                .windows(2)
                .any(|pair| pair[1].before_sha != pair[0].head_sha)
            {
                bail!("push-head denominator chain has a coverage gap");
            }
        }
        DenominatorSource::Fixture => {
            if proof.fetch_succeeded
                || proof.source_workflow.is_some()
                || proof.source_run_count != 0
                || !push_heads.is_empty()
                || !matches!(proof.boundary, DenominatorBoundary::Fixture)
            {
                bail!("fixture denominator contains durable-source provenance");
            }
        }
    }
    for commit in history {
        if commit.sha.is_empty() || commit.tree_sha.is_empty() {
            bail!("denominator commit is missing SHA/tree identity");
        }
        parse_timestamp(&commit.committed_at)?;
    }
    Ok(())
}

fn validate_git_commit_identity(
    root: &Path,
    sha: &str,
    tree_sha: &str,
    committed_at: &str,
) -> Result<()> {
    if !is_git_sha(sha) || !is_git_sha(tree_sha) {
        bail!("Git evidence contains a malformed commit or tree identity");
    }
    cmd::run(Command::new("git").current_dir(root).args([
        "cat-file",
        "-e",
        &format!("{sha}^{{commit}}"),
    ]))
    .with_context(|| format!("verifying Git commit object {sha}"))?;
    let actual_tree = required_tree_sha(root, sha)?;
    if actual_tree != tree_sha {
        bail!("Git tree identity mismatch for commit {sha}");
    }
    let actual_time = required_commit_time(root, sha)?;
    if api_timestamp(&actual_time) != api_timestamp(committed_at) {
        bail!("Git commit timestamp mismatch for commit {sha}");
    }
    Ok(())
}

fn validate_git_state(root: &Path, evidence: &EvidenceFile) -> Result<()> {
    if evidence.denominator.source != DenominatorSource::PushHeadLedger {
        bail!("Git state binding requires the durable push-head denominator");
    }
    validate_git_remote_identity(root, &evidence.repository)?;
    let status = cmd::output_string(
        Command::new("git")
            .current_dir(root)
            .args(["status", "--porcelain", "--untracked-files=all"]),
    )?;
    if !status.trim().is_empty() {
        bail!("Git state is dirty; refusing qualification");
    }
    let head = cmd::output_string(
        Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "HEAD"]),
    )?;
    let main_tip = cmd::output_string(
        Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "refs/remotes/origin/main"]),
    )?;
    let main_tip = main_tip.trim();
    if head.trim() != main_tip {
        bail!("checked-out Git head does not match origin/main");
    }
    let boundary_predecessor = match &evidence.denominator.boundary {
        DenominatorBoundary::PushHead { predecessor } => predecessor,
        DenominatorBoundary::Fixture => {
            bail!("Git state is missing the push-head boundary predecessor")
        }
    };
    validate_git_commit_identity(
        root,
        &boundary_predecessor.head_sha,
        &boundary_predecessor.tree_sha,
        &boundary_predecessor.committed_at,
    )?;
    for observation in &evidence.push_heads {
        validate_git_commit_identity(
            root,
            &observation.head_sha,
            &observation.tree_sha,
            &observation.committed_at,
        )?;
        cmd::run(Command::new("git").current_dir(root).args([
            "cat-file",
            "-e",
            &format!("{}^{{commit}}", observation.before_sha),
        ]))
        .with_context(|| format!("verifying Git predecessor {}", observation.before_sha))?;
        for pushed_commit in &observation.pushed_commits {
            cmd::run(Command::new("git").current_dir(root).args([
                "cat-file",
                "-e",
                &format!("{pushed_commit}^{{commit}}"),
            ]))
            .with_context(|| format!("verifying Git pushed commit {pushed_commit}"))?;
        }
    }
    for history in &evidence.history {
        validate_git_commit_identity(root, &history.sha, &history.tree_sha, &history.committed_at)?;
    }
    validate_push_head_chain(
        root,
        &evidence.denominator.branch,
        &evidence.push_heads,
        boundary_predecessor,
    )?;
    Ok(())
}

fn validate_workflow_contract(root: &Path) -> Result<()> {
    let contract_path = root.join(WORKFLOW_CONTRACT_PATH);
    let contract_bytes = fs::read(&contract_path)
        .with_context(|| format!("reading workflow contract {}", contract_path.display()))?;
    let contract_text = std::str::from_utf8(&contract_bytes)
        .context("workflow contract is not UTF-8")?;
    let contract: toml::Value = toml::from_str(contract_text)
        .with_context(|| format!("parsing workflow contract {}", contract_path.display()))?;
    let profiles = contract
        .get("check_profile")
        .and_then(toml::Value::as_array)
        .context("workflow contract has no check profiles")?;
    for (profile_id, task, artifact) in [
        (
            "ci-evidence",
            "ci-evidence",
            DEFAULT_CI_EVIDENCE_ARTIFACT,
        ),
        (
            "ci-push-head-ledger",
            "ci-push-head-ledger",
            "target/ci-push-head-ledger/",
        ),
    ] {
        let profile = profiles
            .iter()
            .find(|profile| profile.get("id").and_then(toml::Value::as_str) == Some(profile_id))
            .with_context(|| format!("workflow contract is missing profile {profile_id}"))?;
        let has_task = profile
            .get("tasks")
            .and_then(toml::Value::as_array)
            .is_some_and(|tasks| {
                tasks
                    .iter()
                    .any(|value| value.as_str() == Some(task))
            });
        let has_artifact = profile
            .get("artifacts")
            .and_then(toml::Value::as_array)
            .is_some_and(|artifacts| {
                artifacts
                    .iter()
                    .any(|value| value.as_str() == Some(artifact))
            });
        if !has_task || !has_artifact {
            bail!("workflow contract profile {profile_id} is not artifact-backed");
        }
    }
    let state_path = root.join(WORKFLOW_STATE_PATH);
    let state = fs::read_to_string(&state_path)
        .with_context(|| format!("reading generated workflow state {}", state_path.display()))?;
    let mise_path = root.join(MISE_PATH);
    let mise = fs::read_to_string(&mise_path)
        .with_context(|| format!("reading task contract {}", mise_path.display()))?;
    for (workflow, artifact, task) in [
        (
            DEFAULT_CI_EVIDENCE_WORKFLOW,
            "target/ci-evidence/",
            "ci-evidence",
        ),
        (
            DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW,
            "target/ci-push-head-ledger/",
            "ci-push-head-ledger",
        ),
    ] {
        let workflow_rel = format!(".github/workflows/{workflow}");
        let workflow_path = root.join(&workflow_rel);
        let workflow_bytes = fs::read(&workflow_path)
            .with_context(|| format!("reading workflow {}", workflow_path.display()))?;
        let workflow_text = std::str::from_utf8(&workflow_bytes)
            .with_context(|| format!("workflow {} is not UTF-8", workflow_path.display()))?;
        if workflow_text.is_empty()
            || !workflow_text.starts_with("# Generated by velnor-workflow.")
            || !workflow_text.contains(artifact)
            || !workflow_text.contains(task)
        {
            bail!("workflow {workflow} is not the trusted generated artifact contract");
        }
        if !state.lines().any(|line| line.starts_with(&format!("{workflow_rel}\t"))) {
            bail!("generated workflow state does not cover {workflow_rel}");
        }
        if !mise.contains(&format!("[tasks.{task}]")) || !mise.contains(artifact) {
            bail!("task contract is missing artifact-backed task {task}");
        }
    }
    if !contract_text.contains("ci-evidence.yml")
        || !contract_text.contains("ci-push-head-ledger.yml")
    {
        bail!("workflow contract does not declare the evidence workflows");
    }
    Ok(())
}

fn validate_qualification_binding(root: &Path, evidence: &EvidenceFile) -> Result<()> {
    if evidence.provenance.repository != evidence.repository
        || evidence.provenance.event != "schedule"
        || evidence.provenance.branch != "main"
        || evidence.provenance.workflow_path != DEFAULT_CI_EVIDENCE_WORKFLOW
        || evidence.provenance.run_id.is_none()
    {
        bail!("collection provenance is not the actual scheduled evidence workflow");
    }
    validate_workflow_contract(root)?;
    let runtime = runtime_identity(root, evidence.runtime.clone())?;
    if runtime != evidence.runtime {
        bail!("evidence runtime identity is not bound to the workflow contract");
    }
    validate_git_state(root, evidence)?;
    for observation in &evidence.push_heads {
        validate_push_head_observation(
            &evidence.repository,
            &evidence.denominator.branch,
            observation,
        )?;
    }
    Ok(())
}
