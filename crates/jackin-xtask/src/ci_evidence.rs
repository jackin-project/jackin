//! Durable first-attempt evidence for post-merge CI cohorts.
//!
//! GitHub pagination and workflow identities are normalized before aggregation.
//! The rollup joins observed attempts to an explicit expected obligation set,
//! so a workflow that never started is recorded as `missing` rather than
//! disappearing from the denominator.
//!
//! The collector validates a durable push-head artifact produced by CI; it does
//! not create that producer workflow. A local fixture is intentionally
//! ineligible for a qualified green claim, and this module makes no live-CI
//! production-qualification claim by itself.

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

const SCHEMA: u32 = 5;
const DEFAULT_WINDOW_DAYS: i64 = 31;
const DEFAULT_CI_WORKFLOW: &str = "ci-main.yml";
const DEFAULT_DESKTOP_WORKFLOW: &str = "desktop-merge.yml";
const DEFAULT_CI_EVIDENCE_WORKFLOW: &str = "ci-evidence.yml";
const DEFAULT_CI_EVIDENCE_ARTIFACT: &str = "target/ci-evidence/";
const DEFAULT_PUSH_HEAD_LEDGER_WORKFLOW: &str = "ci-push-head-ledger.yml";
const DEFAULT_PUSH_HEAD_LEDGER_ARTIFACT: &str = "ci-push-head-ledger";
const WORKFLOW_CONTRACT_PATH: &str = ".github-gen/velnor-workflow.toml";
const WORKFLOW_STATE_PATH: &str = ".github/ci/.github-actions-generator-state";
const MISE_PATH: &str = "mise.toml";
const GENERATED_STATE_SCHEMA: u32 = 2;
// Velnor 4dec6b9 emits ownership-state generator revision 69. This is a
// checked-in consumer contract: changing it requires regenerating the state
// with the new pinned renderer and updating this validator in the same change.
const GENERATED_STATE_GENERATOR: &str = "69";

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
    pub(crate) boundary: DenominatorBoundary,
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
    pub(crate) artifact: PushHeadArtifactProof,
}

/// The exact artifact bytes validated at collection time.
///
/// Retaining the bytes makes the digest a check over an actual artifact, not
/// merely an unchecked 64-character marker in the evidence JSON.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct PushHeadArtifactProof {
    pub(crate) manifest: String,
    pub(crate) event: String,
    pub(crate) manifest_sha256: String,
}

/// Proof that the first in-window push is attached to the immediately prior
/// durable ledger entry. A missing predecessor is a missing denominator proof.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum DenominatorBoundary {
    PushHead {
        predecessor: Box<PushHeadObservation>,
    },
    Fixture,
}

/// Provenance of the collector invocation that wrote an evidence file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct CollectionProvenance {
    pub(crate) repository: String,
    pub(crate) branch: String,
    pub(crate) event: String,
    pub(crate) workflow_path: String,
    pub(crate) run_id: Option<u64>,
    pub(crate) workflow_ref: Option<String>,
    pub(crate) workflow_sha: Option<String>,
    pub(crate) artifact_name: Option<String>,
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
    pub(crate) head_branch: Option<String>,
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
    )?;
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
            workflow_ref: None,
            workflow_sha: None,
            artifact_name: None,
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
    let workflow_sha = env::var("GITHUB_SHA").context("GITHUB_SHA is missing")?;
    if !is_git_sha(&workflow_sha) {
        bail!("GITHUB_SHA is not a 40-character hexadecimal identity");
    }
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
        workflow_ref: Some(workflow_ref),
        workflow_sha: Some(workflow_sha),
        artifact_name: Some(DEFAULT_CI_EVIDENCE_ARTIFACT.trim_end_matches('/').to_owned()),
    })
}

fn names_or_default(values: Vec<String>, default: &str) -> Vec<String> {
    if values.is_empty() {
        vec![default.to_owned()]
    } else {
        values
    }
}

fn runtime_identity(root: &Path, mut identity: RuntimeIdentity) -> Result<RuntimeIdentity> {
    let config = root.join(".github-gen/velnor-workflow.toml");
    let contents = fs::read_to_string(&config)
        .with_context(|| format!("reading workflow contract {}", config.display()))?;
    let expected_revision = contents.lines().find_map(|line| {
        let value = line
            .trim()
            .strip_prefix("revision = \"")?
            .strip_suffix('"')?;
        (value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| value.to_owned())
    });
    if let Some(revision) = identity.runtime_revision.as_deref() {
        if expected_revision.as_deref() != Some(revision) {
            bail!("runtime revision marker does not match the workflow contract");
        }
    } else {
        identity.runtime_revision = expected_revision;
    }
    let mut digest = Sha256::new();
    digest.update(contents.as_bytes());
    let expected_digest = hex::encode(digest.finalize());
    if let Some(contract_digest) = identity.contract_digest.as_deref() {
        if contract_digest != expected_digest {
            bail!("contract digest marker does not match the workflow contract");
        }
    } else {
        identity.contract_digest = Some(expected_digest);
    }
    Ok(identity)
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
            boundary: DenominatorBoundary::Fixture,
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

include!("ci_evidence/ledger.rs");
include!("ci_evidence/github.rs");
include!("ci_evidence/storage.rs");
include!("ci_evidence/validation.rs");
include!("ci_evidence/rollup.rs");
