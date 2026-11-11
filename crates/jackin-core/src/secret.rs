// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::sync::Arc;

use zeroize::{Zeroize, ZeroizeOnDrop};

const MASK: &str = "<redacted>";

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Secret(Box<str>);

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SecretBytes(Box<[u8]>);

#[derive(Clone)]
pub struct Redactor {
    secrets: Arc<[Secret]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TerminalText(String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes(<redacted>)")
    }
}

impl fmt::Debug for Redactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Redactor(<redacted>)")
    }
}

impl Secret {
    pub fn new(mut value: String) -> Secret {
        let exact: Box<str> = Box::from(value.as_str());
        value.zeroize();
        Secret(exact)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl SecretBytes {
    pub fn new(mut value: Vec<u8>) -> SecretBytes {
        let exact: Box<[u8]> = Box::from(value.as_slice());
        value.zeroize();
        SecretBytes(exact)
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl Redactor {
    pub fn new<'a>(secrets: impl IntoIterator<Item = &'a Secret>) -> Redactor {
        let mut patterns: Vec<&str> = Vec::new();
        for secret in secrets {
            let whole = secret.expose();
            patterns.push(whole);
            patterns.extend(whole.lines());
        }
        patterns.retain(|pattern| !pattern.is_empty());
        patterns.sort_unstable();
        patterns.dedup();
        Redactor {
            secrets: patterns
                .into_iter()
                .map(|pattern| Secret::new(pattern.to_owned()))
                .collect(),
        }
    }

    pub fn apply(&self, text: &str) -> String {
        let mut spans: Vec<(usize, usize)> = Vec::new();
        for (start, _) in text.char_indices() {
            let rest = &text[start..];
            for secret in self.secrets.iter() {
                if rest.starts_with(secret.expose()) {
                    spans.push((start, start + secret.expose().len()));
                }
            }
        }
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (start, end) in spans {
            match merged.last_mut() {
                Some(last) if start < last.1 => last.1 = last.1.max(end),
                _ => merged.push((start, end)),
            }
        }
        let mut out = String::with_capacity(text.len());
        let mut cursor = 0;
        for (start, end) in merged {
            out.push_str(&text[cursor..start]);
            out.push_str(MASK);
            cursor = end;
        }
        out.push_str(&text[cursor..]);
        out
    }
}

fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

#[derive(Clone, Copy)]
enum Scan {
    Text,
    Escape,
    Csi,
    Str { bel_ends: bool },
    StrEscape,
}

impl fmt::Display for TerminalText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TerminalText {
    pub fn sanitize(bytes: &[u8]) -> TerminalText {
        let decoded = String::from_utf8_lossy(bytes);
        let mut out = String::with_capacity(decoded.len());
        let mut state = Scan::Text;
        for c in decoded.chars() {
            let mut pending = Some(c);
            while let Some(c) = pending.take() {
                state = match state {
                    Scan::Text => match c {
                        '\x1b' => Scan::Escape,
                        c if is_bidi_control(c) => Scan::Text,
                        '\n' | '\t' => {
                            out.push(c);
                            Scan::Text
                        }
                        c if c.is_control() => Scan::Text,
                        c => {
                            out.push(c);
                            Scan::Text
                        }
                    },
                    Scan::Escape => match c {
                        '[' => Scan::Csi,
                        ']' => Scan::Str { bel_ends: true },
                        'P' | 'X' | '^' | '_' => Scan::Str { bel_ends: false },
                        '\x20'..='\x2f' => Scan::Escape,
                        '\x30'..='\x7e' => Scan::Text,
                        c => {
                            pending = Some(c);
                            Scan::Text
                        }
                    },
                    Scan::Csi => match c {
                        '\x20'..='\x3f' => Scan::Csi,
                        '\x40'..='\x7e' => Scan::Text,
                        c => {
                            pending = Some(c);
                            Scan::Text
                        }
                    },
                    Scan::Str { bel_ends } => match c {
                        '\x07' if bel_ends => Scan::Text,
                        '\n' => {
                            out.push('\n');
                            Scan::Text
                        }
                        '\x1b' => Scan::StrEscape,
                        _ => Scan::Str { bel_ends },
                    },
                    Scan::StrEscape => match c {
                        '\\' => Scan::Text,
                        c => {
                            pending = Some(c);
                            Scan::Escape
                        }
                    },
                };
            }
        }
        TerminalText(out)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
