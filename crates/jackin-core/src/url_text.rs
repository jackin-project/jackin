//! URL text helpers that are small enough to avoid a parser dependency at
//! shared-core level. Verbatim copy of `jackin_tui::url_text` per
//! A5 unblock — runtime no longer needs the L3 jackin-tui dep
//! for these.

/// Return true when a URL is safe for host-side open requests.
pub fn is_host_open_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// Return true when a token looks like it carries a URL scheme.
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

/// Redact query or fragment text before writing a URL to logs.
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