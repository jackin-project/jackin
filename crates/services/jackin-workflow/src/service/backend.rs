// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Durable service implementation of the backend contract.
use super::{CheckpointRecord, RecordTestParams, SessionAttachment, WorkflowService};
use super::{
    InstanceInventory, OperatorDecision, RecordedDispatch, RecordedReview, RecordedTest,
    RecoveryReport, RunDetail, RunSummary, WorkflowBackend,
};
use crate::WorkflowError;
use std::path::Path;

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

    fn list_runs(&mut self, limit: u32) -> Result<Vec<RunSummary>, WorkflowError> {
        WorkflowService::list_runs(self, limit)
    }

    fn run_detail(
        &mut self,
        run_id: &str,
        after_seq: i64,
        event_limit: u32,
    ) -> Result<RunDetail, WorkflowError> {
        WorkflowService::run_detail(self, run_id, after_seq, event_limit)
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

    fn record_session(
        &mut self,
        session: &SessionAttachment<'_>,
        expected_version: i64,
    ) -> Result<RunSummary, WorkflowError> {
        WorkflowService::record_session(self, session, expected_version, "operator")
    }

    fn record_candidate(
        &mut self,
        run_id: &str,
        candidate_id: &str,
        expected_version: i64,
    ) -> Result<RunSummary, WorkflowError> {
        WorkflowService::record_candidate(self, run_id, candidate_id, expected_version, "operator")
    }

    fn record_checkpoint(
        &mut self,
        checkpoint: &CheckpointRecord<'_>,
        expected_version: i64,
    ) -> Result<RunSummary, WorkflowError> {
        WorkflowService::record_checkpoint(self, checkpoint, expected_version)
    }

    fn record_test(
        &mut self,
        run_id: &str,
        expected_version: i64,
        params: &RecordTestParams<'_>,
    ) -> Result<(RunSummary, RecordedTest), WorkflowError> {
        WorkflowService::record_test(self, run_id, expected_version, params)
    }

    fn record_review(
        &mut self,
        run_id: &str,
        expected_version: i64,
        review_file: &str,
        policy_hash: &str,
    ) -> Result<(RunSummary, RecordedReview), WorkflowError> {
        WorkflowService::record_review(
            self,
            run_id,
            expected_version,
            Path::new(review_file),
            policy_hash,
        )
    }

    fn record_backup(
        &mut self,
        run_id: &str,
        expected_version: i64,
        remote: &str,
        git_ref: &str,
    ) -> Result<RunSummary, WorkflowError> {
        WorkflowService::record_backup(self, run_id, expected_version, remote, git_ref, "operator")
    }

    fn advance_run(
        &mut self,
        run_id: &str,
        expected_version: i64,
        actor: &str,
        observation: &str,
        expected_policy_hash: Option<&str>,
    ) -> Result<RunSummary, WorkflowError> {
        WorkflowService::advance_run(
            self,
            run_id,
            expected_version,
            actor,
            observation,
            expected_policy_hash,
        )
    }

    fn record_dispatch_intent(
        &mut self,
        run_id: &str,
        session_ref: &str,
        prompt: &str,
        turn: u32,
        lease_generation: u64,
    ) -> Result<(RunSummary, RecordedDispatch), WorkflowError> {
        WorkflowService::record_dispatch_intent(
            self,
            run_id,
            session_ref,
            prompt,
            turn,
            lease_generation,
        )
    }

    fn recover(&mut self, inventory: &InstanceInventory) -> Result<RecoveryReport, WorkflowError> {
        WorkflowService::recover(self, inventory)
    }
}
