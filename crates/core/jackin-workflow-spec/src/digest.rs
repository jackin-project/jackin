// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use sha2::{Digest as _, Sha256};

use crate::error::{Diagnostic, ErrorCode, WorkflowError, WorkflowResult};
use crate::model::ExecutionPlan;

pub const PLAN_HASH_PREFIX: &[u8] = b"jackin-workflow-plan/v1\n";

#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

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
    let value = sort_json_keys(value);
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

fn sort_json_keys(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> = map.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut sorted = serde_json::Map::with_capacity(entries.len());
            for (key, item) in entries {
                sorted.insert(key, sort_json_keys(item));
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_json_keys).collect())
        }
        scalar => scalar,
    }
}

pub fn bundle_manifest_digest<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut entries: Vec<(&str, &str)> = files.into_iter().collect();
    entries.sort_unstable();
    let mut input = Vec::new();
    for (logical_path, digest) in entries {
        input.extend_from_slice(logical_path.as_bytes());
        input.push(0);
        input.extend_from_slice(digest.as_bytes());
        input.push(0);
    }
    sha256_hex(&input)
}

pub fn plan_hash(plan: &ExecutionPlan) -> WorkflowResult<String> {
    let canonical = canonical_plan_bytes(plan)?;
    let mut hasher = Sha256::new();
    hasher.update(PLAN_HASH_PREFIX);
    hasher.update(&canonical);
    Ok(hex::encode(hasher.finalize()))
}

pub fn finalize_plan(mut plan: ExecutionPlan) -> WorkflowResult<ExecutionPlan> {
    let hash = plan_hash(&plan)?;
    plan.plan_hash = hash;
    Ok(plan)
}

#[cfg(test)]
mod tests;
