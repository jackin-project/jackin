//! Best-effort token-shape scrubbing for diagnostic command output.

use std::borrow::Cow;

const VALUE_MARKER: &str = "<secret redacted>";
const KEY_MARKER: &str = "<key material redacted>";
const MIN_TOKEN_BODY: usize = 8;

#[must_use]
pub fn scrub_secrets(input: &str) -> Cow<'_, str> {
    let mut output = input.to_owned();
    let mut changed = false;

    changed |= redact_pem_keys(&mut output);
    changed |= redact_prefixed_tokens(&mut output, "op://", 1, VALUE_MARKER);
    changed |= redact_prefixed_tokens(&mut output, "ghp_", MIN_TOKEN_BODY, VALUE_MARKER);
    changed |= redact_prefixed_tokens(&mut output, "gho_", MIN_TOKEN_BODY, VALUE_MARKER);
    changed |= redact_prefixed_tokens(&mut output, "ghs_", MIN_TOKEN_BODY, VALUE_MARKER);
    changed |= redact_prefixed_tokens(&mut output, "sk-", MIN_TOKEN_BODY, VALUE_MARKER);
    changed |= redact_aws_access_keys(&mut output);
    changed |= redact_secret_assignments(&mut output);

    if changed {
        Cow::Owned(output)
    } else {
        Cow::Borrowed(input)
    }
}

fn redact_pem_keys(s: &mut String) -> bool {
    let mut changed = false;
    let mut search_from = 0;
    while let Some(relative_start) = s[search_from..].find("-----BEGIN ") {
        let start = search_from + relative_start;
        let header_end = s[start..]
            .find('\n')
            .map_or_else(|| s.len(), |newline| start + newline);
        let header = &s[start..header_end];
        if !header.contains("KEY") {
            search_from = header_end;
            continue;
        }
        let Some(end_relative) = s[header_end..].find("-----END ") else {
            break;
        };
        let end_start = header_end + end_relative;
        let Some(close_relative) = s[end_start..].find("-----") else {
            break;
        };
        let remove_end = end_start + close_relative + "-----".len();
        s.replace_range(start..remove_end, KEY_MARKER);
        search_from = start + KEY_MARKER.len();
        changed = true;
    }
    changed
}

fn redact_prefixed_tokens(s: &mut String, prefix: &str, min_body: usize, marker: &str) -> bool {
    let mut changed = false;
    let mut search_from = 0;
    while let Some(relative_start) = s[search_from..].find(prefix) {
        let start = search_from + relative_start;
        let end = token_end(s, start + prefix.len());
        let body_len = end.saturating_sub(start + prefix.len());
        if body_len < min_body {
            search_from = end.max(start + prefix.len());
            continue;
        }
        s.replace_range(start..end, marker);
        search_from = start + marker.len();
        changed = true;
    }
    changed
}

fn token_end(s: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in s[start..].char_indices() {
        if !is_token_char(ch) {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn is_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '/' | '+' | '=' | '.' | ':')
}

fn redact_aws_access_keys(s: &mut String) -> bool {
    let mut changed = false;
    let mut search_from = 0;
    while let Some(relative_start) = s[search_from..].find("AKIA") {
        let start = search_from + relative_start;
        let end = start + 20;
        if end <= s.len()
            && s.is_char_boundary(end)
            && s[start..end]
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
        {
            s.replace_range(start..end, VALUE_MARKER);
            search_from = start + VALUE_MARKER.len();
            changed = true;
        } else {
            search_from = start + "AKIA".len();
        }
    }
    changed
}

fn redact_secret_assignments(s: &mut String) -> bool {
    let mut changed = false;
    let mut cursor = 0;
    while let Some(eq_relative) = s[cursor..].find('=') {
        let eq = cursor + eq_relative;
        let key_start = assignment_key_start(s, eq);
        let key = &s[key_start..eq];
        if !is_secret_key(key) {
            cursor = eq + 1;
            continue;
        }
        let value_start = eq + 1;
        let value_end = assignment_value_end(s, value_start);
        if value_end.saturating_sub(value_start) < MIN_TOKEN_BODY {
            cursor = value_end.max(value_start);
            continue;
        }
        s.replace_range(value_start..value_end, VALUE_MARKER);
        cursor = value_start + VALUE_MARKER.len();
        changed = true;
    }
    changed
}

fn assignment_key_start(s: &str, eq: usize) -> usize {
    let prefix = &s[..eq];
    prefix
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| (!is_key_char(ch)).then_some(idx + ch.len_utf8()))
        .unwrap_or(0)
}

fn assignment_value_end(s: &str, start: usize) -> usize {
    let mut end = start;
    for (offset, ch) in s[start..].char_indices() {
        if ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | ';') {
            break;
        }
        end = start + offset + ch.len_utf8();
    }
    end
}

fn is_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    key.contains("TOKEN")
        || key.contains("SECRET")
        || key.contains("PASSWORD")
        || key.contains("PASSWD")
        || key.contains("API_KEY")
        || key.contains("AUTH")
        || key.contains("CREDENTIAL")
}

#[cfg(test)]
mod tests;
