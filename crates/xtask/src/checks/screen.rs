// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Range;
use std::path::Path;

use super::Check;
use crate::ci::CiStep;
use crate::lex::{Tok, Token, lex_rust};
use crate::repo::{FileKind, Repo};
use crate::violation::{Violation, sorted};

pub const ALLOWED_LITERALS: &[&str] = &["JACKIN_NO_MOTION"];

const SCREEN_SOURCE: &str = "crates/jackin-tui/src";
const SCREEN_LIB: &str = "crates/jackin-tui/src/lib.rs";

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

const KEYWORDS_BEFORE_PATH: [&str; 13] = [
    "use", "pub", "impl", "for", "dyn", "as", "where", "mut", "in", "let", "return", "else",
    "const",
];

const KEYWORDS_BEFORE_ARRAY: [&str; 13] = [
    "in", "return", "mut", "else", "break", "let", "const", "ref", "dyn", "impl", "if", "while",
    "match",
];

const NON_PATTERN_KEYWORDS: [&str; 7] = ["for", "struct", "impl", "enum", "trait", "type", "union"];

const SILENCING_ATTRIBUTES: [&str; 2] = ["allow", "expect"];

const DRAWING_LINTS: [&str; 2] = ["disallowed_methods", "disallowed_fields"];

const RATATUI_ALLOWED: [&[&str]; 4] = [
    &["Frame"],
    &["layout", "Rect"],
    &["buffer", "Buffer"],
    &["crossterm", "event"],
];

const BUFFER_RENAME: &str = "C1: renames a ratatui Buffer; jackin-tui names the type as Buffer so the drawing gate can see it";
const ALIAS_WORK_LIMIT: usize = 16_384;

#[derive(Clone, Copy, Debug)]
pub struct ScreenCheck {
    allowed: &'static [&'static str],
    alias_work_limit: usize,
}

impl ScreenCheck {
    #[must_use]
    pub const fn production() -> Self {
        Self::with_allowed(ALLOWED_LITERALS)
    }

    #[must_use]
    pub const fn with_allowed(allowed: &'static [&'static str]) -> Self {
        Self {
            allowed,
            alias_work_limit: ALIAS_WORK_LIMIT,
        }
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
            let types = TypeScopes::collect(&tokens, file.path() == Path::new(SCREEN_LIB));
            violations.extend(import_violations(file.path(), &tokens));
            violations.extend(types.renamed_imports(file.path()));
            violations.extend(index_violations(file.path(), &tokens));
            violations.extend(silencing_violations(file.path(), &tokens));
            violations.extend(pattern_violations(
                file.path(),
                &tokens,
                &types,
                self.alias_work_limit,
            ));
            violations.extend(type_alias_violations(file.path(), &tokens, &types));
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
        if tokens.get(position).is_some_and(|t| t.is_punct('(')) {
            position = closing(tokens, position) + 1;
        }
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
    let opens = ['(', '[', '{']
        .iter()
        .any(|c| tokens.get(open).is_some_and(|t| t.is_punct(*c)));
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

struct PathLeaf<'a> {
    segments: Vec<&'a str>,
    rename: Option<(&'a str, u32)>,
}

fn path_tree<'a>(
    tokens: &'a [Token],
    position: &mut usize,
    prefix: &[&'a str],
    out: &mut Vec<PathLeaf<'a>>,
) {
    let mut path = prefix.to_vec();
    let mut rename = None;
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
        path.push(segment);
        *position += 1;
        if segment != "*" && is_path_separator(tokens, *position) {
            *position += 2;
            continue;
        }
        if tokens.get(*position).is_some_and(|t| t.is_ident("as")) {
            rename = tokens
                .get(*position + 1)
                .and_then(Token::ident)
                .map(|name| (name, token.line));
            *position += 2;
        }
        break;
    }
    if path.len() > prefix.len() || prefix.is_empty() {
        out.push(PathLeaf {
            segments: path,
            rename,
        });
    }
}

fn import_violations(path: &Path, tokens: &[Token]) -> Vec<Violation> {
    let mut violations = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let continues_path = index >= 3
            && is_path_separator(tokens, index - 2)
            && tokens[index - 3]
                .ident()
                .is_some_and(|ident| !KEYWORDS_BEFORE_PATH.contains(&ident));
        let starts_path = token.is_ident("ratatui") && !continues_path;
        if !starts_path {
            continue;
        }
        let mut position = index;
        let mut paths = Vec::new();
        path_tree(tokens, &mut position, &[], &mut paths);
        for leaf in paths {
            let rest = &leaf.segments[1..];
            let allowed = RATATUI_ALLOWED
                .iter()
                .any(|prefix| rest.starts_with(prefix));
            if rest == ["buffer", "Buffer"]
                && let Some((_, line)) = leaf.rename
            {
                violations.push(Violation::at_line(
                    CiStep::Screen,
                    path,
                    line,
                    BUFFER_RENAME,
                ));
            }
            if !allowed {
                violations.push(Violation::at_line(
                    CiStep::Screen,
                    path,
                    token.line,
                    format!(
                        "C1: imports {}; jackin-tui may import only Frame, Rect, Buffer and crossterm::event from ratatui",
                        leaf.segments.join("::")
                    ),
                ));
            }
        }
    }
    violations
}

fn test_modules(tokens: &[Token]) -> Vec<Range<usize>> {
    (0..tokens.len())
        .filter_map(|index| cfg_test_module(tokens, index))
        .collect()
}

fn is_index_open(tokens: &[Token], index: usize) -> bool {
    if !tokens[index].is_punct('[') || index == 0 {
        return false;
    }
    let before = &tokens[index - 1];
    before.is_punct(')')
        || before.is_punct(']')
        || before.is_punct('?')
        || before
            .ident()
            .is_some_and(|ident| !KEYWORDS_BEFORE_ARRAY.contains(&ident))
}

fn is_range_index(tokens: &[Token], open: usize) -> bool {
    let close = closing(tokens, open).min(tokens.len());
    let mut depth = 0usize;
    for index in open + 1..close {
        let token = &tokens[index];
        if ['(', '[', '{'].iter().any(|c| token.is_punct(*c)) {
            depth += 1;
        } else if [')', ']', '}'].iter().any(|c| token.is_punct(*c)) {
            depth = depth.saturating_sub(1);
        } else if depth == 0
            && token.is_punct('.')
            && tokens.get(index + 1).is_some_and(|next| next.is_punct('.'))
        {
            return true;
        }
    }
    false
}

fn index_violations(path: &Path, tokens: &[Token]) -> Vec<Violation> {
    let tests = test_modules(tokens);
    (0..tokens.len())
        .filter(|index| is_index_open(tokens, *index) && !is_range_index(tokens, *index))
        .filter(|index| !tests.iter().any(|range| range.contains(index)))
        .map(|index| {
            Violation::at_line(
                CiStep::Screen,
                path,
                tokens[index].line,
                "C1: indexes with [..]; indexing a ratatui Buffer draws a cell, and jackin-tui draws nothing",
            )
        })
        .collect()
}

fn type_alias_violations<'a>(
    path: &Path,
    tokens: &'a [Token],
    types: &TypeScopes<'a>,
) -> Vec<Violation> {
    tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.is_ident("type"))
        .filter(|(index, _)| {
            TypeAlias::parse(tokens, *index).is_some_and(|alias| {
                alias.value.clone().any(|target| {
                    types.may_be_ratatui_buffer(tokens, target)
                        && !associated_argument_name(tokens, target)
                        && !tokens[target].ident().is_some_and(|name| {
                            path_start(tokens, target) == (target, false)
                                && alias.type_parameter_slot(tokens, name).is_some()
                        })
                })
            })
        })
        .map(|(_, token)| Violation::at_line(CiStep::Screen, path, token.line, BUFFER_RENAME))
        .collect()
}

fn generic_slots(tokens: &[Token], open: usize) -> Option<(Vec<Range<usize>>, usize)> {
    if !tokens.get(open)?.is_punct('<') {
        return None;
    }
    let mut slots = Vec::new();
    let mut start = open + 1;
    let mut index = start;
    let mut depth = 0;
    while let Some(token) = tokens.get(index) {
        if ['(', '[', '{'].iter().any(|c| token.is_punct(*c)) {
            index = closing(tokens, index) + 1;
            continue;
        }
        if token.is_punct('<') {
            depth += 1;
        } else if token.is_punct('>') && !tokens[index - 1].is_punct('-') {
            if depth == 0 {
                if start < index {
                    slots.push(start..index);
                }
                return Some((slots, index + 1));
            }
            depth -= 1;
        } else if token.is_punct(',') && depth == 0 {
            slots.push(start..index);
            start = index + 1;
        }
        index += 1;
    }
    None
}

fn generic_ends(tokens: &[Token]) -> Vec<Option<usize>> {
    let mut ends = vec![None; tokens.len()];
    let mut opens = Vec::new();
    let mut groups = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            Tok::Punct('(' | '[' | '{') => groups.push(opens.len()),
            Tok::Punct(')' | ']' | '}') => {
                if let Some(start) = groups.pop() {
                    opens.truncate(start);
                }
            }
            Tok::Punct('<') => opens.push(index),
            Tok::Punct('>')
                if opens.len() > groups.last().copied().unwrap_or(0)
                    && !tokens[index - 1].is_punct('-') =>
            {
                if let Some(open) = opens.pop() {
                    ends[open] = Some(index + 1);
                }
            }
            _ => {}
        }
    }
    ends
}

fn associated_argument_name(tokens: &[Token], index: usize) -> bool {
    if !tokens
        .get(index.wrapping_sub(1))
        .is_some_and(|token| matches!(token.kind, Tok::Punct('<' | ',')))
    {
        return false;
    }
    let after_name = index + 1;
    let arguments = after_name + 2 * usize::from(is_path_separator(tokens, after_name));
    let boundary = if tokens
        .get(arguments)
        .is_some_and(|token| token.is_punct('<'))
    {
        let Some((_, end)) = generic_slots(tokens, arguments) else {
            return false;
        };
        end
    } else {
        after_name
    };
    tokens.get(boundary).is_some_and(|token| {
        token.is_punct('=')
            || token.is_punct(':')
                && (!is_path_separator(tokens, boundary) || is_path_separator(tokens, boundary + 1))
    })
}

fn alias_boundary(tokens: &[Token], mut index: usize) -> Option<usize> {
    let mut depth = 0usize;
    while let Some(token) = tokens.get(index) {
        match &token.kind {
            Tok::Punct('(' | '[' | '{') => index = closing(tokens, index),
            Tok::Punct('<') => depth += 1,
            Tok::Punct('>') if !tokens[index - 1].is_punct('-') => depth = depth.checked_sub(1)?,
            Tok::Punct('=') if depth == 0 => return Some(index),
            Tok::Ident(name) if depth == 0 && name == "where" => return Some(index),
            Tok::Punct(';') => return (depth == 0).then_some(index),
            Tok::Punct(')' | ']' | '}') => return None,
            _ => {}
        }
        index += 1;
    }
    None
}

struct TypeAlias {
    declaration: usize,
    parameters: Vec<Range<usize>>,
    value: Range<usize>,
}

impl TypeAlias {
    fn parse(tokens: &[Token], index: usize) -> Option<Self> {
        tokens.get(index + 1)?.ident()?;
        let after_name = index + 2;
        let (parameters, mut assignment) = if tokens.get(after_name)?.is_punct('<') {
            generic_slots(tokens, after_name)?
        } else {
            (Vec::new(), after_name)
        };
        if tokens.get(assignment)?.is_ident("where") {
            assignment = alias_boundary(tokens, assignment + 1)?;
        }
        if !tokens[assignment].is_punct('=') {
            return None;
        }
        let start = assignment + 1;
        let end = alias_boundary(tokens, start)?;
        let terminator = if tokens[end].is_ident("where") {
            alias_boundary(tokens, end + 1)?
        } else {
            end
        };
        (start < end && tokens[terminator].is_punct(';')).then_some(Self {
            declaration: index,
            parameters,
            value: start..end,
        })
    }

    fn type_parameter_slot(&self, tokens: &[Token], name: &str) -> Option<usize> {
        self.parameters.iter().position(|range| {
            !range.is_empty()
                && !tokens[range.start].is_ident("const")
                && tokens[range.start].is_ident(name)
        })
    }

    fn type_parameter_default(&self, tokens: &[Token], slot: usize) -> Option<Range<usize>> {
        let parameters = self.parameters.get(..=slot)?;
        if parameters
            .iter()
            .any(|range| range.is_empty() || tokens[range.start].ident().is_none())
        {
            return None;
        }
        let parameter = parameters.last()?;
        let equals = alias_boundary(&tokens[..parameter.end], parameter.start + 1)?;
        let start = equals + 1;
        (tokens[equals].is_punct('=') && start < parameter.end).then_some(start..parameter.end)
    }
}

struct AliasApplication<'a> {
    alias: &'a TypeAlias,
    arguments: Vec<Range<usize>>,
    available: usize,
}

impl<'a> AliasApplication<'a> {
    fn new(alias: &'a TypeAlias, arguments: Vec<Range<usize>>) -> Self {
        Self {
            alias,
            arguments,
            available: alias.parameters.len(),
        }
    }
}

struct AliasWork(usize);

struct AliasWorkLimit;

impl AliasWork {
    fn charge(&mut self, units: usize) -> Result<(), AliasWorkLimit> {
        self.0 = self.0.checked_sub(units).ok_or(AliasWorkLimit)?;
        Ok(())
    }
}

enum TypeBinding<'a> {
    Alias(TypeAlias),
    RatatuiBuffer,
    Nominal,
    InlineModule(usize),
    Import(ImportPath<'a>),
    Unresolved,
}

struct ImportPath<'a> {
    segments: Vec<&'a str>,
    absolute: bool,
}

#[derive(Default)]
struct TypeScope<'a> {
    parent: Option<usize>,
    module: usize,
    module_parent: Option<usize>,
    bindings: BTreeMap<&'a str, TypeBinding<'a>>,
    renamed: Vec<(&'a str, u32)>,
    glob_import: bool,
}

impl<'a> TypeScope<'a> {
    fn bind(&mut self, name: &'a str, binding: TypeBinding<'a>) {
        self.bindings
            .entry(name)
            .and_modify(|existing| *existing = TypeBinding::Unresolved)
            .or_insert(binding);
    }

    fn import(&mut self, tokens: &'a [Token], index: usize) -> usize {
        let mut position = index + 1;
        let absolute = is_path_separator(tokens, position);
        if absolute {
            position += 2;
        }
        let mut paths = Vec::new();
        path_tree(tokens, &mut position, &[], &mut paths);
        let attributed = has_outer_attribute(tokens, index);
        for mut path in paths {
            let segments = path.segments.as_slice();
            let name = match (path.rename, segments) {
                (Some((name, _)), _) => name,
                (None, [.., "*"]) => {
                    self.glob_import = true;
                    continue;
                }
                (None, [.., name, "self"] | [.., name]) => *name,
                _ => continue,
            };
            let binding = match segments {
                ["ratatui", "buffer", "Buffer"] => TypeBinding::RatatuiBuffer,
                _ if !attributed => {
                    if let Some(rename) = path.rename {
                        self.renamed.push(rename);
                    }
                    if matches!(segments, [_, .., "self"]) {
                        path.segments.pop();
                    }
                    TypeBinding::Import(ImportPath {
                        segments: path.segments,
                        absolute,
                    })
                }
                _ => TypeBinding::Unresolved,
            };
            self.bind(name, binding);
        }
        position
    }

    fn bind_item(&mut self, tokens: &'a [Token], index: usize) {
        let Some(name) = tokens.get(index + 1).and_then(Token::ident) else {
            return;
        };
        let binding = match tokens[index].ident() {
            Some("type") => {
                TypeAlias::parse(tokens, index).map_or(TypeBinding::Unresolved, TypeBinding::Alias)
            }
            Some("struct" | "enum" | "union")
                if nominal_attributes_preserve_binding(tokens, index) =>
            {
                TypeBinding::Nominal
            }
            Some("struct" | "enum" | "union") => TypeBinding::Unresolved,
            Some("mod") if tokens.get(index + 2).is_some_and(|t| t.is_punct(';')) => {
                TypeBinding::Unresolved
            }
            _ => return,
        };
        self.bind(name, binding);
    }
}

fn group_start(tokens: &[Token], close: usize, left: char, right: char) -> Option<usize> {
    let mut depth = 0;
    for (index, token) in tokens.get(..close)?.iter().enumerate().rev() {
        if token.is_punct(right) {
            depth += 1;
        } else if token.is_punct(left) {
            if depth == 0 {
                return Some(index);
            }
            depth -= 1;
        }
    }
    None
}

fn before_visibility(tokens: &[Token], item: usize) -> usize {
    let mut end = item;
    if tokens
        .get(end.wrapping_sub(1))
        .is_some_and(|t| t.is_punct(')'))
        && let Some(open) = group_start(tokens, end - 1, '(', ')')
        && tokens
            .get(open.wrapping_sub(1))
            .is_some_and(|t| t.is_ident("pub"))
    {
        end = open - 1;
    } else if tokens
        .get(end.wrapping_sub(1))
        .is_some_and(|t| t.is_ident("pub"))
    {
        end -= 1;
    }
    end
}

fn outer_attribute_before(tokens: &[Token], end: usize) -> Option<Range<usize>> {
    let close = end.checked_sub(1)?;
    if !tokens.get(close)?.is_punct(']') {
        return None;
    }
    let start = group_start(tokens, close, '[', ']')?.checked_sub(1)?;
    tokens[start].is_punct('#').then_some(start..end)
}

fn has_outer_attribute(tokens: &[Token], item: usize) -> bool {
    outer_attribute_before(tokens, before_visibility(tokens, item)).is_some()
}

fn nominal_attributes_preserve_binding(tokens: &[Token], item: usize) -> bool {
    let mut end = before_visibility(tokens, item);
    while let Some(attribute) = outer_attribute_before(tokens, end) {
        let inner = &tokens[attribute.start + 2..attribute.end - 1];
        let [name, input, ..] = inner else {
            return false;
        };
        if !matches!(
            name.ident(),
            Some("derive" | "repr" | "allow" | "warn" | "deny" | "forbid" | "expect")
        ) || !input.is_punct('(')
            || closing(inner, 1) + 1 != inner.len()
        {
            return false;
        }
        end = attribute.start;
    }
    true
}

fn opaque_group_open(tokens: &[Token], index: usize) -> Option<usize> {
    let token = &tokens[index];
    if token.is_punct('#') {
        let open = index + 1 + usize::from(tokens.get(index + 1)?.is_punct('!'));
        return tokens.get(open)?.is_punct('[').then_some(open);
    }
    if !token.is_punct('!') || tokens.get(index.checked_sub(1)?)?.ident().is_none() {
        return None;
    }
    let open = index + 1 + usize::from(tokens[index - 1].is_ident("macro_rules"));
    let next = tokens.get(open)?;
    ['(', '[', '{']
        .iter()
        .any(|&c| next.is_punct(c))
        .then_some(open)
}

#[derive(Clone, Copy)]
enum ScopePosition {
    Rust(usize),
    Opaque(usize),
}

struct TypeScopes<'a> {
    scopes: Vec<TypeScope<'a>>,
    enclosing: Vec<Option<ScopePosition>>,
    crate_root: bool,
}

impl<'a> TypeScopes<'a> {
    fn collect(tokens: &'a [Token], crate_root: bool) -> Self {
        let mut scopes = vec![TypeScope::default()];
        let mut enclosing = vec![None; tokens.len()];
        let mut stack = vec![0];
        let mut index = 0;
        while let Some(token) = tokens.get(index) {
            let scope = *stack.last().unwrap_or(&0);
            if let Some(open) = opaque_group_open(tokens, index) {
                let end = (closing(tokens, open) + 1).min(tokens.len());
                enclosing[index..end].fill(Some(ScopePosition::Opaque(scope)));
                index = end;
                continue;
            }
            enclosing[index] = Some(ScopePosition::Rust(scope));
            if token.is_punct('{') {
                stack.push(Self::enter_scope(tokens, index, &mut scopes, scope));
            } else if token.is_punct('}') && stack.len() > 1 {
                stack.pop();
            } else if token.is_ident("use") {
                index = scopes[scope].import(tokens, index);
                continue;
            } else {
                scopes[scope].bind_item(tokens, index);
            }
            index += 1;
        }
        Self {
            scopes,
            enclosing,
            crate_root,
        }
    }

    fn enter_scope(
        tokens: &'a [Token],
        open: usize,
        scopes: &mut Vec<TypeScope<'a>>,
        parent: usize,
    ) -> usize {
        let child = scopes.len();
        let enclosing_module = scopes[parent].module;
        let module = open
            .checked_sub(2)
            .filter(|&index| tokens[index].is_ident("mod"))
            .and_then(|index| tokens[index + 1].ident().map(|name| (index, name)));
        if let Some((index, name)) = module {
            let binding = if has_outer_attribute(tokens, index) {
                TypeBinding::Unresolved
            } else {
                TypeBinding::InlineModule(child)
            };
            scopes[parent].bind(name, binding);
        }
        scopes.push(TypeScope {
            parent: module.is_none().then_some(parent),
            module: if module.is_some() {
                child
            } else {
                enclosing_module
            },
            module_parent: module.map(|_| enclosing_module),
            ..TypeScope::default()
        });
        child
    }

    fn lexical_binding(&self, mut scope: usize, name: &str) -> Option<(usize, &TypeBinding<'a>)> {
        loop {
            let current = &self.scopes[scope];
            if let Some(binding) = current.bindings.get(name) {
                return Some((scope, binding));
            }
            if current.glob_import {
                return None;
            }
            scope = current.parent?;
        }
    }

    fn super_module(&self, scope: usize, path: &mut VecDeque<&str>) -> Option<usize> {
        let mut module = self.scopes[scope].module;
        loop {
            module = self.scopes[module].module_parent?;
            if path.front() != Some(&"super") {
                return Some(module);
            }
            path.pop_front();
        }
    }

    fn resolve(
        &self,
        scope: usize,
        segments: impl IntoIterator<Item = &'a str>,
        absolute: bool,
    ) -> Option<&TypeBinding<'a>> {
        let mut path: VecDeque<_> = segments.into_iter().collect();
        let mut state = Some((scope, absolute, path.len()));
        let mut current = scope;
        let mut visited = BTreeSet::new();
        loop {
            let (owner, binding) = if let Some((start, absolute, edge_len)) = state.take() {
                if edge_len == 3
                    && path
                        .iter()
                        .take(edge_len)
                        .copied()
                        .eq(["ratatui", "buffer", "Buffer"])
                {
                    return (path.len() == edge_len).then_some(&TypeBinding::RatatuiBuffer);
                }
                if absolute {
                    return None;
                }
                match path.pop_front()? {
                    "crate" if self.crate_root => {
                        current = 0;
                        continue;
                    }
                    "crate" => return None,
                    "self" => {
                        current = self.scopes[start].module;
                        continue;
                    }
                    "super" => {
                        current = self.super_module(start, &mut path)?;
                        continue;
                    }
                    name => {
                        let (owner, binding) = self.lexical_binding(start, name)?;
                        ((owner, name), binding)
                    }
                }
            } else {
                let name = path.pop_front()?;
                ((current, name), self.scopes[current].bindings.get(name)?)
            };
            match binding {
                TypeBinding::Import(import) => {
                    if !visited.insert(owner) {
                        return None;
                    }
                    for segment in import.segments.iter().rev() {
                        path.push_front(segment);
                    }
                    state = Some((owner.0, import.absolute, import.segments.len()));
                }
                _ if path.is_empty() => return Some(binding),
                TypeBinding::InlineModule(child) => current = *child,
                _ => return None,
            }
        }
    }

    fn binding_at(&self, tokens: &'a [Token], end: usize) -> Option<&TypeBinding<'a>> {
        tokens[end].ident()?;
        let scope = match self.enclosing[end]? {
            ScopePosition::Rust(scope) | ScopePosition::Opaque(scope) => scope,
        };
        let (start, absolute) = path_start(tokens, end);
        self.resolve(
            scope,
            tokens[start..=end]
                .iter()
                .step_by(3)
                .map_while(Token::ident),
            absolute,
        )
    }

    fn apply_alias(
        &self,
        tokens: &'a [Token],
        alias: &TypeAlias,
        arguments: Vec<Range<usize>>,
        work: &mut AliasWork,
    ) -> Result<Option<Range<usize>>, AliasWorkLimit> {
        let mut value = alias.value.clone();
        let mut applications = vec![AliasApplication::new(alias, arguments)];
        loop {
            work.charge(value.len().max(1))?;
            let first = value.start + 2 * usize::from(is_path_separator(tokens, value.start));
            let mut end = first;
            let Some(name) = tokens.get(end).and_then(Token::ident) else {
                return Ok(None);
            };
            while end + 3 < value.end
                && is_path_separator(tokens, end + 1)
                && tokens[end + 3].ident().is_some()
            {
                end += 3;
            }
            if end + 1 == value.end {
                let Some(current) = applications.last_mut().filter(|_| value.start == end) else {
                    return Ok(Some(value));
                };
                work.charge(current.alias.parameters.len())?;
                let Some(slot) = current.alias.type_parameter_slot(tokens, name) else {
                    return Ok(Some(value));
                };
                if slot >= current.available {
                    return Ok(None);
                }
                if let Some(argument) = current.arguments.get(slot) {
                    value = argument.clone();
                    applications.truncate(applications.len() - 1);
                } else {
                    work.charge(slot + 1 + current.alias.parameters[slot].len())?;
                    let Some(default) = current.alias.type_parameter_default(tokens, slot) else {
                        return Ok(None);
                    };
                    value = default;
                    current.available = slot;
                }
                continue;
            }
            let Some(TypeBinding::Alias(alias)) = self.binding_at(tokens, end) else {
                return Ok(None);
            };
            let arguments = end + 1 + 2 * usize::from(is_path_separator(tokens, end + 1));
            let Some((slots, boundary)) = generic_slots(&tokens[..value.end], arguments) else {
                return Ok(None);
            };
            if boundary != value.end {
                return Ok(None);
            }
            work.charge(applications.len())?;
            if applications
                .iter()
                .any(|active| active.alias.declaration == alias.declaration)
            {
                return Ok(None);
            }
            value = alias.value.clone();
            applications.push(AliasApplication::new(alias, slots));
        }
    }

    fn may_be_ratatui_buffer(&self, tokens: &'a [Token], index: usize) -> bool {
        match self.binding_at(tokens, index) {
            Some(TypeBinding::RatatuiBuffer) => true,
            Some(TypeBinding::Nominal)
                if matches!(self.enclosing[index], Some(ScopePosition::Rust(_))) =>
            {
                false
            }
            _ => {
                tokens[index].is_ident("Buffer")
                    && !is_path_separator(tokens, index.wrapping_sub(2))
            }
        }
    }

    fn renamed_imports(&self, path: &Path) -> Vec<Violation> {
        self.scopes
            .iter()
            .enumerate()
            .flat_map(|(scope, bindings)| {
                bindings
                    .renamed
                    .iter()
                    .filter(move |&&(name, _)| {
                        matches!(
                            self.resolve(scope, [name], false),
                            Some(TypeBinding::RatatuiBuffer)
                        )
                    })
                    .map(move |&(_, line)| {
                        Violation::at_line(CiStep::Screen, path, line, BUFFER_RENAME)
                    })
            })
            .collect()
    }
}

fn path_start(tokens: &[Token], end: usize) -> (usize, bool) {
    let mut start = end;
    while is_path_separator(tokens, start.wrapping_sub(2)) {
        let preceding = start.checked_sub(3).and_then(|at| tokens[at].ident());
        if preceding.is_none_or(|name| KEYWORDS_BEFORE_PATH.contains(&name)) {
            return (start, true);
        }
        start -= 3;
    }
    (start, false)
}

fn field_pattern(tokens: &[Token], index: usize, open: usize, tests: &[Range<usize>]) -> bool {
    tokens.get(open).is_some_and(|t| t.is_punct('{'))
        && opens_fields(tokens, open + 1)
        && !declares_variant(tokens, index)
        && !is_type_position(tokens, index, open)
        && !tests.iter().any(|range| range.contains(&index))
}

fn buffer_pattern_matches<'a>(
    tokens: &'a [Token],
    index: usize,
    types: &TypeScopes<'a>,
    generic_ends: &[Option<usize>],
    tests: &[Range<usize>],
    work_limit: usize,
) -> Result<bool, AliasWorkLimit> {
    if tokens.get(index + 1).is_some_and(|t| t.is_punct('{')) {
        return Ok(types.may_be_ratatui_buffer(tokens, index)
            && field_pattern(tokens, index, index + 1, tests));
    }
    if !is_path_separator(tokens, index + 1) {
        return Ok(false);
    }
    let start = index + 3;
    let Some(open) = generic_ends.get(start).copied().flatten() else {
        return Ok(false);
    };
    let Some(TypeBinding::Alias(alias)) = types.binding_at(tokens, index) else {
        return Ok(false);
    };
    if !field_pattern(tokens, index, open, tests) {
        return Ok(false);
    }
    let mut work = AliasWork(work_limit);
    work.charge(open - start)?;
    let Some((slots, _)) = generic_slots(&tokens[..open], start) else {
        return Ok(false);
    };
    let argument = types.apply_alias(tokens, alias, slots, &mut work)?;
    Ok(argument.is_some_and(|range| {
        matches!(
            types.binding_at(tokens, range.end - 1),
            Some(TypeBinding::RatatuiBuffer)
        )
    }))
}

fn pattern_violations<'a>(
    path: &Path,
    tokens: &'a [Token],
    aliases: &TypeScopes<'a>,
    work_limit: usize,
) -> Vec<Violation> {
    let tests = test_modules(tokens);
    let generic_ends = generic_ends(tokens);
    (1..tokens.len())
        .filter(|&index| {
            buffer_pattern_matches(tokens, index, aliases, &generic_ends, &tests, work_limit)
                .unwrap_or(false)
        })
        .map(|index| {
            Violation::at_line(
                CiStep::Screen,
                path,
                tokens[index].line,
                "C1: destructures a ratatui Buffer; its cells are public fields, and jackin-tui draws nothing",
            )
        })
        .collect()
}

fn declares_variant(tokens: &[Token], index: usize) -> bool {
    let mut depth = 0usize;
    for back in (0..index).rev() {
        if tokens[back].is_punct('}') {
            depth += 1;
        } else if tokens[back].is_punct('{') {
            if depth == 0 {
                return tokens[back.saturating_sub(6)..back]
                    .iter()
                    .rev()
                    .take_while(|t| !t.is_punct('}') && !t.is_punct(';') && !t.is_punct('|'))
                    .any(|t| t.is_ident("enum"));
            }
            depth -= 1;
        }
    }
    false
}

fn opens_fields(tokens: &[Token], start: usize) -> bool {
    let mut first = start;
    loop {
        if tokens.get(first).is_some_and(|t| t.is_punct('#'))
            && tokens.get(first + 1).is_some_and(|t| t.is_punct('['))
        {
            first = closing(tokens, first + 1) + 1;
        } else if tokens
            .get(first)
            .is_some_and(|t| t.is_ident("ref") || t.is_ident("mut"))
        {
            first += 1;
        } else {
            break;
        }
    }
    let named = tokens.get(first).is_some_and(|t| t.ident().is_some())
        && tokens
            .get(first + 1)
            .is_some_and(|t| t.is_punct(',') || t.is_punct(':') || t.is_punct('}'));
    named
        || (tokens.get(first).is_some_and(|t| t.is_punct('.'))
            && tokens.get(first + 1).is_some_and(|t| t.is_punct('.')))
}

fn is_type_position(tokens: &[Token], index: usize, open: usize) -> bool {
    let mut start = index;
    while start >= 3 && is_path_separator(tokens, start - 2) && tokens[start - 3].ident().is_some()
    {
        start -= 3;
    }
    if start >= 2 && is_path_separator(tokens, start - 2) {
        start -= 2;
    }
    while start >= 1
        && (tokens[start - 1].is_punct('&')
            || tokens[start - 1].is_punct('*')
            || tokens[start - 1].is_ident("mut")
            || tokens[start - 1].is_ident("const")
            || tokens[start - 1].kind == Tok::Other)
    {
        start -= 1;
    }
    let Some(before) = start.checked_sub(1).map(|at| &tokens[at]) else {
        return false;
    };
    if opens_for_loop_pattern(tokens, open) {
        return false;
    }
    (before.is_punct('>') && start >= 2 && tokens[start - 2].is_punct('-'))
        || NON_PATTERN_KEYWORDS
            .iter()
            .any(|keyword| before.is_ident(keyword))
}

fn opens_for_loop_pattern(tokens: &[Token], open: usize) -> bool {
    let close = closing(tokens, open);
    tokens.get(close + 1).is_some_and(|t| t.is_ident("in"))
}

fn silencing_violations(path: &Path, tokens: &[Token]) -> Vec<Violation> {
    let mut violations = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if !token.is_punct('#') {
            continue;
        }
        let open = if tokens.get(index + 1).is_some_and(|t| t.is_punct('!')) {
            index + 2
        } else {
            index + 1
        };
        if !tokens.get(open).is_some_and(|t| t.is_punct('[')) {
            continue;
        }
        let attribute = &tokens[open..=closing(tokens, open).min(tokens.len() - 1)];
        let silences = attribute
            .iter()
            .any(|t| SILENCING_ATTRIBUTES.iter().any(|name| t.is_ident(name)))
            && attribute
                .iter()
                .any(|t| DRAWING_LINTS.iter().any(|lint| t.is_ident(lint)));
        if silences {
            violations.push(Violation::at_line(
                CiStep::Screen,
                path,
                token.line,
                "C1: silences clippy::disallowed_methods or clippy::disallowed_fields; the ban on ratatui drawing cannot be allowed or expected in jackin-tui",
            ));
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;
    use std::path::Path;

    use super::{Check, CiStep, Repo, SCREEN_LIB, ScreenCheck, Violation};

    #[test]
    fn alias_budget_reports_an_explicit_screen_violation() -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        let root = directory.path();
        let status = jackin_testkit::git_command()
            .args(["init", "-q"])
            .current_dir(root)
            .status()?;
        assert!(status.success(), "git init exited with {status}");
        fs::create_dir_all(root.join("crates/jackin-tui/src"))?;
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/jackin-tui\"]\nresolver = \"3\"\n",
        )?;
        fs::write(
            root.join("crates/jackin-tui/Cargo.toml"),
            "[package]\nname = \"jackin-tui\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )?;
        fs::write(
            root.join(SCREEN_LIB),
            "use ratatui::buffer::Buffer;\n\
             type Identity<T> = T;\n\
             fn view(buf: Buffer) {\n\
             \x20   let _: Option<Identity::<Buffer>> = None;\n\
             \x20   let Identity::<Buffer> { content, .. } = buf;\n\
             }\n",
        )?;
        let repo = Repo::open(root.to_owned())?;
        let normal = ScreenCheck::with_allowed(&[]);
        let capacity_limited = ScreenCheck {
            alias_work_limit: 3,
            ..normal
        };
        let path = Path::new(SCREEN_LIB);
        assert_eq!(
            normal.run(&repo),
            [Violation::at_line(
                CiStep::Screen,
                path,
                5,
                "C1: destructures a ratatui Buffer; its cells are public fields, and jackin-tui draws nothing",
            )]
        );
        assert_eq!(
            capacity_limited.run(&repo),
            [Violation::at_line(
                CiStep::Screen,
                path,
                5,
                "C1: Screen alias resolution exceeded its work limit for this pattern",
            )]
        );
        Ok(())
    }
}
