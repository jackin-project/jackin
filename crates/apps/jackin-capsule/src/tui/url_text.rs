// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[must_use]
pub fn is_host_open_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

#[must_use]
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

#[must_use]
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
