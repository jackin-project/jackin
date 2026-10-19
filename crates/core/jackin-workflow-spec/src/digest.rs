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
/// removed and every map key-sorted recursively. Key order is pinned
/// explicitly by [`sort_json_keys`] instead of relying on `serde_json`
/// map ordering, so the bytes (and the plan hash) cannot shift with
/// ambient feature unification. Deterministic across runs for the same
/// plan value.
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

/// Recursively sort every JSON object key. Inserting entries in sorted
/// order pins the serialized byte order no matter how `serde_json`
/// maps order keys under the active feature set.
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

/// Manifest digest over test-bundle entries: `logical_path\0`
/// `file_digest\0` per entry with entries sorted by logical path, so
/// the digest is order-insensitive. Shared by bundle resolution
/// ([`crate::resolve`]) and test verification: both sides must agree
/// on these exact bytes, and the plan pins both the per-file entries
/// and this digest together.
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

#[cfg(test)]
mod tests;
