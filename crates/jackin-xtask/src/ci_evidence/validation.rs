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
            let mut seen_heads = BTreeSet::new();
            let mut seen_runs = BTreeSet::new();
            for observation in push_heads {
                if observation.repository != repository
                    || observation.branch != proof.branch
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
                    || !seen_heads.insert(observation.head_sha.clone())
                    || !seen_runs.insert(observation.run_id)
                {
                    bail!("push-head denominator contains invalid or duplicate provenance");
                }
                parse_timestamp(&observation.committed_at)?;
                parse_timestamp(&observation.created_at)?;
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
