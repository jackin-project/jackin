// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Canonical plan bytes and SHA-256 plan hashes.
//!
//! Canonical bytes are the plan serialized to JSON with deterministic
//! map order (`serde_json` maps sort keys), integer numerics only, and
//! meaningful array order preserved. The plan hash covers a version
//! prefix plus the exact stored bytes; the derived `plan_hash` field is
//! excluded from its own hash input. Hashing uses `sha2` 0.11 through
//! the workspace pin, matching the existing hash-field precedent.

use sha2::{Digest as _, Sha256};

use crate::error::{Diagnostic, ErrorCode, WorkflowError, WorkflowResult};
use crate::model::ExecutionPlan;

/// Version prefix hashed ahead of the canonical plan bytes.
pub const PLAN_HASH_PREFIX: &[u8] = b"jackin-workflow-plan/v1\n";

/// Hex SHA-256 over bytes.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Canonical bytes of a plan: JSON with the derived `plan_hash` field
/// removed. Deterministic across runs for the same plan value.
pub fn canonical_plan_bytes(plan: &ExecutionPlan) -> WorkflowResult<Vec<u8>> {
    let mut value = serde_json::to_value(plan).map_err(|error| {
        WorkflowError::single(Diagnostic::new(
            "",
            "plan",
            ErrorCode::InvalidDefinition,
            format!("cannot encode the plan: {error}"),
            "recompile the workflow",
        ))
    })?;
    if let Some(object) = value.as_object_mut() {
        object.remove("plan_hash");
    }
    serde_json::to_vec(&value).map_err(|error| {
        WorkflowError::single(Diagnostic::new(
            "",
            "plan",
            ErrorCode::InvalidDefinition,
            format!("cannot encode the plan: {error}"),
            "recompile the workflow",
        ))
    })
}

/// Hash a plan: SHA-256 over the version prefix plus canonical bytes.
pub fn plan_hash(plan: &ExecutionPlan) -> WorkflowResult<String> {
    let canonical = canonical_plan_bytes(plan)?;
    let mut hasher = Sha256::new();
    hasher.update(PLAN_HASH_PREFIX);
    hasher.update(&canonical);
    Ok(hex::encode(hasher.finalize()))
}

/// Attach the derived hash to a plan whose `plan_hash` is still empty.
pub fn finalize_plan(mut plan: ExecutionPlan) -> WorkflowResult<ExecutionPlan> {
    let hash = plan_hash(&plan)?;
    plan.plan_hash = hash;
    Ok(plan)
}
