fn read_evidence(
    path: &Path,
    repository: &str,
    branch: &str,
    window: &TimeWindow,
    runtime: RuntimeIdentity,
) -> Result<EvidenceFile> {
    if !path.is_file() {
        return Ok(empty_evidence(repository, window, runtime));
    }
    let raw: serde_json::Value = read_json(path)?;
    let stored_schema = raw.get("schema").and_then(serde_json::Value::as_u64);
    if stored_schema != Some(u64::from(SCHEMA)) {
        // Schema 5 is a hard migration boundary: discard stale ledgers before
        // deserialization so old rows can never enter the hardened merge.
        return Ok(empty_evidence(repository, window, runtime));
    }
    let existing: EvidenceFile = serde_json::from_value(raw).context("parsing CI evidence")?;
    validate_evidence(&existing)?;
    if existing.repository != repository {
        bail!("evidence repository differs from requested repository");
    }
    if existing.provenance.repository != repository || existing.provenance.branch != branch {
        bail!("evidence collection provenance differs from requested repository/branch");
    }
    if existing.runtime != runtime {
        bail!("evidence runtime identity differs from the current collector runtime");
    }
    let existing_since = parse_timestamp(&existing.window.since)?;
    let current_since = parse_timestamp(&window.since)?;
    if parse_timestamp(&existing.window.until)? > parse_timestamp(&window.until)? {
        bail!("evidence window extends beyond the current collection window");
    }
    if existing_since > current_since {
        bail!("evidence window starts after the current collection window");
    }
    Ok(existing)
}

fn empty_evidence(repository: &str, window: &TimeWindow, runtime: RuntimeIdentity) -> EvidenceFile {
    EvidenceFile {
        schema: SCHEMA,
        repository: repository.to_owned(),
        window: window.clone(),
        generated_at: now_rfc3339(),
        runtime,
        provenance: CollectionProvenance {
            repository: repository.to_owned(),
            branch: String::new(),
            event: "local".to_owned(),
            workflow_path: "local".to_owned(),
            run_id: None,
            workflow_ref: None,
            workflow_sha: None,
            artifact_name: None,
        },
        denominator: DenominatorProof {
            source: DenominatorSource::Fixture,
            branch: String::new(),
            window: window.clone(),
            fetch_succeeded: false,
            commit_count: 0,
            source_workflow: None,
            source_run_count: 0,
            boundary: DenominatorBoundary::Fixture,
        },
        history: Vec::new(),
        push_heads: Vec::new(),
        expected: Vec::new(),
        attempts: Vec::new(),
        unclassified_runs: Vec::new(),
    }
}

struct EvidenceUpdate {
    expected: Vec<ExpectedObligation>,
    attempts: Vec<AttemptEvidence>,
    unclassified_runs: Vec<UnclassifiedRun>,
    denominator: DenominatorProof,
    history: Vec<HistoryCommitObservation>,
    push_heads: Vec<PushHeadObservation>,
    repository: String,
    window: TimeWindow,
    runtime: RuntimeIdentity,
    provenance: CollectionProvenance,
}

fn merge_evidence(mut existing: EvidenceFile, update: EvidenceUpdate) -> Result<EvidenceFile> {
    let EvidenceUpdate {
        expected,
        attempts,
        mut unclassified_runs,
        denominator,
        history,
        push_heads,
        repository,
        window,
        runtime,
        provenance,
    } = update;
    let derived_expected = expected_from_history(&history, denominator.source)?;
    if expected != derived_expected {
        bail!("expected obligations do not match raw denominator history");
    }
    validate_expected(&expected)?;
    validate_denominator(&repository, &denominator, &history, &push_heads, &window)?;
    let has_restored_rows = !existing.expected.is_empty()
        || !existing.history.is_empty()
        || !existing.attempts.is_empty();
    if has_restored_rows && existing.denominator.source != denominator.source {
        bail!("restored evidence uses a different denominator source");
    }
    if has_restored_rows
        && (existing.repository != repository
            || existing.provenance.repository != repository
            || existing.provenance.branch != provenance.branch
            || existing.runtime != runtime)
    {
        bail!("restored evidence immutable identity differs from the current collection");
    }
    let expected_keys = expected
        .iter()
        .map(|obligation| (obligation.commit.sha.clone(), obligation.cohort))
        .collect::<BTreeSet<_>>();
    let mut by_key = existing
        .attempts
        .drain(..)
        .map(|attempt| ((attempt.run_id, attempt.attempt), attempt))
        .collect::<BTreeMap<_, _>>();
    for mut attempt in attempts {
        preserve_raw_attempt_snapshot(&mut attempt);
        let key = (attempt.run_id, attempt.attempt);
        if let Some(previous) = by_key.get_mut(&key) {
            merge_attempt_observation(previous, attempt);
        } else {
            by_key.insert(key, attempt);
        }
    }
    // The collector recomputes a rolling window. Retain only attempts bound
    // to this run's denominator; otherwise a later rollup rejects stale rows
    // from a prior window as foreign obligations.
    by_key.retain(|_, attempt| expected_keys.contains(&(attempt.head_sha.clone(), attempt.cohort)));
    let mut attempts = by_key.into_values().collect::<Vec<_>>();
    attempts.sort_by_key(|attempt| (attempt.created_at.clone(), attempt.run_id, attempt.attempt));
    existing.schema = SCHEMA;
    existing.repository = repository;
    existing.window = window;
    existing.generated_at = now_rfc3339();
    existing.runtime = runtime;
    existing.provenance = provenance;
    existing.denominator = denominator;
    existing.history = history;
    existing.push_heads = push_heads;
    existing.expected = expected;
    existing.attempts = attempts;
    unclassified_runs.sort_by_key(|run| (run.created_at.clone(), run.run_id));
    unclassified_runs.dedup_by_key(|run| run.run_id);
    existing.unclassified_runs = unclassified_runs;
    validate_evidence(&existing)?;
    Ok(existing)
}

fn raw_attempt_observation(attempt: &AttemptEvidence) -> RawAttemptObservation {
    RawAttemptObservation {
        status: attempt.status.clone(),
        conclusion: attempt.conclusion.clone(),
        jobs: attempt.jobs.clone(),
    }
}

fn preserve_raw_attempt_snapshot(attempt: &mut AttemptEvidence) {
    let current = raw_attempt_observation(attempt);
    if !attempt.raw_observations.contains(&current) {
        attempt.raw_observations.push(current);
    }
    for observation in attempt.conflicting_observations.clone() {
        if !attempt.raw_observations.contains(&observation) {
            attempt.raw_observations.push(observation);
        }
    }
}

fn merge_attempt_observation(previous: &mut AttemptEvidence, incoming: AttemptEvidence) {
    let previous_raw = raw_attempt_observation(previous);
    let incoming_raw = raw_attempt_observation(&incoming);
    let first_observed_at = previous.first_observed_at.clone();
    let previous_terminal = is_terminal(&previous.status, previous.conclusion.as_deref());
    let incoming_terminal = is_terminal(&incoming.status, incoming.conclusion.as_deref());
    let keep_previous = previous_terminal && (!incoming_terminal || previous_raw != incoming_raw);
    let mut raw_observations = previous.raw_observations.clone();
    for observation in incoming.raw_observations.iter().cloned() {
        if !raw_observations.contains(&observation) {
            raw_observations.push(observation);
        }
    }
    for observation in [previous_raw, incoming_raw] {
        if !raw_observations.contains(&observation) {
            raw_observations.push(observation);
        }
    }
    let sticky_reason = previous
        .data_quality_reason
        .or(incoming.data_quality_reason);
    let terminal_observations = terminal_raw_observations(&raw_observations);
    let conflicting = sticky_reason == Some(DataQualityReason::ConflictingTerminalObservation)
        || terminal_observations.len() > 1;
    if !keep_previous {
        *previous = incoming;
    }
    previous.first_observed_at = first_observed_at;
    previous.raw_observations = raw_observations;
    if conflicting {
        previous.data_quality_reason = Some(DataQualityReason::ConflictingTerminalObservation);
        previous.conflicting_observations = terminal_observations;
        previous.classification = OutcomeClass::DataQuality;
    } else {
        previous.data_quality_reason = sticky_reason;
    }
}

fn terminal_raw_observations(observations: &[RawAttemptObservation]) -> Vec<RawAttemptObservation> {
    let mut terminal = Vec::new();
    for observation in observations {
        if is_terminal(&observation.status, observation.conclusion.as_deref())
            && !terminal.contains(observation)
        {
            terminal.push(observation.clone());
        }
    }
    terminal
}

fn is_terminal(status: &str, _conclusion: Option<&str>) -> bool {
    status.eq_ignore_ascii_case("completed")
}
