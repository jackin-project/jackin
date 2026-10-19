// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Unavailable backend implements the same closed service contract.
use super::UnavailableBackend;
use super::{CheckpointRecord, RecordTestParams, SessionAttachment};
use super::{
    InstanceInventory, OperatorDecision, RecordedDispatch, RecordedReview, RecordedTest,
    RecoveryReport, RunDetail, RunSummary, WorkflowBackend,
};
use crate::WorkflowError;

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

    fn list_runs(&mut self, _limit: u32) -> Result<Vec<RunSummary>, WorkflowError> {
        Err(self.error())
    }

    fn run_detail(
        &mut self,
        _run_id: &str,
        _after_seq: i64,
        _event_limit: u32,
    ) -> Result<RunDetail, WorkflowError> {
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

    fn record_session(
        &mut self,
        _session: &SessionAttachment<'_>,
        _expected_version: i64,
    ) -> Result<RunSummary, WorkflowError> {
        Err(self.error())
    }

    fn record_candidate(
        &mut self,
        _run_id: &str,
        _candidate_id: &str,
        _expected_version: i64,
    ) -> Result<RunSummary, WorkflowError> {
        Err(self.error())
    }

    fn record_checkpoint(
        &mut self,
        _checkpoint: &CheckpointRecord<'_>,
        _expected_version: i64,
    ) -> Result<RunSummary, WorkflowError> {
        Err(self.error())
    }

    fn record_test(
        &mut self,
        _run_id: &str,
        _expected_version: i64,
        _params: &RecordTestParams<'_>,
    ) -> Result<(RunSummary, RecordedTest), WorkflowError> {
        Err(self.error())
    }

    fn record_review(
        &mut self,
        _run_id: &str,
        _expected_version: i64,
        _review_file: &str,
        _policy_hash: &str,
    ) -> Result<(RunSummary, RecordedReview), WorkflowError> {
        Err(self.error())
    }

    fn record_backup(
        &mut self,
        _run_id: &str,
        _expected_version: i64,
        _remote: &str,
        _git_ref: &str,
    ) -> Result<RunSummary, WorkflowError> {
        Err(self.error())
    }

    fn advance_run(
        &mut self,
        _run_id: &str,
        _expected_version: i64,
        _actor: &str,
        _observation: &str,
        _expected_policy_hash: Option<&str>,
    ) -> Result<RunSummary, WorkflowError> {
        Err(self.error())
    }

    fn record_dispatch_intent(
        &mut self,
        _run_id: &str,
        _session_ref: &str,
        _prompt: &str,
        _turn: u32,
        _lease_generation: u64,
    ) -> Result<(RunSummary, RecordedDispatch), WorkflowError> {
        Err(self.error())
    }

    fn recover(&mut self, _inventory: &InstanceInventory) -> Result<RecoveryReport, WorkflowError> {
        Err(self.error())
    }
}
