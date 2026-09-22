#[expect(
    clippy::too_many_lines,
    reason = "the validator keeps the evidence invariants in one fail-closed boundary"
)]
fn validate_evidence(evidence: &EvidenceFile) -> Result<()> {
    if evidence.schema != SCHEMA {
        bail!("unsupported evidence schema {}", evidence.schema);
    }
    validate_window(&evidence.window)?;
    let generated_at = parse_timestamp(&evidence.generated_at)?;
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
        if attempt.run_id == 0 {
            bail!("attempt has an empty workflow run identity");
        }
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
        let expected_workflow = match attempt.cohort {
            Cohort::CiMain => DEFAULT_CI_WORKFLOW,
            Cohort::Desktop => DEFAULT_DESKTOP_WORKFLOW,
        };
        if attempt.workflow_id.is_none_or(|id| id == 0)
            || attempt
                .workflow_path
                .as_deref()
                .is_none_or(|path| !workflow_path_matches(path, &[expected_workflow.to_owned()]))
            || attempt.workflow_name.as_deref().is_none_or(str::is_empty)
            || attempt.event.as_deref() != Some("push")
            || attempt.head_branch.as_deref() != Some(evidence.denominator.branch.as_str())
        {
            bail!(
                "attempt {} is not bound to the generated {} workflow and main push",
                attempt.run_id,
                expected_workflow
            );
        }
        if attempt.tree_sha.is_empty() || attempt.tree_sha != expected_commit.commit.tree_sha {
            bail!(
                "attempt {} has a missing or stale tree identity",
                attempt.run_id
            );
        }
        if attempt.base_sha != expected_commit.commit.base_sha {
            bail!(
                "attempt {} has a stale or forged base SHA",
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
        let created_at = parse_timestamp(&attempt.created_at)?;
        if created_at < parse_timestamp(&evidence.window.since)?
            || created_at > parse_timestamp(&evidence.window.until)?
        {
            bail!(
                "attempt {} was created outside the evidence window",
                attempt.run_id
            );
        }
        let first_observed_at = parse_timestamp(&attempt.first_observed_at)?;
        if first_observed_at > generated_at {
            bail!(
                "attempt {} was first observed after the evidence was generated",
                attempt.run_id
            );
        }
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
        let created_at = parse_timestamp(&run.created_at)?;
        if created_at < parse_timestamp(&evidence.window.since)?
            || created_at > parse_timestamp(&evidence.window.until)?
        {
            bail!("unclassified run {} is outside the evidence window", run.run_id);
        }
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
                || provenance
                    .workflow_ref
                    .as_deref()
                    .is_none_or(|value| !value.contains("/.github/workflows/ci-evidence.yml@"))
                || provenance
                    .workflow_sha
                    .as_deref()
                    .is_none_or(|value| !is_git_sha(value))
                || provenance.artifact_name.as_deref()
                    != Some(DEFAULT_CI_EVIDENCE_ARTIFACT.trim_end_matches('/'))
            {
                bail!("scheduled evidence provenance is not bound to main ci-evidence");
            }
        }
        "local" | "test" => {
            if provenance.workflow_ref.is_some()
                || provenance.workflow_sha.is_some()
                || provenance.artifact_name.is_some()
            {
                bail!("non-CI collection contains forged workflow provenance");
            }
        }
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
    if proof.branch != "main" || proof.window != *window {
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
            let since = parse_timestamp(&window.since)?;
            let until = parse_timestamp(&window.until)?;
            let predecessor_created_at = parse_timestamp(&boundary_predecessor.created_at)?;
            let first_created_at = parse_timestamp(&first.created_at)?;
            if predecessor_created_at >= since
                || first_created_at < since
                || first_created_at > until
            {
                bail!(
                    "push-head boundary is not adjacent to the requested collection window"
                );
            }
            if boundary_predecessor.head_sha != first.before_sha
                || boundary_predecessor.run_id == first.run_id
                || boundary_predecessor.head_sha == first.head_sha
                || push_heads.iter().any(|observation| {
                    observation.run_id == boundary_predecessor.run_id
                        || observation.head_sha == boundary_predecessor.head_sha
                })
            {
                bail!("push-head denominator boundary predecessor is not adjacent");
            }
            for pair in push_heads.windows(2) {
                let previous_created_at = parse_timestamp(&pair[0].created_at)?;
                let current_created_at = parse_timestamp(&pair[1].created_at)?;
                if current_created_at < previous_created_at
                    || (current_created_at == previous_created_at
                        && pair[1].run_id <= pair[0].run_id)
                {
                    bail!("push-head denominator order is not monotonic");
                }
            }
            for observation in push_heads {
                let created_at = parse_timestamp(&observation.created_at)?;
                if created_at < since || created_at > until {
                    bail!(
                        "push-head denominator contains an entry outside the collection window"
                    );
                }
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
    if contract.get("schema").and_then(toml::Value::as_integer) != Some(2) {
        bail!("workflow contract has an unsupported schema");
    }
    let generator = contract
        .get("generator")
        .and_then(toml::Value::as_table)
        .context("workflow contract has no generator identity")?;
    if generator.get("repository").and_then(toml::Value::as_str)
        != Some("jackin-project/jackin")
    {
        bail!("workflow contract generator repository is not jackin-project/jackin");
    }
    let revision = generator
        .get("revision")
        .and_then(toml::Value::as_str)
        .context("workflow contract has no pinned generator revision")?;
    if !is_git_sha(revision) {
        bail!("workflow contract generator revision is not a 40-character SHA");
    }
    let workflow = contract
        .get("workflow")
        .and_then(toml::Value::as_table)
        .context("workflow contract has no workflow policy")?;
    if workflow
        .get("default_branch")
        .and_then(toml::Value::as_str)
        != Some("main")
        || workflow
            .get("providers")
            .and_then(toml::Value::as_array)
            .is_none_or(|providers| providers != &[toml::Value::String("github-hosted".to_owned())])
        || workflow
            .get("automatic_providers")
            .and_then(toml::Value::as_array)
            .is_none_or(|providers| providers != &[toml::Value::String("github-hosted".to_owned())])
    {
        bail!("workflow contract provider/default-branch policy is not bound");
    }
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
        if profile.get("runner").and_then(toml::Value::as_str) != Some("github")
            || profile.get("status").and_then(toml::Value::as_str) != Some("required")
        {
            bail!("workflow contract profile {profile_id} is not a required GitHub check");
        }
        let expected_env = (profile_id == "ci-evidence").then_some("GH_TOKEN");
        if expected_env.is_some()
            && profile
                .get("env")
                .and_then(toml::Value::as_table)
                .and_then(|env| env.get("GH_TOKEN"))
                .and_then(toml::Value::as_str)
                != Some("${{ github.token }}")
        {
            bail!("workflow contract profile {profile_id} does not bind the GitHub API token");
        }
    }
    validate_declared_evidence_workflows(&contract)?;
    let state_path = root.join(WORKFLOW_STATE_PATH);
    let state = fs::read_to_string(&state_path)
        .with_context(|| format!("reading generated workflow state {}", state_path.display()))?;
    validate_generated_ownership_state(root, &state)?;
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
        if workflow_text.is_empty() || !workflow_text.starts_with("# Generated by velnor-workflow.") {
            bail!("workflow {workflow} is not the trusted generated artifact contract");
        }
        let required_fragments = if workflow == DEFAULT_CI_EVIDENCE_WORKFLOW {
            [
                "schedule:",
                "- cron: \"47 4 * * *\"",
                "workflow_dispatch:",
                "permissions:\n  contents: read",
                "run: mise run ci-evidence",
                "name: ci-evidence",
                "target/ci-evidence/",
                "if: always()",
            ]
        } else {
            [
                "push:",
                "branches: [main]",
                "workflow_dispatch:",
                "permissions:\n  contents: read",
                "run: mise run ci-push-head-ledger",
                "name: ci-push-head-ledger",
                "target/ci-push-head-ledger/",
                "if: always()",
            ]
        };
        if !required_fragments.iter().all(|fragment| workflow_text.contains(fragment)) {
            bail!("workflow {workflow} is missing a producer/artifact contract fragment");
        }
        if !mise.contains(&format!("[tasks.{task}]")) || !mise.contains(artifact) {
            bail!("task contract is missing artifact-backed task {task}");
        }
    }
    Ok(())
}

fn validate_declared_evidence_workflows(contract: &toml::Value) -> Result<()> {
    let declarations = contract
        .get("declare")
        .and_then(toml::Value::as_array)
        .context("workflow contract has no declarations")?;
    for (workflow, profile, expected_events, expected_branches) in [
        (DEFAULT_CI_EVIDENCE_WORKFLOW, "ci-evidence", &[][..], &[][..]),
        (
            DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW,
            "ci-push-head-ledger",
            &["push"][..],
            &["main"][..],
        ),
    ] {
        let declaration = declarations
            .iter()
            .find(|declaration| {
                declaration.get("primitive").and_then(toml::Value::as_str)
                    == Some("scheduled-checks")
                    && declaration.get("file").and_then(toml::Value::as_str) == Some(workflow)
            })
            .with_context(|| format!("workflow contract does not declare {workflow}"))?;
        let args = declaration
            .get("args")
            .and_then(toml::Value::as_table)
            .context("evidence workflow declaration has no args")?;
        let profiles = args
            .get("profiles")
            .and_then(toml::Value::as_array)
            .context("evidence workflow declaration has no profiles")?;
        if profiles != &[toml::Value::String(profile.to_owned())] {
            bail!("workflow {workflow} is bound to the wrong check profile");
        }
        let events = args
            .get("events")
            .and_then(toml::Value::as_array)
            .map(|values| values.iter().filter_map(toml::Value::as_str).collect::<Vec<_>>())
            .unwrap_or_default();
        if events != expected_events {
            bail!("workflow {workflow} has an unexpected event declaration");
        }
        let branches = args
            .get("branches")
            .and_then(toml::Value::as_array)
            .map(|values| values.iter().filter_map(toml::Value::as_str).collect::<Vec<_>>())
            .unwrap_or_default();
        if branches != expected_branches {
            bail!("workflow {workflow} has an unexpected branch declaration");
        }
    }
    Ok(())
}

fn validate_generated_ownership_state(root: &Path, state: &str) -> Result<()> {
    let mut lines = state.lines();
    if lines.next() != Some("# Generated ownership state; do not edit.") {
        bail!("generated workflow state header/schema is invalid");
    }
    let schema = lines
        .next()
        .and_then(|line| line.strip_prefix("schema = "))
        .and_then(|value| value.parse::<u32>().ok());
    if schema != Some(GENERATED_STATE_SCHEMA) || lines.next() != Some("[inputs]") {
        bail!("generated workflow state header/schema is invalid");
    }
    let config = state_field(&mut lines, "config")?;
    let scan = state_field(&mut lines, "scan")?;
    let generator = state_field(&mut lines, "generator")?;
    if !is_state_digest(config) || !is_state_digest(scan) {
        bail!("generated workflow state input hash is invalid");
    }
    if generator != GENERATED_STATE_GENERATOR {
        bail!("generated workflow state generator identity is invalid");
    }
    if lines.next() != Some("[outputs]") {
        bail!("generated workflow state has no output section");
    }
    let mut outputs = BTreeSet::new();
    for line in lines {
        let (relative, expected) = line
            .split_once('\t')
            .context("generated workflow state contains a malformed output row")?;
        if !outputs.insert(relative) || !is_state_digest(expected) {
            bail!("generated workflow state contains a duplicate or malformed output row");
        }
        let path = Path::new(relative);
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            bail!("generated workflow state contains an unsafe output path");
        }
        let output_path = root.join(path);
        let actual = if fs::symlink_metadata(&output_path)
            .with_context(|| format!("reading generated output metadata {relative}"))?
            .file_type()
            .is_symlink()
        {
            let target = fs::read_link(&output_path)
                .with_context(|| format!("reading generated symlink target {relative}"))?;
            fnv1a_digest(target.to_string_lossy().as_bytes())
        } else {
            let bytes = fs::read(&output_path)
                .with_context(|| format!("reading generated output {relative}"))?;
            fnv1a_digest(&bytes)
        };
        if actual != expected {
            bail!("generated workflow output hash mismatch for {relative}");
        }
    }
    for workflow in [DEFAULT_CI_EVIDENCE_WORKFLOW, DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW] {
        let relative = format!(".github/workflows/{workflow}");
        if !outputs.contains(relative.as_str()) {
            bail!("generated workflow state does not cover {relative}");
        }
    }
    Ok(())
}

fn state_field<'a>(lines: &mut impl Iterator<Item = &'a str>, name: &str) -> Result<&'a str> {
    let line = lines
        .next()
        .with_context(|| format!("generated workflow state is missing {name}"))?;
    let (actual_name, value) = line
        .split_once('\t')
        .with_context(|| format!("generated workflow state has malformed {name}"))?;
    if actual_name != name {
        bail!("generated workflow state expected {name}, got {actual_name}");
    }
    Ok(value)
}

fn is_state_digest(value: &str) -> bool {
    value.len() == 16 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn fnv1a_digest(bytes: &[u8]) -> String {
    let digest = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{digest:016x}")
}

fn validate_live_api_binding(repository: &str, evidence: &EvidenceFile) -> Result<()> {
    if env::var("GITHUB_ACTIONS").ok().as_deref() != Some("true") {
        bail!("live GitHub API binding is unavailable outside GitHub Actions");
    }
    let collector_run_id = evidence
        .provenance
        .run_id
        .context("scheduled evidence has no collector run identity")?;
    let collector_run = api_run(repository, collector_run_id)
        .context("re-fetching the scheduled evidence collector run")?;
    validate_api_run_contract(
        &collector_run,
        collector_run_id,
        "main",
        "schedule",
        DEFAULT_CI_EVIDENCE_WORKFLOW,
        None,
        evidence.provenance.workflow_sha.as_deref(),
    )?;
    for attempt in &evidence.attempts {
        let expected_workflow = match attempt.cohort {
            Cohort::CiMain => DEFAULT_CI_WORKFLOW,
            Cohort::Desktop => DEFAULT_DESKTOP_WORKFLOW,
        };
        let workflow_id = attempt
            .workflow_id
            .context("attempt has no workflow identity for live API binding")?;
        let run = api_run(repository, attempt.run_id)
            .with_context(|| format!("re-fetching attempt run {}", attempt.run_id))?;
        validate_api_run_contract(
            &run,
            attempt.run_id,
            evidence.denominator.branch.as_str(),
            "push",
            expected_workflow,
            Some(workflow_id),
            Some(attempt.head_sha.as_str()),
        )?;
        if let Some(name) = &attempt.workflow_name {
            if run.workflow_name.as_deref() != Some(name.as_str()) {
                bail!("attempt {} workflow name changed after collection", attempt.run_id);
            }
        }
        let api_attempt = api_attempt(repository, attempt.run_id, attempt.attempt)?;
        if api_attempt.run_attempt != attempt.attempt
            || api_timestamp(&api_attempt.created_at) != api_timestamp(&attempt.created_at)
        {
            bail!("attempt {} API attempt identity changed after collection", attempt.run_id);
        }
    }
    let mut live_push_heads = evidence.push_heads.iter().collect::<Vec<_>>();
    if let DenominatorBoundary::PushHead { predecessor } = &evidence.denominator.boundary {
        live_push_heads.push(predecessor);
    }
    for observation in live_push_heads {
        let run = api_run(repository, observation.run_id).with_context(|| {
            format!("re-fetching push-head ledger run {}", observation.run_id)
        })?;
        validate_api_run_contract(
            &run,
            observation.run_id,
            observation.branch.as_str(),
            "push",
            DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW,
            Some(observation.workflow_id),
            Some(observation.head_sha.as_str()),
        )?;
        if api_timestamp(&run.created_at) != api_timestamp(&observation.created_at) {
            bail!(
                "push-head ledger run {} creation time changed after collection",
                observation.run_id
            );
        }
        let artifacts = api_artifacts(repository, observation.run_id).with_context(|| {
            format!("re-fetching push-head artifact metadata for run {}", observation.run_id)
        })?;
        let matches = artifacts
            .iter()
            .filter(|artifact| artifact.name == DEFAULT_PUSH_HEAD_LEDGER_ARTIFACT)
            .collect::<Vec<_>>();
        if matches.len() != 1
            || matches[0].expired
            || matches[0].size_in_bytes == 0
            || matches[0]
                .workflow_run
                .as_ref()
                .is_none_or(|workflow_run| workflow_run.id != observation.run_id)
        {
            bail!(
                "push-head ledger run {} has no uniquely bound live artifact",
                observation.run_id
            );
        }
    }
    Ok(())
}

fn validate_api_run_contract(
    run: &ApiRun,
    expected_run_id: u64,
    branch: &str,
    event: &str,
    workflow: &str,
    workflow_id: Option<u64>,
    head_sha: Option<&str>,
) -> Result<()> {
    if run.id != expected_run_id
        || workflow_id.is_some_and(|expected| run.workflow_id != Some(expected))
        || run.event.as_deref() != Some(event)
        || run.head_branch.as_deref() != Some(branch)
        || !run
            .path
            .as_deref()
            .is_some_and(|path| workflow_path_matches(path, &[workflow.to_owned()]))
        || head_sha.is_some_and(|sha| run.head_sha != sha)
    {
        bail!(
            "GitHub API run {} is not bound to {workflow} {event} on {branch}",
            expected_run_id
        );
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
    validate_live_api_binding(&evidence.repository, evidence)?;
    Ok(())
}
