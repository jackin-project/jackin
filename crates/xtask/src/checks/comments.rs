// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::Check;
use crate::ci::CiStep;
use crate::lex::{Cursor, lex_rust};
use crate::repo::{FileKind, Repo};
use crate::violation::{Violation, sorted};

#[derive(Clone, Copy, Debug, Default)]
pub struct CommentsCheck;

impl Check for CommentsCheck {
    fn step(&self) -> CiStep {
        CiStep::Comments
    }

    fn run(&self, repo: &Repo) -> Vec<Violation> {
        let mut violations = Vec::new();
        for file in repo.files() {
            if file.is_third_party_source() {
                continue;
            }
            let found = match file.kind() {
                FileKind::Rust => rust_comments(file.text()),
                FileKind::Toml => toml_comments(file.text()),
                FileKind::Shell => shell_comments(file.text()),
                FileKind::Dockerfile => dockerfile_comments(file.text()),
            };
            let mut next_directive = 1;
            for comment in found {
                if !is_allowed(file.kind(), &comment, &mut next_directive) {
                    violations.push(Violation::at_line(
                        self.step(),
                        file.path(),
                        comment.line,
                        "comments are not allowed (D17); only SPDX headers, a shebang, and parser directives are",
                    ));
                }
            }
        }
        sorted(violations)
    }
}

const SPDX_TAGS: [&str; 2] = ["SPDX-FileCopyrightText:", "SPDX-License-Identifier:"];

struct Found {
    line: u32,
    text: String,
    after_content: bool,
}

fn is_allowed(kind: FileKind, comment: &Found, next_directive: &mut u32) -> bool {
    let header = !comment.after_content && is_spdx_header(kind, &comment.text);
    match kind {
        FileKind::Shell => header || (comment.line == 1 && comment.text.starts_with("#!")),
        FileKind::Dockerfile => {
            if comment.line == *next_directive && is_directive(&comment.text) {
                *next_directive += 1;
                true
            } else {
                header
            }
        }
        FileKind::Rust | FileKind::Toml => header,
    }
}

fn is_spdx_header(kind: FileKind, text: &str) -> bool {
    let marker = if kind == FileKind::Rust { "//" } else { "#" };
    let Some(body) = text.strip_prefix(marker) else {
        return false;
    };
    SPDX_TAGS.iter().any(|tag| {
        body.trim_start()
            .strip_prefix(tag)
            .is_some_and(|value| !value.trim().is_empty())
    })
}

fn is_directive(text: &str) -> bool {
    let body = text.trim_start_matches('#').trim_start();
    ["syntax", "escape", "check"].iter().any(|name| {
        body.strip_prefix(name)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })
}

fn rust_comments(text: &str) -> Vec<Found> {
    lex_rust(text)
        .comments
        .into_iter()
        .map(|comment| Found {
            line: comment.line,
            text: comment.text,
            after_content: comment.tokens_before > 0,
        })
        .collect()
}

fn hash_comment(cursor: &mut Cursor, after_content: bool) -> Found {
    let line = cursor.line();
    Found {
        line,
        text: cursor.rest_of_line(),
        after_content,
    }
}

fn toml_comments(text: &str) -> Vec<Found> {
    let mut cursor = Cursor::new(text);
    let mut found = Vec::new();
    let mut content = false;
    while let Some(c) = cursor.peek(0) {
        match c {
            '#' => found.push(hash_comment(&mut cursor, content)),
            '"' | '\'' => {
                content = true;
                toml_string(&mut cursor, c);
            }
            c if c.is_whitespace() => {
                cursor.bump();
            }
            _ => {
                content = true;
                cursor.bump();
            }
        }
    }
    found
}

fn toml_string(cursor: &mut Cursor, quote: char) {
    if cursor.run_of(quote) >= 3 {
        toml_multiline(cursor, quote);
    } else {
        toml_single_line(cursor, quote);
    }
}

fn toml_multiline(cursor: &mut Cursor, quote: char) {
    let escapes = quote == '"';
    cursor.skip(3);
    loop {
        match cursor.peek(0) {
            None => return,
            Some('\\') if escapes => cursor.skip(2),
            Some(c) if c == quote && cursor.run_of(quote) >= 3 => {
                let run = cursor.run_of(quote);
                cursor.skip(run.min(5));
                return;
            }
            Some(_) => {
                cursor.bump();
            }
        }
    }
}

fn toml_single_line(cursor: &mut Cursor, quote: char) {
    let escapes = quote == '"';
    cursor.bump();
    loop {
        match cursor.peek(0) {
            None | Some('\n') => return,
            Some('\\') if escapes => {
                cursor.bump();
                if cursor.peek(0) != Some('\n') {
                    cursor.bump();
                }
            }
            Some(c) if c == quote => {
                cursor.bump();
                return;
            }
            Some(_) => {
                cursor.bump();
            }
        }
    }
}

struct Heredoc {
    delimiter: String,
    strip_tabs: bool,
}

fn is_shell_boundary(c: char) -> bool {
    c.is_whitespace() || matches!(c, ';' | '&' | '|' | '(' | ')' | '`' | '<' | '>')
}

fn shell_comments(text: &str) -> Vec<Found> {
    let mut cursor = Cursor::new(text);
    let mut found = Vec::new();
    let mut content = false;
    let mut word_start = true;
    let mut pending: Vec<Heredoc> = Vec::new();
    while let Some(c) = cursor.peek(0) {
        match c {
            '#' if word_start => found.push(hash_comment(&mut cursor, content)),
            '\\' => {
                content = true;
                cursor.bump();
                word_start = cursor.bump() == Some('\n');
            }
            '\'' => {
                content = true;
                word_start = false;
                cursor.bump();
                while cursor.bump().is_some_and(|inner| inner != '\'') {}
            }
            '"' => {
                content = true;
                word_start = false;
                cursor.bump();
                shell_double_quoted(&mut cursor);
            }
            '$' if cursor.peek(1) == Some('(') && cursor.peek(2) == Some('(') => {
                content = true;
                word_start = false;
                skip_arithmetic(&mut cursor);
            }
            '<' if cursor.run_of('<') >= 3 => {
                content = true;
                word_start = true;
                cursor.skip(3);
            }
            '<' if cursor.run_of('<') == 2 => {
                content = true;
                cursor.skip(2);
                pending.extend(heredoc_delimiter(&mut cursor));
                word_start = false;
            }
            '\n' => {
                cursor.bump();
                skip_heredoc_bodies(&mut cursor, &mut pending);
                word_start = true;
            }
            c if is_shell_boundary(c) => {
                if !c.is_whitespace() {
                    content = true;
                }
                word_start = true;
                cursor.bump();
            }
            _ => {
                content = true;
                word_start = false;
                cursor.bump();
            }
        }
    }
    found
}

fn skip_arithmetic(cursor: &mut Cursor) {
    cursor.skip(3);
    let mut depth = 2usize;
    while depth > 0 {
        match cursor.bump() {
            None => break,
            Some('(') => depth += 1,
            Some(')') => depth -= 1,
            Some(_) => {}
        }
    }
}

fn shell_double_quoted(cursor: &mut Cursor) {
    loop {
        match cursor.bump() {
            None | Some('"') => break,
            Some('\\') => {
                cursor.bump();
            }
            Some(_) => {}
        }
    }
}

fn heredoc_delimiter(cursor: &mut Cursor) -> Option<Heredoc> {
    let strip_tabs = cursor.peek(0) == Some('-');
    if strip_tabs {
        cursor.bump();
    }
    while matches!(cursor.peek(0), Some(' ' | '\t')) {
        cursor.bump();
    }
    let mut delimiter = String::new();
    while let Some(c) = cursor.peek(0) {
        match c {
            '\'' | '"' => {
                cursor.bump();
                while let Some(inner) = cursor.bump() {
                    if inner == c {
                        break;
                    }
                    delimiter.push(inner);
                }
            }
            '\\' => {
                cursor.bump();
                delimiter.extend(cursor.bump());
            }
            c if is_shell_boundary(c) => break,
            c => {
                delimiter.push(c);
                cursor.bump();
            }
        }
    }
    (!delimiter.is_empty()).then_some(Heredoc {
        delimiter,
        strip_tabs,
    })
}

fn skip_heredoc_bodies(cursor: &mut Cursor, pending: &mut Vec<Heredoc>) {
    for heredoc in pending.drain(..) {
        while cursor.peek(0).is_some() {
            let line = cursor.rest_of_line();
            cursor.bump();
            let line = line.trim_end_matches('\r');
            let line = if heredoc.strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line
            };
            if line == heredoc.delimiter {
                break;
            }
        }
    }
}

fn dockerfile_comments(text: &str) -> Vec<Found> {
    let mut found = Vec::new();
    let mut content = false;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            found.push(Found {
                line: u32::try_from(index + 1).unwrap_or(u32::MAX),
                text: trimmed.to_owned(),
                after_content: content,
            });
        } else if !trimmed.is_empty() {
            content = true;
        }
    }
    found
}
