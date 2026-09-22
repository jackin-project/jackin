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

const SCHEMA: u32 = 4;
const DEFAULT_WINDOW_DAYS: i64 = 31;
const DEFAULT_CI_WORKFLOW: &str = "ci-main.yml";
const DEFAULT_DESKTOP_WORKFLOW: &str = "desktop-merge.yml";
const DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW: &str = "ci-push-head-ledger.yml";
const DEFAULT_PUSH_HEAD_LEDGER_ARTIFACT: &str = "ci-push-head-ledger";

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
        !matches!(self, Self::Success)
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
    pub(crate) tree_sha: String,
    pub(crate) committed_at: Option<String>,
    pub(crate) source: DenominatorSource,
}

/// Provenance of a main-branch head in the expected-work denominator.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, Serialize, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DenominatorSource {
    PushHeadLedger,
    Fixture,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, Serialize, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ObligationProvenance {
    PushHeadLedger,
    Fixture,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct DenominatorProof {
    pub(crate) source: DenominatorSource,
    pub(crate) branch: String,
    pub(crate) window: TimeWindow,
    pub(crate) fetch_succeeded: bool,
    pub(crate) commit_count: usize,
    pub(crate) source_workflow: Option<String>,
    pub(crate) source_run_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct HistoryCommitObservation {
    pub(crate) sha: String,
    pub(crate) base_sha: Option<String>,
    pub(crate) tree_sha: String,
    pub(crate) committed_at: String,
}

/// One durable push event and its producer-run binding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PushHeadObservation {
    pub(crate) repository: String,
    pub(crate) branch: String,
    pub(crate) event: String,
    pub(crate) workflow_id: u64,
    pub(crate) workflow_path: String,
    pub(crate) run_id: u64,
    pub(crate) head_sha: String,
    pub(crate) before_sha: String,
    pub(crate) tree_sha: String,
    pub(crate) committed_at: String,
    pub(crate) created_at: String,
    pub(crate) pushed_commits: Vec<String>,
    pub(crate) raw_event_sha256: String,
}

/// Provenance of the collector invocation that wrote an evidence file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct CollectionProvenance {
    pub(crate) repository: String,
    pub(crate) branch: String,
    pub(crate) event: String,
    pub(crate) workflow_path: String,
    pub(crate) run_id: Option<u64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum DataQualityReason {
    ConflictingTerminalObservation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct ExpectedObligation {
    pub(crate) commit: ExpectedCommit,
    pub(crate) cohort: Cohort,
    pub(crate) provenance: ObligationProvenance,
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
pub(crate) struct RawAttemptObservation {
    pub(crate) status: String,
    pub(crate) conclusion: Option<String>,
    pub(crate) jobs: Vec<JobEvidence>,
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
    pub(crate) tree_sha: String,
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
    pub(crate) data_quality_reason: Option<DataQualityReason>,
    pub(crate) conflicting_observations: Vec<RawAttemptObservation>,
    pub(crate) raw_observations: Vec<RawAttemptObservation>,
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
    pub(crate) reason: UnclassifiedRunReason,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, Serialize, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UnclassifiedRunReason {
    UnknownWorkflowId,
    OutsideDenominator,
    ContaminatedProvenance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct EvidenceFile {
    pub(crate) schema: u32,
    pub(crate) repository: String,
    pub(crate) window: TimeWindow,
    pub(crate) generated_at: String,
    pub(crate) runtime: RuntimeIdentity,
    pub(crate) provenance: CollectionProvenance,
    pub(crate) denominator: DenominatorProof,
    pub(crate) history: Vec<HistoryCommitObservation>,
    pub(crate) push_heads: Vec<PushHeadObservation>,
    pub(crate) expected: Vec<ExpectedObligation>,
    pub(crate) attempts: Vec<AttemptEvidence>,
    pub(crate) unclassified_runs: Vec<UnclassifiedRun>,
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
    pub(crate) denominator: DenominatorProof,
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
    /// Optional expected-obligation fixture. Without it, expected obligations
    /// are derived only from the durable main push-head ledger.
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
    let repository = canonical_repository(&nonempty_or_env(args.repository, "GITHUB_REPOSITORY")?)?;
    validate_git_remote_identity(&root, &repository)?;
    let until = args.until.unwrap_or_else(now_rfc3339);
    let since = args
        .since
        .unwrap_or_else(|| offset_rfc3339(&until, -DEFAULT_WINDOW_DAYS));
    let window = TimeWindow {
        since: since.clone(),
        until: until.clone(),
    };
    validate_window(&window)?;
    let provenance = collection_provenance(&repository, &args.branch)?;
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
    let (ci_workflow_ids, desktop_workflow_ids, ledger_workflow_ids) =
        list_workflow_ids(&repository, &ci_workflows, &desktop_workflows)?;
    let denominator = match args.expected {
        Some(path) => denominator_from_fixture(read_expected(&path)?, &args.branch, &window)?,
        None => expected_from_push_head_ledger(
            &root,
            &repository,
            &args.branch,
            &window,
            &ledger_workflow_ids,
        )?,
    };
    let expected = denominator.expected.clone();
    let runs = list_runs(&repository, &args.branch, &window)?;
    let mut attempts = Vec::new();
    let mut unclassified_runs = Vec::new();
    for run in runs {
        if run
            .workflow_id
            .is_some_and(|id| ledger_workflow_ids.contains(&id))
        {
            continue;
        }
        if run.event.as_deref() != Some("push")
            || run.head_branch.as_deref() != Some(args.branch.as_str())
        {
            unclassified_runs.push(unclassified_run(
                &run,
                UnclassifiedRunReason::ContaminatedProvenance,
            ));
            continue;
        }
        let Some(cohort) = classify_workflow(&run, &ci_workflow_ids, &desktop_workflow_ids) else {
            unclassified_runs.push(unclassified_run(
                &run,
                UnclassifiedRunReason::UnknownWorkflowId,
            ));
            continue;
        };
        if !expected
            .iter()
            .any(|obligation| obligation.commit.sha == run.head_sha && obligation.cohort == cohort)
        {
            unclassified_runs.push(unclassified_run(
                &run,
                UnclassifiedRunReason::OutsideDenominator,
            ));
            continue;
        }
        let run_attempts = list_attempts(&repository, &run)?;
        if run_attempts.is_empty() {
            bail!(
                "GitHub returned no attempts for run {}; refusing to omit history",
                run.id
            );
        }
        for attempt in run_attempts {
            let jobs = list_jobs(&repository, run.id, attempt.run_attempt)?;
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
    let existing = read_evidence(
        &args.output,
        &repository,
        &args.branch,
        &window,
        runtime.clone(),
    )?;
    let merged = merge_evidence(
        existing,
        EvidenceUpdate {
            expected,
            attempts,
            unclassified_runs,
            denominator: denominator.denominator,
            history: denominator.history,
            push_heads: denominator.push_heads,
            repository,
            window,
            runtime,
            provenance,
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
    require_qualified(&summary)
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

fn canonical_repository(value: &str) -> Result<String> {
    let value = value.trim().trim_matches('/');
    let mut parts = value.split('/');
    let owner = parts.next().unwrap_or_default();
    let name = parts.next().unwrap_or_default().trim_end_matches(".git");
    if owner.is_empty()
        || name.is_empty()
        || parts.next().is_some()
        || owner == "."
        || owner == ".."
        || name == "."
        || name == ".."
    {
        bail!("repository must be an owner/name identity, got `{value}`");
    }
    Ok(format!("{owner}/{name}"))
}

fn remote_repository_identity(url: &str) -> Result<String> {
    let url = url.trim().trim_end_matches('/');
    let path = if let Some(path) = url.strip_prefix("git@github.com:") {
        path
    } else if let Some(path) = url.strip_prefix("https://github.com/") {
        path
    } else if let Some(path) = url.strip_prefix("http://github.com/") {
        path
    } else if let Some(path) = url.strip_prefix("ssh://git@github.com/") {
        path
    } else if let Some(path) = url.strip_prefix("git://github.com/") {
        path
    } else {
        bail!("origin remote is not a supported github.com repository URL: `{url}`");
    };
    canonical_repository(path)
}

fn validate_git_remote_identity(root: &Path, repository: &str) -> Result<()> {
    let remote = cmd::output_string(
        Command::new("git")
            .current_dir(root)
            .args(["remote", "get-url", "origin"]),
    )?;
    let remote_repository = remote_repository_identity(&remote)?;
    if remote_repository != repository {
        bail!("git origin `{remote_repository}` does not match --repository `{repository}`");
    }
    Ok(())
}

fn collection_provenance(repository: &str, branch: &str) -> Result<CollectionProvenance> {
    if env::var("GITHUB_ACTIONS").ok().as_deref() != Some("true") {
        return Ok(CollectionProvenance {
            repository: repository.to_owned(),
            branch: branch.to_owned(),
            event: "local".to_owned(),
            workflow_path: "local".to_owned(),
            run_id: None,
        });
    }
    let event = env::var("GITHUB_EVENT_NAME").context("GITHUB_EVENT_NAME is missing")?;
    let ref_name = env::var("GITHUB_REF_NAME").context("GITHUB_REF_NAME is missing")?;
    let workflow_ref = env::var("GITHUB_WORKFLOW_REF").context("GITHUB_WORKFLOW_REF is missing")?;
    let workflow_path = workflow_ref
        .split_once("/.github/workflows/")
        .and_then(|(_, suffix)| suffix.split_once('@').map(|(path, _)| path.to_owned()))
        .context("GITHUB_WORKFLOW_REF has no workflow path")?;
    let run_id = env::var("GITHUB_RUN_ID")
        .context("GITHUB_RUN_ID is missing")?
        .parse::<u64>()
        .context("GITHUB_RUN_ID is not a number")?;
    if event != "schedule"
        || ref_name != branch
        || branch != "main"
        || workflow_path != "ci-evidence.yml"
    {
        bail!(
            "CI evidence must run as the main scheduled ci-evidence workflow; event={event}, ref={ref_name}, workflow={workflow_path}"
        );
    }
    Ok(CollectionProvenance {
        repository: repository.to_owned(),
        branch: branch.to_owned(),
        event,
        workflow_path,
        run_id: Some(run_id),
    })
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

fn validate_runtime_identity(identity: &RuntimeIdentity) -> Result<()> {
    let revision = identity
        .runtime_revision
        .as_deref()
        .context("runtime revision proof is missing")?;
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("runtime revision proof is not a 40-character hexadecimal identity");
    }
    let digest = identity
        .contract_digest
        .as_deref()
        .context("contract digest proof is missing")?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("contract digest proof is not a 64-character hexadecimal identity");
    }
    Ok(())
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

fn denominator_from_fixture(
    expected: Vec<ExpectedObligation>,
    branch: &str,
    window: &TimeWindow,
) -> Result<ExpectedDenominator> {
    let expected = expected
        .into_iter()
        .map(|obligation| ExpectedObligation {
            commit: ExpectedCommit {
                source: DenominatorSource::Fixture,
                ..obligation.commit
            },
            cohort: obligation.cohort,
            provenance: ObligationProvenance::Fixture,
        })
        .collect::<Vec<_>>();
    let mut history = BTreeMap::<String, HistoryCommitObservation>::new();
    for obligation in &expected {
        history
            .entry(obligation.commit.sha.clone())
            .or_insert_with(|| HistoryCommitObservation {
                sha: obligation.commit.sha.clone(),
                base_sha: obligation.commit.base_sha.clone(),
                tree_sha: obligation.commit.tree_sha.clone(),
                committed_at: obligation
                    .commit
                    .committed_at
                    .clone()
                    .unwrap_or_else(now_rfc3339),
            });
    }
    let denominator = ExpectedDenominator {
        expected,
        denominator: DenominatorProof {
            source: DenominatorSource::Fixture,
            branch: branch.to_owned(),
            window: window.clone(),
            fetch_succeeded: false,
            commit_count: history.len(),
            source_workflow: None,
            source_run_count: 0,
        },
        history: history.into_values().collect(),
        push_heads: Vec::new(),
    };
    validate_expected(&denominator.expected)?;
    Ok(denominator)
}

fn validate_expected(expected: &[ExpectedObligation]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for obligation in expected {
        if obligation.commit.sha.is_empty() {
            bail!("expected obligation has an empty commit SHA");
        }
        if obligation.commit.tree_sha.is_empty() {
            bail!(
                "expected obligation {} / {} has no tree identity",
                obligation.commit.sha,
                obligation.cohort.label()
            );
        }
        if !seen.insert((obligation.commit.sha.clone(), obligation.cohort)) {
            bail!(
                "duplicate expected obligation for {} / {}",
                obligation.commit.sha,
                obligation.cohort.label()
            );
        }
        let expected_source = match obligation.provenance {
            ObligationProvenance::PushHeadLedger => DenominatorSource::PushHeadLedger,
            ObligationProvenance::Fixture => DenominatorSource::Fixture,
        };
        if obligation.commit.source != expected_source {
            bail!(
                "{} / {} has inconsistent denominator and obligation provenance",
                obligation.commit.sha,
                obligation.cohort.label()
            );
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ExpectedDenominator {
    expected: Vec<ExpectedObligation>,
    denominator: DenominatorProof,
    history: Vec<HistoryCommitObservation>,
    push_heads: Vec<PushHeadObservation>,
}

fn required_tree_sha(root: &Path, sha: &str) -> Result<String> {
    let tree = cmd::output_string(
        Command::new("git")
            .current_dir(root)
            .args(["rev-parse", &format!("{sha}^{{tree}}")]),
    )?;
    let tree = tree.trim();
    if tree.is_empty() {
        bail!("commit {sha} has no tree identity");
    }
    Ok(tree.to_owned())
}

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
    let mut observations = Vec::with_capacity(runs.len());
    for run in runs {
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
            || run.path.as_deref().is_some_and(|path| {
                !workflow_path_matches(path, &[DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW.to_owned()])
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
    validate_push_head_chain(root, branch, &observations)?;
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
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(temp.path()).context("reading push-head artifact contents")? {
        let entry = entry.context("reading push-head artifact entry")?;
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
        fs::read(temp.path().join("push-head.json")).context("reading push-head manifest")?,
        fs::read(temp.path().join("event.json")).context("reading raw push event")?,
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
    if manifest.before_sha.is_empty()
        || manifest.head_sha.is_empty()
        || manifest.tree_sha.is_empty()
        || manifest.pushed_commits.is_empty()
        || manifest.raw_event_sha256.is_empty()
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
    if raw_repository != Some(repository)
        || raw_ref != Some(format!("refs/heads/{branch}").as_str())
        || raw_before != Some(manifest.before_sha.as_str())
        || raw_after != Some(manifest.head_sha.as_str())
        || raw_commits != manifest.pushed_commits
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

fn validate_push_head_chain(
    root: &Path,
    branch: &str,
    observations: &[PushHeadObservation],
) -> Result<()> {
    let mut seen_runs = BTreeSet::new();
    let mut seen_heads = BTreeSet::new();
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
        // Schema 4 is a hard migration boundary: discard stale ledgers before
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
        },
        denominator: DenominatorProof {
            source: DenominatorSource::Fixture,
            branch: String::new(),
            window: window.clone(),
            fetch_succeeded: false,
            commit_count: 0,
            source_workflow: None,
            source_run_count: 0,
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
    if !existing.expected.is_empty() && existing.denominator.source != denominator.source {
        bail!("restored evidence uses a different denominator source");
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
                    || observation.run_id == 0
                    || observation.head_sha.is_empty()
                    || observation.before_sha.is_empty()
                    || observation.tree_sha.is_empty()
                    || observation.pushed_commits.is_empty()
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
            if let Some(attempts) = first_by_obligation
                .get(&(obligation.commit.sha.clone(), cohort))
                .filter(|attempts| attempts.len() == 1)
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
                + cohort.inapplicable
                + cohort.data_quality
        })
        .sum();
    let green_claim_qualified = total_first_attempt_failures == 0
        && !evidence.expected.is_empty()
        && evidence.denominator.source == DenominatorSource::PushHeadLedger
        && evidence.denominator.fetch_succeeded
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
        denominator: evidence.denominator.clone(),
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

fn require_qualified(rollup: &RollupFile) -> Result<()> {
    let mut reasons = Vec::new();
    if rollup.denominator.source != DenominatorSource::PushHeadLedger {
        reasons.push("denominator is not the durable push-head ledger");
    }
    if !rollup.denominator.fetch_succeeded {
        reasons.push("denominator fetch proof is incomplete");
    }
    if rollup.unclassified_runs != 0 {
        reasons.push("unclassified workflow runs are present");
    }
    if rollup.total_first_attempt_failures != 0 {
        reasons.push("first-attempt failures or missing obligations are present");
    }
    if !rollup.green_claim_qualified {
        reasons.push("green claim is not qualified");
    }
    if reasons.is_empty() {
        Ok(())
    } else {
        bail!("CI evidence rollup is unqualified: {}", reasons.join(", "))
    }
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
        "\nDenominator: {:?}, branch `{}`, commits {}, fetch proof {}\nUnclassified workflow runs: {}\nQualified green claim: {}\n",
        rollup.denominator.source,
        rollup.denominator.branch,
        rollup.denominator.commit_count,
        rollup.denominator.fetch_succeeded,
        rollup.unclassified_runs,
        rollup.green_claim_qualified
    ));
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
        "collected {} expected obligations and {} unique attempts from {:?} into {}",
        evidence.expected.len(),
        evidence.attempts.len(),
        evidence.denominator.source,
        path.display()
    )?;
    Ok(())
}

fn print_rollup_summary(rollup: &RollupFile) -> Result<()> {
    let mut output = io::stdout().lock();
    writeln!(
        output,
        "rollup: {} successes, {} failures/missing; denominator: {:?}; qualified: {}; six-nines claim: false",
        rollup.total_first_attempt_successes,
        rollup.total_first_attempt_failures,
        rollup.denominator.source,
        rollup.green_claim_qualified
    )?;
    Ok(())
}
