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

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::model::{Actor, NextState, Observation, RunStatus, RunView, Stage, advance};
use super::store::{
    CheckpointRecord, NewEffect, RunStore, RunTransition, StoredEffect, StoredRun, StoredSession,
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
    fn recover(&mut self, inventory: &InstanceInventory) -> Result<RecoveryReport, WorkflowError>;
}

/// Unavailable backend for request paths without a store (tests and
/// degraded serve). Every run-control call reports the stored reason.
#[derive(Debug, Clone)]
pub struct UnavailableBackend {
    reason: String,
}

impl UnavailableBackend {
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    fn error(&self) -> WorkflowError {
        WorkflowError::storage(format!("workflow service unavailable: {}", self.reason))
    }
}

impl WorkflowBackend for UnavailableBackend {
    fn backend_epoch(&self) -> u64 {
        0
    }

    fn start_run(
        &mut self,
        _plan_hash: &str,
        _max_repair_turns: u32,
        _idempotency_key: &str,
    ) -> Result<RunSummary, WorkflowError> {
        Err(self.error())
    }

    fn run_status(&mut self, _run_id: Option<&str>) -> Result<RunSummary, WorkflowError> {
        Err(self.error())
    }

    fn pause_run(
        &mut self,
        _run_id: &str,
        _actor: &str,
        _reason: &str,
        _checkpoint: bool,
    ) -> Result<RunSummary, WorkflowError> {
        Err(self.error())
    }

    fn resume_run(&mut self, _run_id: &str, _actor: &str) -> Result<RunSummary, WorkflowError> {
        Err(self.error())
    }

    fn cancel_run(
        &mut self,
        _run_id: &str,
        _actor: &str,
        _reason: &str,
    ) -> Result<RunSummary, WorkflowError> {
        Err(self.error())
    }

    fn resolve_input(
        &mut self,
        _run_id: &str,
        _command_id: &str,
        _decision: OperatorDecision,
        _actor: &str,
        _observed_session_state: &str,
    ) -> Result<RunSummary, WorkflowError> {
        Err(self.error())
    }

    fn recover(&mut self, _inventory: &InstanceInventory) -> Result<RecoveryReport, WorkflowError> {
        Err(self.error())
    }
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

    /// Recover incomplete runs after a restart against a live instance
    /// inventory (C5 recovery table). Crash-left `dispatching`
    /// commands become `delivery_unknown` and are never resubmitted
    /// automatically.
    pub fn recover(&self, inventory: &InstanceInventory) -> Result<RecoveryReport, WorkflowError> {
        let run = self.store.active_run()?;
        let Some(run) = run else {
            return Ok(RecoveryReport {
                run_id: None,
                actions: vec!["no incomplete run".to_owned()],
            });
        };
        let run_id = run.run_id.clone();
        let mut actions = Vec::new();
        actions.extend(self.recover_commands(&run_id)?);
        actions.extend(self.recover_effects(&run_id, inventory)?);
        if let Some(candidate) = run.candidate.clone() {
            let discarded = self.store.discard_partial_evidence(&run_id, &candidate)?;
            if discarded > 0 {
                actions.push(format!(
                    "discarded {discarded} partial test/review rows; one fresh attempt allowed"
                ));
            }
        }
        actions.extend(self.recover_sessions(&run_id, inventory)?);
        actions.extend(self.recover_artifacts(&run_id)?);
        if actions.is_empty() {
            actions.push("recovered with no findings".to_owned());
        }
        Ok(RecoveryReport {
            run_id: Some(run_id),
            actions,
        })
    }

    /// Crash-left `dispatching` commands become `delivery_unknown` and
    /// block automatic input. They are never resubmitted.
    fn recover_commands(&self, run_id: &str) -> Result<Vec<String>, WorkflowError> {
        let mut actions = Vec::new();
        for command in self
            .store
            .commands_in_state(run_id, CommandState::Dispatching)?
        {
            self.store.set_command_state(
                &command.command_id,
                CommandState::DeliveryUnknown,
                None,
                Some("recovered: dispatch interrupted before receipt"),
            )?;
            actions.push(format!(
                "command {} is delivery_unknown; automatic input blocked",
                command.command_id
            ));
        }
        if !self
            .store
            .commands_in_state(run_id, CommandState::DeliveryUnknown)?
            .is_empty()
        {
            self.escalate_to_operator(run_id, "uncertain_delivery")?;
            actions.push("run needs_operator: uncertain input delivery".to_owned());
        }
        Ok(actions)
    }

    /// Adopt matching launch instances by ownership label, report
    /// paused capture staging, and hold uncertain backups for an
    /// exact remote-ref query.
    fn recover_effects(
        &self,
        run_id: &str,
        inventory: &InstanceInventory,
    ) -> Result<Vec<String>, WorkflowError> {
        let mut actions = Vec::new();
        for effect in self.store.load_effects(run_id)? {
            let kind = EffectKind::parse(&effect.kind).ok_or_else(|| {
                WorkflowError::storage(format!("unknown effect kind {}", effect.kind))
            })?;
            let pending = effect.status == EffectStatus::Intended.as_str()
                || effect.status == EffectStatus::Dispatched.as_str();
            match kind {
                EffectKind::Launch => {
                    if pending {
                        actions.push(self.recover_launch(run_id, &effect, inventory)?);
                    }
                }
                EffectKind::Capture => {
                    if effect.status != EffectStatus::Done.as_str()
                        && effect.status != EffectStatus::Abandoned.as_str()
                        && capture_staging_paused(&effect, inventory)
                    {
                        self.escalate_to_operator(run_id, "capture_staging_paused")?;
                        actions.push(format!(
                            "capture staging paused for operation {}; inspect then resume or preserve",
                            effect.operation_id
                        ));
                    }
                }
                EffectKind::Backup => {
                    if pending {
                        actions.push(self.recover_backup(run_id, &effect, inventory)?);
                    }
                }
            }
        }
        Ok(actions)
    }

    /// Adopt the exact live instance matching a launch intent, or
    /// require backend-absence confirmation before any relaunch.
    fn recover_launch(
        &self,
        run_id: &str,
        effect: &StoredEffect,
        inventory: &InstanceInventory,
    ) -> Result<String, WorkflowError> {
        let label = effect.ownership_label.clone().unwrap_or_default();
        match inventory
            .instances
            .iter()
            .find(|instance| instance.ownership_label == label)
        {
            Some(instance) => {
                self.store.set_effect(
                    run_id,
                    &effect.operation_id,
                    EffectStatus::Adopted,
                    Some(&instance.instance_id),
                    None,
                )?;
                Ok(format!(
                    "adopted {} for operation {}",
                    instance.instance_id, effect.operation_id
                ))
            }
            None => Ok(format!(
                "no live instance for operation {}; confirm backend absence before relaunch",
                effect.operation_id
            )),
        }
    }

    /// Confirm a backup at its exact remote ref, or hold it uncertain
    /// until that ref is queried. Never pushes without comparison.
    fn recover_backup(
        &self,
        run_id: &str,
        effect: &StoredEffect,
        inventory: &InstanceInventory,
    ) -> Result<String, WorkflowError> {
        let confirmed = effect
            .external_identity
            .as_deref()
            .is_some_and(|identity| inventory.backup_refs.iter().any(|known| known == identity));
        if confirmed {
            self.store.set_effect(
                run_id,
                &effect.operation_id,
                EffectStatus::Done,
                effect.external_identity.as_deref(),
                None,
            )?;
            Ok(format!(
                "backup operation {} confirmed at its remote ref",
                effect.operation_id
            ))
        } else {
            Ok(format!(
                "backup operation {} is uncertain; query the exact remote ref before retry",
                effect.operation_id
            ))
        }
    }

    /// Record lost sessions without claiming same-session
    /// continuation. Sources are always kept.
    fn recover_sessions(
        &self,
        run_id: &str,
        inventory: &InstanceInventory,
    ) -> Result<Vec<String>, WorkflowError> {
        let mut actions = Vec::new();
        for session in self.store.load_sessions(run_id)? {
            if session_lost(&session, inventory) {
                self.store.append_event(
                    run_id,
                    "host",
                    "session_lost",
                    session.instance_id.as_deref(),
                )?;
                self.escalate_to_operator(run_id, "session_lost")?;
                actions.push(format!(
                    "session for worker {} lost; source kept, no same-session claim",
                    session.worker_id
                ));
            }
        }
        Ok(actions)
    }

    /// Verify recorded prompt artifacts still match bytes on disk.
    fn recover_artifacts(&self, run_id: &str) -> Result<Vec<String>, WorkflowError> {
        let mut actions = Vec::new();
        for command in self
            .store
            .commands_in_state(run_id, CommandState::Written)?
            .into_iter()
            .chain(
                self.store
                    .commands_in_state(run_id, CommandState::TurnEnded)?,
            )
        {
            if !self
                .store
                .verify_artifact(&command.command_id, &command.prompt_digest)?
            {
                self.store.append_event(
                    run_id,
                    "host",
                    "artifact_digest_mismatch",
                    Some(&command.command_id),
                )?;
                actions.push(format!(
                    "prompt artifact for command {} no longer verifies",
                    command.command_id
                ));
            }
        }
        Ok(actions)
    }

    /// Move a run to `needs_operator`, keeping its stage and round.
    fn escalate_to_operator(&self, run_id: &str, event_kind: &str) -> Result<(), WorkflowError> {
        let stored = self
            .store
            .load_run(run_id)?
            .ok_or_else(|| WorkflowError::not_found(format!("run {run_id} not found")))?;
        if stored.status == RunStatus::NeedsOperator.as_str() {
            self.store.append_event(run_id, "host", event_kind, None)?;
            return Ok(());
        }
        let status = RunStatus::parse(&stored.status)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown status")))?;
        if status.is_terminal() || status == RunStatus::Paused {
            self.store.append_event(run_id, "host", event_kind, None)?;
            return Ok(());
        }
        let stage = Stage::parse(&stored.stage)
            .ok_or_else(|| WorkflowError::storage(format!("run {run_id} has unknown stage")))?;
        self.apply(
            &stored,
            TransitionRequest {
                status: RunStatus::NeedsOperator,
                stage,
                round: u32::try_from(stored.round.max(0)).unwrap_or(0),
                pause_kind: stored.pause_kind.as_deref(),
                actor: "host",
                event_kind,
                effect: None,
            },
        )
    }
}

impl WorkflowBackend for WorkflowService {
    fn backend_epoch(&self) -> u64 {
        self.epoch()
    }

    fn start_run(
        &mut self,
        plan_hash: &str,
        max_repair_turns: u32,
        idempotency_key: &str,
    ) -> Result<RunSummary, WorkflowError> {
        WorkflowService::start_run(self, plan_hash, max_repair_turns, idempotency_key)
    }

    fn run_status(&mut self, run_id: Option<&str>) -> Result<RunSummary, WorkflowError> {
        WorkflowService::run_summary(self, run_id)
    }

    fn pause_run(
        &mut self,
        run_id: &str,
        actor: &str,
        reason: &str,
        checkpoint: bool,
    ) -> Result<RunSummary, WorkflowError> {
        WorkflowService::pause_run(self, run_id, actor, reason, checkpoint)
    }

    fn resume_run(&mut self, run_id: &str, actor: &str) -> Result<RunSummary, WorkflowError> {
        WorkflowService::resume_run(self, run_id, actor)
    }

    fn cancel_run(
        &mut self,
        run_id: &str,
        actor: &str,
        reason: &str,
    ) -> Result<RunSummary, WorkflowError> {
        WorkflowService::cancel_run(self, run_id, actor, reason)
    }

    fn resolve_input(
        &mut self,
        run_id: &str,
        command_id: &str,
        decision: OperatorDecision,
        actor: &str,
        observed_session_state: &str,
    ) -> Result<RunSummary, WorkflowError> {
        WorkflowService::resolve_input(
            self,
            run_id,
            command_id,
            decision,
            actor,
            observed_session_state,
        )
    }

    fn recover(&mut self, inventory: &InstanceInventory) -> Result<RecoveryReport, WorkflowError> {
        WorkflowService::recover(self, inventory)
    }
}

/// A recorded worker session is lost when its instance is gone,
/// its session died, or its Capsule epoch / process identity changed.
/// The source is always kept; recovery never claims same-session
/// continuation.
fn session_lost(session: &StoredSession, inventory: &InstanceInventory) -> bool {
    let live = session.instance_id.as_deref().and_then(|identity| {
        inventory
            .instances
            .iter()
            .find(|instance| instance.instance_id == identity)
    });
    let Some(instance) = live else {
        return session.instance_id.is_some();
    };
    if !instance.session_alive {
        return true;
    }
    if session.capsule_epoch.is_some_and(|epoch| {
        instance
            .capsule_epoch
            .is_some_and(|live_epoch| u64::try_from(epoch).unwrap_or(0) != live_epoch)
    }) {
        return true;
    }
    session.process_identity.as_deref().is_some_and(|identity| {
        instance
            .process_identity
            .as_deref()
            .is_some_and(|live| live != identity)
    })
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

/// A capture effect needs inspection when a live instance with the
/// same ownership label is left paused mid-staging.
fn capture_staging_paused(effect: &StoredEffect, inventory: &InstanceInventory) -> bool {
    inventory.instances.iter().any(|instance| {
        instance.capture_paused
            && effect
                .ownership_label
                .as_deref()
                .is_some_and(|label| instance.ownership_label == label)
    })
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
