//! Durable first-attempt evidence for post-merge CI cohorts.
//!
//! GitHub pagination and workflow identities are normalized before aggregation.
//! The rollup joins observed attempts to an explicit expected obligation set,
//! so a workflow that never started is recorded as `missing` rather than
//! disappearing from the denominator.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{cmd, docs};

#[cfg(test)]
mod tests;

const SCHEMA: u32 = 3;
const DEFAULT_WINDOW_DAYS: i64 = 31;
const DEFAULT_CI_WORKFLOW: &str = "ci-main.yml";
const DEFAULT_DESKTOP_WORKFLOW: &str = "desktop-merge.yml";

/// Independently counted post-merge obligations.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, Serialize, PartialOrd)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Cohort {
    CiMain,
    Desktop,
}

impl Cohort {
    const ALL: [Self; 2] = [Self::CiMain, Self::Desktop];

    const fn label(self) -> &'static str {
        match self {
            Self::CiMain => "CI/Main",
            Self::Desktop => "Desktop",
        }
    }

    const fn expected_work(self) -> &'static [&'static str] {
        match self {
            Self::CiMain => &[
                "Control / Planning",
                "Policy",
                "ci-required",
                "Control / Required",
            ],
            Self::Desktop => &["Desktop merge cadence"],
        }
    }
}

/// Classification is evidence classification, not a claim of root cause.
/// A failed run remains non-green even when it is infrastructure or cancelled.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, Serialize, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OutcomeClass {
    Success,
    Product,
    Infrastructure,
    Cancellation,
    Missing,
    Inapplicable,
    DataQuality,
}

impl OutcomeClass {
    const fn label(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Product => "product",
            Self::Infrastructure => "infrastructure",
            Self::Cancellation => "cancellation",
            Self::Missing => "missing",
            Self::Inapplicable => "inapplicable",
            Self::DataQuality => "data_quality",
        }
    }

    const fn is_failure(self) -> bool {
        !matches!(self, Self::Success | Self::Inapplicable)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct TimeWindow {
    pub(crate) since: String,
    pub(crate) until: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct RuntimeIdentity {
    pub(crate) runtime_revision: Option<String>,
    pub(crate) contract_digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ExpectedCommit {
    pub(crate) sha: String,
    pub(crate) base_sha: Option<String>,
    pub(crate) tree_sha: Option<String>,
    pub(crate) committed_at: Option<String>,
    pub(crate) source: DenominatorSource,
}

/// Provenance of a main-branch head in the expected-work denominator.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, Serialize, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DenominatorSource {
    PushEvent,
    ObservedRunFallback,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ExpectedObligation {
    pub(crate) commit: ExpectedCommit,
    pub(crate) cohort: Cohort,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct JobEvidence {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) conclusion: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) completed_at: Option<String>,
    pub(crate) evidence_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AttemptEvidence {
    pub(crate) run_id: u64,
    pub(crate) attempt: u32,
    pub(crate) is_first_attempt: bool,
    pub(crate) cohort: Cohort,
    pub(crate) workflow_id: Option<u64>,
    pub(crate) workflow_name: Option<String>,
    pub(crate) workflow_path: Option<String>,
    pub(crate) event: Option<String>,
    pub(crate) head_sha: String,
    pub(crate) denominator_source: DenominatorSource,
    pub(crate) base_sha: Option<String>,
    pub(crate) tree_sha: Option<String>,
    pub(crate) created_at: String,
    pub(crate) started_at: Option<String>,
    pub(crate) completed_at: Option<String>,
    pub(crate) duration_seconds: Option<i64>,
    pub(crate) within_120_seconds: Option<bool>,
    pub(crate) status: String,
    pub(crate) conclusion: Option<String>,
    pub(crate) expected_work: Vec<String>,
    pub(crate) observed_work: Vec<String>,
    pub(crate) jobs: Vec<JobEvidence>,
    pub(crate) classification: OutcomeClass,
    pub(crate) data_quality_reason: Option<String>,
    pub(crate) runtime: RuntimeIdentity,
    pub(crate) evidence_urls: Vec<String>,
    /// Retained when a later collection turns a nonterminal row terminal.
    pub(crate) first_observed_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct UnclassifiedRun {
    pub(crate) run_id: u64,
    pub(crate) workflow_id: Option<u64>,
    pub(crate) workflow_name: Option<String>,
    pub(crate) workflow_path: Option<String>,
    pub(crate) event: Option<String>,
    pub(crate) head_sha: String,
    pub(crate) created_at: String,
    pub(crate) evidence_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct EventSourceGap {
    pub(crate) head_sha: String,
    pub(crate) observed_run_ids: Vec<u64>,
    pub(crate) observed_cohorts: Vec<Cohort>,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct EvidenceFile {
    pub(crate) schema: u32,
    pub(crate) repository: String,
    pub(crate) window: TimeWindow,
    pub(crate) generated_at: String,
    pub(crate) runtime: RuntimeIdentity,
    pub(crate) expected: Vec<ExpectedObligation>,
    pub(crate) attempts: Vec<AttemptEvidence>,
    pub(crate) unclassified_runs: Vec<UnclassifiedRun>,
    pub(crate) event_source_gaps: Vec<EventSourceGap>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct CohortRollup {
    pub(crate) cohort: Cohort,
    pub(crate) expected: usize,
    pub(crate) observed_first_attempts: usize,
    pub(crate) success: usize,
    pub(crate) product: usize,
    pub(crate) infrastructure: usize,
    pub(crate) cancellation: usize,
    pub(crate) missing: usize,
    pub(crate) inapplicable: usize,
    pub(crate) data_quality: usize,
    pub(crate) within_120_seconds: usize,
    pub(crate) over_120_seconds: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct CommitRollup {
    pub(crate) sha: String,
    pub(crate) statuses: BTreeMap<Cohort, OutcomeClass>,
    pub(crate) end_to_end: OutcomeClass,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct RollupFile {
    pub(crate) schema: u32,
    pub(crate) repository: String,
    pub(crate) window: TimeWindow,
    pub(crate) generated_at: String,
    pub(crate) cohorts: Vec<CohortRollup>,
    pub(crate) commits: Vec<CommitRollup>,
    pub(crate) total_first_attempt_successes: usize,
    pub(crate) total_first_attempt_failures: usize,
    pub(crate) unclassified_runs: usize,
    pub(crate) event_source_gaps: Vec<EventSourceGap>,
    pub(crate) green_claim_qualified: bool,
    pub(crate) six_nines_claimed: bool,
}

#[derive(Subcommand, Debug)]
pub(crate) enum CiEvidenceCommand {
    /// Fetch main-branch runs/attempts and append normalized evidence.
    Collect(CollectArgs),
    /// Join evidence to expected obligations and write JSON + Markdown.
    Rollup(RollupArgs),
}

#[derive(Args, Debug)]
pub(crate) struct CollectArgs {
    #[arg(long, default_value = "")]
    repository: String,
    #[arg(long)]
    since: Option<String>,
    #[arg(long)]
    until: Option<String>,
    #[arg(long, default_value = "main")]
    branch: String,
    #[arg(long, default_value = "target/ci-evidence/attempts.json")]
    output: PathBuf,
    /// Optional expected-obligation fixture. Without it, main push-event heads
    /// are fetched from GitHub and expanded into CI/Main and Desktop rows.
    #[arg(long)]
    expected: Option<PathBuf>,
    #[arg(long, value_name = "PATH", action = clap::ArgAction::Append)]
    ci_workflow: Vec<String>,
    #[arg(long, value_name = "PATH", action = clap::ArgAction::Append)]
    desktop_workflow: Vec<String>,
    #[arg(long)]
    runtime_revision: Option<String>,
    #[arg(long)]
    contract_digest: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct RollupArgs {
    #[arg(long, default_value = "target/ci-evidence/attempts.json")]
    input: PathBuf,
    #[arg(long, default_value = "target/ci-evidence/rollup.json")]
    json: PathBuf,
    #[arg(long, default_value = "target/ci-evidence/rollup.md")]
    markdown: PathBuf,
}

pub(crate) fn run(command: CiEvidenceCommand) -> Result<()> {
    match command {
        CiEvidenceCommand::Collect(args) => collect(args),
        CiEvidenceCommand::Rollup(args) => rollup(args),
    }
}

fn collect(args: CollectArgs) -> Result<()> {
    let root = docs::repo_root()?;
    let repository = nonempty_or_env(args.repository, "GITHUB_REPOSITORY")?;
    let until = args.until.unwrap_or_else(now_rfc3339);
    let since = args
        .since
        .unwrap_or_else(|| offset_rfc3339(&until, -DEFAULT_WINDOW_DAYS));
    let window = TimeWindow {
        since: since.clone(),
        until: until.clone(),
    };
    validate_window(&window)?;
    let runtime = runtime_identity(
        &root,
        RuntimeIdentity {
            runtime_revision: args
                .runtime_revision
                .or_else(|| env::var("VELNOR_WORKFLOW_REVISION").ok()),
            contract_digest: args
                .contract_digest
                .or_else(|| env::var("VELNOR_WORKFLOW_CONTRACT_DIGEST").ok()),
        },
    );
    let ci_workflows = names_or_default(args.ci_workflow, DEFAULT_CI_WORKFLOW);
    let desktop_workflows = names_or_default(args.desktop_workflow, DEFAULT_DESKTOP_WORKFLOW);
    let (ci_workflow_ids, desktop_workflow_ids) =
        list_workflow_ids(&repository, &ci_workflows, &desktop_workflows)?;
    let runs = list_runs(&repository, &args.branch, &window)?;
    let mut classified_runs = Vec::new();
    let mut attempts = Vec::new();
    let mut unclassified_runs = Vec::new();
    for run in runs {
        let Some(cohort) = classify_workflow(
            &run,
            &ci_workflow_ids,
            &desktop_workflow_ids,
            &ci_workflows,
            &desktop_workflows,
        ) else {
            unclassified_runs.push(unclassified_run(&run));
            continue;
        };
        classified_runs.push((run, cohort));
    }
    let denominator = match args.expected {
        Some(path) => ExpectedDenominator {
            expected: read_expected(&path)?,
            event_source_gaps: Vec::new(),
        },
        None => expected_from_push_events(&repository, &args.branch, &window, &classified_runs)?,
    };
    let expected = denominator.expected;
    for (run, cohort) in classified_runs {
        let run_attempts = list_attempts(&repository, &run)?;
        if run_attempts.is_empty() {
            bail!(
                "GitHub returned no attempts for run {}; refusing to omit history",
                run.id
            );
        }
        for attempt in run_attempts {
            let jobs = list_jobs(&repository, run.id, attempt.run_attempt.max(1))?;
            attempts.push(normalize_attempt(
                &run,
                &attempt,
                cohort,
                jobs,
                &expected,
                runtime.clone(),
            )?);
        }
    }
    let existing = read_evidence(&args.output, &repository, &window, runtime.clone())?;
    let merged = merge_evidence(
        existing,
        EvidenceUpdate {
            expected,
            attempts,
            unclassified_runs,
            event_source_gaps: denominator.event_source_gaps,
            repository,
            window,
            runtime,
        },
    )?;
    write_json(&args.output, &merged)?;
    print_collection_summary(&merged, &args.output)?;
    Ok(())
}

fn rollup(args: RollupArgs) -> Result<()> {
    let evidence: EvidenceFile = read_json(&args.input)?;
    validate_evidence(&evidence)?;
    let summary = build_rollup(&evidence);
    write_json(&args.json, &summary)?;
    write_markdown(&args.markdown, &summary, &evidence)?;
    print_rollup_summary(&summary)?;
    Ok(())
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn offset_rfc3339(value: &str, days: i64) -> String {
    match DateTime::parse_from_rfc3339(value) {
        Ok(date) => (date + chrono::Duration::days(days))
            .with_timezone(&Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        Err(_) => value.to_owned(),
    }
}

fn validate_window(window: &TimeWindow) -> Result<()> {
    let since = parse_timestamp(&window.since)?;
    let until = parse_timestamp(&window.until)?;
    if since > until {
        bail!("evidence window starts after it ends");
    }
    Ok(())
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.with_timezone(&Utc))
        .with_context(|| format!("parsing RFC3339 timestamp `{value}`"))
}

fn nonempty_or_env(value: String, name: &str) -> Result<String> {
    if value.is_empty() {
        env::var(name).with_context(|| format!("{name} must be set when --repository is empty"))
    } else {
        Ok(value)
    }
}

fn names_or_default(values: Vec<String>, default: &str) -> Vec<String> {
    if values.is_empty() {
        vec![default.to_owned()]
    } else {
        values
    }
}

fn runtime_identity(root: &Path, mut identity: RuntimeIdentity) -> RuntimeIdentity {
    let config = root.join(".github-gen/velnor-workflow.toml");
    if identity.runtime_revision.is_none() {
        identity.runtime_revision = fs::read_to_string(&config).ok().and_then(|contents| {
            contents.lines().find_map(|line| {
                let value = line
                    .trim()
                    .strip_prefix("revision = \"")?
                    .strip_suffix('"')?;
                (value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
                    .then(|| value.to_owned())
            })
        });
    }
    if identity.contract_digest.is_none() {
        identity.contract_digest = fs::read(&config).ok().map(|contents| {
            let mut digest = Sha256::new();
            digest.update(contents);
            hex::encode(digest.finalize())
        });
    }
    identity
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(value).context("serializing CI evidence")?;
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}

fn read_expected(path: &Path) -> Result<Vec<ExpectedObligation>> {
    let value: serde_json::Value = read_json(path)?;
    let obligations: Vec<ExpectedObligation> = if value.is_array() {
        serde_json::from_value(value).context("parsing expected obligations")?
    } else {
        value
            .get("expected")
            .cloned()
            .context("expected fixture must contain an `expected` array")
            .and_then(|value| {
                serde_json::from_value(value).context("parsing expected obligations")
            })?
    };
    validate_expected(&obligations)?;
    Ok(obligations)
}

fn validate_expected(expected: &[ExpectedObligation]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for obligation in expected {
        if obligation.commit.sha.is_empty() {
            bail!("expected obligation has an empty commit SHA");
        }
        if !seen.insert((obligation.commit.sha.clone(), obligation.cohort)) {
            bail!(
                "duplicate expected obligation for {} / {}",
                obligation.commit.sha,
                obligation.cohort.label()
            );
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ObservedHead {
    run_ids: BTreeSet<u64>,
    cohorts: BTreeSet<Cohort>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExpectedDenominator {
    expected: Vec<ExpectedObligation>,
    event_source_gaps: Vec<EventSourceGap>,
}

fn expected_from_push_events(
    repository: &str,
    branch: &str,
    window: &TimeWindow,
    observed_runs: &[(ApiRun, Cohort)],
) -> Result<ExpectedDenominator> {
    // A push event's head is the exact tree for which GitHub creates the
    // post-merge workflow. Git history alone cannot prove this mapping when a
    // single push contains multiple commits, so the event feed is the
    // denominator source. Refuse an API response truncated before the window.
    let pages = api_pages(&format!("repos/{repository}/events?per_page=100"))?;
    let events: Vec<ApiEvent> = decode_pages(&pages, "events")?;
    let since = parse_timestamp(&window.since)?;
    let until = parse_timestamp(&window.until)?;
    let oldest_event = events
        .iter()
        .filter_map(|event| parse_timestamp(&event.created_at).ok())
        .min();
    if events.len() >= 300 && oldest_event.is_some_and(|created| created > since) {
        bail!(
            "cannot derive expected obligations: GitHub event history is truncated before the evidence window"
        );
    }
    let branch_ref = format!("refs/heads/{branch}");
    let mut commits = BTreeMap::<String, ExpectedCommit>::new();
    for event in events {
        if event.event_type != "PushEvent"
            || event.payload.ref_name.as_deref() != Some(branch_ref.as_str())
        {
            continue;
        }
        let created_at = parse_timestamp(&event.created_at)?;
        if created_at < since || created_at > until {
            continue;
        }
        let Some(head_sha) = event.payload.head else {
            bail!("main push event {} has no head SHA", event.id);
        };
        if head_sha.is_empty() {
            bail!("main push event {} has an empty head SHA", event.id);
        }
        let base_sha = event
            .payload
            .before
            .filter(|sha| !sha.is_empty() && !sha.chars().all(|character| character == '0'));
        commits.entry(head_sha.clone()).or_insert(ExpectedCommit {
            sha: head_sha,
            base_sha,
            tree_sha: None,
            committed_at: Some(event.created_at),
            source: DenominatorSource::PushEvent,
        });
    }
    let mut observed_heads = BTreeMap::<String, ObservedHead>::new();
    for (run, cohort) in observed_runs {
        let observed = observed_heads.entry(run.head_sha.clone()).or_default();
        observed.run_ids.insert(run.id);
        observed.cohorts.insert(*cohort);
    }
    let (commits, event_source_gaps) = merge_observed_heads(commits, observed_heads);
    let mut expected = Vec::with_capacity(commits.len() * Cohort::ALL.len());
    for commit in commits.into_values() {
        for cohort in Cohort::ALL {
            expected.push(ExpectedObligation {
                commit: commit.clone(),
                cohort,
            });
        }
    }
    validate_expected(&expected)?;
    Ok(ExpectedDenominator {
        expected,
        event_source_gaps,
    })
}

fn merge_observed_heads(
    mut commits: BTreeMap<String, ExpectedCommit>,
    observed_heads: BTreeMap<String, ObservedHead>,
) -> (BTreeMap<String, ExpectedCommit>, Vec<EventSourceGap>) {
    let mut event_source_gaps = Vec::new();
    for (head_sha, observed) in observed_heads {
        if commits.contains_key(&head_sha) {
            continue;
        }
        commits.insert(
            head_sha.clone(),
            ExpectedCommit {
                sha: head_sha.clone(),
                base_sha: None,
                tree_sha: None,
                committed_at: None,
                source: DenominatorSource::ObservedRunFallback,
            },
        );
        event_source_gaps.push(EventSourceGap {
            head_sha,
            observed_run_ids: observed.run_ids.into_iter().collect(),
            observed_cohorts: observed.cohorts.into_iter().collect(),
            reason: "observed main-branch workflow run head was absent from the GitHub push-event feed; both cohort obligations were reconstructed from observed runs".to_owned(),
        });
    }
    (commits, event_source_gaps)
}

#[derive(Clone, Debug, Deserialize)]
struct ApiWorkflow {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiEvent {
    id: String,
    #[serde(rename = "type")]
    event_type: String,
    created_at: String,
    #[serde(default)]
    payload: ApiPushPayload,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct ApiPushPayload {
    #[serde(rename = "ref")]
    ref_name: Option<String>,
    head: Option<String>,
    before: Option<String>,
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
    head_sha: String,
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
    updated_at: Option<String>,
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
    let mut runs = decode_pages(&api_pages(&endpoint)?, "workflow_runs")?;
    runs.sort_by_key(|run: &ApiRun| (run.created_at.clone(), run.id));
    runs.dedup_by_key(|run| run.id);
    Ok(runs)
}

fn list_workflow_ids(
    repository: &str,
    ci_workflows: &[String],
    desktop_workflows: &[String],
) -> Result<(BTreeSet<u64>, BTreeSet<u64>)> {
    let pages = api_pages(&format!(
        "repos/{repository}/actions/workflows?per_page=100"
    ))?;
    let workflows: Vec<ApiWorkflow> = decode_pages(&pages, "workflows")?;
    let ci_ids = workflows
        .iter()
        .filter(|workflow| {
            matches_workflow_definition(
                Some(&workflow.path),
                Some(&workflow.name),
                ci_workflows,
                &["ci-main", "ci/main", "ci main"],
            )
        })
        .map(|workflow| workflow.id)
        .collect::<BTreeSet<_>>();
    let desktop_ids = workflows
        .iter()
        .filter(|workflow| {
            matches_workflow_definition(
                Some(&workflow.path),
                Some(&workflow.name),
                desktop_workflows,
                &["desktop-merge", "desktop merge", "desktop cadence"],
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
    Ok((ci_ids, desktop_ids))
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
    let latest = run.run_attempt.max(1);
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
    ci_workflows: &[String],
    desktop_workflows: &[String],
) -> Option<Cohort> {
    if run
        .workflow_id
        .is_some_and(|id| ci_workflow_ids.contains(&id))
        || matches_workflow(run, ci_workflows, &["ci-main", "ci/main", "ci main"])
    {
        Some(Cohort::CiMain)
    } else if run
        .workflow_id
        .is_some_and(|id| desktop_workflow_ids.contains(&id))
        || matches_workflow(
            run,
            desktop_workflows,
            &["desktop-merge", "desktop merge", "desktop cadence"],
        )
    {
        Some(Cohort::Desktop)
    } else {
        None
    }
}

fn matches_workflow(run: &ApiRun, configured: &[String], semantic_names: &[&str]) -> bool {
    matches_workflow_definition(
        run.path.as_deref(),
        run.workflow_name.as_deref(),
        configured,
        semantic_names,
    )
}

fn matches_workflow_definition(
    path: Option<&str>,
    name: Option<&str>,
    configured: &[String],
    semantic_names: &[&str],
) -> bool {
    let path = path.map(normalize_name);
    let name = name.map(normalize_name);
    configured.iter().any(|candidate| {
        let candidate = normalize_name(candidate);
        path.as_deref() == Some(candidate.as_str()) || name.as_deref() == Some(candidate.as_str())
    }) || semantic_names.iter().any(|candidate| {
        let candidate = normalize_name(candidate);
        path.as_deref() == Some(candidate.as_str()) || name.as_deref() == Some(candidate.as_str())
    })
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

fn unclassified_run(run: &ApiRun) -> UnclassifiedRun {
    UnclassifiedRun {
        run_id: run.id,
        workflow_id: run.workflow_id,
        workflow_name: run.workflow_name.clone(),
        workflow_path: run.path.clone(),
        event: run.event.clone(),
        head_sha: run.head_sha.clone(),
        created_at: run.created_at.clone(),
        evidence_url: run.html_url.clone(),
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
    let mut evidence_urls = Vec::new();
    if let Some(url) = attempt.html_url.clone().or_else(|| run.html_url.clone()) {
        evidence_urls.push(url);
    }
    evidence_urls.extend(jobs.iter().filter_map(|job| job.evidence_url.clone()));
    evidence_urls.sort();
    evidence_urls.dedup();
    let created_at = attempt.created_at.clone();
    let completed_at = attempt.updated_at.clone();
    let duration_seconds = completed_at
        .as_deref()
        .map(|completed| {
            let started = parse_timestamp(&created_at)?;
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
        started_at: attempt.run_started_at.clone(),
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
    if conclusion.is_empty() && status != "completed" {
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

fn read_evidence(
    path: &Path,
    repository: &str,
    window: &TimeWindow,
    runtime: RuntimeIdentity,
) -> Result<EvidenceFile> {
    if !path.is_file() {
        return Ok(EvidenceFile {
            schema: SCHEMA,
            repository: repository.to_owned(),
            window: window.clone(),
            generated_at: now_rfc3339(),
            runtime,
            expected: Vec::new(),
            attempts: Vec::new(),
            unclassified_runs: Vec::new(),
            event_source_gaps: Vec::new(),
        });
    }
    let existing: EvidenceFile = read_json(path)?;
    validate_evidence(&existing)?;
    if existing.repository != repository {
        bail!("evidence repository differs from requested repository");
    }
    Ok(existing)
}

struct EvidenceUpdate {
    expected: Vec<ExpectedObligation>,
    attempts: Vec<AttemptEvidence>,
    unclassified_runs: Vec<UnclassifiedRun>,
    event_source_gaps: Vec<EventSourceGap>,
    repository: String,
    window: TimeWindow,
    runtime: RuntimeIdentity,
}

fn merge_evidence(mut existing: EvidenceFile, update: EvidenceUpdate) -> Result<EvidenceFile> {
    let EvidenceUpdate {
        expected,
        attempts,
        unclassified_runs,
        event_source_gaps,
        repository,
        window,
        runtime,
    } = update;
    validate_expected(&expected)?;
    let expected_keys = expected
        .iter()
        .map(|obligation| (obligation.commit.sha.clone(), obligation.cohort))
        .collect::<BTreeSet<_>>();
    let expected_heads = expected
        .iter()
        .map(|obligation| obligation.commit.sha.clone())
        .collect::<BTreeSet<_>>();
    let mut by_key = existing
        .attempts
        .drain(..)
        .map(|attempt| ((attempt.run_id, attempt.attempt), attempt))
        .collect::<BTreeMap<_, _>>();
    for attempt in attempts {
        let key = (attempt.run_id, attempt.attempt);
        if let Some(previous) = by_key.get_mut(&key) {
            // A delayed API response must not replace a terminal verdict with
            // an older in-progress snapshot. The collector is append-only for
            // the identity `(run_id, attempt)`; only a nonterminal row may be
            // completed by a later terminal observation.
            if is_terminal(&previous.status, previous.conclusion.as_deref())
                && !is_terminal(&attempt.status, attempt.conclusion.as_deref())
            {
                continue;
            }
            if is_terminal(&previous.status, previous.conclusion.as_deref())
                && is_terminal(&attempt.status, attempt.conclusion.as_deref())
                && (previous.classification != attempt.classification
                    || previous.conclusion != attempt.conclusion)
            {
                previous.classification = OutcomeClass::DataQuality;
                previous.data_quality_reason =
                    Some("conflicting terminal observations for one run attempt".to_owned());
                continue;
            }
            let first_observed_at = previous.first_observed_at.clone();
            *previous = attempt;
            previous.first_observed_at = first_observed_at;
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
    existing.expected = expected;
    existing.attempts = attempts;
    let mut gaps_by_head = existing
        .event_source_gaps
        .drain(..)
        .map(|gap| (gap.head_sha.clone(), gap))
        .collect::<BTreeMap<_, _>>();
    for gap in event_source_gaps {
        if let Some(previous) = gaps_by_head.get_mut(&gap.head_sha) {
            previous.observed_run_ids.extend(gap.observed_run_ids);
            previous.observed_run_ids.sort_unstable();
            previous.observed_run_ids.dedup();
            previous.observed_cohorts.extend(gap.observed_cohorts);
            previous.observed_cohorts.sort_unstable();
            previous.observed_cohorts.dedup();
            previous.reason = gap.reason;
        } else {
            gaps_by_head.insert(gap.head_sha.clone(), gap);
        }
    }
    gaps_by_head.retain(|head_sha, _| expected_heads.contains(head_sha));
    existing.event_source_gaps = gaps_by_head.into_values().collect();
    let mut by_run = existing
        .unclassified_runs
        .drain(..)
        .map(|run| (run.run_id, run))
        .collect::<BTreeMap<_, _>>();
    for run in unclassified_runs {
        by_run.insert(run.run_id, run);
    }
    by_run.retain(|_, run| {
        parse_timestamp(&run.created_at).is_ok_and(|created| {
            let since = parse_timestamp(&existing.window.since).ok();
            let until = parse_timestamp(&existing.window.until).ok();
            since.is_none_or(|since| created >= since) && until.is_none_or(|until| created <= until)
        })
    });
    existing.unclassified_runs = by_run.into_values().collect();
    rebind_attempt_sources(&mut existing);
    validate_evidence(&existing)?;
    Ok(existing)
}

fn rebind_attempt_sources(evidence: &mut EvidenceFile) {
    let sources = evidence
        .expected
        .iter()
        .map(|obligation| {
            (
                (obligation.commit.sha.as_str(), obligation.cohort),
                obligation.commit.source,
            )
        })
        .collect::<BTreeMap<_, _>>();
    for attempt in &mut evidence.attempts {
        if let Some(source) = sources.get(&(attempt.head_sha.as_str(), attempt.cohort)) {
            attempt.denominator_source = *source;
        }
    }
}

fn is_terminal(status: &str, conclusion: Option<&str>) -> bool {
    conclusion.is_some() || status.eq_ignore_ascii_case("completed")
}

#[expect(
    clippy::too_many_lines,
    reason = "the validator keeps the evidence invariants in one fail-closed boundary"
)]
fn validate_evidence(evidence: &EvidenceFile) -> Result<()> {
    if evidence.schema != SCHEMA {
        bail!("unsupported evidence schema {}", evidence.schema);
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
    let mut gap_heads = BTreeSet::new();
    for gap in &evidence.event_source_gaps {
        if !gap_heads.insert(gap.head_sha.as_str()) {
            bail!("duplicate event-source gap for {}", gap.head_sha);
        }
        if gap.head_sha.is_empty()
            || gap.observed_run_ids.is_empty()
            || gap.observed_cohorts.is_empty()
            || gap.reason.trim().is_empty()
        {
            bail!(
                "event-source gap for {} has incomplete provenance",
                gap.head_sha
            );
        }
        if !evidence
            .expected
            .iter()
            .any(|obligation| obligation.commit.sha == gap.head_sha)
        {
            bail!(
                "event-source gap {} is not bound to an expected head",
                gap.head_sha
            );
        }
        let mut cohorts = BTreeSet::new();
        for cohort in &gap.observed_cohorts {
            if !cohorts.insert(*cohort) {
                bail!("event-source gap {} repeats a cohort", gap.head_sha);
            }
            if !expected.contains(&(gap.head_sha.as_str(), *cohort)) {
                bail!(
                    "event-source gap {} names an unexpected {} cohort",
                    gap.head_sha,
                    cohort.label()
                );
            }
        }
        // A later event-feed read may resolve a previously recorded gap;
        // retaining that warning is valid historical provenance.
    }
    for obligation in &evidence.expected {
        if obligation.commit.source == DenominatorSource::ObservedRunFallback
            && !gap_heads.contains(obligation.commit.sha.as_str())
        {
            bail!(
                "fallback denominator head {} has no event-source gap provenance",
                obligation.commit.sha
            );
        }
    }
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
        if attempt.data_quality_reason.is_none() && attempt.classification != recomputed {
            bail!(
                "attempt {} classification does not match its raw status, conclusion, and jobs",
                attempt.run_id
            );
        }
        if attempt
            .data_quality_reason
            .as_deref()
            .is_some_and(str::is_empty)
        {
            bail!(
                "attempt {} has an empty data-quality reason",
                attempt.run_id
            );
        }
        if attempt.data_quality_reason.is_some()
            && attempt.classification != OutcomeClass::DataQuality
        {
            bail!(
                "attempt {} has a reason without a data-quality classification",
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

fn build_rollup(evidence: &EvidenceFile) -> RollupFile {
    let mut first_by_obligation = BTreeMap::<(String, Cohort), Vec<&AttemptEvidence>>::new();
    for attempt in evidence
        .attempts
        .iter()
        .filter(|attempt| attempt.is_first_attempt)
    {
        let key = (attempt.head_sha.clone(), attempt.cohort);
        first_by_obligation.entry(key).or_default().push(attempt);
    }
    let mut cohorts = Vec::new();
    for cohort in Cohort::ALL {
        let obligations = evidence
            .expected
            .iter()
            .filter(|obligation| obligation.cohort == cohort)
            .collect::<Vec<_>>();
        let mut summary = CohortRollup {
            cohort,
            expected: obligations.len(),
            observed_first_attempts: 0,
            success: 0,
            product: 0,
            infrastructure: 0,
            cancellation: 0,
            missing: 0,
            inapplicable: 0,
            data_quality: 0,
            within_120_seconds: 0,
            over_120_seconds: 0,
        };
        for obligation in obligations {
            let status = first_by_obligation
                .get(&(obligation.commit.sha.clone(), cohort))
                .map_or(OutcomeClass::Missing, |attempts| {
                    if attempts.len() == 1 {
                        attempts[0].classification
                    } else {
                        OutcomeClass::DataQuality
                    }
                });
            if status != OutcomeClass::Missing {
                summary.observed_first_attempts += 1;
            }
            if let Some(attempts) =
                first_by_obligation.get(&(obligation.commit.sha.clone(), cohort))
            {
                let (within, over) = timing_counts(attempts);
                summary.within_120_seconds += within;
                summary.over_120_seconds += over;
            }
            match status {
                OutcomeClass::Success => summary.success += 1,
                OutcomeClass::Product => summary.product += 1,
                OutcomeClass::Infrastructure => summary.infrastructure += 1,
                OutcomeClass::Cancellation => summary.cancellation += 1,
                OutcomeClass::Missing => summary.missing += 1,
                OutcomeClass::Inapplicable => summary.inapplicable += 1,
                OutcomeClass::DataQuality => summary.data_quality += 1,
            }
        }
        cohorts.push(summary);
    }
    let mut commits_by_sha = BTreeMap::<String, BTreeMap<Cohort, OutcomeClass>>::new();
    for obligation in &evidence.expected {
        let status = first_by_obligation
            .get(&(obligation.commit.sha.clone(), obligation.cohort))
            .map_or(OutcomeClass::Missing, |attempts| {
                if attempts.len() == 1 {
                    attempts[0].classification
                } else {
                    OutcomeClass::DataQuality
                }
            });
        commits_by_sha
            .entry(obligation.commit.sha.clone())
            .or_default()
            .insert(obligation.cohort, status);
    }
    let commits = commits_by_sha
        .into_iter()
        .map(|(sha, statuses)| {
            let end_to_end = if statuses.values().any(|status| status.is_failure()) {
                statuses
                    .values()
                    .copied()
                    .find(|status| status.is_failure())
                    .unwrap_or(OutcomeClass::DataQuality)
            } else if statuses
                .values()
                .any(|status| *status == OutcomeClass::Inapplicable)
            {
                OutcomeClass::Inapplicable
            } else {
                OutcomeClass::Success
            };
            CommitRollup {
                sha,
                statuses,
                end_to_end,
            }
        })
        .collect::<Vec<_>>();
    let total_first_attempt_successes = cohorts.iter().map(|cohort| cohort.success).sum();
    let total_first_attempt_failures = cohorts
        .iter()
        .map(|cohort| {
            cohort.product
                + cohort.infrastructure
                + cohort.cancellation
                + cohort.missing
                + cohort.data_quality
        })
        .sum();
    let green_claim_qualified = total_first_attempt_failures == 0
        && !evidence.expected.is_empty()
        && evidence.event_source_gaps.is_empty()
        && evidence.unclassified_runs.is_empty();
    RollupFile {
        schema: SCHEMA,
        repository: evidence.repository.clone(),
        window: evidence.window.clone(),
        generated_at: now_rfc3339(),
        cohorts,
        commits,
        total_first_attempt_successes,
        total_first_attempt_failures,
        unclassified_runs: evidence.unclassified_runs.len(),
        event_source_gaps: evidence.event_source_gaps.clone(),
        green_claim_qualified,
        six_nines_claimed: false,
    }
}

fn timing_counts(attempts: &[&AttemptEvidence]) -> (usize, usize) {
    attempts.iter().fold((0, 0), |(within, over), attempt| {
        match attempt.within_120_seconds {
            Some(true) => (within + 1, over),
            Some(false) => (within, over + 1),
            None => (within, over),
        }
    })
}

fn write_markdown(path: &Path, rollup: &RollupFile, evidence: &EvidenceFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut text = String::new();
    text.push_str("# CI first-attempt rollup\n\n");
    text.push_str(&format!(
        "Repository: `{}`\n\nWindow: `{}` → `{}`\n\n",
        rollup.repository, rollup.window.since, rollup.window.until
    ));
    text.push_str(
        "This is an observed first-attempt ledger. Reruns remain in the input evidence and do not replace a first-attempt verdict. Six-nines is not claimed.\n\n",
    );
    text.push_str("## Cohorts\n\n| Cohort | Expected | Observed first | Success | Product | Infrastructure | Cancellation | Missing | Inapplicable | Data quality | ≤120s | >120s |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for cohort in &rollup.cohorts {
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            cohort.cohort.label(),
            cohort.expected,
            cohort.observed_first_attempts,
            cohort.success,
            cohort.product,
            cohort.infrastructure,
            cohort.cancellation,
            cohort.missing,
            cohort.inapplicable,
            cohort.data_quality,
            cohort.within_120_seconds,
            cohort.over_120_seconds
        ));
    }
    text.push_str(&format!(
        "\nUnclassified workflow runs: {}\nEvent-source gaps: {}\nQualified green claim: {}\n",
        rollup.unclassified_runs,
        rollup.event_source_gaps.len(),
        rollup.green_claim_qualified
    ));
    if !rollup.event_source_gaps.is_empty() {
        text.push_str(
            "\nEvent-source warnings: observed main-branch heads absent from the push-event feed were added as fallback denominator heads. This blocks a qualified green claim and any six-nines interpretation.\n\n",
        );
        for gap in &rollup.event_source_gaps {
            text.push_str(&format!(
                "- `{}`: observed runs {:?}; cohorts {:?}; {}\n",
                gap.head_sha, gap.observed_run_ids, gap.observed_cohorts, gap.reason
            ));
        }
    }
    text.push_str("\n## End-to-end per commit\n\n| Commit | CI/Main | Desktop | End-to-end |\n| --- | --- | --- | --- |\n");
    for commit in &rollup.commits {
        let short_sha = &commit.sha[..commit.sha.len().min(12)];
        text.push_str(&format!(
            "| `{short_sha}` | {} | {} | {} |\n",
            commit
                .statuses
                .get(&Cohort::CiMain)
                .map_or("missing", |status| status.label()),
            commit
                .statuses
                .get(&Cohort::Desktop)
                .map_or("missing", |status| status.label()),
            commit.end_to_end.label()
        ));
    }
    text.push_str(&format!(
        "\nObserved first-attempt successes: {}\nObserved first-attempt failures or missing obligations: {}\nStored run attempts: {}\n",
        rollup.total_first_attempt_successes,
        rollup.total_first_attempt_failures,
        evidence.attempts.len()
    ));
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

fn print_collection_summary(evidence: &EvidenceFile, path: &Path) -> Result<()> {
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "collected {} expected obligations, {} unique attempts, and {} event-source gaps into {}",
        evidence.expected.len(),
        evidence.attempts.len(),
        evidence.event_source_gaps.len(),
        path.display()
    )?;
    Ok(())
}

fn print_rollup_summary(rollup: &RollupFile) -> Result<()> {
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "rollup: {} successes, {} failures/missing; six-nines claim: false",
        rollup.total_first_attempt_successes, rollup.total_first_attempt_failures
    )?;
    Ok(())
}
