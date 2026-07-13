use std::borrow::Cow;
use std::sync::OnceLock;

use regex::Regex;

const REDACTED: &str = "<redacted>";
const TRUNCATED_PREFIX: &str = "(truncated to last ";

pub fn redact_text(input: &str) -> Cow<'_, str> {
    let mut current: Cow<'_, str> = Cow::Borrowed(input);
    for regex in redaction_patterns() {
        if regex.is_match(&current) {
            current = Cow::Owned(regex.replace_all(&current, REDACTED).into_owned());
        }
    }
    current
}

pub fn redact_and_cap(input: &str, max_bytes: usize) -> String {
    let redacted = redact_text(input);
    cap_text(redacted.as_ref(), max_bytes)
}

fn cap_text(input: &str, max_bytes: usize) -> String {
    if input.len() <= max_bytes {
        return input.to_owned();
    }

    let mut start = input.len() - max_bytes;
    while !input.is_char_boundary(start) {
        start += 1;
    }
    format!("{TRUNCATED_PREFIX}{max_bytes} bytes)\n{}", &input[start..])
}

fn redaction_patterns() -> &'static [Regex] {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"(?is)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----",
            r"(?i)\bauthorization\b\s*[:=]\s*bearer\s+[^\s,'\x22}\]]+",
            r"(?i)\b(?:authorization|bearer|token|secret|password|passwd|credential|api[_-]?key|access[_-]?key|private[_-]?key)\b\s*[:=]\s*['\x22]?[^\s,'\x22}\]]+",
            r"\bgithub_pat_[A-Za-z0-9_]{20,}\b",
            r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b",
            r"\bsk-[A-Za-z0-9_-]{20,}\b",
            r"\bxox[bpars]-[A-Za-z0-9-]{20,}\b",
            r"\bAKIA[0-9A-Z]{16}\b",
            r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b",
            r"(?i)(?:[:=]\s*)[A-F0-9]{32,}\b",
            r"(?:[:=]\s*)[A-Za-z0-9+/]{40,}={0,2}\b",
        ]
        .into_iter()
        .map(|pattern| match Regex::new(pattern) {
            Ok(regex) => regex,
            Err(error) => unreachable!("valid diagnostics redaction regex: {error}"),
        })
        .collect()
    })
}

#[cfg(test)]
mod tests;
