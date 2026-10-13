// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
        parse_statusline(br#"{"session_id":"session-1","rate_limits":{"five_hour":{}}}"#).unwrap();
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
    let input = br#"{"session_id":"session-1","version":"2.1.79","rate_limits":{"five_hour":{}}}"#;
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
