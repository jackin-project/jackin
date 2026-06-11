//! URL text helpers that are small enough to avoid a parser dependency at
//! shared-core level.

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
}
