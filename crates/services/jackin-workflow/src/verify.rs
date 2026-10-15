// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! C6 verification and repair: trusted tests, independent review,
//! bounded feedback, and the acceptance decision.
//!
//! The host runs the pinned test command in a fresh credential-free
//! test worker, validates one structured review result through the
//! adapter path, binds both to the run, round, candidate, and policy
//! hash in the evidence table, and delivers at most one bounded
//! feedback document per round to the same healthy implementer
//! session. Workers submit observations; only the host decides.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::{EvidenceRecord, RunStore, WorkflowError, WorkflowService};

/// Captured bytes kept per test output stream. Anything beyond this
/// is dropped with a truncation marker; the exit status is unaffected.
pub const MAX_TEST_OUTPUT_BYTES: usize = 64 * 1024;

/// Largest accepted review JSON document. The review worker writes one
/// exact file; anything larger is rejected, never scraped for fragments.
pub const MAX_REVIEW_BYTES: u64 = 64 * 1024;

/// Largest repair feedback document delivered to the implementer.
pub const MAX_FEEDBACK_BYTES: usize = 4 * 1024;

/// Excerpt bytes carried on a test result for feedback construction.
pub const MAX_TEST_EXCERPT_BYTES: usize = 1_500;

/// At most one correction request for malformed review output in a
/// review session. Further failure requires operator action.
pub const MAX_REVIEW_CORRECTIONS: u32 = 1;

/// Timeout joining output-reader threads after the test process ends.
const READER_JOIN_TIMEOUT: Duration = Duration::from_secs(5);

/// Frozen trusted test policy: the exact argv from the plan plus its
/// canonical hash. A changed policy requires a new plan; the hash
/// check rejects stale evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestPolicy {
    argv: Vec<String>,
    timeout_secs: u64,
    policy_hash: String,
}

impl TestPolicy {
    /// Freeze an exact argv with a timeout. No shell is ever involved:
    /// the first element is the program, the rest are literal arguments.
    pub fn freeze(argv: Vec<String>, timeout_secs: u64) -> Result<Self, WorkflowError> {
        if argv.is_empty() || argv.iter().any(|part| part.trim().is_empty()) {
            return Err(WorkflowError::invalid(
                "frozen test policy needs a non-empty exact argv".to_owned(),
            ));
        }
        if !(1..=3_600).contains(&timeout_secs) {
            return Err(WorkflowError::invalid(
                "frozen test policy timeout must be within 1..=3600 seconds".to_owned(),
            ));
        }
        let mut hasher = Sha256::new();
        for part in &argv {
            hasher.update(part.as_bytes());
            hasher.update([0]);
        }
        hasher.update(timeout_secs.to_be_bytes());
        Ok(Self {
            argv,
            timeout_secs,
            policy_hash: hex::encode(hasher.finalize()),
        })
    }

    /// Exact argv: element zero is the program, the rest are arguments.
    #[must_use]
    pub fn argv(&self) -> &[String] {
        &self.argv
    }

    /// Program to execute, never a shell command line.
    #[must_use]
    pub fn program(&self) -> &str {
        &self.argv[0]
    }

    /// Literal arguments, passed without shell interpretation.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.argv[1..]
    }

    /// Execution deadline for one test attempt.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }

    /// Canonical hash of the frozen policy. Evidence rows carry it;
    /// [`decide_candidate`] rejects rows with a different hash.
    #[must_use]
    pub fn policy_hash(&self) -> &str {
        &self.policy_hash
    }
}

/// Marker that a test worker carries no agent credentials: no account
/// binding, no credential files, no picker dispatch. The only
/// constructible value is [`NoAgentCredentials::NONE`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoAgentCredentials {
    _private: (),
}

impl NoAgentCredentials {
    /// The sole credential state a test worker may carry: none.
    pub const NONE: Self = Self { _private: () };
}

/// Exact identity and checkpoint for one non-interactive test worker.
/// The worker is single-use: its directory must not exist before
/// preparation and is never reused after a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestWorkerSpec {
    worker_id: String,
    candidate_id: String,
    commit: String,
    tree: String,
    policy_hash: String,
    timeout_secs: u64,
    credentials: NoAgentCredentials,
}

impl TestWorkerSpec {
    /// Define the worker for one candidate. The ID is deterministic
    /// (`test-<run>-r<round>-<candidate>`) so transcripts name the exact
    /// worker that produced an exit status.
    pub fn new(
        run_id: &str,
        round: u32,
        candidate_id: &str,
        commit: &str,
        tree: &str,
        policy: &TestPolicy,
    ) -> Result<Self, WorkflowError> {
        for (label, value) in [
            ("run", run_id),
            ("candidate", candidate_id),
            ("commit", commit),
            ("tree", tree),
        ] {
            if value.trim().is_empty()
                || value
                    .chars()
                    .any(|ch| ch.is_whitespace() || ch == '/' || ch == '\0')
            {
                return Err(WorkflowError::invalid(format!(
                    "test worker {label} must be a single non-empty path-safe token"
                )));
            }
        }
        Ok(Self {
            worker_id: format!("test-{run_id}-r{round}-{candidate_id}"),
            candidate_id: candidate_id.to_owned(),
            commit: commit.to_owned(),
            tree: tree.to_owned(),
            policy_hash: policy.policy_hash().to_owned(),
            timeout_secs: policy.timeout().as_secs(),
            credentials: NoAgentCredentials::NONE,
        })
    }

    /// Deterministic single-use worker identity.
    #[must_use]
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Candidate under test.
    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    /// Exact commit the worker source must match.
    #[must_use]
    pub fn commit(&self) -> &str {
        &self.commit
    }

    /// Exact tree the worker source must match.
    #[must_use]
    pub fn tree(&self) -> &str {
        &self.tree
    }

    /// Policy hash the run binds to its evidence rows.
    #[must_use]
    pub fn policy_hash(&self) -> &str {
        &self.policy_hash
    }

    /// Credential state: always none. Reading it proves the launch
    /// path threaded the marker through.
    #[must_use]
    pub const fn credentials(&self) -> NoAgentCredentials {
        self.credentials
    }
}

/// A prepared test worker: a fresh private clone pinned to the exact
/// checkpoint, verified before any test byte runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedTestWorker {
    worker_id: String,
    work_dir: PathBuf,
    commit: String,
    tree: String,
}

impl PreparedTestWorker {
    /// Single-use worker identity.
    #[must_use]
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Private clone directory holding the pinned source.
    #[must_use]
    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    /// Verified commit of the prepared source.
    #[must_use]
    pub fn commit(&self) -> &str {
        &self.commit
    }

    /// Verified tree of the prepared source.
    #[must_use]
    pub fn tree(&self) -> &str {
        &self.tree
    }
}

pub(crate) fn run_git(dir: &Path, args: &[&str]) -> Result<String, WorkflowError> {
    // Spawn plus wait_with_output (never Command::output): short-lived
    // local git plumbing on the host verification path, off
    // render/runtime threads like the rest of this module.
    let child = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| WorkflowError::storage(format!("test worker git failed to spawn: {err}")))?;
    let output = child
        .wait_with_output()
        .map_err(|err| WorkflowError::storage(format!("test worker git wait failed: {err}")))?;
    if !output.status.success() {
        return Err(WorkflowError::storage(format!(
            "test worker git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// Prepare a fresh test worker from the source repository: private
/// clone with no hardlinks, detached checkout of the exact commit,
/// then proof that HEAD, the tree, and a clean worktree all match the
/// checkpoint. The trusted bundle is never copied into the worker.
pub fn prepare_test_worker(
    repo: &Path,
    spec: &TestWorkerSpec,
    scratch_root: &Path,
) -> Result<PreparedTestWorker, WorkflowError> {
    if !repo.is_dir() {
        return Err(WorkflowError::not_found(format!(
            "test worker source repository {} does not exist",
            repo.display()
        )));
    }
    std::fs::create_dir_all(scratch_root).map_err(|err| {
        WorkflowError::storage(format!("create test worker scratch root failed: {err}"))
    })?;
    let work_dir = scratch_root.join(spec.worker_id());
    if work_dir.exists() {
        return Err(WorkflowError::invalid(format!(
            "test worker directory {} already exists; workers are single-use",
            work_dir.display()
        )));
    }
    run_git(
        scratch_root,
        &[
            "clone",
            "--no-hardlinks",
            "--no-checkout",
            "--",
            &repo.to_string_lossy(),
            spec.worker_id(),
        ],
    )?;
    run_git(&work_dir, &["checkout", "--detach", spec.commit()])?;
    let head = run_git(&work_dir, &["rev-parse", "HEAD"])?;
    if head != spec.commit() {
        return Err(WorkflowError::storage(format!(
            "test worker HEAD {head} does not match checkpoint {}",
            spec.commit()
        )));
    }
    let tree = run_git(&work_dir, &["rev-parse", "HEAD^{tree}"])?;
    if tree != spec.tree() {
        return Err(WorkflowError::storage(format!(
            "test worker tree {tree} does not match checkpoint {}",
            spec.tree()
        )));
    }
    let dirty = run_git(&work_dir, &["status", "--porcelain"])?;
    if !dirty.is_empty() {
        return Err(WorkflowError::storage(
            "test worker checkout is not clean after pinning the checkpoint".to_owned(),
        ));
    }
    Ok(PreparedTestWorker {
        worker_id: spec.worker_id().to_owned(),
        work_dir,
        commit: head,
        tree,
    })
}

/// Environment allowlist for the test process. Default-deny: agent
/// credentials live in files and host variables that candidate code
/// must never observe, so only locale, temp, and path entries pass.
///
/// Python bytecode writing is always disabled: trusted checks import
/// candidate modules, and an emitted `__pycache__` would dirty the
/// pinned worker source (and any host-mounted fixture tree) outside
/// the candidate diff. The frozen policy argv stays untouched; the
/// flag travels in the environment instead.
fn test_command_env() -> Vec<(String, String)> {
    const ALLOWED: &[&str] = &["PATH", "TMPDIR", "TZ", "LANG", "LC_ALL", "LC_CTYPE"];
    let mut env: Vec<(String, String)> = ALLOWED
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| (key.to_string(), value))
        })
        .collect();
    env.push(("PYTHONDONTWRITEBYTECODE".to_owned(), "1".to_owned()));
    env
}

/// Bounded test output: at most [`MAX_TEST_OUTPUT_BYTES`] kept, with
/// a flag recording whether the tail was dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

impl BoundedOutput {
    /// Kept bytes, lossy-decoded for feedback use.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    /// Whether output beyond the bound was dropped.
    #[must_use]
    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    /// Kept raw bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

fn read_bounded(stream: impl std::io::Read) -> BoundedOutput {
    let limit = u64::try_from(MAX_TEST_OUTPUT_BYTES).unwrap_or(u64::MAX);
    let mut capped = stream.take(limit + 1);
    let mut bytes = Vec::new();
    let truncated = capped
        .read_to_end(&mut bytes)
        .is_ok_and(|_| bytes.len() > MAX_TEST_OUTPUT_BYTES);
    bytes.truncate(MAX_TEST_OUTPUT_BYTES);
    BoundedOutput { bytes, truncated }
}

/// Host-observed outcome of one pinned test execution. A completed
/// process always yields an outcome, whatever its exit code; only
/// spawn, timeout, and reader failures are errors, and they never
/// read as pass or fail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestExecution {
    exit_code: i32,
    timed_out: bool,
    stdout: BoundedOutput,
    stderr: BoundedOutput,
}

impl TestExecution {
    /// Actual process exit code, or `-1` when killed by a signal.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        self.exit_code
    }

    /// Whether the deadline killed the process. Timeouts are recorded
    /// separately from code failures: one clean retry is allowed for a
    /// recorded infrastructure failure, never for a code failure.
    #[must_use]
    pub const fn timed_out(&self) -> bool {
        self.timed_out
    }

    /// Bounded captured stdout.
    #[must_use]
    pub const fn stdout(&self) -> &BoundedOutput {
        &self.stdout
    }

    /// Bounded captured stderr.
    #[must_use]
    pub const fn stderr(&self) -> &BoundedOutput {
        &self.stderr
    }

    #[cfg(test)]
    pub fn from_test_parts(exit_code: i32, output: &str) -> Self {
        let text = output.as_bytes();
        let half = text.len() / 2;
        Self {
            exit_code,
            timed_out: false,
            stdout: BoundedOutput {
                bytes: text[..half].to_vec(),
                truncated: false,
            },
            stderr: BoundedOutput {
                bytes: text[half..].to_vec(),
                truncated: false,
            },
        }
    }

    /// Short exit label for evidence and feedback (`exit 0`, `exit 3`,
    /// `signal`, `timeout`).
    #[must_use]
    pub fn exit_label(&self) -> String {
        if self.timed_out {
            "timeout".to_owned()
        } else if self.exit_code < 0 {
            "signal".to_owned()
        } else {
            format!("exit {}", self.exit_code)
        }
    }
}

fn join_reader(
    handle: std::thread::JoinHandle<BoundedOutput>,
    stream: &str,
) -> Result<BoundedOutput, WorkflowError> {
    let deadline = Instant::now() + READER_JOIN_TIMEOUT;
    loop {
        if handle.is_finished() {
            return handle.join().map_err(|_| {
                WorkflowError::storage(format!("test worker {stream} reader panicked"))
            });
        }
        if Instant::now() >= deadline {
            return Err(WorkflowError::blocked(format!(
                "test worker {stream} reader did not finish; \
                 a child process may hold the pipe open"
            )));
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "bounded reader join on the host verification path, off render/runtime threads"
        )]
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Run the frozen policy argv in the prepared worker directory with
/// the default-deny environment. Returns the actual exit status with
/// bounded output; the caller maps it to pass or fail.
pub fn run_test_command(
    policy: &TestPolicy,
    worker: &PreparedTestWorker,
) -> Result<TestExecution, WorkflowError> {
    let mut child = std::process::Command::new(policy.program())
        .args(policy.args())
        .current_dir(worker.work_dir())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .env_clear()
        .envs(test_command_env())
        .spawn()
        .map_err(|err| {
            WorkflowError::storage(format!(
                "test worker {} failed to spawn {}: {err}",
                worker.worker_id(),
                policy.program()
            ))
        })?;
    let stdout_handle = std::thread::spawn({
        let stdout = child.stdout.take();
        move || {
            stdout.map_or(
                BoundedOutput {
                    bytes: Vec::new(),
                    truncated: false,
                },
                read_bounded,
            )
        }
    });
    let stderr_handle = std::thread::spawn({
        let stderr = child.stderr.take();
        move || {
            stderr.map_or(
                BoundedOutput {
                    bytes: Vec::new(),
                    truncated: false,
                },
                read_bounded,
            )
        }
    });
    let deadline = Instant::now() + policy.timeout();
    let mut timed_out = false;
    let code = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| WorkflowError::storage(format!("test worker wait failed: {err}")))?
        {
            break status.code().unwrap_or(-1);
        }
        if Instant::now() >= deadline {
            timed_out = true;
            child
                .kill()
                .map_err(|err| WorkflowError::storage(format!("test worker kill failed: {err}")))?;
            let status = child
                .wait()
                .map_err(|err| WorkflowError::storage(format!("test worker reap failed: {err}")))?;
            break status.code().unwrap_or(-1);
        }
        #[expect(
            clippy::disallowed_methods,
            reason = "bounded test deadline poll on the host verification path, off render/runtime threads"
        )]
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = join_reader(stdout_handle, "stdout")?;
    let stderr = join_reader(stderr_handle, "stderr")?;
    if timed_out {
        return Err(WorkflowError::blocked(format!(
            "test worker {} hit the {}s timeout; \
             one clean retry is allowed for this recorded infrastructure failure",
            worker.worker_id(),
            policy.timeout().as_secs()
        )));
    }
    Ok(TestExecution {
        exit_code: code,
        timed_out: false,
        stdout,
        stderr,
    })
}

/// Host verdict on one candidate: the exact revision, the actual exit
/// status, and a bounded excerpt. Never an agent-written JSON file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestResult {
    candidate_id: String,
    commit: String,
    tree: String,
    policy_hash: String,
    worker_id: String,
    exit_code: i32,
    passed: bool,
    excerpt: String,
    output_digest: String,
}

impl TestResult {
    /// Map a host-observed execution to pass or fail. Exit zero passes;
    /// anything else fails. Infrastructure errors never reach this
    /// mapping: they return as errors from [`run_test_command`].
    #[must_use]
    pub fn from_execution(
        spec: &TestWorkerSpec,
        policy: &TestPolicy,
        execution: &TestExecution,
    ) -> Self {
        debug_assert_eq!(spec.policy_hash(), policy.policy_hash());
        let mut hasher = Sha256::new();
        hasher.update(execution.stdout().bytes());
        hasher.update([0]);
        hasher.update(execution.stderr().bytes());
        let combined = format!(
            "{}\n{}",
            execution.stderr().text(),
            execution.stdout().text()
        );
        let excerpt: String = combined.chars().take(MAX_TEST_EXCERPT_BYTES).collect();
        Self {
            candidate_id: spec.candidate_id().to_owned(),
            commit: spec.commit().to_owned(),
            tree: spec.tree().to_owned(),
            policy_hash: policy.policy_hash().to_owned(),
            worker_id: spec.worker_id().to_owned(),
            exit_code: execution.exit_code(),
            passed: execution.exit_code() == 0 && !execution.timed_out(),
            excerpt,
            output_digest: hex::encode(hasher.finalize()),
        }
    }

    /// Candidate under test.
    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    /// Whether the pinned command exited zero.
    #[must_use]
    pub const fn passed(&self) -> bool {
        self.passed
    }

    /// Actual exit code.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        self.exit_code
    }

    /// Bounded output excerpt for feedback construction.
    #[must_use]
    pub fn excerpt(&self) -> &str {
        &self.excerpt
    }

    /// Digest of the full bounded outputs, stored with the artifact.
    #[must_use]
    pub fn output_digest(&self) -> &str {
        &self.output_digest
    }
}

/// Persist the bounded test outputs as a content-addressed artifact
/// and return its digest for the evidence row.
pub fn persist_test_output(
    store: &RunStore,
    candidate_id: &str,
    execution: &TestExecution,
) -> Result<String, WorkflowError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        format!(
            "candidate {candidate_id} {}\n--- stdout ---\n",
            execution.exit_label()
        )
        .as_bytes(),
    );
    bytes.extend_from_slice(execution.stdout().bytes());
    bytes.extend_from_slice(b"\n--- stderr ---\n");
    bytes.extend_from_slice(execution.stderr().bytes());
    let artifact = store.write_artifact(&format!("test-output-{candidate_id}"), &bytes)?;
    Ok(artifact.digest)
}

/// Bind a test verdict to the run, candidate, and policy hash in the
/// evidence table. Source and quality are host-assigned: only the
/// trusted worker path writes these rows.
pub fn record_test_evidence(
    store: &RunStore,
    run_id: &str,
    result: &TestResult,
    artifact_digest: &str,
) -> Result<(), WorkflowError> {
    store.record_evidence(&EvidenceRecord {
        run_id,
        candidate: result.candidate_id(),
        kind: "test",
        source: "trusted-test-worker",
        quality: "host-observed",
        result: if result.passed() { "pass" } else { "fail" },
        policy_hash: Some(&result.policy_hash),
        artifact_digest: Some(artifact_digest),
    })
}

/// Finding severity. Any `blocker`, `major`, or `minor` finding
/// prevents acceptance; a `note` never does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Blocker,
    Major,
    Minor,
    Note,
}

/// One structured review finding against the candidate source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewFinding {
    pub id: String,
    pub severity: Severity,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_start: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_end: Option<u32>,
    pub explanation: String,
    pub suggestion: String,
}

/// Review verdict: pass or fail with structured findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Pass,
    Fail,
}

impl ReviewStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
}

/// Closed review result schema. The host adds trusted source and
/// session metadata after validation; the reviewer cannot grant
/// permissions or remove a required test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewResult {
    pub schema_version: u8,
    pub candidate_id: String,
    pub commit: String,
    pub tree: String,
    pub status: ReviewStatus,
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
}

/// Expected candidate identity for review validation, supplied by the
/// host from the checkpoint, never by the reviewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewExpectation<'a> {
    pub candidate_id: &'a str,
    pub commit: &'a str,
    pub tree: &'a str,
}

/// Separate review worker: fresh instance and session, read-only
/// source, one writable result file. The implementer's conversation,
/// memory, and repository are never shared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewWorkerSpec {
    worker_id: String,
    candidate_id: String,
    commit: String,
    tree: String,
    result_path: PathBuf,
    corrections_used: u32,
}

impl ReviewWorkerSpec {
    /// Define the reviewer for one candidate. The result path is the
    /// single file the adapter reads; transcripts are never scraped.
    pub fn new(
        run_id: &str,
        round: u32,
        candidate_id: &str,
        commit: &str,
        tree: &str,
        result_dir: &Path,
    ) -> Result<Self, WorkflowError> {
        for (label, value) in [
            ("run", run_id),
            ("candidate", candidate_id),
            ("commit", commit),
            ("tree", tree),
        ] {
            if value.trim().is_empty()
                || value
                    .chars()
                    .any(|ch| ch.is_whitespace() || ch == '/' || ch == '\0')
            {
                return Err(WorkflowError::invalid(format!(
                    "review worker {label} must be a single non-empty path-safe token"
                )));
            }
        }
        Ok(Self {
            worker_id: format!("review-{run_id}-r{round}-{candidate_id}"),
            candidate_id: candidate_id.to_owned(),
            commit: commit.to_owned(),
            tree: tree.to_owned(),
            result_path: result_dir.join("review.json"),
            corrections_used: 0,
        })
    }

    /// Deterministic review worker identity.
    #[must_use]
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// The single result file the adapter reads.
    #[must_use]
    pub fn result_path(&self) -> &Path {
        &self.result_path
    }

    /// Corrections consumed in this review session (at most one).
    #[must_use]
    pub const fn corrections_used(&self) -> u32 {
        self.corrections_used
    }

    /// Record a consumed correction request. Further malformed output
    /// after the first correction requires operator action.
    pub fn take_correction(&mut self) -> Result<(), WorkflowError> {
        if !correction_allowed(self.corrections_used) {
            return Err(WorkflowError::blocked(
                "review correction budget is spent; operator action required".to_owned(),
            ));
        }
        self.corrections_used += 1;
        Ok(())
    }

    /// Candidate identity the review must name exactly.
    #[must_use]
    pub fn expectation(&self) -> ReviewExpectation<'_> {
        ReviewExpectation {
            candidate_id: &self.candidate_id,
            commit: &self.commit,
            tree: &self.tree,
        }
    }
}

/// Whether another correction request fits the budget.
#[must_use]
pub const fn correction_allowed(used: u32) -> bool {
    used < MAX_REVIEW_CORRECTIONS
}

/// Read the review result through the adapter's tested output path:
/// the exact file the review worker was told to write, size-checked.
/// Missing files and oversize documents are errors, never fragments
/// scraped from a transcript.
pub fn read_review_bytes(path: &Path) -> Result<Vec<u8>, WorkflowError> {
    let len = std::fs::metadata(path)
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                WorkflowError::not_found(format!(
                    "review result {} is missing; a missing review blocks acceptance",
                    path.display()
                ))
            } else {
                WorkflowError::storage(format!(
                    "read review result {} failed: {err}",
                    path.display()
                ))
            }
        })?
        .len();
    if len > MAX_REVIEW_BYTES {
        return Err(WorkflowError::invalid(format!(
            "review result is {len} bytes; the limit is {MAX_REVIEW_BYTES}"
        )));
    }
    let bytes = std::fs::read(path).map_err(|err| {
        WorkflowError::storage(format!(
            "read review result {} failed: {err}",
            path.display()
        ))
    })?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_REVIEW_BYTES {
        return Err(WorkflowError::invalid(format!(
            "review result is {} bytes; the limit is {MAX_REVIEW_BYTES}",
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn valid_relative_path(path: &str) -> bool {
    if path.is_empty() || path.len() > 512 || path.starts_with('/') {
        return false;
    }
    !path.split('/').any(|part| part.is_empty() || part == "..")
}

/// Validate a review document: size, schema version, exact candidate
/// identity, finding shape, permitted paths, and internal consistency
/// (a passing review carries no blocking findings; a failing review
/// carries at least one finding). Symbolic or mismatched IDs fail.
pub fn validate_review(
    bytes: &[u8],
    expected: ReviewExpectation<'_>,
) -> Result<ReviewResult, WorkflowError> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_REVIEW_BYTES {
        return Err(WorkflowError::invalid(format!(
            "review result is {} bytes; the limit is {MAX_REVIEW_BYTES}",
            bytes.len()
        )));
    }
    let result: ReviewResult = serde_json::from_slice(bytes).map_err(|err| {
        WorkflowError::invalid(format!("review result is not valid review JSON: {err}"))
    })?;
    if result.schema_version != 1 {
        return Err(WorkflowError::invalid(format!(
            "review schema_version is {}; only version 1 is accepted",
            result.schema_version
        )));
    }
    for (label, actual, wanted) in [
        (
            "candidate_id",
            result.candidate_id.as_str(),
            expected.candidate_id,
        ),
        ("commit", result.commit.as_str(), expected.commit),
        ("tree", result.tree.as_str(), expected.tree),
    ] {
        if actual.trim().is_empty() || actual != wanted {
            return Err(WorkflowError::invalid(format!(
                "review {label} does not name the candidate under review"
            )));
        }
    }
    if result.findings.len() > 256 {
        return Err(WorkflowError::invalid(
            "review carries more than 256 findings".to_owned(),
        ));
    }
    for finding in &result.findings {
        if finding.id.trim().is_empty() || finding.id.len() > 128 {
            return Err(WorkflowError::invalid(
                "review finding needs a non-empty id of at most 128 bytes".to_owned(),
            ));
        }
        if !valid_relative_path(&finding.path) {
            return Err(WorkflowError::invalid(format!(
                "review finding path {:?} is outside the candidate source",
                finding.path
            )));
        }
        match (finding.line_start, finding.line_end) {
            (Some(0), _) | (_, Some(0)) => {
                return Err(WorkflowError::invalid(
                    "review line numbers are 1-based".to_owned(),
                ));
            }
            (Some(start), Some(end)) if end < start => {
                return Err(WorkflowError::invalid(
                    "review line range ends before it starts".to_owned(),
                ));
            }
            (None, Some(_)) => {
                return Err(WorkflowError::invalid(
                    "review line_end needs a line_start".to_owned(),
                ));
            }
            _ => {}
        }
        for (label, text) in [
            ("explanation", finding.explanation.as_str()),
            ("suggestion", finding.suggestion.as_str()),
        ] {
            if text.trim().is_empty() || text.len() > 4_096 {
                return Err(WorkflowError::invalid(format!(
                    "review finding {label} must be non-empty and at most 4096 bytes"
                )));
            }
        }
    }
    let blocking = result
        .findings
        .iter()
        .filter(|finding| !matches!(finding.severity, Severity::Note))
        .count();
    match result.status {
        ReviewStatus::Pass if blocking > 0 => {
            return Err(WorkflowError::invalid(
                "a passing review must not carry blocker, major, or minor findings".to_owned(),
            ));
        }
        ReviewStatus::Fail if result.findings.is_empty() => {
            return Err(WorkflowError::invalid(
                "a failing review needs at least one finding".to_owned(),
            ));
        }
        _ => {}
    }
    Ok(result)
}

/// Whether a validated review lets the candidate proceed: status pass
/// with no blocker, major, or minor findings. Notes never block.
#[must_use]
pub fn review_acceptable(result: &ReviewResult) -> bool {
    result.status == ReviewStatus::Pass
        && result
            .findings
            .iter()
            .all(|finding| finding.severity == Severity::Note)
}

/// Bind a validated review to the run, candidate, and policy hash in
/// the evidence table. Reviews are stored before any repair turn, so
/// feedback can only cite persisted verdicts.
pub fn record_review_evidence(
    store: &RunStore,
    run_id: &str,
    candidate_id: &str,
    policy_hash: &str,
    result: &ReviewResult,
    artifact_digest: &str,
) -> Result<(), WorkflowError> {
    store.record_evidence(&EvidenceRecord {
        run_id,
        candidate: candidate_id,
        kind: "review",
        source: "independent-review-worker",
        quality: "host-validated",
        result: result.status.as_str(),
        policy_hash: Some(policy_hash),
        artifact_digest: Some(artifact_digest),
    })
}

/// One failing check distilled from host-observed output: the test
/// name, a short excerpt, and an optional `file:line` location with
/// the host path stripped to its file name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailingCheck {
    pub name: String,
    pub excerpt: String,
    pub location: Option<String>,
}

/// Distill one failing execution into a [`FailingCheck`]. Only the
/// current candidate's output is used; outdated findings are never
/// recycled as instructions for newer code.
#[must_use]
pub fn summarize_test_failure(
    policy: &TestPolicy,
    execution: &TestExecution,
    excerpt: &str,
) -> FailingCheck {
    let argv = policy.argv().join(" ");
    let argv_short: String = argv.chars().take(160).collect();
    let name = format!("{argv_short} ({})", execution.exit_label());
    let lines: Vec<&str> = excerpt.lines().collect();
    let mut picked: Vec<&str> = lines
        .iter()
        .filter(|line| {
            let lower = line.to_lowercase();
            lower.contains("assert")
                || lower.contains("fail")
                || lower.contains("error")
                || lower.contains("traceback")
        })
        .take(10)
        .copied()
        .collect();
    if picked.is_empty() {
        let tail: Vec<&str> = lines.iter().rev().take(10).rev().copied().collect();
        picked = tail;
    }
    let excerpt: String = picked
        .iter()
        .map(|line| line.chars().take(200).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    FailingCheck {
        name,
        location: traceback_location(&lines),
        excerpt,
    }
}

/// First `File "…", line N` traceback hit, reduced to `file:line` so
/// feedback never leaks host worker paths.
fn traceback_location(lines: &[&str]) -> Option<String> {
    for line in lines {
        let Some(file_start) = line.find("File \"") else {
            continue;
        };
        let rest = &line[file_start + 6..];
        let Some(file_end) = rest.find('"') else {
            continue;
        };
        let file = &rest[..file_end];
        let after = &rest[file_end..];
        let Some(line_start) = after.find(", line ") else {
            continue;
        };
        let digits: String = after[line_start + 7..]
            .chars()
            .take_while(|ch| ch.is_numeric())
            .collect();
        if digits.is_empty() {
            continue;
        }
        let base = file.rsplit('/').next().unwrap_or(file);
        return Some(format!("{base}:{digits}"));
    }
    None
}

/// Redact credential-shaped values from feedback text. Feedback is
/// built from allowlisted fields only, so this is defense in depth:
/// known token prefixes and `label=value` secrets never reach the
/// implementer, and neither do session or agent identities.
fn redact_secrets(text: &str) -> String {
    const TOKEN_PREFIXES: &[&str] = &[
        "sk-", "AKIA", "xoxa-", "xoxb-", "xoxp-", "ghp_", "gho_", "gsk_", "Bearer ",
    ];
    const SECRET_LABELS: &[&str] = &[
        "api_key",
        "apikey",
        "secret",
        "token",
        "password",
        "session_id",
        "agent_id",
        "account_id",
    ];
    let mut out = Vec::new();
    for line in text.lines() {
        let mut redacted = line.to_owned();
        for prefix in TOKEN_PREFIXES {
            let mut search_from = 0;
            while let Some(hit) = redacted[search_from..].find(prefix) {
                let start = search_from + hit;
                let end = redacted[start..]
                    .find(|ch: char| ch.is_whitespace() || ch == '"' || ch == '\'')
                    .map_or(redacted.len(), |offset| start + offset);
                if end - start > prefix.len() + 3 {
                    redacted.replace_range(start..end, "[redacted]");
                    search_from = start + "[redacted]".len();
                } else {
                    search_from = start + prefix.len();
                }
            }
        }
        let lower = redacted.to_lowercase();
        for label in SECRET_LABELS {
            let mut search_from = 0;
            while let Some(hit) = lower[search_from..].find(label) {
                let label_start = search_from + hit;
                let after = &redacted[label_start + label.len()..];
                let value_start = after
                    .find(|ch: char| ch != ' ' && ch != '\t' && ch != ':' && ch != '=')
                    .unwrap_or(after.len());
                if value_start >= after.len() {
                    break;
                }
                let absolute = label_start + label.len() + value_start;
                let value_end = redacted[absolute..]
                    .find(|ch: char| ch.is_whitespace() || ch == '"' || ch == '\'' || ch == ';')
                    .map_or(redacted.len(), |offset| absolute + offset);
                if value_end > absolute {
                    redacted.replace_range(absolute..value_end, "[redacted]");
                    search_from = absolute + "[redacted]".len();
                    continue;
                }
                search_from = absolute;
            }
        }
        out.push(redacted);
    }
    out.join("\n")
}

/// Input for one bounded feedback document: the current candidate and
/// its own failures only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedbackInput<'a> {
    pub candidate_id: &'a str,
    pub commit: &'a str,
    pub round: u32,
    pub remaining_repairs: u32,
    pub checks: Vec<FailingCheck>,
    pub findings: Vec<&'a ReviewFinding>,
    pub evidence_refs: Vec<String>,
}

fn finding_location(finding: &ReviewFinding) -> String {
    match (finding.line_start, finding.line_end) {
        (Some(start), Some(end)) if end != start => {
            format!("{}:{start}-{end}", finding.path)
        }
        (Some(start), _) => format!("{}:{start}", finding.path),
        (None, _) => finding.path.clone(),
    }
}

/// Build one bounded feedback document for the current round. It names
/// the candidate, its failing checks with short excerpts and
/// `file:line` locations, the blocking review findings, the evidence
/// references, and the remaining repair count. Secrets, session IDs,
/// and agent IDs never appear: fields are allowlisted and the text is
/// scrubbed before the size cap is applied.
pub fn build_feedback(input: &FeedbackInput<'_>) -> Result<String, WorkflowError> {
    if input.candidate_id.trim().is_empty() || input.commit.trim().is_empty() {
        return Err(WorkflowError::invalid(
            "feedback needs the current candidate identity".to_owned(),
        ));
    }
    if input.checks.is_empty() && input.findings.is_empty() {
        return Err(WorkflowError::invalid(
            "feedback needs at least one failing check or finding".to_owned(),
        ));
    }
    let short_commit: String = input.commit.chars().take(12).collect();
    let mut doc = format!(
        "repair feedback for {} ({short_commit}) round {}: {} repair turn(s) left\n",
        input.candidate_id, input.round, input.remaining_repairs
    );
    if !input.checks.is_empty() {
        doc.push_str("failing checks:\n");
        for check in &input.checks {
            doc.push_str(&format!("- {}", check.name));
            if let Some(location) = &check.location {
                doc.push_str(&format!(" at {location}"));
            }
            doc.push('\n');
            for line in check.excerpt.lines().take(10) {
                doc.push_str("  ");
                doc.push_str(&line.chars().take(200).collect::<String>());
                doc.push('\n');
            }
        }
    }
    if !input.findings.is_empty() {
        doc.push_str("review findings:\n");
        for finding in &input.findings {
            let explanation: String = finding.explanation.chars().take(300).collect();
            let suggestion: String = finding.suggestion.chars().take(300).collect();
            doc.push_str(&format!(
                "- [{}] {} {} — {explanation} → {suggestion}\n",
                severity_label(finding.severity),
                finding.id,
                finding_location(finding),
            ));
        }
    }
    if !input.evidence_refs.is_empty() {
        doc.push_str("evidence:\n");
        for reference in &input.evidence_refs {
            doc.push_str(&format!(
                "- {}\n",
                reference.chars().take(160).collect::<String>()
            ));
        }
    }
    let scrubbed = redact_secrets(&doc);
    if scrubbed.len() <= MAX_FEEDBACK_BYTES {
        return Ok(scrubbed);
    }
    let mut capped: String = scrubbed.chars().take(MAX_FEEDBACK_BYTES).collect();
    capped.push_str("\n[truncated]");
    Ok(capped)
}

const fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Blocker => "blocker",
        Severity::Major => "major",
        Severity::Minor => "minor",
        Severity::Note => "note",
    }
}

/// Candidate identity: exact revision plus its generation in the
/// run record. Evidence for a superseded identity never decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateRef<'a> {
    pub candidate_id: &'a str,
    pub commit: &'a str,
    pub tree: &'a str,
    pub generation: u64,
}

/// What verification observed, and the policy it ran under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerificationOutcome<'a> {
    pub test_passed: bool,
    pub review_acceptable: bool,
    pub test_policy_hash: &'a str,
    pub expected_policy_hash: &'a str,
}

/// Host-side readiness: the source still matches the candidate and
/// required backup completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostReadiness {
    pub source_current: bool,
    pub backup_satisfied: bool,
}

/// Input-ownership state for automatic progress: the host owns
/// automatic input, the owner epoch is current, and no delivery is
/// uncertain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputOwnership {
    pub host_owned: bool,
    pub epoch_current: bool,
    pub uncertainty_pending: bool,
}

/// Evidence snapshot the host decision reads. Every field is
/// host-observed; workers cannot supply any of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateEvidence<'a> {
    pub candidate: CandidateRef<'a>,
    pub current: CandidateRef<'a>,
    pub verification: VerificationOutcome<'a>,
    pub readiness: HostReadiness,
    pub ownership: InputOwnership,
    pub round: u32,
    pub max_repair_turns: u32,
}

/// Host verdict on a candidate: accept it, repair it in the same
/// session, fail it with the budget spent, or hold for the operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Accept,
    Repair,
    Fail(String),
    NeedsOperator(String),
}

/// Decide a candidate from complete current evidence. Acceptance needs
/// every condition at once: passing tests on the current policy,
/// an acceptable review of the same candidate, a C4-current source,
/// host-owned automatic input on the current epoch, no unresolved
/// uncertainty, and satisfied backup. Stale, absent, malformed, or
/// incomplete evidence never accepts; failures with rounds left
/// repair, without rounds fail.
#[must_use]
pub fn decide_candidate(evidence: &CandidateEvidence<'_>) -> Decision {
    let candidate = &evidence.candidate;
    if candidate.candidate_id.trim().is_empty()
        || candidate.commit.trim().is_empty()
        || candidate.tree.trim().is_empty()
    {
        return Decision::NeedsOperator("incomplete candidate evidence".to_owned());
    }
    if *candidate != evidence.current {
        return Decision::NeedsOperator(
            "stale evidence: the candidate moved; re-verify the current candidate".to_owned(),
        );
    }
    let verification = &evidence.verification;
    if verification.test_policy_hash != verification.expected_policy_hash {
        return Decision::NeedsOperator(
            "stale evidence: the test policy changed; a new plan is required".to_owned(),
        );
    }
    let ownership = &evidence.ownership;
    if ownership.uncertainty_pending {
        return Decision::NeedsOperator(
            "unresolved delivery uncertainty blocks the decision".to_owned(),
        );
    }
    if !ownership.epoch_current {
        return Decision::NeedsOperator(
            "owner epoch moved; automatic input from this epoch is stale".to_owned(),
        );
    }
    if !ownership.host_owned {
        return Decision::NeedsOperator(
            "automatic input is not host-owned; resolve input ownership first".to_owned(),
        );
    }
    let readiness = &evidence.readiness;
    if !readiness.source_current {
        return Decision::NeedsOperator(
            "source drifted from the candidate; recapture before deciding".to_owned(),
        );
    }
    if !readiness.backup_satisfied {
        return Decision::NeedsOperator("required backup has not completed".to_owned());
    }
    if verification.test_passed && verification.review_acceptable {
        return Decision::Accept;
    }
    if evidence.round < evidence.max_repair_turns {
        return Decision::Repair;
    }
    Decision::Fail("repair budget is exhausted with failing verification".to_owned())
}

/// Physical implementer session identity for repair delivery: the
/// recorded attachment and the freshly observed live values must
/// match exactly, or delivery is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionIdentity {
    pub instance_id: String,
    pub session_id: u64,
    pub capsule_epoch: Option<u64>,
    pub process_identity: Option<String>,
}

/// Admission gate for one repair delivery: same healthy session,
/// current owner epoch, no operator input ownership, and a finished
/// implementer turn. Never deliver on uncertain input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryGate {
    pub recorded: SessionIdentity,
    pub observed: SessionIdentity,
    pub owner_epoch_current: bool,
    pub operator_owned: bool,
    pub turn_ended: bool,
}

/// Admit or refuse one repair delivery. Any mismatch refuses with its
/// reason; a refused delivery sends nothing.
pub fn admit_repair_delivery(gate: &DeliveryGate) -> Result<(), WorkflowError> {
    if gate.recorded != gate.observed {
        return Err(WorkflowError::blocked(
            "implementer session identity changed; \
             refusing repair delivery to an uncertain session"
                .to_owned(),
        ));
    }
    if !gate.owner_epoch_current {
        return Err(WorkflowError::blocked(
            "owner epoch moved; repair prompt from this epoch is stale".to_owned(),
        ));
    }
    if gate.operator_owned {
        return Err(WorkflowError::blocked(
            "operator owns session input; automatic repair input is refused".to_owned(),
        ));
    }
    if !gate.turn_ended {
        return Err(WorkflowError::blocked(
            "implementer turn has not ended; repair delivery waits".to_owned(),
        ));
    }
    Ok(())
}

/// Staged repair delivery: the frozen prompt bytes saved through C5
/// before any transport send, with its command correlation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairDelivery {
    pub command_id: String,
    pub prompt_digest: String,
}

/// Freeze the repair prompt and commit its dispatch BEFORE the
/// transport send. The caller sends `feedback` through the proven
/// session path and records the matched receipt; the receipt later
/// moves the command to `written`, never this staging call.
pub fn stage_repair_feedback(
    service: &WorkflowService,
    run_id: &str,
    session_ref: &str,
    feedback: &str,
    turn: u32,
    lease_generation: u64,
    gate: &DeliveryGate,
) -> Result<RepairDelivery, WorkflowError> {
    admit_repair_delivery(gate)?;
    if feedback.trim().is_empty() || feedback.len() > MAX_FEEDBACK_BYTES + 16 {
        return Err(WorkflowError::invalid(
            "repair feedback must be the bounded document from build_feedback".to_owned(),
        ));
    }
    let (command_id, prompt_digest) =
        service.begin_dispatch(run_id, session_ref, feedback, turn, lease_generation)?;
    service
        .store()
        .append_event(run_id, "host", "repair_feedback_staged", Some(&command_id))?;
    Ok(RepairDelivery {
        command_id,
        prompt_digest,
    })
}
