const API_PAGE_SIZE: usize = 100;
const MAX_API_PAGES: usize = 10;
const MAX_API_PAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_RUN_ATTEMPTS: u32 = 20;

#[derive(Clone, Debug, Deserialize)]
struct ApiWorkflow {
    id: u64,
    #[serde(default)]
    path: String,
    #[serde(default)]
    state: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiArtifact {
    id: u64,
    name: String,
    #[serde(default)]
    digest: Option<String>,
    #[serde(default)]
    expired: bool,
    #[serde(default)]
    size_in_bytes: u64,
    #[serde(default)]
    workflow_run: Option<ApiArtifactRun>,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiArtifactRun {
    id: u64,
    #[serde(default)]
    head_branch: Option<String>,
    #[serde(default)]
    head_sha: Option<String>,
    #[serde(default)]
    repository_id: Option<u64>,
    #[serde(default)]
    head_repository_id: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiRepository {
    id: u64,
    full_name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiRunRepository {
    id: u64,
    full_name: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiRef {
    #[serde(rename = "ref")]
    name: String,
    object: ApiGitObject,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiGitObject {
    sha: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiRun {
    id: u64,
    #[serde(default)]
    repository: Option<ApiRunRepository>,
    #[serde(default)]
    head_repository: Option<ApiRunRepository>,
    #[serde(default)]
    workflow_id: Option<u64>,
    /// GitHub's `name` is the workflow run name (`run-name`, or the
    /// workflow's `name` when no `run-name` is declared). The jobs API binds
    /// its `workflow_name` to this value.
    #[serde(default)]
    name: Option<String>,
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
    id: u64,
    #[serde(default)]
    workflow_id: Option<u64>,
    #[serde(default)]
    head_sha: String,
    #[serde(default)]
    head_branch: Option<String>,
    #[serde(default)]
    repository: Option<ApiRunRepository>,
    #[serde(default)]
    head_repository: Option<ApiRunRepository>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    event: Option<String>,
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
    #[serde(default)]
    run_id: Option<u64>,
    #[serde(default)]
    run_attempt: Option<u32>,
    #[serde(default)]
    head_sha: Option<String>,
    #[serde(default)]
    head_branch: Option<String>,
    #[serde(default)]
    workflow_name: Option<String>,
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

fn api_pages(
    endpoint: &str,
    parameters: &[(&str, String)],
    response_key: &str,
) -> Result<Vec<serde_json::Value>> {
    let mut pages = Vec::new();
    let mut collected = 0_usize;
    let mut total_count = None;
    for page_number in 1..=MAX_API_PAGES {
        let arguments = api_page_arguments(endpoint, parameters, page_number, API_PAGE_SIZE);
        let output = cmd::output_timeout_limited(
            Command::new("gh").args(&arguments),
            Duration::from_secs(90),
            MAX_API_PAGE_BYTES,
            MAX_API_STDERR_BYTES,
        )?;
        if output.len() > MAX_API_PAGE_BYTES {
            bail!("GitHub API page {page_number} exceeds the response-size limit");
        }
        let response: serde_json::Value = serde_json::from_slice(&output)
            .with_context(|| format!("parsing GitHub API page {page_number} for {endpoint}"))?;
        let (count, page_total) = api_page_count(&response, response_key)?;
        if let Some(page_total) = page_total {
            if page_total > MAX_API_PAGES * API_PAGE_SIZE {
                bail!("GitHub API result for {endpoint} exceeds the 1,000-item safety limit");
            }
            if total_count.is_some_and(|prior| prior != page_total) {
                bail!("GitHub API result count changed while paging {endpoint}");
            }
            total_count = Some(page_total);
        }
        collected = collected
            .checked_add(count)
            .context("GitHub API result count overflowed")?;
        if total_count.is_some_and(|total| collected > total) {
            bail!("GitHub API returned more {response_key} items than total_count");
        }
        pages.push(response);
        if page_is_complete(count, collected, total_count, endpoint)? {
            return Ok(pages);
        }
    }
    bail!("GitHub API search for {endpoint} reached the 1,000-result safety limit")
}

fn api_page_arguments(
    endpoint: &str,
    parameters: &[(&str, String)],
    page_number: usize,
    page_size: usize,
) -> Vec<String> {
    let mut arguments = vec![
        "api".to_owned(),
        "--method".to_owned(),
        "GET".to_owned(),
        endpoint.to_owned(),
    ];
    for (name, value) in parameters {
        arguments.push("-f".to_owned());
        arguments.push(format!("{name}={value}"));
    }
    arguments.push("-F".to_owned());
    arguments.push(format!("per_page={page_size}"));
    arguments.push("-F".to_owned());
    arguments.push(format!("page={page_number}"));
    arguments
}

fn api_page_count(
    response: &serde_json::Value,
    response_key: &str,
) -> Result<(usize, Option<usize>)> {
    let items = if response.is_array() {
        response.as_array()
    } else {
        response.get(response_key).and_then(serde_json::Value::as_array)
    }
    .with_context(|| format!("GitHub API response has no `{response_key}` array"))?;
    let total = if response.is_array() {
        None
    } else {
        Some(
            response
                .get("total_count")
                .and_then(serde_json::Value::as_u64)
                .context("GitHub API list response has no numeric total_count")?
                .try_into()
                .context("GitHub API total_count does not fit in memory bounds")?,
        )
    };
    Ok((items.len(), total))
}

fn page_is_complete(
    page_count: usize,
    collected: usize,
    total_count: Option<usize>,
    endpoint: &str,
) -> Result<bool> {
    if let Some(total) = total_count {
        if collected == total {
            return Ok(true);
        }
        if page_count < API_PAGE_SIZE {
            bail!("GitHub API returned a short {endpoint} page before total_count items");
        }
        return Ok(false);
    }
    Ok(page_count < API_PAGE_SIZE)
}

fn api_single_page(
    endpoint: &str,
    parameters: &[(&str, String)],
    response_key: &str,
    page_size: usize,
) -> Result<serde_json::Value> {
    if page_size == 0 || page_size > API_PAGE_SIZE {
        bail!("single GitHub API page size is outside the configured bound");
    }
    let arguments = api_page_arguments(endpoint, parameters, 1, page_size);
    let output = cmd::output_timeout_limited(
        Command::new("gh").args(arguments),
        Duration::from_secs(90),
        MAX_API_PAGE_BYTES,
        MAX_API_STDERR_BYTES,
    )?;
    if output.len() > MAX_API_PAGE_BYTES {
        bail!("GitHub API page for {endpoint} exceeds the response-size limit");
    }
    let response: serde_json::Value = serde_json::from_slice(&output)
        .with_context(|| format!("parsing GitHub API page for {endpoint}"))?;
    let (count, total) = api_page_count(&response, response_key)?;
    if count > page_size || total.is_some_and(|total| total < count) {
        bail!("GitHub API returned an invalid bounded page for {endpoint}");
    }
    Ok(response)
}

fn api_run(repository: &str, run_id: u64) -> Result<ApiRun> {
    let endpoint = format!("repos/{repository}/actions/runs/{run_id}");
    let output = cmd::output_timeout_limited(
        Command::new("gh").args(["api", &endpoint]),
        Duration::from_secs(90),
        MAX_API_PAGE_BYTES,
        MAX_API_STDERR_BYTES,
    )?;
    let run: ApiRun =
        serde_json::from_slice(&output).with_context(|| format!("parsing API run {run_id}"))?;
    validate_api_run_repository(&run)?;
    Ok(run)
}

fn api_attempt(repository: &str, run_id: u64, attempt: u32) -> Result<ApiAttempt> {
    let endpoint = format!("repos/{repository}/actions/runs/{run_id}/attempts/{attempt}");
    let output = cmd::output_timeout_limited(
        Command::new("gh").args(["api", &endpoint]),
        Duration::from_secs(90),
        MAX_API_PAGE_BYTES,
        MAX_API_STDERR_BYTES,
    )?;
    let api_attempt: ApiAttempt = serde_json::from_slice(&output)
        .with_context(|| format!("parsing API run attempt {run_id}/{attempt}"))?;
    if api_attempt.id != run_id
        || api_attempt.run_attempt != attempt
        || !is_git_sha(&api_attempt.head_sha)
        || api_attempt.repository.as_ref().is_none_or(|repository| {
            repository.id != TARGET_REPOSITORY_ID
                || !repository.full_name.eq_ignore_ascii_case(TARGET_REPOSITORY)
        })
        || api_attempt.head_repository.as_ref().is_none_or(|repository| {
            repository.id != TARGET_REPOSITORY_ID
                || !repository.full_name.eq_ignore_ascii_case(TARGET_REPOSITORY)
        })
    {
        bail!("GitHub returned a run attempt with a different repository or identity");
    }
    Ok(api_attempt)
}

fn api_repository(repository: &str) -> Result<ApiRepository> {
    let endpoint = format!("repos/{repository}");
    let output = cmd::output_timeout_limited(
        Command::new("gh").args(["api", &endpoint]),
        Duration::from_secs(90),
        MAX_API_PAGE_BYTES,
        MAX_API_STDERR_BYTES,
    )?;
    let identity: ApiRepository = serde_json::from_slice(&output)
        .context("parsing GitHub repository identity")?;
    if identity.id != TARGET_REPOSITORY_ID
        || !identity.full_name.eq_ignore_ascii_case(TARGET_REPOSITORY)
        || !identity.full_name.eq_ignore_ascii_case(repository)
    {
        bail!("GitHub API returned a different repository identity");
    }
    Ok(identity)
}

fn api_branch_tip(repository: &str, branch: &str) -> Result<String> {
    validate_branch_name(branch)?;
    let endpoint = format!("repos/{repository}/git/ref/heads/{branch}");
    let output = cmd::output_timeout_limited(
        Command::new("gh").args(["api", &endpoint]),
        Duration::from_secs(90),
        MAX_API_PAGE_BYTES,
        MAX_API_STDERR_BYTES,
    )?;
    let reference: ApiRef = serde_json::from_slice(&output)
        .with_context(|| format!("parsing GitHub branch ref {branch}"))?;
    let expected_ref = format!("refs/heads/{branch}");
    if reference.name != expected_ref || reference.object.kind != "commit" || !is_git_sha(&reference.object.sha) {
        bail!("GitHub returned an invalid branch ref for {branch}");
    }
    Ok(reference.object.sha)
}

fn api_artifacts(repository: &str, run_id: u64) -> Result<Vec<ApiArtifact>> {
    let endpoint = format!("repos/{repository}/actions/runs/{run_id}/artifacts");
    decode_pages(&api_pages(&endpoint, &[], "artifacts")?, "artifacts")
}

fn list_runs(repository: &str, branch: &str, window: &TimeWindow) -> Result<Vec<ApiRun>> {
    validate_branch_name(branch)?;
    let endpoint = format!("repos/{repository}/actions/runs");
    let parameters = vec![
        ("branch", branch.to_owned()),
        ("event", "push".to_owned()),
        (
            "created",
            format!("{}..{}", api_timestamp(&window.since), api_timestamp(&window.until)),
        ),
    ];
    let mut runs: Vec<ApiRun> =
        decode_pages(&api_pages(&endpoint, &parameters, "workflow_runs")?, "workflow_runs")?;
    for run in &runs {
        validate_api_run_repository(run)?;
    }
    runs.sort_by_key(|run: &ApiRun| (run.created_at.clone(), run.id));
    runs.dedup_by_key(|run| run.id);
    Ok(runs)
}

fn list_workflow_ids(
    repository: &str,
    ci_workflows: &[String],
    desktop_workflows: &[String],
) -> Result<(BTreeSet<u64>, BTreeSet<u64>, BTreeSet<u64>)> {
    let pages = api_pages(
        &format!("repos/{repository}/actions/workflows"),
        &[],
        "workflows",
    )?;
    let workflows: Vec<ApiWorkflow> = decode_pages(&pages, "workflows")?;
    let ci_ids = unique_workflow_id(&workflows, ci_workflows, "CI/Main")?;
    let desktop_ids = unique_workflow_id(&workflows, desktop_workflows, "Desktop")?;
    let ledger_ids = unique_workflow_id(
        &workflows,
        &[DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned()],
        "push-head ledger",
    )?;
    Ok((ci_ids, desktop_ids, ledger_ids))
}

fn unique_workflow_id(
    workflows: &[ApiWorkflow],
    configured: &[String],
    label: &str,
) -> Result<BTreeSet<u64>> {
    let ids = workflows
        .iter()
        .filter(|workflow| workflow.state == "active")
        .filter(|workflow| workflow_path_matches(&workflow.path, configured))
        .map(|workflow| workflow.id)
        .collect::<BTreeSet<_>>();
    if ids.len() != 1 {
        bail!("expected exactly one active {label} workflow, found {}", ids.len());
    }
    Ok(ids)
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
        let attempt = api_attempt(repository, run.id, number)?;
        validate_api_attempt_binding(run, &attempt)?;
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
    if run_attempt == 0 || run_attempt > MAX_RUN_ATTEMPTS {
        bail!("GitHub returned an invalid attempt count for run {run_id}");
    }
    Ok(run_attempt)
}

fn list_jobs(repository: &str, run: &ApiRun, attempt: u32) -> Result<Vec<ApiJob>> {
    let endpoint = format!("repos/{repository}/actions/runs/{}/attempts/{attempt}/jobs", run.id);
    let mut jobs: Vec<ApiJob> = decode_pages(&api_pages(&endpoint, &[], "jobs")?, "jobs")?;
    validate_api_jobs(run, attempt, &jobs)?;
    jobs.sort_by_key(|job: &ApiJob| job.id);
    if jobs.windows(2).any(|pair| pair[0].id == pair[1].id) {
        bail!("GitHub returned duplicate job identities for one workflow attempt");
    }
    Ok(jobs)
}

fn validate_api_jobs(run: &ApiRun, attempt: u32, jobs: &[ApiJob]) -> Result<()> {
    if attempt == 0
        || jobs.iter().any(|job| {
            job.run_id != Some(run.id)
                || job.run_attempt != Some(attempt)
                || job.head_sha.as_deref() != Some(run.head_sha.as_str())
                || job.head_branch != run.head_branch
                || job.workflow_name != run.name
        })
    {
        bail!("GitHub returned jobs bound to a different workflow run or attempt");
    }
    Ok(())
}

fn validate_api_run_repository(run: &ApiRun) -> Result<()> {
    for (label, identity) in [
        ("repository", run.repository.as_ref()),
        ("head repository", run.head_repository.as_ref()),
    ] {
        let identity = identity.with_context(|| format!("workflow run {} has no {label} identity", run.id))?;
        if identity.id != TARGET_REPOSITORY_ID
            || !identity.full_name.eq_ignore_ascii_case(TARGET_REPOSITORY)
        {
            bail!("workflow run {} is bound to a different {label}", run.id);
        }
    }
    Ok(())
}

fn validate_api_attempt_binding(run: &ApiRun, attempt: &ApiAttempt) -> Result<()> {
    if attempt.id != run.id
        || attempt.workflow_id != run.workflow_id
        || attempt.head_sha != run.head_sha
        || attempt.head_branch != run.head_branch
        || attempt.name != run.name
        || !run.head_branch.as_deref().is_some_and(|branch| {
            workflow_paths_equivalent(attempt.path.as_deref(), run.path.as_deref(), branch)
        })
        || attempt.event != run.event
    {
        bail!("workflow run {} attempt has a different run/workflow/head identity", run.id);
    }
    for (label, identity) in [
        ("repository", attempt.repository.as_ref()),
        ("head repository", attempt.head_repository.as_ref()),
    ] {
        let identity = identity.with_context(|| format!("workflow attempt {} has no {label} identity", run.id))?;
        if identity.id != TARGET_REPOSITORY_ID
            || !identity.full_name.eq_ignore_ascii_case(TARGET_REPOSITORY)
        {
            bail!("workflow attempt {} is bound to a different {label}", run.id);
        }
    }
    Ok(())
}

fn workflow_paths_equivalent(left: Option<&str>, right: Option<&str>, branch: &str) -> bool {
    fn normalize<'a>(path: &'a str, branch: &str) -> Option<&'a str> {
        let (file, reference) = match path.split_once('@') {
            Some((file, reference)) if !reference.is_empty() => (file, Some(reference)),
            Some(_) => return None,
            None => (path, None),
        };
        if file.is_empty()
            || reference.is_some_and(|reference| {
                reference != branch && reference != format!("refs/heads/{branch}")
            })
        {
            return None;
        }
        Some(file)
    }

    left.zip(right)
        .and_then(|(left, right)| Some((normalize(left, branch)?, normalize(right, branch)?)))
        .is_some_and(|(left, right)| left == right)
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
    let path = path.split_once('@').map_or(path, |(path, _)| path);
    configured.iter().any(|candidate| {
        let expected = if candidate.starts_with(".github/workflows/") {
            candidate.clone()
        } else {
            format!(".github/workflows/{candidate}")
        };
        path == expected
    })
}

fn workflow_ref_matches(path: &str, workflow: &str, branch: &str) -> bool {
    let expected = format!(".github/workflows/{workflow}");
    let Some((actual_path, actual_ref)) = path.split_once('@') else {
        return path == expected;
    };
    actual_path == expected
        && (actual_ref == branch || actual_ref == format!("refs/heads/{branch}"))
}

fn workflow_blob_sha(root: &Path, commit: &str, workflow: &str) -> Result<String> {
    if !is_git_sha(commit) {
        bail!("workflow revision is not a full Git commit SHA");
    }
    let object = format!("{commit}:.github/workflows/{workflow}");
    let object_type = cmd::output_string(
        Command::new("git")
            .current_dir(root)
            .args(["cat-file", "-t", &object]),
    )?;
    if object_type.trim() != "blob" {
        bail!("workflow file at {commit} is not a Git blob");
    }
    let sha = cmd::output_string(
        Command::new("git")
            .current_dir(root)
            .args(["rev-parse", "--verify", &object]),
    )?;
    let sha = sha.trim();
    if !is_git_sha(sha) {
        bail!("workflow file at {commit} has no valid Git blob identity");
    }
    Ok(sha.to_owned())
}

fn workflow_run_name_at(
    root: &Path,
    commit: &str,
    workflow: &str,
    event: &str,
    branch: &str,
) -> Result<String> {
    if !is_git_sha(commit) {
        bail!("workflow run head is not a full Git commit SHA");
    }
    let object = format!("{commit}:.github/workflows/{workflow}");
    let bytes = cmd::output(
        Command::new("git")
            .current_dir(root)
            .args(["show", &object]),
    )?;
    if bytes.len() > 1024 * 1024 {
        bail!("workflow file at {commit} exceeds the identity size limit");
    }
    let workflow_yaml: serde_json::Value = serde_yaml_ng::from_slice(&bytes)
        .with_context(|| format!("parsing workflow file at {commit}"))?;
    let template = workflow_yaml
        .get("run-name")
        .and_then(serde_json::Value::as_str)
        .or_else(|| workflow_yaml.get("name").and_then(serde_json::Value::as_str))
        .context("historical workflow has no run name or workflow name")?;
    let name = template
        .replace("${{ github.event_name }}", event)
        .replace("${{ github.ref_name }}", branch);
    if name.contains("${{") || name.is_empty() {
        bail!("workflow run name uses an unsupported expression");
    }
    Ok(name)
}

fn validate_branch_name(branch: &str) -> Result<()> {
    if branch.is_empty()
        || branch.starts_with('-')
        || branch.contains(['?', '*', '[', '\\', '^', ':', '~', ' '])
        || branch.contains("..")
        || branch.contains("@{")
        || branch.ends_with('/')
        || branch.ends_with('.')
        || branch.split('/').any(|part| {
            part.is_empty()
                || part.starts_with('.')
                || Path::new(part)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("lock"))
        })
    {
        bail!("branch is not a valid Git ref name");
    }
    Ok(())
}

fn unclassified_run(run: &ApiRun, reason: UnclassifiedRunReason) -> UnclassifiedRun {
    UnclassifiedRun {
        run_id: run.id,
        workflow_id: run.workflow_id,
        workflow_name: run.name.clone(),
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
    let workflow_file_sha = if matching.commit.source == DenominatorSource::PushHeadLedger {
        let workflow = match cohort {
            Cohort::CiMain => DEFAULT_CI_WORKFLOW,
            Cohort::Desktop => DEFAULT_DESKTOP_WORKFLOW,
        };
        Some(workflow_blob_sha(&docs::repo_root()?, &run.head_sha, workflow)?)
    } else {
        None
    };
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
        workflow_name: run.name.clone(),
        workflow_path: run.path.clone(),
        workflow_file_sha,
        event: run.event.clone(),
        head_branch: run.head_branch.clone(),
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
    let expected_matches = expected_work
        .iter()
        .map(|expected| (expected, jobs.iter().filter(|job| job.name == *expected).count()))
        .collect::<Vec<_>>();
    if expected_matches.iter().any(|(_, count)| *count == 0) {
        return OutcomeClass::DataQuality;
    }
    if expected_matches.iter().any(|(_, count)| *count != 1) {
        return OutcomeClass::DataQuality;
    }
    let expected_job_not_success = expected_work.iter().any(|expected| {
        jobs.iter().find(|job| job.name == *expected).is_some_and(|job| {
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
