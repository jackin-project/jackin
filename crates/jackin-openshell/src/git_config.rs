// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ProcessStatus, TerminalText};

use crate::error::{GatewayError, TransferOp};

const URL_SEPARATOR: &str = "://";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitConfigEntry {
    pub key: String,
    pub value: Option<String>,
}

struct Parts<'a> {
    section: &'a str,
    subsection: Option<&'a str>,
    name: &'a str,
}

impl GitConfigEntry {
    fn parts(&self) -> Option<Parts<'_>> {
        let (section, rest) = self.key.split_once('.')?;
        Some(match rest.rsplit_once('.') {
            Some((subsection, name)) => Parts {
                section,
                subsection: Some(subsection),
                name,
            },
            None => Parts {
                section,
                subsection: None,
                name: rest,
            },
        })
    }

    fn is_dropped(&self) -> bool {
        let Some(parts) = self.parts() else {
            return true;
        };
        let secret_section = matches!(parts.section, "credential" | "include" | "includeif");
        let header = parts.section == "http" && parts.name == "extraheader";
        let userinfo_key = parts.subsection.is_some_and(has_userinfo);
        secret_section || header || userinfo_key
    }
}

fn is_scheme_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
}

fn authority_end(rest: &str) -> usize {
    rest.find(|c: char| matches!(c, '/' | '?' | '#' | '"' | '\'') || c.is_whitespace())
        .unwrap_or(rest.len())
}

fn ends_with_scheme(head: &str) -> bool {
    let run = head
        .chars()
        .rev()
        .take_while(|c| is_scheme_char(*c))
        .count();
    head[head.len() - run..]
        .chars()
        .any(|c| c.is_ascii_alphabetic())
}

fn has_userinfo(text: &str) -> bool {
    strip_userinfo(text) != text
}

fn strip_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find(URL_SEPARATOR) {
        let after = index + URL_SEPARATOR.len();
        out.push_str(&rest[..after]);
        let tail = &rest[after..];
        let end = authority_end(tail);
        let authority = &tail[..end];
        match authority.rfind('@') {
            Some(at) if ends_with_scheme(&rest[..index]) => out.push_str(&authority[at + 1..]),
            _ => out.push_str(authority),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

pub fn sanitize_entries(entries: Vec<GitConfigEntry>) -> Vec<GitConfigEntry> {
    entries
        .into_iter()
        .filter(|entry| !entry.is_dropped())
        .map(|entry| GitConfigEntry {
            value: entry.value.map(|value| strip_userinfo(&value)),
            ..entry
        })
        .collect()
}

fn listing_failure(note: &str) -> GatewayError {
    GatewayError::Transfer {
        op: TransferOp::GitConfig,
        status: ProcessStatus::Exited(0),
        tail: TerminalText::sanitize(note.as_bytes()),
    }
}

pub(crate) fn parse_listing(bytes: &[u8]) -> Result<Vec<GitConfigEntry>, GatewayError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| listing_failure("the git config listing is not valid UTF-8"))?;
    let Some(body) = text.strip_suffix('\0') else {
        return if text.is_empty() {
            Ok(Vec::new())
        } else {
            Err(listing_failure("the git config listing is not terminated"))
        };
    };
    Ok(body
        .split('\0')
        .map(|item| match item.split_once('\n') {
            Some((key, value)) => GitConfigEntry {
                key: key.to_owned(),
                value: Some(value.to_owned()),
            },
            None => GitConfigEntry {
                key: item.to_owned(),
                value: None,
            },
        })
        .collect())
}

fn quote(text: &str) -> Result<String, GatewayError> {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\r' => {
                return Err(listing_failure(
                    "a git config value contains a carriage return",
                ));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    Ok(out)
}

pub(crate) fn render(entries: &[GitConfigEntry]) -> Result<String, GatewayError> {
    let mut out = String::new();
    let mut current: Option<(&str, Option<&str>)> = None;
    for entry in entries {
        let parts = entry
            .parts()
            .ok_or_else(|| listing_failure("a git config key has no section"))?;
        let header = (parts.section, parts.subsection);
        if current != Some(header) {
            out.push('[');
            out.push_str(parts.section);
            if let Some(subsection) = parts.subsection {
                out.push(' ');
                out.push_str(&quote(subsection)?);
            }
            out.push_str("]\n");
            current = Some(header);
        }
        out.push('\t');
        out.push_str(parts.name);
        if let Some(value) = &entry.value {
            out.push_str(" = ");
            out.push_str(&quote(value)?);
        }
        out.push('\n');
    }
    Ok(out)
}
