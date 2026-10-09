// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded parser for Claude Code's documented statusline fields.

use serde_json::{Map, Value};

use jackin_protocol::usage_monitor::{
    MonitorIssue, MonitorIssueCode, StatuslineObservation, StatuslineQuotaWindow,
    StatuslineRateLimits, USAGE_MONITOR_MAX_STATUSLINE_BYTES, USAGE_MONITOR_SCHEMA_VERSION,
};

/// Parse the supported Claude Code statusline JSON fields.
///
/// Unrelated fields, including all session cost fields, are deliberately
/// ignored. Missing quota windows remain absent so callers cannot turn missing
/// data into a zero-utilization observation.
pub fn parse_statusline(bytes: &[u8]) -> Result<StatuslineObservation, MonitorIssue> {
    if bytes.len() > USAGE_MONITOR_MAX_STATUSLINE_BYTES {
        return Err(issue(
            MonitorIssueCode::StatuslineTooLarge,
            "statusline input exceeds the 16 KiB limit",
        ));
    }

    let value: Value = serde_json::from_slice(bytes).map_err(|_| {
        issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline JSON is invalid",
        )
    })?;
    let root = value.as_object().ok_or_else(|| {
        issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline root must be a JSON object",
        )
    })?;
    let session_id = required_text(root, "session_id", 128)?;
    let model = parse_model(root.get("model"))?;
    let rate_limits = match root.get("rate_limits") {
        None | Some(Value::Null) => StatuslineRateLimits::default(),
        Some(value) => parse_rate_limits(value.as_object().ok_or_else(|| {
            issue(
                MonitorIssueCode::StatuslineInvalid,
                "rate_limits must be a JSON object when present",
            )
        })?)?,
    };

    Ok(StatuslineObservation {
        schema_version: USAGE_MONITOR_SCHEMA_VERSION,
        session_id,
        model,
        rate_limits,
    })
}

fn parse_rate_limits(value: &Map<String, Value>) -> Result<StatuslineRateLimits, MonitorIssue> {
    Ok(StatuslineRateLimits {
        five_hour: optional_window(value, "five_hour")?,
        seven_day: optional_window(value, "seven_day")?,
    })
}

fn optional_window(
    value: &Map<String, Value>,
    name: &str,
) -> Result<Option<StatuslineQuotaWindow>, MonitorIssue> {
    let Some(value) = value.get(name) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let object = value.as_object().ok_or_else(|| {
        issue(
            MonitorIssueCode::StatuslineInvalid,
            "rate limit window must be a JSON object when present",
        )
    })?;

    let used_percentage_basis_points = match object.get("used_percentage") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_used_percentage(value)?),
    };
    let reset_at_epoch = match object.get("resets_at") {
        None | Some(Value::Null) => None,
        Some(value) => Some(parse_reset(value)?),
    };

    Ok(Some(StatuslineQuotaWindow {
        used_percentage_basis_points,
        reset_at_epoch,
    }))
}

fn parse_used_percentage(value: &Value) -> Result<i32, MonitorIssue> {
    let percentage = value.as_f64().ok_or_else(|| {
        issue(
            MonitorIssueCode::StatuslineInvalid,
            "used_percentage must be a finite number from 0 through 100",
        )
    })?;
    if !percentage.is_finite() || !(0.0..=100.0).contains(&percentage) {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "used_percentage must be a finite number from 0 through 100",
        ));
    }
    // Round toward greater usage. A boundary value must never be understated
    // by binary floating-point conversion.
    let basis_points = (percentage * 100.0).ceil();
    if !(0.0..=10_000.0).contains(&basis_points) {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "used_percentage cannot be represented in basis points",
        ));
    }
    Ok(basis_points as i32)
}

fn parse_reset(value: &Value) -> Result<i64, MonitorIssue> {
    let Some(seconds) = value.as_i64() else {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "resets_at must be a nonnegative integer Unix timestamp",
        ));
    };
    if seconds < 0 {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "resets_at must be a nonnegative integer Unix timestamp",
        ));
    }
    Ok(seconds)
}

fn required_text(
    object: &Map<String, Value>,
    key: &str,
    max_len: usize,
) -> Result<String, MonitorIssue> {
    optional_text(object, key, max_len)?.ok_or_else(|| {
        issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline session_id is required",
        )
    })
}

fn parse_model(value: Option<&Value>) -> Result<Option<String>, MonitorIssue> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(model) = value.as_object() else {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "model must be a JSON object when present",
        ));
    };
    optional_text(model, "id", 128)
}

fn optional_text(
    object: &Map<String, Value>,
    key: &str,
    max_len: usize,
) -> Result<Option<String>, MonitorIssue> {
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let text = value.as_str().ok_or_else(|| {
        issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline text field has an invalid type",
        )
    })?;
    let text = text.trim();
    if text.is_empty()
        || text.len() > max_len
        || text.chars().any(char::is_control)
        || !text.is_ascii()
    {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline text field is empty or outside its accepted bounds",
        ));
    }
    Ok(Some(text.to_owned()))
}

fn issue(code: MonitorIssueCode, message: &str) -> MonitorIssue {
    MonitorIssue {
        code,
        message: message.to_owned(),
        retry_at_epoch: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jackin_protocol::usage_monitor::MonitorIssueCode;

    #[test]
    fn parses_documented_quota_fields_and_ignores_cost() {
        let input = br#"{
            "session_id":"session-1",
            "model":{"id":"claude-sonnet-4-5","display_name":"Claude Sonnet 4.5"},
            "rate_limits":{
                "five_hour":{"used_percentage":89.75,"resets_at":2000000000},
                "seven_day":{"used_percentage":100,"resets_at":2000003600}
            },
            "cost":{"total_cost_usd":999999},
            "extra_usage":{"used_credits":100}
        }"#;
        let parsed = parse_statusline(input).unwrap();
        assert_eq!(parsed.session_id, "session-1");
        assert_eq!(parsed.model.as_deref(), Some("claude-sonnet-4-5"));
        let five_hour = parsed.rate_limits.five_hour.unwrap();
        assert_eq!(five_hour.used_percentage_basis_points, Some(8975));
        assert_eq!(five_hour.reset_at_epoch, Some(2_000_000_000));
        let seven_day = parsed.rate_limits.seven_day.unwrap();
        assert_eq!(seven_day.used_percentage_basis_points, Some(10_000));
        assert_eq!(seven_day.reset_at_epoch, Some(2_000_003_600));
    }

    #[test]
    fn absent_rate_limits_and_window_fields_stay_unknown() {
        let parsed = parse_statusline(br#"{"session_id":"session-1"}"#).unwrap();
        assert!(parsed.rate_limits.five_hour.is_none());
        assert!(parsed.rate_limits.seven_day.is_none());

        let parsed =
            parse_statusline(br#"{"session_id":"session-1","rate_limits":{"five_hour":{}}}"#)
                .unwrap();
        let five_hour = parsed.rate_limits.five_hour.unwrap();
        assert_eq!(five_hour.used_percentage_basis_points, None);
        assert_eq!(five_hour.reset_at_epoch, None);
    }

    #[test]
    fn rejects_oversized_malformed_or_out_of_range_input() {
        let oversized = vec![b' '; USAGE_MONITOR_MAX_STATUSLINE_BYTES + 1];
        assert_eq!(
            parse_statusline(&oversized).unwrap_err().code,
            MonitorIssueCode::StatuslineTooLarge
        );
        assert_eq!(
            parse_statusline(b"{").unwrap_err().code,
            MonitorIssueCode::StatuslineInvalid
        );
        let invalid =
            br#"{"session_id":"session-1","rate_limits":{"five_hour":{"used_percentage":100.01}}}"#;
        assert_eq!(
            parse_statusline(invalid).unwrap_err().code,
            MonitorIssueCode::StatuslineInvalid
        );
        let float_reset =
            br#"{"session_id":"session-1","rate_limits":{"five_hour":{"resets_at":2000000000.0}}}"#;
        assert_eq!(
            parse_statusline(float_reset).unwrap_err().code,
            MonitorIssueCode::StatuslineInvalid
        );
    }
}
