// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub fn is_host_open_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

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
    token.get(..colon).is_some_and(|scheme| {
        scheme
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
    })
}

pub fn redact_url_for_log(url: &str) -> String {
    let query = url.find('?');
    let fragment = url.find('#');
    match (query, fragment) {
        (Some(query), Some(fragment)) if query < fragment => url
            .get(..query)
            .map_or_else(|| url.to_owned(), |prefix| format!("{prefix}?<redacted>")),
        (Some(query), _) => url
            .get(..query)
            .map_or_else(|| url.to_owned(), |prefix| format!("{prefix}?<redacted>")),
        (None, Some(fragment)) => url
            .get(..fragment)
            .map_or_else(|| url.to_owned(), |prefix| format!("{prefix}#<redacted>")),
        (None, None) => url.to_owned(),
    }
}
