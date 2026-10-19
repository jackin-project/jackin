// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host workflow service: the only applier of run transitions (C5).
//!
//! The service owns the single active-run slot, the persisted owner
//! epoch, command delivery correlation, the launch/capture/backup
//! effect dispatcher, pause/cancel semantics, operator resolution of
//! uncertain delivery, and restart recovery. Workers submit
//! observations through [`WorkflowService::submit_observation`]; they
//! cannot mark a run accepted.

mod backend;
mod recovery;
mod unavailable;

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use jackin_workflow_spec::digest::bundle_manifest_digest;
use jackin_workflow_spec::model::TestBundleFile;

use super::model::{Actor, NextState, Observation, RunStatus, RunView, Stage, advance};
use super::store::{
    CheckpointRecord, NewEffect, RunStore, RunTransition, SessionAttachment, StoredCheckpoint,
    StoredEffect, StoredEvent, StoredEvidence, StoredRun, StoredSession,
};
use super::verify::{
    CandidateEvidence, CandidateRef, Decision, HostReadiness, InputOwnership, ReviewExpectation,
    TestPolicy, TestResult, TestWorkerSpec, VerificationOutcome, decide_candidate,
    persist_test_output, prepare_test_worker, read_review_bytes, record_review_evidence,
    record_test_evidence, review_acceptable, run_git, run_test_command, validate_review,
};
use super::{CommandState, EffectKind, EffectStatus, WorkflowError};

/// Machine-readable run snapshot served to the CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSummary {
    pub run_id: String,
    pub plan_hash: String,
    pub status: String,
    pub stage: String,
    pub round: u32,
    pub candidate: Option<String>,
    pub candidate_generation: u64,
    pub version: i64,
    pub owner_epoch: u64,
    pub pause_kind: Option<String>,
    pub max_repair_turns: u32,
}

/// Machine-readable run detail for the C7 Console view, `graph`,
/// `events`, and Markdown export: the summary plus the sessions that
/// pin the exact Capsule attachment, the effects that carry backup
/// state, and the bounded durable event/evidence/checkpoint history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunDetail {
    pub summary: RunSummary,
    pub sessions: Vec<StoredSession>,
    pub effects: Vec<StoredEffect>,
    pub events: Vec<StoredEvent>,
    pub evidence: Vec<StoredEvidence>,
    pub checkpoints: Vec<StoredCheckpoint>,
}

/// Fixed pipeline stage list for the C7 graph view: every run
/// moves through these stages in order; the detail response marks
/// each with its node state.
pub const GRAPH_STAGES: [&str; 6] = [
    "prepare",
    "implement",
    "capture",
    "test",
    "review",
    "decide",
];

/// Node state for one graph stage relative to the run's current
/// stage: done (passed), current, or pending.
#[must_use]
pub fn graph_node_state(stage: &str, current: &str) -> &'static str {
    let order = |name: &str| GRAPH_STAGES.iter().position(|s| *s == name);
    match (order(stage), order(current)) {
        (Some(a), Some(b)) if a < b => "done",
        (Some(a), Some(b)) if a == b => "current",
        (Some(_), Some(_)) => "pending",
        _ => "unknown",
    }
}

/// Operator decision for uncertain command delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorDecision {
    ContinueFromCurrent,
    SubmitNew,
    Cancel,
}

impl OperatorDecision {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContinueFromCurrent => "continue_from_current",
            Self::SubmitNew => "submit_new",
            Self::Cancel => "cancel",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "continue_from_current" => Some(Self::ContinueFromCurrent),
            "submit_new" => Some(Self::SubmitNew),
            "cancel" => Some(Self::Cancel),
            _ => None,
        }
    }
}

/// One live instance observed during recovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveInstance {
    pub instance_id: String,
    pub ownership_label: String,
    pub capture_paused: bool,
    pub session_alive: bool,
    pub capsule_epoch: Option<u64>,
    pub process_identity: Option<String>,
}

/// Instance inventory plus known remote backup refs for recovery.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceInventory {
    pub instances: Vec<LiveInstance>,
    pub backup_refs: Vec<String>,
}

/// Outcome of `record_dispatch_intent`: the committed command ID
/// plus the staged prompt digest. The command row is left in
/// `dispatching` with no transport send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedDispatch {
    pub command_id: String,
    pub prompt_digest: String,
}

/// Recovery findings for one restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryReport {
    pub run_id: Option<String>,
    pub actions: Vec<String>,
}

/// Backend surface the host daemon drives. The trait keeps the
/// daemon's trait-param injection shape: production serves a
/// [`WorkflowService`], request-line tests serve an unavailable stub.
pub trait WorkflowBackend {
    fn backend_epoch(&self) -> u64;
    fn start_run(
        &mut self,
        plan_hash: &str,
        max_repair_turns: u32,
        idempotency_key: &str,
    ) -> Result<RunSummary, WorkflowError>;
    fn run_status(&mut self, run_id: Option<&str>) -> Result<RunSummary, WorkflowError>;
    fn list_runs(&mut self, limit: u32) -> Result<Vec<RunSummary>, WorkflowError>;
    fn run_detail(
        &mut self,
        run_id: &str,
        after_seq: i64,
        event_limit: u32,
    ) -> Result<RunDetail, WorkflowError>;
    fn pause_run(
        &mut self,
        run_id: &str,
        actor: &str,
        reason: &str,
        checkpoint: bool,
    ) -> Result<RunSummary, WorkflowError>;
    fn resume_run(&mut self, run_id: &str, actor: &str) -> Result<RunSummary, WorkflowError>;
    fn cancel_run(
        &mut self,
        run_id: &str,
        actor: &str,
        reason: &str,
    ) -> Result<RunSummary, WorkflowError>;
    fn resolve_input(
        &mut self,
        run_id: &str,
        command_id: &str,
        decision: OperatorDecision,
        actor: &str,
        observed_session_state: &str,
    ) -> Result<RunSummary, WorkflowError>;
    fn record_session(
        &mut self,
        session: &SessionAttachment<'_>,
        expected_version: i64,
    ) -> Result<RunSummary, WorkflowError>;
    fn record_candidate(
        &mut self,
        run_id: &str,
        candidate_id: &str,
        expected_version: i64,
    ) -> Result<RunSummary, WorkflowError>;
    fn record_checkpoint(
        &mut self,
        checkpoint: &CheckpointRecord<'_>,
        expected_version: i64,
    ) -> Result<RunSummary, WorkflowError>;
    fn record_test(
        &mut self,
        run_id: &str,
        expected_version: i64,
        params: &RecordTestParams<'_>,
    ) -> Result<(RunSummary, RecordedTest), WorkflowError>;
    fn record_review(
        &mut self,
        run_id: &str,
        expected_version: i64,
        review_file: &str,
        policy_hash: &str,
    ) -> Result<(RunSummary, RecordedReview), WorkflowError>;
    fn record_backup(
        &mut self,
        run_id: &str,
        expected_version: i64,
        remote: &str,
        git_ref: &str,
    ) -> Result<RunSummary, WorkflowError>;
    fn advance_run(
        &mut self,
        run_id: &str,
        expected_version: i64,
        actor: &str,
        observation: &str,
        expected_policy_hash: Option<&str>,
    ) -> Result<RunSummary, WorkflowError>;
    fn record_dispatch_intent(
        &mut self,
        run_id: &str,
        session_ref: &str,
        prompt: &str,
        turn: u32,
        lease_generation: u64,
    ) -> Result<(RunSummary, RecordedDispatch), WorkflowError>;
    fn recover(&mut self, inventory: &InstanceInventory) -> Result<RecoveryReport, WorkflowError>;
}

/// Unavailable backend for request paths without a store (tests and
/// degraded serve). Every run-control call reports the stored reason.
#[derive(Debug, Clone)]
pub struct UnavailableBackend {
    reason: String,
}

/// Durable host workflow service.
#[derive(Debug)]
pub struct WorkflowService {
    store: RunStore,
}

impl WorkflowService {
    /// Open the service on `db_path` (dedicated Turso file) with
    /// artifacts under `artifacts_dir`. Opening persists the
    /// incremented owner epoch, so every `serve()` startup invalidates
    /// automatic input stamped with older epochs.
    pub fn open(db_path: &Path, artifacts_dir: &Path) -> Result<Self, WorkflowError> {
        Ok(Self {
            store: RunStore::open(db_path, artifacts_dir)?,
        })
    }

    /// Current persisted owner epoch.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.store.epoch()
    }

    /// Borrow the durable store (command correlation, tests).
    #[must_use]
    pub fn store(&self) -> &RunStore {
        &self.store
    }

    /// Start a run for a stored plan hash. The same idempotency key
    /// returns the existing run; a different key while a run is active
    /// returns `busy`. No queue exists in the MVP.
    pub fn start_run(
        &self,
        plan_hash: &str,
        max_repair_turns: u32,
        idempotency_key: &str,
    ) -> Result<RunSummary, WorkflowError> {
        if idempotency_key.trim().is_empty() || plan_hash.trim().is_empty() {
            return Err(WorkflowError::invalid(
                "plan hash and idempotency key are required".to_owned(),
            ));
        }
        if let Some(existing) = self.store.load_run_by_key(idempotency_key)? {
            return summarize(&existing);
        }
        if let Some(active) = self.store.active_run()? {
            return Err(WorkflowError::busy(format!(
                "run {} is already active; no queue in the MVP",
                active.run_id
            )));
        }
        let run_id = format!("run-{}", uuid::Uuid::new_v4().as_simple());
        self.store
            .create_run(&run_id, plan_hash, max_repair_turns, idempotency_key)?;
        let effect =
            self.store
                .ensure_effect(&run_id, EffectKind::Launch, &format!("plan:{plan_hash}"))?;
        let _unused = effect;
        self.summary_of(&run_id)
    }

    /// Current snapshot of a run (or the active slot when `None`).
    pub fn run_summary(&self, run_id: Option<&str>) -> Result<RunSummary, WorkflowError> {
        match run_id {
            Some(run_id) => self.summary_of(run_id),
            None => match self.store.active_run()? {
                Some(run) => summarize(&run),
                None => Err(WorkflowError::not_found("no active run".to_owned())),
            },
        }
    }

    fn summary_of(&self, run_id: &str) -> Result<RunSummary, WorkflowError> {
        match self.store.load_run(run_id)? {
            Some(run) => summarize(&run),
            None => Err(WorkflowError::not_found(format!("run {run_id} not found"))),
        }
    }

    /// Bounded run list, newest first (C7 `workflow.list`).
    pub fn list_runs(&self, limit: u32) -> Result<Vec<RunSummary>, WorkflowError> {
        self.store
            .list_runs(limit)
            .and_then(|runs| runs.iter().map(summarize).collect())
    }

    /// Full run detail for the C7 Console view, `graph`, `events`,
    /// and Markdown export. The event window is bounded by
    /// `event_limit` starting after `after_seq` (resume cursor).
    pub fn run_detail(
        &self,
        run_id: &str,
        after_seq: i64,
        event_limit: u32,
    ) -> Result<RunDetail, WorkflowError> {
        let summary = self.summary_of(run_id)?;
        Ok(RunDetail {
            summary,
            sessions: self.store.load_sessions(run_id)?,
            effects: self.store.load_effects(run_id)?,
            events: self.store.load_events(run_id, after_seq, event_limit)?,
            evidence: self.store.load_evidence(run_id)?,
            checkpoints: self.store.load_checkpoints(run_id)?,
        })
    }

    /// Stamp a pending transition onto the stored row version.
    fn apply(
        &self,
        stored: &StoredRun,
        request: TransitionRequest<'_>,
    ) -> Result<(), WorkflowError> {
        self.store.apply_transition(&RunTransition {
            run_id: &stored.run_id,
            expected_version: stored.version,
            status: request.status,
            stage: request.stage,
            round: request.round,
            candidate: stored.candidate.as_deref(),
            candidate_generation: u64::try_from(stored.candidate_generation.max(0)).unwrap_or(0),
            pause_kind: request.pause_kind,
            event_actor: request.actor,
            event_kind: request.event_kind,
            effect: request.effect,
        })
    }

    /// Submit a worker/host observation. `caller_epoch` must equal the
    /// persisted owner epoch: automatic input stamped with an older
    /// epoch (pre-restart) is rejected instead of applied.
    pub fn submit_observation(
        &self,
        run_id: &str,
        expected_version: i64,
        caller_epoch: u64,
        actor: Actor,
        observation: &Observation,
    ) -> Result<RunSummary, WorkflowError> {
        if caller_epoch != self.store.epoch() {
            return Err(WorkflowError::stale_epoch(format!(
                "caller epoch {caller_epoch} is older than owner epoch {}",
                self.store.epoch()
            )));
        }
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        let view = view_of(&stored)?;
        let max = u32::try_from(stored.max_repair_turns.max(0)).unwrap_or(u32::MAX);
        let next: NextState = advance(&view, max, observation, actor)
            .map_err(|refusal| WorkflowError::blocked(refusal.reason.to_owned()))?;
        let effect = effect_for(&next, run_id);
        let actor_label = if actor == Actor::Host {
            "host"
        } else {
            "worker"
        };
        self.store.apply_transition(&RunTransition {
            run_id,
            expected_version,
            status: next.status,
            stage: next.stage,
            round: next.round,
            candidate: stored.candidate.as_deref(),
            candidate_generation: u64::try_from(stored.candidate_generation.max(0)).unwrap_or(0),
            pause_kind: stored.pause_kind.as_deref(),
            event_actor: actor_label,
            event_kind: next.event_kind,
            effect,
        })?;
        self.summary_of(run_id)
    }
}

/// Verified local source: the tree HEAD pins, for checkpoint rows.
struct VerifiedSource {
    tree: String,
}

/// Outcome of `record_test`: the host-observed verdict plus the
/// digests that bind it to the run, candidate, and frozen policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedTest {
    pub passed: bool,
    pub policy_hash: String,
    pub artifact_digest: String,
    pub worker_id: String,
    pub exit_code: i32,
    pub excerpt: String,
}

/// Outcome of `record_review`: host validation of the isolated
/// review file plus its content digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedReview {
    pub acceptable: bool,
    pub artifact_digest: String,
}

/// Host paths for one trusted test recording. The executed check
/// script must be a member of `bundle_files`, and `bundle_files`
/// must recompute to `bundle_digest` (the manifest digest the daemon
/// already bound to the run's stored plan); the repository defaults
/// to the recorded checkpoint's local path.
#[derive(Debug, Clone)]
pub struct RecordTestParams<'a> {
    pub check_program: &'a str,
    pub check_script: &'a Path,
    pub bundle_digest: &'a str,
    pub bundle_files: &'a [TestBundleFile],
    pub repo: Option<&'a Path>,
    pub scratch: &'a Path,
    pub timeout_secs: u64,
}

/// Optimistic-concurrency guard for record paths. The caller names
/// the run row version it acted on; a moved row refuses instead of
/// collecting side rows against a stale run. Paths that also move
/// the run row re-check the version in their store compare-and-swap.
fn check_expected_version(
    stored: &StoredRun,
    run_id: &str,
    expected_version: i64,
) -> Result<(), WorkflowError> {
    if stored.version != expected_version {
        return Err(WorkflowError::stale_version(format!(
            "run {run_id} moved: expected version {expected_version}, found {}",
            stored.version
        )));
    }
    Ok(())
}

/// A candidate identity is a single non-empty path-safe token, so it
/// can name workers, artifacts, and evidence rows without escaping.
fn check_candidate_id(candidate: &str) -> Result<(), WorkflowError> {
    if candidate.trim().is_empty()
        || candidate
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '/' || ch == '\0')
    {
        return Err(WorkflowError::invalid(
            "candidate must be a single non-empty path-safe token".to_owned(),
        ));
    }
    Ok(())
}

impl WorkflowService {
    /// Record the candidate under verification. The row version must
    /// match: a moved run refuses the record. The generation counts
    /// every recorded identity, so evidence for a superseded identity
    /// never decides. An identity that already carries verification
    /// evidence is never re-recorded: changed source needs a new
    /// candidate, and identical source keeps its existing rows.
    /// Terminal runs refuse new candidates.
    pub fn record_candidate(
        &self,
        run_id: &str,
        candidate_id: &str,
        expected_version: i64,
        actor: &str,
    ) -> Result<RunSummary, WorkflowError> {
        check_candidate_id(candidate_id)?;
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        check_expected_version(&stored, run_id, expected_version)?;
        let status = RunStatus::parse(&stored.status)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown status")))?;
        if status.is_terminal() {
            return Err(WorkflowError::invalid("run is terminal".to_owned()));
        }
        let stage = Stage::parse(&stored.stage)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown stage")))?;
        if self.store.load_evidence(run_id)?.iter().any(|row| {
            row.candidate == candidate_id && (row.kind == "test" || row.kind == "review")
        }) {
            return Err(WorkflowError::invalid(format!(
                "candidate `{candidate_id}` already carries verification evidence; \
                 record a new candidate"
            )));
        }
        let generation = u64::try_from(stored.candidate_generation.max(0)).unwrap_or(0) + 1;
        self.store.apply_transition(&RunTransition {
            run_id,
            expected_version,
            status,
            stage,
            round: u32::try_from(stored.round.max(0)).unwrap_or(0),
            candidate: Some(candidate_id),
            candidate_generation: generation,
            pause_kind: stored.pause_kind.as_deref(),
            event_actor: actor,
            event_kind: "candidate_recorded",
            effect: None,
        })?;
        self.summary_of(run_id)
    }

    /// Record a worker session attachment for recovery comparison.
    /// The run must exist and stay non-terminal; the row is an upsert
    /// on the worker, so re-recording the same healthy session is
    /// idempotent.
    pub fn record_session(
        &self,
        session: &SessionAttachment<'_>,
        expected_version: i64,
        actor: &str,
    ) -> Result<RunSummary, WorkflowError> {
        let run_id = session.run_id;
        let worker_id = session.worker_id;
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        check_expected_version(&stored, run_id, expected_version)?;
        let status = RunStatus::parse(&stored.status)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown status")))?;
        if status.is_terminal() {
            return Err(WorkflowError::invalid("run is terminal".to_owned()));
        }
        if worker_id.trim().is_empty() {
            return Err(WorkflowError::invalid("worker id is required".to_owned()));
        }
        self.store.record_session(session)?;
        self.store
            .append_event(run_id, actor, "worker_started", Some(worker_id))?;
        self.summary_of(run_id)
    }

    /// Record a checkpoint for the run's current candidate. The local
    /// source is verified before the row is written: HEAD and the tree
    /// must equal the recorded identity and the worktree must be
    /// clean. A checkpoint that moves the commit after verification
    /// evidence exists is refused: changed source needs a new
    /// candidate. A backup-state flip for the same commit is allowed.
    pub fn record_checkpoint(
        &self,
        checkpoint: &CheckpointRecord<'_>,
        expected_version: i64,
    ) -> Result<RunSummary, WorkflowError> {
        let stored = self.store.load_run(checkpoint.run_id)?.ok_or_else(|| {
            WorkflowError::not_found(format!("run {} not found", checkpoint.run_id))
        })?;
        check_expected_version(&stored, checkpoint.run_id, expected_version)?;
        let status = RunStatus::parse(&stored.status).ok_or_else(|| {
            WorkflowError::storage(format!("run {} has unknown status", checkpoint.run_id))
        })?;
        if status.is_terminal() {
            return Err(WorkflowError::invalid("run is terminal".to_owned()));
        }
        if stored.candidate.as_deref() != Some(checkpoint.candidate) {
            return Err(WorkflowError::invalid(format!(
                "checkpoint candidate `{}` does not match the recorded run candidate `{}`",
                checkpoint.candidate,
                stored.candidate.as_deref().unwrap_or("none")
            )));
        }
        let verified = Self::verify_source(
            checkpoint.local_path,
            checkpoint.commit,
            checkpoint.candidate,
        )?;
        if !checkpoint.tree.trim().is_empty() && checkpoint.tree != verified.tree {
            return Err(WorkflowError::invalid(format!(
                "recorded tree {} does not match the local source tree {}",
                checkpoint.tree, verified.tree
            )));
        }
        let evidenced = self
            .store
            .load_evidence(checkpoint.run_id)?
            .iter()
            .any(|row| {
                row.candidate == checkpoint.candidate
                    && (row.kind == "test" || row.kind == "review")
            });
        if evidenced {
            let Some(previous) = self
                .store
                .load_checkpoints(checkpoint.run_id)?
                .into_iter()
                .find(|row| row.candidate == checkpoint.candidate)
            else {
                return Err(WorkflowError::storage(format!(
                    "candidate `{}` has evidence but no checkpoint",
                    checkpoint.candidate
                )));
            };
            if previous.commit_hash != checkpoint.commit || previous.tree_hash != verified.tree {
                return Err(WorkflowError::invalid(format!(
                    "candidate `{}` already has verification evidence; \
                     changed source needs a new candidate",
                    checkpoint.candidate
                )));
            }
        }
        self.store.record_checkpoint(&CheckpointRecord {
            backup_state: checkpoint.backup_state,
            commit: checkpoint.commit,
            tree: &verified.tree,
            ..*checkpoint
        })?;
        self.store.append_event(
            checkpoint.run_id,
            "host",
            "checkpoint_created",
            Some(checkpoint.candidate),
        )?;
        self.summary_of(checkpoint.run_id)
    }

    /// Verify that a local path holds the candidate identity: HEAD
    /// equals the commit, the tree matches, and the worktree is clean.
    fn verify_source(
        local_path: &str,
        commit: &str,
        candidate: &str,
    ) -> Result<VerifiedSource, WorkflowError> {
        if commit.trim().is_empty() {
            return Err(WorkflowError::invalid(
                "checkpoint commit is required".to_owned(),
            ));
        }
        let dir = Path::new(local_path);
        let head = run_git(dir, &["rev-parse", "HEAD"])?;
        if head != commit {
            return Err(WorkflowError::blocked(format!(
                "local source HEAD {head} does not match candidate `{candidate}` commit {commit}"
            )));
        }
        let tree = run_git(dir, &["rev-parse", "HEAD^{tree}"])?;
        let dirty = run_git(dir, &["status", "--porcelain"])?;
        if !dirty.is_empty() {
            return Err(WorkflowError::blocked(format!(
                "local source for candidate `{candidate}` has uncommitted changes; \
                 commit the candidate first"
            )));
        }
        Ok(VerifiedSource { tree })
    }

    /// Run the frozen check in a fresh pinned test worker and bind the
    /// host-observed verdict to the run. Only the test stage records
    /// tests, and only one verdict per candidate: re-verification
    /// needs a new candidate. The executed script must be a manifest
    /// member and the manifest must recompute to the frozen bundle
    /// digest; the recorded policy hash covers the exact host argv
    /// that ran.
    pub fn record_test(
        &self,
        run_id: &str,
        expected_version: i64,
        params: &RecordTestParams<'_>,
    ) -> Result<(RunSummary, RecordedTest), WorkflowError> {
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        check_expected_version(&stored, run_id, expected_version)?;
        let status = RunStatus::parse(&stored.status)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown status")))?;
        if status.is_terminal() {
            return Err(WorkflowError::invalid("run is terminal".to_owned()));
        }
        let stage = Stage::parse(&stored.stage)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown stage")))?;
        if stage != Stage::Test {
            return Err(WorkflowError::invalid(
                "recorded tests belong to the test stage".to_owned(),
            ));
        }
        let candidate = stored.candidate.clone().ok_or_else(|| {
            WorkflowError::blocked("no candidate is recorded for this run".to_owned())
        })?;
        let checkpoint = self
            .store
            .load_checkpoints(run_id)?
            .into_iter()
            .find(|row| row.candidate == candidate)
            .ok_or_else(|| {
                WorkflowError::blocked(format!(
                    "candidate `{candidate}` has no checkpoint; capture it first"
                ))
            })?;
        if self
            .store
            .load_evidence(run_id)?
            .iter()
            .any(|row| row.candidate == candidate && row.kind == "test")
        {
            return Err(WorkflowError::invalid(format!(
                "test evidence already recorded for `{candidate}`; \
                 record a new candidate to re-verify"
            )));
        }
        let script_bytes = std::fs::read(params.check_script).map_err(|err| {
            WorkflowError::invalid(format!(
                "cannot read check script {}: {err}",
                params.check_script.display()
            ))
        })?;
        let mut hasher = Sha256::new();
        hasher.update(&script_bytes);
        let script_digest = hex::encode(hasher.finalize());
        if !params
            .bundle_files
            .iter()
            .any(|file| file.digest == script_digest)
        {
            return Err(WorkflowError::invalid(
                "check script is not a member of the frozen test bundle".to_owned(),
            ));
        }
        let manifest_digest = bundle_manifest_digest(
            params
                .bundle_files
                .iter()
                .map(|file| (file.logical_path.as_str(), file.digest.as_str())),
        );
        if manifest_digest != params.bundle_digest {
            return Err(WorkflowError::invalid(
                "test bundle manifest does not match the frozen bundle digest".to_owned(),
            ));
        }
        let round = u32::try_from(stored.round.max(0)).unwrap_or(0);
        let worker_id = format!("test-{run_id}-r{round}-{candidate}");
        let work_dir = params.scratch.join(&worker_id);
        let policy = TestPolicy::freeze(
            vec![
                params.check_program.to_owned(),
                params.check_script.to_string_lossy().into_owned(),
                work_dir.to_string_lossy().into_owned(),
            ],
            params.timeout_secs,
        )?;
        let spec = TestWorkerSpec::new(
            run_id,
            round,
            &candidate,
            &checkpoint.commit_hash,
            &checkpoint.tree_hash,
            &policy,
        )?;
        debug_assert_eq!(spec.worker_id(), worker_id);
        let repo = params.repo.map_or_else(
            || Path::new(&checkpoint.local_path).to_path_buf(),
            std::path::PathBuf::from,
        );
        let worker = match prepare_test_worker(&repo, &spec, params.scratch) {
            Ok(worker) => worker,
            Err(error) => {
                drop(std::fs::remove_dir_all(&work_dir));
                return Err(error);
            }
        };
        let execution = match run_test_command(&policy, &worker, params.bundle_files) {
            Ok(execution) => execution,
            Err(error) => {
                drop(std::fs::remove_dir_all(&work_dir));
                return Err(error);
            }
        };
        let result = TestResult::from_execution(&spec, &policy, &execution);
        let artifact = persist_test_output(&self.store, &candidate, &execution)?;
        record_test_evidence(&self.store, run_id, &result, &artifact)?;
        self.store.append_event(
            run_id,
            "host",
            "test_completed",
            Some(result.candidate_id()),
        )?;
        let recorded = RecordedTest {
            passed: result.passed(),
            policy_hash: policy.policy_hash().to_owned(),
            artifact_digest: artifact,
            worker_id,
            exit_code: result.exit_code(),
            excerpt: result.excerpt().to_owned(),
        };
        Ok((self.summary_of(run_id)?, recorded))
    }

    /// Validate one isolated review file against the checkpoint
    /// identity and bind the verdict to the run. Only the review stage
    /// records reviews, and only one verdict per candidate.
    pub fn record_review(
        &self,
        run_id: &str,
        expected_version: i64,
        review_file: &Path,
        policy_hash: &str,
    ) -> Result<(RunSummary, RecordedReview), WorkflowError> {
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        check_expected_version(&stored, run_id, expected_version)?;
        let status = RunStatus::parse(&stored.status)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown status")))?;
        if status.is_terminal() {
            return Err(WorkflowError::invalid("run is terminal".to_owned()));
        }
        let stage = Stage::parse(&stored.stage)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown stage")))?;
        if stage != Stage::Review {
            return Err(WorkflowError::invalid(
                "recorded reviews belong to the review stage".to_owned(),
            ));
        }
        if policy_hash.trim().is_empty() {
            return Err(WorkflowError::invalid(
                "review evidence needs the recorded test policy hash".to_owned(),
            ));
        }
        let candidate = stored.candidate.clone().ok_or_else(|| {
            WorkflowError::blocked("no candidate is recorded for this run".to_owned())
        })?;
        let checkpoint = self
            .store
            .load_checkpoints(run_id)?
            .into_iter()
            .find(|row| row.candidate == candidate)
            .ok_or_else(|| {
                WorkflowError::blocked(format!(
                    "candidate `{candidate}` has no checkpoint; capture it first"
                ))
            })?;
        let evidence = self.store.load_evidence(run_id)?;
        // The review binds to the recorded test policy: a review for a
        // candidate with no recorded test verdict, or with a policy
        // hash that disagrees with it, refuses here instead of
        // storing a verdict the acceptance gate must later reject.
        let recorded_policy = evidence
            .iter()
            .find(|row| row.candidate == candidate && row.kind == "test")
            .and_then(|row| row.policy_hash.clone())
            .ok_or_else(|| {
                WorkflowError::blocked(format!(
                    "candidate `{candidate}` has no recorded test verdict; \
                     record the test verdict first"
                ))
            })?;
        if recorded_policy != policy_hash {
            return Err(WorkflowError::invalid(format!(
                "review policy `{policy_hash}` does not match the recorded test policy \
                 `{recorded_policy}` for `{candidate}`"
            )));
        }
        if evidence
            .iter()
            .any(|row| row.candidate == candidate && row.kind == "review")
        {
            return Err(WorkflowError::invalid(format!(
                "review evidence already recorded for `{candidate}`; \
                 record a new candidate to re-verify"
            )));
        }
        let bytes = read_review_bytes(review_file)?;
        let result = validate_review(
            &bytes,
            ReviewExpectation {
                candidate_id: &candidate,
                commit: &checkpoint.commit_hash,
                tree: &checkpoint.tree_hash,
            },
        )?;
        let artifact = self
            .store
            .write_artifact(&format!("review-{candidate}"), &bytes)?;
        record_review_evidence(
            &self.store,
            run_id,
            &candidate,
            policy_hash,
            &result,
            &artifact.digest,
        )?;
        self.store
            .append_event(run_id, "host", "review_completed", Some(&candidate))?;
        let recorded = RecordedReview {
            acceptable: review_acceptable(&result),
            artifact_digest: artifact.digest,
        };
        Ok((self.summary_of(run_id)?, recorded))
    }

    /// Confirm the candidate at its backup ref, then mark the backup
    /// effect done. The ref must resolve to the checkpoint commit:
    /// nothing is pushed or marked without comparison.
    pub fn record_backup(
        &self,
        run_id: &str,
        expected_version: i64,
        remote: &str,
        git_ref: &str,
        actor: &str,
    ) -> Result<RunSummary, WorkflowError> {
        if remote.trim().is_empty() || git_ref.trim().is_empty() {
            return Err(WorkflowError::invalid(
                "backup remote and ref are required".to_owned(),
            ));
        }
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        check_expected_version(&stored, run_id, expected_version)?;
        let status = RunStatus::parse(&stored.status)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown status")))?;
        if status.is_terminal() {
            return Err(WorkflowError::invalid("run is terminal".to_owned()));
        }
        let candidate = stored.candidate.clone().ok_or_else(|| {
            WorkflowError::blocked("no candidate is recorded for this run".to_owned())
        })?;
        let checkpoint = self
            .store
            .load_checkpoints(run_id)?
            .into_iter()
            .find(|row| row.candidate == candidate)
            .ok_or_else(|| {
                WorkflowError::blocked(format!(
                    "candidate `{candidate}` has no checkpoint; capture it first"
                ))
            })?;
        let listing = run_git(
            Path::new(&checkpoint.local_path),
            &["ls-remote", remote, git_ref],
        )?;
        let resolved = listing.split_whitespace().next().unwrap_or_default();
        if resolved != checkpoint.commit_hash {
            return Err(WorkflowError::blocked(format!(
                "backup ref `{git_ref}` resolves to `{resolved}`, not the candidate commit {}; \
                 push the candidate first",
                checkpoint.commit_hash
            )));
        }
        let effect =
            self.store
                .ensure_effect(run_id, EffectKind::Backup, &format!("{run_id}:backup"))?;
        self.store.set_effect(
            run_id,
            &effect.operation_id,
            EffectStatus::Done,
            Some(&format!("{remote} {git_ref}")),
            None,
        )?;
        self.store.record_checkpoint(&CheckpointRecord {
            run_id,
            candidate: &candidate,
            commit: &checkpoint.commit_hash,
            tree: &checkpoint.tree_hash,
            source_manifest: &checkpoint.source_manifest,
            local_path: &checkpoint.local_path,
            backup_state: &format!("done:{git_ref}"),
            preservation_hold: checkpoint.preservation_hold.as_deref(),
        })?;
        self.store.append_event(
            run_id,
            actor,
            "checkpoint_backup_changed",
            Some(&effect.operation_id),
        )?;
        self.summary_of(run_id)
    }

    /// Submit one named observation with optimistic-concurrency and
    /// epoch checks. Every call executes in the live service at its
    /// current owner epoch: RPC callers are operator-originated, so
    /// the epoch is read here rather than trusted from the wire, and
    /// the reducer still rejects pre-restart automatic input through
    /// [`WorkflowService::submit_observation`]. `verified_current`
    /// does not go through the plain reducer path: it runs the
    /// acceptance gate, which consults the recorded evidence before
    /// anything can mark a run accepted.
    pub fn advance_run(
        &self,
        run_id: &str,
        expected_version: i64,
        actor: &str,
        observation: &str,
        expected_policy_hash: Option<&str>,
    ) -> Result<RunSummary, WorkflowError> {
        let actor = match actor {
            "host" => Actor::Host,
            "worker" => Actor::Worker,
            _ => {
                return Err(WorkflowError::invalid(format!(
                    "unknown advance actor `{actor}`; use host or worker"
                )));
            }
        };
        let observation = match observation {
            "turn_started" => Observation::TurnStarted,
            "turn_ended" => Observation::TurnEnded,
            "test_started" => Observation::TestStarted,
            "test_passed" => Observation::TestPassed,
            "test_failed" => Observation::TestFailed,
            "review_passed" => Observation::ReviewPassed,
            "review_failed" => Observation::ReviewFailed,
            "verified_current" => Observation::VerifiedCurrent,
            "infra_failed" => Observation::InfraFailed,
            "session_lost" => Observation::SessionLost,
            "uncertain_delivery" => Observation::UncertainDelivery,
            "capture_stalled" => Observation::CaptureStalled,
            _ => {
                return Err(WorkflowError::invalid(format!(
                    "unknown advance observation `{observation}`"
                )));
            }
        };
        if observation == Observation::VerifiedCurrent {
            if actor != Actor::Host {
                return Err(WorkflowError::invalid(
                    "only the host can accept a run".to_owned(),
                ));
            }
            let expected = expected_policy_hash.unwrap_or_default();
            if expected.trim().is_empty() {
                return Err(WorkflowError::invalid(
                    "acceptance needs the expected test policy hash".to_owned(),
                ));
            }
            return self.accept_run(run_id, expected_version, expected);
        }
        self.submit_observation(run_id, expected_version, self.epoch(), actor, &observation)
    }

    /// Accept a run only when the recorded evidence decides
    /// acceptance: passing tests and an acceptable review on the same
    /// policy, a current source, host-owned input on the current
    /// epoch, no unresolved uncertainty, and a completed backup.
    /// Anything else refuses with the decision reason; repair and
    /// failure verdicts never mark a run accepted. Host-only: RPC
    /// callers must route through [`WorkflowService::advance_run`],
    /// which admits `verified_current` from the host actor alone.
    pub fn accept_run(
        &self,
        run_id: &str,
        expected_version: i64,
        expected_policy_hash: &str,
    ) -> Result<RunSummary, WorkflowError> {
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        check_expected_version(&stored, run_id, expected_version)?;
        let status = RunStatus::parse(&stored.status)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown status")))?;
        if status.is_terminal() {
            return Err(WorkflowError::invalid("run is terminal".to_owned()));
        }
        let stage = Stage::parse(&stored.stage)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown stage")))?;
        if stage != Stage::Decide {
            return Err(WorkflowError::blocked(
                "acceptance requires the decide stage".to_owned(),
            ));
        }
        let candidate = stored
            .candidate
            .clone()
            .ok_or_else(|| WorkflowError::blocked("incomplete candidate evidence".to_owned()))?;
        let checkpoint = self
            .store
            .load_checkpoints(run_id)?
            .into_iter()
            .find(|row| row.candidate == candidate)
            .ok_or_else(|| WorkflowError::blocked("incomplete candidate evidence".to_owned()))?;
        let rows: Vec<StoredEvidence> = self
            .store
            .load_evidence(run_id)?
            .into_iter()
            .filter(|row| row.candidate == candidate)
            .collect();
        let test_rows: Vec<&StoredEvidence> =
            rows.iter().filter(|row| row.kind == "test").collect();
        let review_rows: Vec<&StoredEvidence> =
            rows.iter().filter(|row| row.kind == "review").collect();
        if test_rows.is_empty() || review_rows.is_empty() {
            return Err(WorkflowError::blocked(
                "incomplete candidate evidence: recorded test and review verdicts are required"
                    .to_owned(),
            ));
        }
        if test_rows.len() > 1 || review_rows.len() > 1 {
            return Err(WorkflowError::blocked(
                "ambiguous candidate evidence: exactly one recorded verdict per kind is required"
                    .to_owned(),
            ));
        }
        let test = test_rows[0];
        let review = review_rows[0];
        let policy = test.policy_hash.as_deref().unwrap_or_default();
        if policy.is_empty() || review.policy_hash.as_deref() != Some(policy) {
            return Err(WorkflowError::blocked(
                "stale evidence: test and review verdicts disagree on the policy".to_owned(),
            ));
        }
        let round = u32::try_from(stored.round.max(0)).unwrap_or(0);
        let max = u32::try_from(stored.max_repair_turns.max(0)).unwrap_or(u32::MAX);
        let generation = u64::try_from(stored.candidate_generation.max(0)).unwrap_or(0);
        let verified =
            Self::verify_source(&checkpoint.local_path, &checkpoint.commit_hash, &candidate);
        let backup_satisfied = self.store.load_effects(run_id)?.iter().any(|effect| {
            effect.kind == EffectKind::Backup.as_str()
                && effect.status == EffectStatus::Done.as_str()
        });
        let uncertainty_pending = !self
            .store
            .commands_in_state(run_id, CommandState::DeliveryUnknown)?
            .is_empty();
        let identity = CandidateRef {
            candidate_id: &candidate,
            commit: &checkpoint.commit_hash,
            tree: &checkpoint.tree_hash,
            generation,
        };
        let evidence = CandidateEvidence {
            candidate: identity,
            current: identity,
            verification: VerificationOutcome {
                test_passed: test.result == "pass",
                review_acceptable: review.result == "pass",
                test_policy_hash: policy,
                expected_policy_hash,
            },
            readiness: HostReadiness {
                source_current: verified.is_ok(),
                backup_satisfied,
            },
            ownership: InputOwnership {
                host_owned: true,
                epoch_current: true,
                uncertainty_pending,
            },
            round,
            max_repair_turns: max,
        };
        match decide_candidate(&evidence) {
            Decision::Accept => self.submit_observation(
                run_id,
                expected_version,
                self.epoch(),
                Actor::Host,
                &Observation::VerifiedCurrent,
            ),
            Decision::Repair => Err(WorkflowError::blocked(
                "verification did not pass and the repair budget remains; \
                 repair the candidate instead of accepting"
                    .to_owned(),
            )),
            Decision::Fail(reason) | Decision::NeedsOperator(reason) => {
                Err(WorkflowError::blocked(reason))
            }
        }
    }

    /// Pause a run. `checkpoint` marks a C4 source-capture freeze,
    /// shown separately from a normal operator pause. An active agent
    /// turn may still finish within its deadline (`TurnEnded` stays
    /// admissible while paused); nothing else advances.
    pub fn pause_run(
        &self,
        run_id: &str,
        actor: &str,
        reason: &str,
        checkpoint: bool,
    ) -> Result<RunSummary, WorkflowError> {
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        let status = RunStatus::parse(&stored.status)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown status")))?;
        if status.is_terminal() {
            return Err(WorkflowError::invalid("run is terminal".to_owned()));
        }
        if status == RunStatus::Paused {
            return Err(WorkflowError::invalid("run is already paused".to_owned()));
        }
        if status == RunStatus::NeedsOperator {
            return Err(WorkflowError::blocked(
                "resolve the operator block before pausing".to_owned(),
            ));
        }
        let stage = Stage::parse(&stored.stage)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown stage")))?;
        let pause_kind = if checkpoint { "checkpoint" } else { "operator" };
        let event_kind = if checkpoint {
            "checkpoint_paused"
        } else {
            "run_paused"
        };
        self.apply(
            &stored,
            TransitionRequest {
                status: RunStatus::Paused,
                stage,
                round: u32::try_from(stored.round.max(0)).unwrap_or(0),
                pause_kind: Some(pause_kind),
                actor,
                event_kind,
                effect: None,
            },
        )?;
        self.store
            .append_event(run_id, actor, "pause_reason", Some(reason))?;
        self.summary_of(run_id)
    }

    /// Resume a paused or operator-blocked run. Resume alone never
    /// clears uncertainty: any command stuck in `delivery_unknown`
    /// blocks the resume until an operator resolves it with
    /// [`WorkflowService::resolve_input`].
    pub fn resume_run(&self, run_id: &str, actor: &str) -> Result<RunSummary, WorkflowError> {
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        if stored.status != RunStatus::Paused.as_str()
            && stored.status != RunStatus::NeedsOperator.as_str()
        {
            return Err(WorkflowError::invalid(
                "only a paused or operator-blocked run can resume".to_owned(),
            ));
        }
        let uncertain = self
            .store
            .commands_in_state(run_id, CommandState::DeliveryUnknown)?;
        if !uncertain.is_empty() {
            return Err(WorkflowError::blocked(format!(
                "command {} has uncertain delivery; resolve it before resume",
                uncertain[0].command_id
            )));
        }
        let stage = Stage::parse(&stored.stage)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown stage")))?;
        self.apply(
            &stored,
            TransitionRequest {
                status: RunStatus::Active,
                stage,
                round: u32::try_from(stored.round.max(0)).unwrap_or(0),
                pause_kind: None,
                actor,
                event_kind: "run_resumed",
                effect: None,
            },
        )?;
        self.summary_of(run_id)
    }

    /// Cancel a run. New actions stop; the checkpoint preservation
    /// hold is recorded before cleanup; the CLI process is never
    /// signalled to deliver this feedback.
    pub fn cancel_run(
        &self,
        run_id: &str,
        actor: &str,
        reason: &str,
    ) -> Result<RunSummary, WorkflowError> {
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        let status = RunStatus::parse(&stored.status)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown status")))?;
        if status.is_terminal() {
            return Err(WorkflowError::invalid("run is terminal".to_owned()));
        }
        let stage = Stage::parse(&stored.stage)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown stage")))?;
        if let Some(candidate) = stored.candidate.as_deref() {
            self.store.record_checkpoint(&CheckpointRecord {
                run_id,
                candidate,
                commit: "preserved",
                tree: "preserved",
                source_manifest: "cancel-preserved-source",
                local_path: "preserved",
                backup_state: "preserved",
                preservation_hold: Some("cancel-hold"),
            })?;
        }
        self.apply(
            &stored,
            TransitionRequest {
                status: RunStatus::Cancelled,
                stage,
                round: u32::try_from(stored.round.max(0)).unwrap_or(0),
                pause_kind: None,
                actor,
                event_kind: "run_cancelled",
                effect: None,
            },
        )?;
        self.store
            .append_event(run_id, actor, "cancel_reason", Some(reason))?;
        self.summary_of(run_id)
    }

    /// Resolve uncertain command delivery. The operator must first
    /// inspect the original session: `observed_session_state` carries
    /// that inspection and is required. The original command stays
    /// marked uncertain with a link to its resolution; a decision
    /// never replaces tests or review.
    pub fn resolve_input(
        &self,
        run_id: &str,
        command_id: &str,
        decision: OperatorDecision,
        actor: &str,
        observed_session_state: &str,
    ) -> Result<RunSummary, WorkflowError> {
        if observed_session_state.trim().is_empty() {
            return Err(WorkflowError::blocked(
                "inspect the original session before resolving uncertain delivery".to_owned(),
            ));
        }
        let command = self
            .store
            .load_command(command_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("command {command_id} not found")))?;
        if command.run_id != run_id {
            return Err(WorkflowError::invalid(
                "command does not belong to this run".to_owned(),
            ));
        }
        if command.state != CommandState::DeliveryUnknown.as_str() {
            return Err(WorkflowError::invalid(
                "only uncertain delivery can be resolved".to_owned(),
            ));
        }
        match decision {
            OperatorDecision::ContinueFromCurrent => {
                self.store.set_command_state(
                    command_id,
                    CommandState::DeliveryUnknown,
                    command.receipt.as_deref(),
                    Some(&format!(
                        "continue_from_current by {actor}: {observed_session_state}"
                    )),
                )?;
                self.reactivate(run_id, actor, "input_resolved_continue")?;
            }
            OperatorDecision::SubmitNew => {
                let new_id = format!("cmd-{}", uuid::Uuid::new_v4().as_simple());
                self.store.insert_queued_command(
                    run_id,
                    &new_id,
                    &command.session_ref,
                    &command.prompt_digest,
                    u32::try_from(command.turn.max(0)).unwrap_or(0),
                    u64::try_from(command.lease_generation.max(0)).unwrap_or(0),
                )?;
                self.store.set_command_state(
                    command_id,
                    CommandState::DeliveryUnknown,
                    command.receipt.as_deref(),
                    Some(&format!("superseded_by {new_id} ({actor})")),
                )?;
                self.reactivate(run_id, actor, "input_resolved_new")?;
            }
            OperatorDecision::Cancel => {
                self.store.set_command_state(
                    command_id,
                    CommandState::DeliveryUnknown,
                    command.receipt.as_deref(),
                    Some(&format!("run cancelled by {actor}")),
                )?;
                return self.cancel_run(run_id, actor, "operator cancelled uncertain delivery");
            }
        }
        self.summary_of(run_id)
    }

    /// Move a `needs_operator` run back to `active` at its current
    /// position after an operator decision.
    fn reactivate(&self, run_id: &str, actor: &str, event_kind: &str) -> Result<(), WorkflowError> {
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        if stored.status != RunStatus::NeedsOperator.as_str() {
            return Ok(());
        }
        let stage = Stage::parse(&stored.stage)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown stage")))?;
        self.apply(
            &stored,
            TransitionRequest {
                status: RunStatus::Active,
                stage,
                round: u32::try_from(stored.round.max(0)).unwrap_or(0),
                pause_kind: None,
                actor,
                event_kind,
                effect: None,
            },
        )
    }

    /// Commit a command to `dispatching` BEFORE the transport send and
    /// return its ID. The caller sends the prompt, then records the
    /// matched receipt with [`WorkflowService::command_written`].
    /// Prompt bytes are staged through tmpfile-flush-rename first.
    pub fn begin_dispatch(
        &self,
        run_id: &str,
        session_ref: &str,
        prompt: &str,
        turn: u32,
        lease_generation: u64,
    ) -> Result<(String, String), WorkflowError> {
        let command_id = format!("cmd-{}", uuid::Uuid::new_v4().as_simple());
        let artifact = self.store.write_artifact(&command_id, prompt.as_bytes())?;
        self.store.insert_dispatching_command(
            run_id,
            &command_id,
            session_ref,
            &artifact.digest,
            turn,
            lease_generation,
        )?;
        Ok((command_id, artifact.digest))
    }

    /// Record a dispatch intent as a `queued` row and immediately
    /// commit it to `dispatching`, WITHOUT any transport send. This
    /// is the fault-test seam for crash-between-commit-and-receipt
    /// coverage: it stages the prompt artifact and leaves the exact
    /// row a restart must recover as `delivery_unknown`.
    ///
    /// Transport stays with the session-control / repair flow; this
    /// method must never gain a send path. Recovery never resubmits:
    /// only an inspected operator resolution moves the run forward.
    pub fn record_dispatch_intent(
        &self,
        run_id: &str,
        session_ref: &str,
        prompt: &str,
        turn: u32,
        lease_generation: u64,
    ) -> Result<(RunSummary, RecordedDispatch), WorkflowError> {
        if self.store.load_run(run_id)?.is_none() {
            return Err(WorkflowError::not_found(format!("run {run_id} not found")));
        }
        if session_ref.trim().is_empty() {
            return Err(WorkflowError::invalid("session ref is required".to_owned()));
        }
        if prompt.is_empty() {
            return Err(WorkflowError::invalid("prompt is required".to_owned()));
        }
        let command_id = format!("cmd-{}", uuid::Uuid::new_v4().as_simple());
        let artifact = self.store.write_artifact(&command_id, prompt.as_bytes())?;
        self.store.insert_queued_command(
            run_id,
            &command_id,
            session_ref,
            &artifact.digest,
            turn,
            lease_generation,
        )?;
        self.store.set_command_state(
            &command_id,
            CommandState::Dispatching,
            None,
            Some("dispatch intent recorded; no transport send"),
        )?;
        let summary = self.summary_of(run_id)?;
        Ok((
            summary,
            RecordedDispatch {
                command_id,
                prompt_digest: artifact.digest,
            },
        ))
    }

    /// Record the matched Capsule receipt: only this moves a command
    /// to `written`. Never called on a lost receipt.
    pub fn command_written(&self, command_id: &str, bytes: u64) -> Result<(), WorkflowError> {
        let command = self
            .store
            .load_command(command_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("command {command_id} not found")))?;
        if command.state != CommandState::Dispatching.as_str() {
            return Err(WorkflowError::invalid(format!(
                "command {command_id} is not dispatching"
            )));
        }
        self.store.set_command_state(
            command_id,
            CommandState::Written,
            Some(&bytes.to_string()),
            None,
        )
    }

    /// Record a correlated session event as `turn_ended` with its
    /// evidence quality. Command states never imply code acceptance.
    pub fn command_turn_ended(
        &self,
        command_id: &str,
        evidence_quality: &str,
    ) -> Result<(), WorkflowError> {
        let command = self
            .store
            .load_command(command_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("command {command_id} not found")))?;
        if command.state != CommandState::Written.as_str() {
            return Err(WorkflowError::invalid(format!(
                "command {command_id} has no confirmed write"
            )));
        }
        self.store.set_command_state(
            command_id,
            CommandState::TurnEnded,
            command.receipt.as_deref(),
            Some(evidence_quality),
        )
    }

    /// Fetch or create the intent for an external effect. Stable
    /// operation IDs (`run:kind:n`) and ownership-label comparison
    /// mean a repeated call adopts the matching intent instead of
    /// duplicating it.
    pub fn plan_effect(
        &self,
        run_id: &str,
        kind: EffectKind,
        ownership_label: &str,
    ) -> Result<StoredEffect, WorkflowError> {
        if self.store.load_run(run_id)?.is_none() {
            return Err(WorkflowError::not_found(format!("run {run_id} not found")));
        }
        self.store.ensure_effect(run_id, kind, ownership_label)
    }

    /// Adopt a planned effect to the exact live identity observed in
    /// the inventory. Never matches by display name: the caller must
    /// pass the ownership label the backend assigned.
    pub fn adopt_effect(
        &self,
        run_id: &str,
        operation_id: &str,
        external_identity: &str,
    ) -> Result<(), WorkflowError> {
        self.store.set_effect(
            run_id,
            operation_id,
            EffectStatus::Adopted,
            Some(external_identity),
            None,
        )
    }
}

/// One pending host transition: position plus event/effect intent.
/// [`WorkflowService::apply`] stamps it onto the stored row version.
#[derive(Debug)]
struct TransitionRequest<'a> {
    status: RunStatus,
    stage: Stage,
    round: u32,
    pause_kind: Option<&'a str>,
    actor: &'a str,
    event_kind: &'a str,
    effect: Option<NewEffect>,
}

fn view_of(stored: &StoredRun) -> Result<RunView, WorkflowError> {
    let status = RunStatus::parse(&stored.status).ok_or_else(|| {
        WorkflowError::storage(format!("run {} has unknown status", stored.run_id))
    })?;
    let stage = Stage::parse(&stored.stage).ok_or_else(|| {
        WorkflowError::storage(format!("run {} has unknown stage", stored.run_id))
    })?;
    Ok(RunView {
        status,
        stage,
        round: u32::try_from(stored.round.max(0)).unwrap_or(0),
        candidate_generation: u64::try_from(stored.candidate_generation.max(0)).unwrap_or(0),
    })
}

fn summarize(stored: &StoredRun) -> Result<RunSummary, WorkflowError> {
    let status = RunStatus::parse(&stored.status).ok_or_else(|| {
        WorkflowError::storage(format!("run {} has unknown status", stored.run_id))
    })?;
    let stage = Stage::parse(&stored.stage).ok_or_else(|| {
        WorkflowError::storage(format!("run {} has unknown stage", stored.run_id))
    })?;
    Ok(RunSummary {
        run_id: stored.run_id.clone(),
        plan_hash: stored.plan_hash.clone(),
        status: status.as_str().to_owned(),
        stage: stage.as_str().to_owned(),
        round: u32::try_from(stored.round.max(0)).unwrap_or(0),
        candidate: stored.candidate.clone(),
        candidate_generation: u64::try_from(stored.candidate_generation.max(0)).unwrap_or(0),
        version: stored.version,
        owner_epoch: u64::try_from(stored.owner_epoch.max(0)).unwrap_or(0),
        pause_kind: stored.pause_kind.clone(),
        max_repair_turns: u32::try_from(stored.max_repair_turns.max(0)).unwrap_or(0),
    })
}

/// Next effect intent for a reducer output. A finished implementer
/// turn needs a capture; everything else dispatches on demand.
fn effect_for(next: &NextState, run_id: &str) -> Option<NewEffect> {
    if next.event_kind == "turn_ended"
        && matches!(next.status, RunStatus::Active | RunStatus::Paused)
    {
        let existing = next.round;
        Some(NewEffect {
            operation_id: format!("{run_id}:capture:{}", existing.saturating_add(1)),
            kind: EffectKind::Capture,
            expected_state: None,
            external_identity: None,
            ownership_label: Some(format!("{run_id}:capture")),
            preservation: None,
        })
    } else {
        None
    }
}
