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
    pub(crate) tree_sha: Option<String>,
    pub(crate) committed_at: Option<String>,
    pub(crate) source: DenominatorSource,
}

/// Provenance of a main-branch head in the expected-work denominator.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, Serialize, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DenominatorSource {
    FirstParentHistory,
    Fixture,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, Serialize, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ObligationProvenance {
    FirstParentHistory,
    Fixture,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct DenominatorProof {
    pub(crate) source: DenominatorSource,
    pub(crate) branch: String,
    pub(crate) window: TimeWindow,
    pub(crate) fetch_succeeded: bool,
    pub(crate) commit_count: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct HistoryCommitObservation {
    pub(crate) sha: String,
    pub(crate) base_sha: Option<String>,
    pub(crate) tree_sha: Option<String>,
    pub(crate) committed_at: String,
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
    pub(crate) data_quality_reason: Option<DataQualityReason>,
    pub(crate) conflicting_observations: Vec<RawAttemptObservation>,
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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct EvidenceFile {
    pub(crate) schema: u32,
    pub(crate) repository: String,
    pub(crate) window: TimeWindow,
    pub(crate) generated_at: String,
    pub(crate) runtime: RuntimeIdentity,
    pub(crate) denominator: DenominatorProof,
    pub(crate) history: Vec<HistoryCommitObservation>,
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
    /// Optional expected-obligation fixture. Without it, first-parent main
    /// commits are derived from the checkout after a shallow-since fetch.
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
    let denominator = match args.expected {
        Some(path) => denominator_from_fixture(read_expected(&path)?, &args.branch, &window),
        None => expected_from_first_parent(&root, &args.branch, &window)?,
    };
    let expected = denominator.expected.clone();
    let runs = list_runs(&repository, &args.branch, &window)?;
    let mut attempts = Vec::new();
    let mut unclassified_runs = Vec::new();
    for run in runs {
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
    let existing = read_evidence(&args.output, &repository, &window, runtime.clone())?;
    let merged = merge_evidence(
        existing,
        EvidenceUpdate {
            expected,
            attempts,
            unclassified_runs,
            denominator: denominator.denominator,
            history: denominator.history,
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

fn denominator_from_fixture(
    expected: Vec<ExpectedObligation>,
    branch: &str,
    window: &TimeWindow,
) -> ExpectedDenominator {
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
    ExpectedDenominator {
        expected,
        denominator: DenominatorProof {
            source: DenominatorSource::Fixture,
            branch: branch.to_owned(),
            window: window.clone(),
            fetch_succeeded: false,
            commit_count: history.len(),
        },
        history: history.into_values().collect(),
    }
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
        let expected_source = match obligation.provenance {
            ObligationProvenance::FirstParentHistory => DenominatorSource::FirstParentHistory,
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
}

fn expected_from_first_parent(
    root: &Path,
    branch: &str,
    window: &TimeWindow,
) -> Result<ExpectedDenominator> {
    // A shallow checkout is not an expected-work source. Extend it by the
    // requested window, then enumerate only the default branch's first-parent
    // history. The fetch result is part of the proof retained in the artifact.
    let fetch_succeeded = cmd::run(Command::new("git").current_dir(root).args([
        "fetch",
        "--no-tags",
        "--shallow-since",
        &window.since,
        "origin",
        branch,
    ]))
    .is_ok();
    if !fetch_succeeded {
        let shallow = cmd::output_string(
            Command::new("git")
                .current_dir(root)
                .args(["rev-parse", "--is-shallow-repository"]),
        )
        .map_or(true, |value| value.trim() == "true");
        if shallow {
            bail!(
                "cannot derive expected main obligations: history fetch failed for shallow checkout"
            );
        }
    }
    let remote = format!("refs/remotes/origin/{branch}");
    let output = cmd::output(Command::new("git").current_dir(root).args([
        "log",
        "--first-parent",
        "--since",
        &window.since,
        "--until",
        &window.until,
        "--format=%H%x00%P%x00%cI",
        &remote,
    ]))?;
    let text = String::from_utf8(output).context("git log returned non-UTF-8")?;
    let mut history = Vec::new();
    for row in text.lines() {
        let mut fields = row.split('\0');
        let sha = fields.next().unwrap_or_default().to_owned();
        let parents = fields.next().unwrap_or_default();
        let committed_at = fields.next().unwrap_or_default().to_owned();
        if sha.is_empty() {
            continue;
        }
        let tree_sha = cmd::output_string(
            Command::new("git")
                .current_dir(root)
                .args(["rev-parse", &format!("{sha}^{{tree}}")]),
        )
        .ok()
        .map(|value| value.trim().to_owned());
        history.push(HistoryCommitObservation {
            sha,
            base_sha: parents.split_whitespace().next().map(str::to_owned),
            tree_sha,
            committed_at,
        });
    }
    let denominator = DenominatorProof {
        source: DenominatorSource::FirstParentHistory,
        branch: branch.to_owned(),
        window: window.clone(),
        fetch_succeeded,
        commit_count: history.len(),
    };
    let expected = expected_from_history(&history, DenominatorSource::FirstParentHistory)?;
    Ok(ExpectedDenominator {
        expected,
        denominator,
        history,
    })
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
            bail!(
                "duplicate first-parent history commit {}",
                history_commit.sha
            );
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
                    DenominatorSource::FirstParentHistory => {
                        ObligationProvenance::FirstParentHistory
                    }
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
        .filter(|workflow| workflow_path_matches(&workflow.path, ci_workflows))
        .map(|workflow| workflow.id)
        .collect::<BTreeSet<_>>();
    let desktop_ids = workflows
        .iter()
        .filter(|workflow| workflow_path_matches(&workflow.path, desktop_workflows))
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
    Ok(existing)
}

fn empty_evidence(repository: &str, window: &TimeWindow, runtime: RuntimeIdentity) -> EvidenceFile {
    EvidenceFile {
        schema: SCHEMA,
        repository: repository.to_owned(),
        window: window.clone(),
        generated_at: now_rfc3339(),
        runtime,
        denominator: DenominatorProof {
            source: DenominatorSource::Fixture,
            branch: String::new(),
            window: window.clone(),
            fetch_succeeded: false,
            commit_count: 0,
        },
        history: Vec::new(),
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
    repository: String,
    window: TimeWindow,
    runtime: RuntimeIdentity,
}

fn merge_evidence(mut existing: EvidenceFile, update: EvidenceUpdate) -> Result<EvidenceFile> {
    let EvidenceUpdate {
        expected,
        attempts,
        mut unclassified_runs,
        denominator,
        history,
        repository,
        window,
        runtime,
    } = update;
    let derived_expected = expected_from_history(&history, denominator.source)?;
    if expected != derived_expected {
        bail!("expected obligations do not match raw first-parent history");
    }
    validate_expected(&expected)?;
    let expected_keys = expected
        .iter()
        .map(|obligation| (obligation.commit.sha.clone(), obligation.cohort))
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
                && raw_attempt_observation(previous) != raw_attempt_observation(&attempt)
            {
                if previous.conflicting_observations.is_empty() {
                    let previous_observation = raw_attempt_observation(previous);
                    previous.conflicting_observations.push(previous_observation);
                }
                previous
                    .conflicting_observations
                    .push(raw_attempt_observation(&attempt));
                previous.classification = OutcomeClass::DataQuality;
                previous.data_quality_reason =
                    Some(DataQualityReason::ConflictingTerminalObservation);
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
    existing.denominator = denominator;
    existing.history = history;
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
    validate_denominator(&evidence.denominator, &evidence.history, &evidence.window)?;
    let derived_expected = expected_from_history(&evidence.history, evidence.denominator.source)?;
    if evidence.expected != derived_expected {
        bail!("expected obligations are not derived from raw first-parent history");
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
        if let Some(reason) = attempt.data_quality_reason {
            if reason != DataQualityReason::ConflictingTerminalObservation
                || attempt.classification != OutcomeClass::DataQuality
                || attempt.conflicting_observations.len() < 2
            {
                bail!(
                    "attempt {} has an unproven data-quality conflict",
                    attempt.run_id
                );
            }
            let first = &attempt.conflicting_observations[0];
            if !attempt
                .conflicting_observations
                .iter()
                .skip(1)
                .any(|observation| {
                    observation.status != first.status
                        || observation.conclusion != first.conclusion
                        || observation.jobs != first.jobs
                })
            {
                bail!(
                    "attempt {} conflict marker has no distinct raw observations",
                    attempt.run_id
                );
            }
            if !attempt.conflicting_observations.iter().any(|observation| {
                classify_outcome(
                    &observation.status,
                    observation.conclusion.as_deref(),
                    &observation.jobs,
                    &attempt.expected_work,
                ) != recomputed
            }) {
                bail!(
                    "attempt {} conflict marker does not change derived classification",
                    attempt.run_id
                );
            }
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

fn validate_denominator(
    proof: &DenominatorProof,
    history: &[HistoryCommitObservation],
    window: &TimeWindow,
) -> Result<()> {
    if proof.branch.is_empty() || proof.window != *window {
        bail!("denominator proof does not match the evidence window or branch");
    }
    if proof.commit_count != history.len() {
        bail!("denominator proof commit count does not match history");
    }
    if proof.source == DenominatorSource::FirstParentHistory && !proof.fetch_succeeded {
        bail!("first-parent denominator is missing a successful history fetch proof");
    }
    for commit in history {
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
        && evidence.denominator.source == DenominatorSource::FirstParentHistory
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
    if rollup.denominator.source != DenominatorSource::FirstParentHistory {
        reasons.push("denominator is not first-parent main history");
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
