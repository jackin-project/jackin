// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

pub const TEMPLATE_REPAIR_V1: &str = "repair-v1";

pub const REPAIR_STAGES: &[&str] = &[
    "prepare",
    "implement",
    "capture",
    "test",
    "review",
    "decide",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSelection {
    pub runtime: String,

    pub agent_slug: String,

    pub role_id: String,

    pub role_revision: String,

    pub account_id: String,

    pub auth_mode: String,

    pub requested_model: String,

    pub actual_model: Option<String>,

    pub binary_digest: String,

    pub image_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputFile {
    pub logical_path: String,

    pub content_digest: String,

    pub byte_length: u64,

    pub mount_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryInput {
    pub repository_id: String,

    pub base_commit: String,

    pub manifest_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu_millicores: u64,

    pub memory_bytes: u64,

    pub max_pids: u32,

    pub max_run_seconds: u64,

    pub disk_monitor_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestBundleFile {
    pub logical_path: String,

    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationPolicy {
    pub test_bundle: String,

    pub test_bundle_digest: String,

    pub test_bundle_files: Vec<TestBundleFile>,

    pub check_argv: Vec<String>,

    pub check_image_digest: String,

    pub timeout_seconds: u64,

    pub review_schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupPolicy {
    pub mode: String,

    pub remote_id: Option<String>,

    pub credential_ref: Option<String>,

    pub ref_prefix: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanNode {
    pub id: String,

    pub stage: String,

    pub round: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityFacts {
    pub sandbox_backend: String,

    pub sandbox_profile: String,

    pub authorized_remote_ids: Vec<String>,

    pub evaluator: String,

    pub schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub schema_version: u32,

    pub template: String,

    pub nodes: Vec<PlanNode>,

    pub implementer: AgentSelection,

    pub reviewer: AgentSelection,

    pub task: InputFile,

    pub references: Vec<InputFile>,

    pub repository: RepositoryInput,

    pub resources: ResourceLimits,

    pub verification: VerificationPolicy,

    pub backup: BackupPolicy,

    pub max_repair_turns: u32,

    pub input_digests: BTreeMap<String, String>,

    pub capability_facts: CapabilityFacts,

    pub plan_hash: String,
}
