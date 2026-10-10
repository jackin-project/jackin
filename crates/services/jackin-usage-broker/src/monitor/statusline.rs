// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded parser for Claude Code's documented statusline fields.

use serde_json::{Map, Value};

use jackin_protocol::usage_monitor::{
    MonitorIssue, MonitorIssueCode, StatuslineObservation, StatuslineQuotaWindow,
    StatuslineRateLimits, USAGE_MONITOR_MAX_STATUSLINE_BYTES, USAGE_MONITOR_SCHEMA_VERSION,
};

const CLAUDE_CODE_RATE_LIMITS_MIN_VERSION: (u64, u64, u64) = (2, 1, 80);
const CLAUDE_CODE_VERSION_MAX_LENGTH: usize = 32;

#[derive(Debug)]
struct ParsedClaudeCodeVersion {
    raw: String,
    components: (u64, u64, u64),
}

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
    let claude_version = parse_claude_code_version(root.get("version"))?;
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
    if (rate_limits.five_hour.is_some() || rate_limits.seven_day.is_some())
        && claude_version
            .as_ref()
            .is_some_and(|version| version.components < CLAUDE_CODE_RATE_LIMITS_MIN_VERSION)
    {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "Claude Code versions before 2.1.80 do not support rate_limits",
        ));
    }

    Ok(StatuslineObservation {
        schema_version: USAGE_MONITOR_SCHEMA_VERSION,
        claude_code_version: claude_version.map(|version| version.raw),
        session_id,
        model,
        rate_limits,
    })
}

fn parse_claude_code_version(
    value: Option<&Value>,
) -> Result<Option<ParsedClaudeCodeVersion>, MonitorIssue> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(raw_version) = value.as_str() else {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline version must be a three-part numeric version string",
        ));
    };
    if raw_version.is_empty()
        || raw_version.len() > CLAUDE_CODE_VERSION_MAX_LENGTH
        || !raw_version.is_ascii()
        || raw_version.chars().any(char::is_control)
    {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline version is empty or outside its accepted bounds",
        ));
    }
    let mut components = raw_version.split('.');
    let parsed = (
        parse_version_component(components.next()),
        parse_version_component(components.next()),
        parse_version_component(components.next()),
    );
    let (Some(major), Some(minor), Some(patch), None) =
        (parsed.0, parsed.1, parsed.2, components.next())
    else {
        return Err(issue(
            MonitorIssueCode::StatuslineInvalid,
            "statusline version must be a three-part numeric version string",
        ));
    };
    Ok(Some(ParsedClaudeCodeVersion {
        raw: raw_version.to_owned(),
        components: (major, minor, patch),
    }))
}

fn parse_version_component(value: Option<&str>) -> Option<u64> {
    let value = value?;
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return None;
    }
    value.parse().ok()
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
            "version":"2.1.80",
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
        assert_eq!(parsed.claude_code_version.as_deref(), Some("2.1.80"));
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
        assert_eq!(parsed.claude_code_version, None);

        let parsed =
            parse_statusline(br#"{"session_id":"session-1","rate_limits":{"five_hour":{}}}"#)
                .unwrap();
        let five_hour = parsed.rate_limits.five_hour.unwrap();
        assert_eq!(five_hour.used_percentage_basis_points, None);
        assert_eq!(five_hour.reset_at_epoch, None);
    }

    #[test]
    fn missing_client_version_stays_unknown_when_quota_is_present() {
        let parsed = parse_statusline(
            br#"{"session_id":"session-1","rate_limits":{"five_hour":{"used_percentage":23.5}}}"#,
        )
        .unwrap();

        assert_eq!(parsed.claude_code_version, None);
        assert_eq!(
            parsed
                .rate_limits
                .five_hour
                .unwrap()
                .used_percentage_basis_points,
            Some(2350)
        );
    }

    #[test]
    fn rejects_rate_limit_windows_claimed_by_pre_support_client_version() {
        let input =
            br#"{"session_id":"session-1","version":"2.1.79","rate_limits":{"five_hour":{}}}"#;
        assert_eq!(
            parse_statusline(input).unwrap_err().code,
            MonitorIssueCode::StatuslineInvalid
        );

        // A pre-support version is still valid metadata when it claims no
        // quota window.
        let input = br#"{"session_id":"session-1","version":"2.1.79"}"#;
        assert_eq!(
            parse_statusline(input)
                .unwrap()
                .claude_code_version
                .as_deref(),
            Some("2.1.79")
        );
    }

    #[test]
    fn rejects_malformed_claimed_client_versions() {
        for version in ["", "2.1", "v2.1.80", "2.1.80.1", "2.1.80-beta", "02.1.80"] {
            let input = format!(r#"{{"session_id":"session-1","version":"{version}"}}"#);
            assert_eq!(
                parse_statusline(input.as_bytes()).unwrap_err().code,
                MonitorIssueCode::StatuslineInvalid,
                "accepted malformed version {version:?}"
            );
        }

        assert_eq!(
            parse_statusline(br#"{"session_id":"session-1","version":2.1}"#)
                .unwrap_err()
                .code,
            MonitorIssueCode::StatuslineInvalid
        );
    }

    #[test]
    fn replaying_unchanged_quota_payload_parses_to_the_same_observation() {
        let input = br#"{"session_id":"session-1","version":"2.1.80","rate_limits":{"five_hour":{"used_percentage":23.5,"resets_at":2000000000}}}"#;
        let first = parse_statusline(input).unwrap();
        let replay = parse_statusline(input).unwrap();

        // The parser carries only source fields; receipt time and freshness
        // remain the broker's responsibility.
        assert_eq!(first, replay);
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
