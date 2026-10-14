// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Plan data transfer objects. All numerics are integers; map fields
//! use [`BTreeMap`] so serialization order is deterministic.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Version of the workflow schema this crate implements.
pub const SCHEMA_VERSION: u32 = 1;

/// The only template this release compiles.
pub const TEMPLATE_REPAIR_V1: &str = "repair-v1";

/// Fixed repair sequence stages. The repair relation is a bounded
/// controller action from `decide` back to `test`, not a graph edge.
pub const REPAIR_STAGES: &[&str] = &[
    "prepare",
    "implement",
    "capture",
    "test",
    "review",
    "decide",
];

/// Resolved agent identity. Account IDs and credential references are
/// identifiers, never secret values; the live model is `None` when the
/// vendor cannot report it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSelection {
    /// Agent runtime name (`claude` or `codex` in this release).
    pub runtime: String,
    /// Agent slug from the existing selection.
    pub agent_slug: String,
    /// Role ID from the existing selection.
    pub role_id: String,
    /// Exact role revision.
    pub role_revision: String,
    /// Account ID from the existing selection.
    pub account_id: String,
    /// Authentication mode used for the account.
    pub auth_mode: String,
    /// Requested model ID.
    pub requested_model: String,
    /// Vendor-reported model ID, `None` when unknowable.
    pub actual_model: Option<String>,
    /// Digest of the agent binary.
    pub binary_digest: String,
    /// Digest of the agent image.
    pub image_digest: String,
}

/// One frozen input file with its content digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputFile {
    /// Path relative to the workflow file's directory.
    pub logical_path: String,
    /// Hex SHA-256 over the frozen bytes.
    pub content_digest: String,
    /// Exact byte length.
    pub byte_length: u64,
    /// Read-only mount path inside the worker.
    pub mount_path: String,
}

/// Frozen repository state. `HEAD` is resolved to [`Self::base_commit`]
/// once, before hashing, and never stored as a selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryInput {
    /// Authorized repository ID.
    pub repository_id: String,
    /// Exact base commit.
    pub base_commit: String,
    /// Digest over the frozen source manifest.
    pub manifest_digest: String,
}

/// Integer resource limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// CPU in millicores.
    pub cpu_millicores: u64,
    /// Memory in bytes.
    pub memory_bytes: u64,
    /// Process count.
    pub max_pids: u32,
    /// Maximum run time in seconds.
    pub max_run_seconds: u64,
    /// Disk monitor threshold in bytes.
    pub disk_monitor_bytes: u64,
}

/// Trusted test bundle plus the exact check command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationPolicy {
    /// Frozen test bundle name.
    pub test_bundle: String,
    /// Digest over the frozen test bundle bytes.
    pub test_bundle_digest: String,
    /// Exact check argv, never empty.
    pub check_argv: Vec<String>,
    /// Digest of the test image.
    pub check_image_digest: String,
    /// Check timeout in seconds.
    pub timeout_seconds: u64,
    /// Review schema version.
    pub review_schema_version: u32,
}

/// Backup policy. Account IDs and credential references are identifiers,
/// never secret values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupPolicy {
    /// `off`, `best_effort`, or `required`.
    pub mode: String,
    /// Authorized remote ID, required unless the mode is `off`.
    pub remote_id: Option<String>,
    /// Host credential reference, never a secret value.
    pub credential_ref: Option<String>,
    /// Permitted ref prefix on the remote.
    pub ref_prefix: Option<String>,
}

/// One closed-sequence node. IDs are stable: `{stage}@r{round}`, with
/// `prepare` existing only at round zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanNode {
    /// Stable node ID.
    pub id: String,
    /// Stage name from [`REPAIR_STAGES`].
    pub stage: String,
    /// Repair round, zero for the initial pass.
    pub round: u32,
}

/// Host capability facts granted to the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityFacts {
    /// Sandbox backend name.
    pub sandbox_backend: String,
    /// Sandbox profile name.
    pub sandbox_profile: String,
    /// Remote IDs the host authorizes, identifiers only.
    pub authorized_remote_ids: Vec<String>,
    /// Evaluator name and version that produced the plan.
    pub evaluator: String,
    /// Workflow schema version the plan was compiled from.
    pub schema_version: u32,
}

/// The complete immutable plan. `plan_hash` is derived from every other
/// field and excluded from its own hash input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    /// Workflow schema version.
    pub schema_version: u32,
    /// Fixed template name.
    pub template: String,
    /// Closed node sequence.
    pub nodes: Vec<PlanNode>,
    /// Implementer selection (claude in this release).
    pub implementer: AgentSelection,
    /// Reviewer selection (codex in this release).
    pub reviewer: AgentSelection,
    /// Frozen task file.
    pub task: InputFile,
    /// Frozen reference files.
    pub references: Vec<InputFile>,
    /// Frozen repository state.
    pub repository: RepositoryInput,
    /// Resource limits after host-grant clamping.
    pub resources: ResourceLimits,
    /// Verification policy.
    pub verification: VerificationPolicy,
    /// Backup policy.
    pub backup: BackupPolicy,
    /// Bounded repair turns, `0..=2`.
    pub max_repair_turns: u32,
    /// Hex digests over every frozen input, keyed by logical path.
    pub input_digests: BTreeMap<String, String>,
    /// Granted capability facts.
    pub capability_facts: CapabilityFacts,
    /// Hex SHA-256 over the version prefix plus canonical plan bytes.
    pub plan_hash: String,
}
