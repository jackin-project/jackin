// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::usage_monitor::{MonitorIssue, MonitorIssueCode};
use serde::Serialize;

const MONITOR_ID_MAX_BYTES: usize = 128;
const MONITOR_MODEL_MAX_BYTES: usize = 128;
const MONITOR_GOAL_MAX_BYTES: usize = 256;
const MONITOR_IDEMPOTENCY_KEY_MAX_BYTES: usize = 512;
const MONITOR_OPERATOR_LABEL_MAX_BYTES: usize = 128;
const MIGRATED_IDEMPOTENCY_PREFIX: &str = "v1-migrated-";

/// A stable machine-readable CLI failure. `main.rs` prints `json` to stdout
/// and uses `exit_code` instead of rendering this error to stderr.
#[derive(Debug, thiserror::Error)]
#[error("{json}")]
pub struct UsageCommandExit {
    exit_code: i32,
    json: String,
}

impl UsageCommandExit {
    #[must_use]
    pub fn new(exit_code: i32, json: String) -> Self {
        Self { exit_code, json }
    }

    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        self.exit_code
    }

    #[must_use]
    pub fn json(&self) -> &str {
        &self.json
    }
}

#[derive(Serialize)]
struct UsageErrorEnvelope<'a> {
    version: u16,
    error: UsageErrorBody<'a>,
}

#[derive(Serialize)]
struct UsageErrorBody<'a> {
    code: &'a str,
    message: &'a str,
}

pub(super) fn usage_error(code: &str, message: &str, exit_code: i32) -> anyhow::Error {
    let json = serde_json::to_string(&UsageErrorEnvelope {
        version: 1,
        error: UsageErrorBody { code, message },
    })
    .unwrap_or_else(|_| {
        "{\"version\":1,\"error\":{\"code\":\"internal\",\"message\":\"unable to encode error\"}}"
            .to_owned()
    });
    UsageCommandExit::new(exit_code, json).into()
}

/// Keep CLI preflight aligned with the broker's durable monitor validators.
/// The broker remains authoritative; these checks prevent malformed commands
/// from starting or attaching to a broker before it can reject them.
pub(super) fn validate_monitor_identifier(field: &str, value: &str) -> anyhow::Result<()> {
    if !value.is_empty()
        && value.len() <= MONITOR_ID_MAX_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(usage_error(
            "invalid_argument",
            &format!("{field} is empty or outside its accepted bounds"),
            3,
        ))
    }
}

pub(super) fn validate_monitor_goal_id(field: &str, value: &str) -> anyhow::Result<()> {
    if !value.trim().is_empty()
        && value.len() <= MONITOR_GOAL_MAX_BYTES
        && !value.chars().any(char::is_control)
    {
        Ok(())
    } else {
        Err(usage_error(
            "invalid_argument",
            &format!("{field} is empty or outside its accepted bounds"),
            3,
        ))
    }
}

pub(super) fn validate_monitor_bounded_text(
    field: &str,
    value: &str,
    max_bytes: usize,
) -> anyhow::Result<()> {
    if !value.trim().is_empty()
        && value.len() <= max_bytes
        && value.is_ascii()
        && !value.chars().any(char::is_control)
    {
        Ok(())
    } else {
        Err(usage_error(
            "invalid_argument",
            &format!("{field} is empty or outside its accepted bounds"),
            3,
        ))
    }
}

pub(super) fn validate_monitor_start_fields(
    idempotency_key: &str,
    expected_model: Option<&str>,
) -> anyhow::Result<()> {
    validate_monitor_bounded_text(
        "idempotency key",
        idempotency_key,
        MONITOR_IDEMPOTENCY_KEY_MAX_BYTES,
    )?;
    if idempotency_key.starts_with(MIGRATED_IDEMPOTENCY_PREFIX) {
        return Err(usage_error(
            "invalid_argument",
            "idempotency key uses a reserved migration prefix",
            3,
        ));
    }
    if let Some(model) = expected_model {
        validate_monitor_bounded_text("expected model", model, MONITOR_MODEL_MAX_BYTES)?;
    }
    Ok(())
}

pub(super) fn validate_monitor_revision(field: &str, revision: u64) -> anyhow::Result<()> {
    if revision > 0 {
        Ok(())
    } else {
        Err(usage_error(
            "invalid_argument",
            &format!("{field} must be greater than zero"),
            3,
        ))
    }
}

pub(super) fn validate_monitor_operator_label(value: &str) -> anyhow::Result<()> {
    validate_monitor_bounded_text("operator label", value, MONITOR_OPERATOR_LABEL_MAX_BYTES)
}

pub(super) fn issue_error(issue: MonitorIssue, _fallback_exit_code: i32) -> anyhow::Error {
    let code = serde_json::to_value(issue.code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "monitor_error".to_owned());
    usage_error(&code, &issue.message, monitor_issue_exit_code(issue.code))
}

pub(super) fn monitor_issue_exit_code(code: MonitorIssueCode) -> i32 {
    match code {
        MonitorIssueCode::BudgetUnverifiable
        | MonitorIssueCode::SpendUnavailable
        | MonitorIssueCode::SpendStale
        | MonitorIssueCode::SpendUnverified
        | MonitorIssueCode::PolicyRequired
        | MonitorIssueCode::BindingRequired
        | MonitorIssueCode::SgdCapAcknowledgementRequired
        | MonitorIssueCode::InteractionRequired => 2,
        _ => 3,
    }
}

pub(super) fn json_value_exit<T: Serialize>(value: &T, exit_code: i32) -> anyhow::Error {
    let json = serde_json::to_string(value).unwrap_or_else(|_| {
        "{\"version\":1,\"error\":{\"code\":\"internal\",\"message\":\"unable to encode result\"}}"
            .to_owned()
    });
    UsageCommandExit::new(exit_code, json).into()
}
