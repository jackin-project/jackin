// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Durable host workflow runs: fixed reducer, single active-run lock,
//! owner epochs, command delivery tracking, effect intents, and C5
//! restart recovery, persisted in a dedicated Turso file.

mod model;
mod service;
pub mod spool;
mod store;
mod verify;

#[cfg(test)]
mod tests;

pub use model::{
    Actor, CommandState, EffectKind, EffectStatus, NextState, Observation, ReducerRefusal,
    RunStatus, RunView, Stage, advance,
};
pub use service::{
    GRAPH_STAGES, InstanceInventory, LiveInstance, OperatorDecision, RecordTestParams,
    RecordedReview, RecordedTest, RecoveryReport, RunDetail, RunSummary, UnavailableBackend,
    WorkflowBackend, WorkflowService, graph_node_state,
};
pub use store::{
    ArtifactRef, CheckpointRecord, EvidenceRecord, NewCommand, NewEffect, RunStore, RunTransition,
    SessionAttachment, StoredCheckpoint, StoredCommand, StoredEffect, StoredEvent, StoredEvidence,
    StoredRun, StoredSession,
};
pub use verify::{
    BoundedOutput, CandidateEvidence, CandidateRef, Decision, DeliveryGate, FailingCheck,
    FeedbackInput, HostReadiness, InputOwnership, MAX_FEEDBACK_BYTES, MAX_REVIEW_BYTES,
    MAX_REVIEW_CORRECTIONS, MAX_TEST_EXCERPT_BYTES, MAX_TEST_OUTPUT_BYTES, NoAgentCredentials,
    PreparedTestWorker, RepairDelivery, ReviewExpectation, ReviewFinding, ReviewResult,
    ReviewStatus, ReviewWorkerSpec, SessionIdentity, Severity, TestExecution, TestPolicy,
    TestResult, TestWorkerSpec, VerificationOutcome, admit_repair_delivery, build_feedback,
    correction_allowed, decide_candidate, persist_test_output, prepare_test_worker,
    read_review_bytes, record_review_evidence, record_test_evidence, review_acceptable,
    run_test_command, stage_repair_feedback, summarize_test_failure, validate_review,
};

/// Machine-readable workflow failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowError {
    kind: ErrorKind,
    message: String,
}

/// Workflow failure kinds for CLI/daemon mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Invalid,
    NotFound,
    Busy,
    StaleVersion,
    StaleEpoch,
    Blocked,
    Storage,
}

impl WorkflowError {
    /// Caller input cannot apply (bad state, bad shape).
    #[must_use]
    pub fn invalid(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Invalid,
            message: message.into(),
        }
    }

    /// Named run/command/effect does not exist.
    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::NotFound,
            message: message.into(),
        }
    }

    /// A second start while the single active-run slot is taken.
    #[must_use]
    pub fn busy(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Busy,
            message: message.into(),
        }
    }

    /// Optimistic-concurrency loss: the row moved underneath the caller.
    #[must_use]
    pub fn stale_version(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::StaleVersion,
            message: message.into(),
        }
    }

    /// Automatic input stamped with a pre-restart owner epoch.
    #[must_use]
    pub fn stale_epoch(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::StaleEpoch,
            message: message.into(),
        }
    }

    /// The run needs an operator decision first.
    #[must_use]
    pub fn blocked(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Blocked,
            message: message.into(),
        }
    }

    /// The durable store cannot save a required record: new actions
    /// must stop while instances are preserved.
    #[must_use]
    pub fn storage(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Storage,
            message: message.into(),
        }
    }

    /// Failure kind for response mapping.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Human-readable failure detail.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<String> for WorkflowError {
    fn from(message: String) -> Self {
        Self::storage(message)
    }
}

impl std::fmt::Display for WorkflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for WorkflowError {}
