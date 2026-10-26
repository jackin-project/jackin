// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub const DEFAULT_SPOOL_CAPACITY_BYTES: usize = 256 * 1024;

pub const MAX_PUSH_BYTES: usize = 1024 * 1024;

#[must_use]
pub fn sanitize_terminal_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' {
            skip_escape(&mut chars);
            continue;
        }
        if character.is_control() && !matches!(character, '\n' | '\t') {
            continue;
        }
        out.push(character);
    }
    out
}

fn skip_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.peek() {
        Some('[') => {
            chars.next();
            skip_csi(chars);
        }
        Some(']') => {
            chars.next();
            skip_osc(chars);
        }
        Some(_) => {
            chars.next();
        }
        None => {}
    }
}

fn skip_csi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for next in chars.by_ref() {
        if ('@'..='~').contains(&next) {
            break;
        }
    }
}

fn skip_osc(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for next in chars.by_ref() {
        if next == '\u{7}' {
            break;
        }
        if next == '\u{1b}' {
            if chars.peek() == Some(&'\\') {
                chars.next();
            }
            break;
        }
    }
}

#[derive(Debug, Default)]
pub struct SessionSpool {
    capacity_bytes: usize,
    text: String,
    dropped_bytes: u64,
    sealed: bool,
}

impl SessionSpool {
    #[must_use]
    pub fn new(capacity_bytes: usize) -> Self {
        Self {
            capacity_bytes: capacity_bytes.max(1),
            text: String::new(),
            dropped_bytes: 0,
            sealed: false,
        }
    }

    pub fn push(&mut self, chunk: &str) {
        if self.sealed {
            return;
        }
        let mut clean = sanitize_terminal_text(chunk);
        if clean.len() > MAX_PUSH_BYTES {
            let cut = clean.len() - MAX_PUSH_BYTES;
            clean = clean.split_off(cut);
            self.dropped_bytes += cut as u64;
        }
        self.text.push_str(&clean);
        if self.text.len() > self.capacity_bytes {
            let cut = self.text.len() - self.capacity_bytes;
            let mut boundary = cut;
            while !self.text.is_char_boundary(boundary) {
                boundary += 1;
            }
            self.text.drain(..boundary);
            self.dropped_bytes += boundary as u64;
        }
    }

    pub fn seal(&mut self) {
        self.sealed = true;
    }

    #[must_use]
    pub const fn has_gap(&self) -> bool {
        self.dropped_bytes > 0
    }

    #[must_use]
    pub const fn dropped_bytes(&self) -> u64 {
        self.dropped_bytes
    }

    #[must_use]
    pub const fn retained_bytes(&self) -> usize {
        self.text.len()
    }

    #[must_use]
    pub const fn capacity_bytes(&self) -> usize {
        self.capacity_bytes
    }

    #[must_use]
    pub fn read(&self) -> SpoolRead<'_> {
        SpoolRead {
            text: &self.text,
            gap: self.has_gap(),
            dropped_bytes: self.dropped_bytes,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SpoolRead<'a> {
    pub text: &'a str,
    pub gap: bool,
    pub dropped_bytes: u64,
}
