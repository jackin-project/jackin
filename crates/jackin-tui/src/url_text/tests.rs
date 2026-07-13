// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::has_url_scheme;
use super::is_host_open_url;
use super::redact_url_for_log;

#[test]
fn redact_url_for_log_preserves_plain_url() {
    assert_eq!(
        redact_url_for_log("https://example.com/path"),
        "https://example.com/path"
    );
}

#[test]
fn redact_url_for_log_removes_query_and_fragment_payloads() {
    assert_eq!(
        redact_url_for_log("https://example.com/path?token=secret#frag"),
        "https://example.com/path?<redacted>"
    );
    assert_eq!(
        redact_url_for_log("https://example.com/path#token=secret"),
        "https://example.com/path#<redacted>"
    );
}

#[test]
fn host_open_url_policy_allows_http_https_mailto_only() {
    assert!(is_host_open_url("http://example.com"));
    assert!(is_host_open_url("https://example.com"));
    assert!(is_host_open_url("mailto:operator@example.com"));
    assert!(is_host_open_url("MAILTO:operator@example.com"));
    assert!(!is_host_open_url("file:///tmp/report.html"));
    assert!(!is_host_open_url("javascript:alert(1)"));
    assert!(!is_host_open_url("data:text/plain,hello"));
}

#[test]
fn has_url_scheme_detects_scheme_bearing_tokens() {
    assert!(has_url_scheme("file:///tmp/report.html"));
    assert!(has_url_scheme("javascript:alert(1)"));
    assert!(has_url_scheme("web+foo:bar"));
    assert!(!has_url_scheme("plain"));
    assert!(!has_url_scheme("not a url: text"));
    assert!(!has_url_scheme("1bad:scheme"));
}
