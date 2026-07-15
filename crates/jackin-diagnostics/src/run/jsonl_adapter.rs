// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Versioned JSONL adapter: read v1 (legacy keys) and v2 (canonical keys)
//! into one shape for summaries and offline tooling.

use serde_json::Value;

/// Current writer schema version (canonical keys only).
pub const SCHEMA_V2: u32 = 2;

/// Prohibited top-level keys on v2 / OTLP export surfaces.
pub const PROHIBITED_TOP_LEVEL_KEYS: &[&str] = &[
    "kind",
    "stage",
    "detail",
    "error_type",
    "log.category",
    "run_id",
];

/// Canonicalized view of one diagnostics JSONL line.
#[derive(Clone, Debug, Default)]
pub struct CanonicalEvent {
    pub schema: u32,
    pub ts_ms: Option<u64>,
    pub run_id: Option<String>,
    /// Legacy kind token (e.g. `stage_started`) for summary `event_counts`.
    pub kind: String,
    pub event_name: String,
    pub event_outcome: Option<String>,
    pub jackin_component: Option<String>,
    pub jackin_operation: Option<String>,
    pub jackin_category: Option<String>,
    pub message: Option<String>,
    pub stage: Option<String>,
    pub detail: Option<String>,
    pub error_type: Option<String>,
}

/// Parse one JSONL line into a [`CanonicalEvent`].
///
/// Absent `schema` ⇒ version 1. Version 2 uses only canonical keys.
pub fn canonicalize_line(line: &str) -> Result<CanonicalEvent, serde_json::Error> {
    let value: Value = serde_json::from_str(line)?;
    Ok(canonicalize_value(&value))
}

/// Map a parsed JSON object to the canonical event shape.
#[must_use]
pub fn canonicalize_value(value: &Value) -> CanonicalEvent {
    let schema = value
        .get("schema")
        .and_then(Value::as_u64)
        .map_or(1, |v| v as u32);

    if schema >= SCHEMA_V2 {
        canonicalize_v2(value)
    } else {
        canonicalize_v1(value)
    }
}

fn canonicalize_v1(value: &Value) -> CanonicalEvent {
    let kind = str_field(value, "kind").unwrap_or_else(|| "unknown".to_owned());
    let event_name = str_field(value, "event.name").unwrap_or_else(|| {
        crate::registry::lookup(&kind).map_or_else(|| kind.clone(), |d| d.name.to_owned())
    });
    let mut outcome = str_field(value, "event.outcome");
    if outcome.as_deref() == Some("expected_shutdown") {
        outcome = Some("expected_close".to_owned());
    }
    CanonicalEvent {
        schema: 1,
        ts_ms: value.get("ts_ms").and_then(Value::as_u64),
        run_id: str_field(value, "run_id").or_else(|| str_field(value, "parallax.run.id")),
        kind,
        event_name,
        event_outcome: outcome,
        jackin_component: str_field(value, "jackin.component"),
        jackin_operation: str_field(value, "jackin.operation"),
        jackin_category: str_field(value, "jackin.category"),
        message: str_field(value, "message"),
        stage: str_field(value, "stage").or_else(|| str_field(value, "jackin.stage")),
        detail: str_field(value, "detail").or_else(|| str_field(value, "jackin.detail")),
        error_type: str_field(value, "error_type").or_else(|| str_field(value, "error.type")),
    }
}

fn canonicalize_v2(value: &Value) -> CanonicalEvent {
    let event_name = str_field(value, "event.name").unwrap_or_else(|| "unknown".to_owned());
    let kind = crate::registry::lookup(&event_name)
        .map_or_else(|| event_name.clone(), |d| d.kind.to_owned());
    CanonicalEvent {
        schema: SCHEMA_V2,
        ts_ms: value.get("ts_ms").and_then(Value::as_u64),
        run_id: str_field(value, "parallax.run.id"),
        kind,
        event_name,
        event_outcome: str_field(value, "event.outcome"),
        jackin_component: str_field(value, "jackin.component"),
        jackin_operation: str_field(value, "jackin.operation"),
        jackin_category: str_field(value, "jackin.category"),
        message: str_field(value, "message"),
        stage: str_field(value, "jackin.stage"),
        detail: str_field(value, "jackin.detail"),
        error_type: str_field(value, "error.type"),
    }
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

/// True when a JSON object has none of the prohibited top-level keys.
#[must_use]
pub fn has_no_prohibited_keys(value: &Value) -> bool {
    let Some(obj) = value.as_object() else {
        return false;
    };
    !PROHIBITED_TOP_LEVEL_KEYS
        .iter()
        .any(|key| obj.contains_key(*key))
}

#[cfg(test)]
mod tests;
