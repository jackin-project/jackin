// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::ops::Range;
use std::path::Path;

use super::Check;
use crate::ci::CiStep;
use crate::lex::{Token, lex_rust};
use crate::repo::{FileKind, Repo};
use crate::violation::{Violation, sorted};

pub const ALLOWED_LITERALS: &[&str] = &["JACKIN_NO_MOTION"];

const SCREEN_SOURCE: &str = "crates/jackin-tui/src";

const SCREEN_CHECK_FILE: &str = "crates/xtask/src/checks/screen.rs";

const TRACING_MACROS: [&str; 10] = [
    "trace",
    "debug",
    "info",
    "warn",
    "error",
    "event",
    "trace_span",
    "debug_span",
    "info_span",
    "span",
];

const ERROR_TRAIT: &str = "Error";

const RATATUI_ALLOWED: [&[&str]; 4] = [
    &["Frame"],
    &["layout", "Rect"],
    &["buffer", "Buffer"],
    &["crossterm", "event"],
];

#[derive(Clone, Copy, Debug)]
pub struct ScreenCheck {
    allowed: &'static [&'static str],
}

impl ScreenCheck {
    #[must_use]
    pub const fn production() -> Self {
        Self::with_allowed(ALLOWED_LITERALS)
    }

    #[must_use]
    pub const fn with_allowed(allowed: &'static [&'static str]) -> Self {
        Self { allowed }
    }
}

impl Check for ScreenCheck {
    fn step(&self) -> CiStep {
        CiStep::Screen
    }

    fn run(&self, repo: &Repo) -> Vec<Violation> {
        let mut violations = Vec::new();
        let mut used: BTreeSet<&str> = BTreeSet::new();
        let mut scanned = false;
        for file in repo.files() {
            if file.kind() != FileKind::Rust || !file.path().starts_with(SCREEN_SOURCE) {
                continue;
            }
            scanned = true;
            let tokens = lex_rust(file.text()).tokens;
            violations.extend(import_violations(file.path(), &tokens));
            violations.extend(literal_violations(
                file.path(),
                &tokens,
                self.allowed,
                &mut used,
            ));
        }
        for entry in self
            .allowed
            .iter()
            .filter(|entry| scanned && !used.contains(**entry))
        {
            violations.push(Violation::at_file(
                self.step(),
                Path::new(SCREEN_CHECK_FILE),
                format!("C2: the ALLOWED_LITERALS entry {entry:?} occurs nowhere in {SCREEN_SOURCE}; remove it"),
            ));
        }
        sorted(violations)
    }
}

fn closing(tokens: &[Token], open: usize) -> usize {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if ['(', '[', '{'].iter().any(|c| token.is_punct(*c)) {
            depth += 1;
        } else if [')', ']', '}'].iter().any(|c| token.is_punct(*c)) {
            depth -= 1;
            if depth == 0 {
                return index;
            }
        }
    }
    tokens.len()
}

fn is_path_separator(tokens: &[Token], index: usize) -> bool {
    tokens.get(index).is_some_and(|t| t.is_punct(':'))
        && tokens.get(index + 1).is_some_and(|t| t.is_punct(':'))
}

struct Impl<'a> {
    trait_name: &'a str,
    target: &'a str,
    body: Range<usize>,
}

fn impl_of(tokens: &[Token], index: usize) -> Option<Impl<'_>> {
    let mut depth = 0usize;
    let mut last_ident: Option<&str> = None;
    let mut trait_name: Option<&str> = None;
    let mut target: Option<&str> = None;
    let mut in_where = false;
    let mut position = index + 1;
    while let Some(token) = tokens.get(position) {
        if token.is_punct('<') {
            depth += 1;
        } else if token.is_punct('>') && !tokens[position - 1].is_punct('-') {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && token.is_punct('{') {
            return Some(Impl {
                trait_name: trait_name?,
                target: target?,
                body: position..closing(tokens, position) + 1,
            });
        } else if depth == 0 && token.is_punct(';') {
            return None;
        } else if depth == 0 && token.is_ident("where") {
            in_where = true;
        } else if depth == 0 && !in_where && token.is_ident("for") {
            trait_name = last_ident;
        } else if depth == 0
            && !in_where
            && let Some(ident) = token.ident()
        {
            last_ident = Some(ident);
            if trait_name.is_some() {
                target = Some(ident);
            }
        }
        position += 1;
    }
    None
}

fn error_types(tokens: &[Token]) -> BTreeSet<&str> {
    (0..tokens.len())
        .filter(|index| tokens[*index].is_ident("impl"))
        .filter_map(|index| impl_of(tokens, index))
        .filter(|found| found.trait_name == ERROR_TRAIT)
        .map(|found| found.target)
        .collect()
}

fn cfg_test_module(tokens: &[Token], index: usize) -> Option<Range<usize>> {
    let attribute = [
        |t: &Token| t.is_punct('#'),
        |t: &Token| t.is_punct('['),
        |t: &Token| t.is_ident("cfg"),
        |t: &Token| t.is_punct('('),
        |t: &Token| t.is_ident("test"),
        |t: &Token| t.is_punct(')'),
        |t: &Token| t.is_punct(']'),
    ];
    for (offset, matches) in attribute.iter().enumerate() {
        if !tokens.get(index + offset).is_some_and(matches) {
            return None;
        }
    }
    let mut position = index + attribute.len();
    if tokens.get(position).is_some_and(|t| t.is_ident("pub")) {
        position += 1;
    }
    if !tokens.get(position)?.is_ident("mod") || tokens.get(position + 1)?.ident().is_none() {
        return None;
    }
    let open = position + 2;
    tokens
        .get(open)?
        .is_punct('{')
        .then(|| open..closing(tokens, open) + 1)
}

fn macro_arguments(tokens: &[Token], index: usize) -> Option<Range<usize>> {
    let name = tokens[index].ident()?;
    if !TRACING_MACROS.contains(&name) || !tokens.get(index + 1)?.is_punct('!') {
        return None;
    }
    let open = index + 2;
    let opens = ['(', '[', '{'].iter().any(|c| tokens[open].is_punct(*c));
    opens.then(|| open..closing(tokens, open) + 1)
}

fn error_attribute(tokens: &[Token], index: usize) -> Option<Range<usize>> {
    let is_error = tokens[index].is_punct('#')
        && tokens.get(index + 1)?.is_punct('[')
        && tokens.get(index + 2)?.is_ident("error")
        && tokens.get(index + 3)?.is_punct('(');
    is_error.then(|| index + 1..closing(tokens, index + 1) + 1)
}

fn allowed_ranges(tokens: &[Token]) -> Vec<Range<usize>> {
    let errors = error_types(tokens);
    let mut ranges = Vec::new();
    for index in 0..tokens.len() {
        if tokens[index].is_ident("impl")
            && let Some(found) = impl_of(tokens, index)
            && (found.trait_name == "Debug"
                || (found.trait_name == "Display" && errors.contains(found.target)))
        {
            ranges.push(found.body);
        }
        ranges.extend(cfg_test_module(tokens, index));
        ranges.extend(macro_arguments(tokens, index));
        ranges.extend(error_attribute(tokens, index));
    }
    ranges
}

fn literal_violations<'a>(
    path: &Path,
    tokens: &[Token],
    allowed: &[&'a str],
    used: &mut BTreeSet<&'a str>,
) -> Vec<Violation> {
    let ranges = allowed_ranges(tokens);
    let mut violations = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let Some(text) = token.string() else {
            continue;
        };
        if ranges.iter().any(|range| range.contains(&index)) {
            continue;
        }
        if allowed.contains(&text) {
            if let Some(entry) = allowed.iter().find(|entry| **entry == text) {
                used.insert(*entry);
            }
            continue;
        }
        violations.push(Violation::at_line(
            CiStep::Screen,
            path,
            token.line,
            format!(
                "C2: the string literal \"{text}\" is outside a Debug impl, a Display impl of an error type, a tracing macro, an error attribute, a test module and ALLOWED_LITERALS; screen text comes from the baseline or from a domain crate"
            ),
        ));
    }
    violations
}

fn path_tree(
    tokens: &[Token],
    position: &mut usize,
    prefix: &[String],
    out: &mut Vec<Vec<String>>,
) {
    let mut path = prefix.to_vec();
    while let Some(token) = tokens.get(*position) {
        if token.is_punct('{') {
            *position += 1;
            loop {
                path_tree(tokens, position, &path, out);
                match tokens.get(*position) {
                    Some(next) if next.is_punct(',') => *position += 1,
                    Some(next) if next.is_punct('}') => {
                        *position += 1;
                        return;
                    }
                    _ => return,
                }
            }
        }
        let segment = if token.is_punct('*') {
            "*"
        } else if let Some(ident) = token.ident() {
            ident
        } else {
            break;
        };
        path.push(segment.to_owned());
        *position += 1;
        if segment != "*" && is_path_separator(tokens, *position) {
            *position += 2;
            continue;
        }
        if tokens.get(*position).is_some_and(|t| t.is_ident("as")) {
            *position += 2;
        }
        break;
    }
    if path.len() > prefix.len() || prefix.is_empty() {
        out.push(path);
    }
}

fn import_violations(path: &Path, tokens: &[Token]) -> Vec<Violation> {
    let mut violations = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.is_ident("buffer")
            && tokens.get(index + 1).is_some_and(|t| t.is_punct('.'))
            && let Some(method) = tokens.get(index + 2).and_then(Token::ident)
            && method.starts_with("set_")
        {
            violations.push(Violation::at_line(
                CiStep::Screen,
                path,
                token.line,
                format!("C1: buffer.{method} draws; the baseline draws, jackin-tui draws nothing"),
            ));
        }
        let starts_path = token.is_ident("ratatui")
            && !(index >= 3
                && is_path_separator(tokens, index - 2)
                && tokens[index - 3].ident().is_some());
        if !starts_path {
            continue;
        }
        let mut position = index;
        let mut paths = Vec::new();
        path_tree(tokens, &mut position, &[], &mut paths);
        for full in paths {
            let rest: Vec<&str> = full.iter().skip(1).map(String::as_str).collect();
            let allowed = RATATUI_ALLOWED
                .iter()
                .any(|prefix| rest.starts_with(prefix));
            if !allowed {
                violations.push(Violation::at_line(
                    CiStep::Screen,
                    path,
                    token.line,
                    format!(
                        "C1: imports {}; jackin-tui may import only Frame, Rect, Buffer and crossterm::event from ratatui",
                        full.join("::")
                    ),
                ));
            }
        }
    }
    violations
}
