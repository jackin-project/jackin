//! URL text helpers that are small enough to avoid a parser dependency at
//! shared-core level.

/// Return true when a URL is safe for host-side open requests.
///
/// This is intentionally a scheme allowlist, not a general URL validator:
/// the host opener receives the URL as a subprocess argument, and the
/// jackin-owned trust boundary is the scheme. `mailto:` is included so OSC 8
/// links that point at an email action keep working while `file:`,
/// `javascript:`, and `data:` stay blocked.
pub fn is_host_open_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// Return true when a token looks like it carries a URL scheme.
///
/// This is deliberately small: it distinguishes ordinary words from explicit
/// scheme-bearing tokens so callers can reject `file:`/`javascript:` without
/// treating every non-URL word under the cursor as a rejected host-open URL.
pub fn has_url_scheme(token: &str) -> bool {
    let Some(colon) = token.find(':') else {
        return false;
    };
    let Some(first) = token.as_bytes().first() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    token[..colon]
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

/// Redact query or fragment text before writing a URL to logs. The host-open
/// path only needs enough detail to identify the destination route; query
/// strings often carry auth tokens, search terms, or CI state.
pub fn redact_url_for_log(url: &str) -> String {
    let query = url.find('?');
    let fragment = url.find('#');
    match (query, fragment) {
        (Some(query), Some(fragment)) if query < fragment => {
            format!("{}?<redacted>", &url[..query])
        }
        (Some(query), _) => format!("{}?<redacted>", &url[..query]),
        (None, Some(fragment)) => format!("{}#<redacted>", &url[..fragment]),
        (None, None) => url.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
