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
    pub fn new(value: String) -> Secret {
        Secret(value.into_boxed_str())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl SecretBytes {
    pub fn new(value: Vec<u8>) -> SecretBytes {
        SecretBytes(value.into_boxed_slice())
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl Redactor {
    pub fn new<'a>(secrets: impl IntoIterator<Item = &'a Secret>) -> Redactor {
        let mut kept: Vec<Secret> = secrets
            .into_iter()
            .filter(|secret| !secret.expose().is_empty())
            .map(|secret| Secret::new(secret.expose().to_owned()))
            .collect();
        kept.sort_by_key(|secret| std::cmp::Reverse(secret.expose().len()));
        Redactor {
            secrets: kept.into(),
        }
    }

    pub fn apply(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(c) = rest.chars().next() {
            if let Some(secret) = self
                .secrets
                .iter()
                .find(|secret| rest.starts_with(secret.expose()))
            {
                out.push_str(MASK);
                rest = &rest[secret.expose().len()..];
            } else {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
enum Scan {
    Text,
    Escape,
    Csi,
    Str { bel_ends: bool },
    StrEscape,
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
