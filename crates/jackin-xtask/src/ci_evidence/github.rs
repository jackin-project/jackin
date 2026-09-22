#[derive(Clone, Debug, Deserialize)]
struct ApiWorkflow {
    id: u64,
    #[serde(default)]
    path: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiRun {
    id: u64,
    #[serde(default)]
    workflow_id: Option<u64>,
    #[serde(default)]
    workflow_name: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    event: Option<String>,
    #[serde(default)]
    head_branch: Option<String>,
    head_sha: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    conclusion: Option<String>,
    #[serde(default)]
    run_attempt: u32,
    created_at: String,
    #[serde(default)]
    html_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiAttempt {
    #[serde(default)]
    run_attempt: u32,
    #[serde(default)]
    status: String,
    #[serde(default)]
    conclusion: Option<String>,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    run_started_at: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiJob {
    id: u64,
    name: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    conclusion: Option<String>,
    #[serde(default)]
    started_at: Option<String>,
    #[serde(default)]
    completed_at: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
}

fn api_pages(endpoint: &str) -> Result<Vec<serde_json::Value>> {
    let output = cmd::output(Command::new("gh").args(["api", "--paginate", "--slurp", endpoint]))?;
    serde_json::from_slice(&output).with_context(|| format!("parsing paginated API {endpoint}"))
}

fn list_runs(repository: &str, branch: &str, window: &TimeWindow) -> Result<Vec<ApiRun>> {
    let endpoint = format!(
        "repos/{repository}/actions/runs?branch={branch}&event=push&per_page=100&created={since}..{until}",
        since = api_timestamp(&window.since),
        until = api_timestamp(&window.until)
    );
    let mut runs: Vec<ApiRun> = decode_pages(&api_pages(&endpoint)?, "workflow_runs")?;
    runs.sort_by_key(|run: &ApiRun| (run.created_at.clone(), run.id));
    runs.dedup_by_key(|run| run.id);
    Ok(runs)
}

fn list_workflow_ids(
    repository: &str,
    ci_workflows: &[String],
    desktop_workflows: &[String],
) -> Result<(BTreeSet<u64>, BTreeSet<u64>, BTreeSet<u64>)> {
    let pages = api_pages(&format!(
        "repos/{repository}/actions/workflows?per_page=100"
    ))?;
    let workflows: Vec<ApiWorkflow> = decode_pages(&pages, "workflows")?;
    let ci_ids = workflows
        .iter()
        .filter(|workflow| workflow_path_matches(&workflow.path, ci_workflows))
        .map(|workflow| workflow.id)
        .collect::<BTreeSet<_>>();
    let desktop_ids = workflows
        .iter()
        .filter(|workflow| workflow_path_matches(&workflow.path, desktop_workflows))
        .map(|workflow| workflow.id)
        .collect::<BTreeSet<_>>();
    let ledger_ids = workflows
        .iter()
        .filter(|workflow| {
            workflow_path_matches(
                &workflow.path,
                &[DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned()],
            )
        })
        .map(|workflow| workflow.id)
        .collect::<BTreeSet<_>>();
    if ci_ids.is_empty() {
        bail!("configured CI/Main workflow was not found in the Actions API");
    }
    if desktop_ids.is_empty() {
        bail!("configured Desktop workflow was not found in the Actions API");
    }
    if ledger_ids.is_empty() {
        bail!(
            "durable push-head ledger workflow {DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW} was not found in the Actions API"
        );
    }
    Ok((ci_ids, desktop_ids, ledger_ids))
}

fn api_timestamp(value: &str) -> String {
    parse_timestamp(value).map_or_else(
        |_| value.to_owned(),
        |date| {
            date.with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        },
    )
}

fn list_attempts(repository: &str, run: &ApiRun) -> Result<Vec<ApiAttempt>> {
    let latest = checked_attempt_count(run.id, run.run_attempt)?;
    let mut attempts = Vec::with_capacity(latest as usize);
    for number in 1..=latest {
        let endpoint = format!(
            "repos/{repository}/actions/runs/{}/attempts/{number}",
            run.id
        );
        let output = cmd::output(Command::new("gh").args(["api", &endpoint]))?;
        let attempt: ApiAttempt = serde_json::from_slice(&output)
            .with_context(|| format!("parsing workflow run attempt {}/{}", run.id, number))?;
        if attempt.run_attempt != number {
            bail!(
                "GitHub returned attempt {} for requested run {}/{}",
                attempt.run_attempt,
                run.id,
                number
            );
        }
        attempts.push(attempt);
    }
    Ok(attempts)
}

fn checked_attempt_count(run_id: u64, run_attempt: u32) -> Result<u32> {
    if run_attempt == 0 {
        bail!("GitHub returned no run attempt number for run {run_id}");
    }
    Ok(run_attempt)
}

fn list_jobs(repository: &str, run_id: u64, attempt: u32) -> Result<Vec<ApiJob>> {
    let endpoint =
        format!("repos/{repository}/actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100");
    let mut jobs = decode_pages(&api_pages(&endpoint)?, "jobs")?;
    jobs.sort_by_key(|job: &ApiJob| job.id);
    jobs.dedup_by_key(|job| job.id);
    Ok(jobs)
}

fn decode_pages<T: for<'de> Deserialize<'de>>(
    pages: &[serde_json::Value],
    key: &str,
) -> Result<Vec<T>> {
    let mut values = Vec::new();
    for page in pages {
        if let Some(items) = page.get(key) {
            let mut page_items: Vec<T> = serde_json::from_value(items.clone())
                .with_context(|| format!("parsing `{key}` page"))?;
            values.append(&mut page_items);
        } else if page.is_array() {
            let mut page_items: Vec<T> = serde_json::from_value(page.clone())
                .with_context(|| format!("parsing `{key}` array page"))?;
            values.append(&mut page_items);
        } else {
            bail!("paginated API page has neither `{key}` nor an array");
        }
    }
    Ok(values)
}

fn classify_workflow(
    run: &ApiRun,
    ci_workflow_ids: &BTreeSet<u64>,
    desktop_workflow_ids: &BTreeSet<u64>,
) -> Option<Cohort> {
    if run
        .workflow_id
        .is_some_and(|id| ci_workflow_ids.contains(&id))
    {
        Some(Cohort::CiMain)
    } else if run
        .workflow_id
        .is_some_and(|id| desktop_workflow_ids.contains(&id))
    {
        Some(Cohort::Desktop)
    } else {
        None
    }
}

fn workflow_path_matches(path: &str, configured: &[String]) -> bool {
    let path = normalize_name(path);
    configured
        .iter()
        .map(|candidate| normalize_name(candidate))
        .any(|candidate| path == candidate)
}

fn normalize_name(value: &str) -> String {
    value
        .trim()
        .trim_start_matches(".github/workflows/")
        .to_ascii_lowercase()
        .split(['/', '_', ' ', '-'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn unclassified_run(run: &ApiRun, reason: UnclassifiedRunReason) -> UnclassifiedRun {
    UnclassifiedRun {
        run_id: run.id,
        workflow_id: run.workflow_id,
        workflow_name: run.workflow_name.clone(),
        workflow_path: run.path.clone(),
        event: run.event.clone(),
        head_sha: run.head_sha.clone(),
        created_at: run.created_at.clone(),
        evidence_url: run.html_url.clone(),
        reason,
    }
}

fn normalize_attempt(
    run: &ApiRun,
    attempt: &ApiAttempt,
    cohort: Cohort,
    jobs: Vec<ApiJob>,
    expected: &[ExpectedObligation],
    runtime: RuntimeIdentity,
) -> Result<AttemptEvidence> {
    let matching = expected
        .iter()
        .find(|obligation| obligation.commit.sha == run.head_sha && obligation.cohort == cohort)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "workflow run {} head {} is not an expected {} obligation",
                run.id,
                run.head_sha,
                cohort.label()
            )
        })?;
    let base_sha = matching.commit.base_sha.clone();
    let tree_sha = matching.commit.tree_sha.clone();
    let jobs = jobs
        .into_iter()
        .map(|job| JobEvidence {
            id: job.id,
            name: job.name,
            status: job.status,
            conclusion: job.conclusion,
            started_at: job.started_at,
            completed_at: job.completed_at,
            evidence_url: job.html_url,
        })
        .collect::<Vec<_>>();
    let observed_work = jobs.iter().map(|job| job.name.clone()).collect::<Vec<_>>();
    if attempt.run_attempt == 0 {
        bail!("workflow run {} returned an empty attempt number", run.id);
    }
    if attempt.status.is_empty() {
        bail!(
            "workflow run {} attempt {} returned no status",
            run.id,
            attempt.run_attempt
        );
    }
    if attempt.created_at.is_empty() {
        bail!(
            "workflow run {} attempt {} returned no creation timestamp",
            run.id,
            attempt.run_attempt
        );
    }
    let status = &attempt.status;
    let conclusion = attempt.conclusion.as_deref();
    let expected_work = cohort
        .expected_work()
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    let classification = classify_outcome(status, conclusion, &jobs, &expected_work);
    let raw_observation = RawAttemptObservation {
        status: status.clone(),
        conclusion: attempt.conclusion.clone(),
        jobs: jobs.clone(),
    };
    let mut evidence_urls = Vec::new();
    if let Some(url) = attempt.html_url.clone().or_else(|| run.html_url.clone()) {
        evidence_urls.push(url);
    }
    evidence_urls.extend(jobs.iter().filter_map(|job| job.evidence_url.clone()));
    evidence_urls.sort();
    evidence_urls.dedup();
    let created_at = attempt.created_at.clone();
    let started_at = attempt.run_started_at.clone();
    let completed_at = (is_terminal(status, conclusion)
        && !jobs.is_empty()
        && jobs.iter().all(|job| job.completed_at.is_some()))
    .then(|| jobs.iter().filter_map(|job| job.completed_at.clone()).max())
    .flatten();
    let duration_seconds = started_at
        .as_deref()
        .zip(completed_at.as_deref())
        .map(|(started, completed)| {
            let started = parse_timestamp(started)?;
            let ended = parse_timestamp(completed)?;
            let seconds = (ended - started).num_seconds();
            if seconds < 0 {
                bail!(
                    "workflow run {} attempt {} completed before it was created",
                    run.id,
                    attempt.run_attempt
                );
            }
            Ok(seconds)
        })
        .transpose()?;
    let attempt_number = attempt.run_attempt;
    Ok(AttemptEvidence {
        run_id: run.id,
        attempt: attempt_number,
        is_first_attempt: attempt_number == 1,
        cohort,
        workflow_id: run.workflow_id,
        workflow_name: run.workflow_name.clone(),
        workflow_path: run.path.clone(),
        event: run.event.clone(),
        head_sha: run.head_sha.clone(),
        denominator_source: matching.commit.source,
        base_sha,
        tree_sha,
        created_at,
        started_at,
        completed_at,
        within_120_seconds: duration_seconds.map(|seconds| seconds <= 120),
        duration_seconds,
        status: status.clone(),
        conclusion: attempt.conclusion.clone(),
        expected_work,
        observed_work,
        jobs,
        classification,
        data_quality_reason: None,
        conflicting_observations: Vec::new(),
        raw_observations: vec![raw_observation],
        runtime,
        evidence_urls,
        first_observed_at: now_rfc3339(),
    })
}

fn classify_outcome(
    status: &str,
    conclusion: Option<&str>,
    jobs: &[JobEvidence],
    expected_work: &[String],
) -> OutcomeClass {
    let status = status.to_ascii_lowercase();
    let conclusion = conclusion.unwrap_or_default().to_ascii_lowercase();
    if status != "completed" {
        return OutcomeClass::DataQuality;
    }
    if matches!(conclusion.as_str(), "cancelled" | "canceled")
        || matches!(status.as_str(), "cancelled" | "canceled")
    {
        return OutcomeClass::Cancellation;
    }
    if matches!(conclusion.as_str(), "skipped" | "neutral") {
        return OutcomeClass::Inapplicable;
    }
    // A run conclusion without any attempt jobs is incomplete API evidence,
    // including a misleading `success` conclusion. Treat it as an
    // infrastructure/data-collection failure so the denominator cannot turn
    // an absent job list into green.
    if jobs.is_empty() {
        return OutcomeClass::Infrastructure;
    }
    let missing_work = expected_work.iter().any(|expected| {
        let expected = normalize_name(expected);
        !jobs.iter().any(|job| normalize_name(&job.name) == expected)
    });
    if missing_work {
        return OutcomeClass::DataQuality;
    }
    let expected_job_not_success = expected_work.iter().any(|expected| {
        jobs.iter()
            .find(|job| normalize_name(&job.name) == normalize_name(expected))
            .is_some_and(|job| {
                !job.status.eq_ignore_ascii_case("completed")
                    || job.conclusion.as_deref() != Some("success")
            })
    });
    if expected_job_not_success && conclusion == "success" {
        return OutcomeClass::DataQuality;
    }
    if conclusion == "success" {
        return OutcomeClass::Success;
    }
    if matches!(
        conclusion.as_str(),
        "timed_out" | "startup_failure" | "stale" | "action_required"
    ) {
        return OutcomeClass::Infrastructure;
    }
    OutcomeClass::Product
}
