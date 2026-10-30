// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::usage_monitor::{MonitorIssueCode, USAGE_MONITOR_MAX_STATUSLINE_BYTES};

use crate::parse_statusline;

#[test]
fn statusline_minimal_valid_observation_parses() {
    let observation =
        parse_statusline(br#"{"session_id":"sess-1"}"#).expect("minimal statusline must parse");

    assert_eq!(observation.session_id, "sess-1");
    assert_eq!(observation.model, None);
}

#[test]
fn statusline_missing_session_id_is_rejected() {
    let error = parse_statusline(br"{}").expect_err("statusline without session_id must fail");

    assert_eq!(error.code, MonitorIssueCode::StatuslineInvalid);
}

#[test]
fn statusline_malformed_json_is_rejected() {
    let error = parse_statusline(b"{not json").expect_err("malformed statusline must fail");

    assert_eq!(error.code, MonitorIssueCode::StatuslineInvalid);
}

#[test]
fn statusline_oversize_input_is_rejected_before_parsing() {
    let oversized = vec![b'a'; USAGE_MONITOR_MAX_STATUSLINE_BYTES + 1];

    let error =
        parse_statusline(&oversized).expect_err("oversize statusline must fail without parsing");

    assert_eq!(error.code, MonitorIssueCode::StatuslineTooLarge);
}
