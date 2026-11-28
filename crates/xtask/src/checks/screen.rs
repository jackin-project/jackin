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
        let files: Vec<_> = repo
            .files()
            .iter()
            .filter(|file| file.kind() == FileKind::Rust && file.path().starts_with(SCREEN_SOURCE))
            .map(|file| ScreenFile::new(file.path(), file.text()))
            .collect();
        let types = TypeScopes::collect(&files);
        for (source, file) in files.iter().enumerate() {
            let path = file.path;
            let tokens = &file.tokens;
            violations.extend(import_violations(path, tokens));
            violations.extend(types.renamed_imports(source));
            violations.extend(index_violations(path, tokens));
            violations.extend(silencing_violations(path, tokens));
            violations.extend(pattern_violations(source, &types, self.alias_work_limit));
            violations.extend(type_alias_violations(source, &types));
            violations.extend(literal_violations(path, tokens, self.allowed, &mut used));
        }
        for entry in self
            .allowed
            .iter()
            .filter(|entry| !files.is_empty() && !used.contains(**entry))
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

struct ScreenFile<'a> {
    path: &'a Path,
    tokens: Vec<Token>,
    group_ends: Vec<Option<usize>>,
    header_bodies: Vec<Option<usize>>,
    may_supply_matches: bool,
}

impl<'a> ScreenFile<'a> {
    fn new(path: &'a Path, text: &str) -> Self {
        let tokens = lex_rust(text).tokens;
        let group_ends = pattern_group_ends(&tokens);
        let header_bodies = header_body_starts(&tokens, &group_ends);
        let may_supply_matches = file_may_supply_matches(&tokens, &group_ends);
        Self {
            path,
            tokens,
            group_ends,
            header_bodies,
            may_supply_matches,
        }
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
    cfg_test_bounds(tokens, index, |open| closing(tokens, open) + 1)
}

fn cfg_test_bounds(
    tokens: &[Token],
    index: usize,
    group_end: impl Fn(usize) -> usize,
) -> Option<Range<usize>> {
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
            position = group_end(position);
        }
    }
    if !tokens.get(position)?.is_ident("mod") || tokens.get(position + 1)?.ident().is_none() {
        return None;
    }
    let open = position + 2;
    tokens
        .get(open)?
        .is_punct('{')
        .then(|| open..group_end(open))
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

fn type_alias_violations(source: usize, types: &TypeScopes<'_>) -> Vec<Violation> {
    let file = &types.files[source];
    let tokens = &file.tokens;
    tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.is_ident("type"))
        .filter(|(index, _)| {
            TypeAlias::parse(tokens, *index, source).is_some_and(|alias| {
                alias.value.clone().any(|target| {
                    types
                        .may_be_ratatui_buffer(source, target, &mut AliasWork::Unlimited)
                        .unwrap_or(false)
                        && !associated_argument_name(tokens, target)
                        && !tokens[target].ident().is_some_and(|name| {
                            !is_path_separator(tokens, target.wrapping_sub(2))
                                && alias.type_parameter_slot(tokens, name).is_some()
                        })
                })
            })
        })
        .map(|(_, token)| Violation::at_line(CiStep::Screen, file.path, token.line, BUFFER_RENAME))
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

fn pattern_group_ends(tokens: &[Token]) -> Vec<Option<usize>> {
    let mut ends = vec![None; tokens.len()];
    let mut opens = Vec::new();
    let mut groups = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            Tok::Punct('(' | '[' | '{') => groups.push((index, opens.len())),
            Tok::Punct(')' | ']' | '}') => {
                if let Some((open, start)) = groups.pop() {
                    ends[open] = Some(index + 1);
                    opens.truncate(start);
                }
            }
            Tok::Punct('<') => opens.push(index),
            Tok::Punct('>')
                if opens.len() > groups.last().map_or(0, |&(_, start)| start)
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
    source: usize,
    declaration: usize,
    parameters: Vec<Range<usize>>,
    value: Range<usize>,
}

impl TypeAlias {
    fn parse(tokens: &[Token], index: usize, source: usize) -> Option<Self> {
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
            source,
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
        if parameters.iter().any(|range| {
            range.is_empty()
                || !(tokens[range.start].ident().is_some()
                    || tokens[range.start].kind == Tok::Lifetime)
        }) {
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
    argument_source: usize,
    elided_lifetimes: usize,
    available: usize,
}

struct TypeSpan {
    source: usize,
    range: Range<usize>,
}

impl<'a> AliasApplication<'a> {
    fn new(
        files: &[ScreenFile<'_>],
        alias: &'a TypeAlias,
        argument_source: usize,
        arguments: Vec<Range<usize>>,
        work: &mut AliasWork,
    ) -> Result<Option<Self>, AliasWorkLimit> {
        let declared =
            count_leading_lifetimes(&files[alias.source].tokens, &alias.parameters, work)?;
        let supplied = count_leading_lifetimes(&files[argument_source].tokens, &arguments, work)?;
        if supplied != 0 && supplied != declared {
            return Ok(None);
        }
        Ok(Some(Self {
            alias,
            arguments,
            argument_source,
            elided_lifetimes: declared - supplied,
            available: alias.parameters.len(),
        }))
    }
}

fn count_leading_lifetimes(
    tokens: &[Token],
    slots: &[Range<usize>],
    work: &mut AliasWork,
) -> Result<usize, AliasWorkLimit> {
    for (index, slot) in slots.iter().enumerate() {
        work.charge(1)?;
        if slot.is_empty() || tokens[slot.start].kind != Tok::Lifetime {
            return Ok(index);
        }
    }
    Ok(slots.len())
}

enum AliasWork {
    Limited(usize),
    Unlimited,
}

struct AliasWorkLimit;

impl AliasWork {
    fn permits_file_modules(&self) -> bool {
        matches!(self, Self::Limited(_))
    }

    fn charge(&mut self, units: usize) -> Result<(), AliasWorkLimit> {
        if let Self::Limited(remaining) = self {
            *remaining = remaining.checked_sub(units).ok_or(AliasWorkLimit)?;
        }
        Ok(())
    }

    fn charge_name(&mut self, name: &str) -> Result<(), AliasWorkLimit> {
        self.charge(name.len().saturating_add(1))
    }
}

enum TypeBinding<'a> {
    Alias(TypeAlias),
    RatatuiBuffer,
    Nominal,
    InlineModule(usize),
    FileModule(FileModule),
    Import(ImportPath<'a>),
    Unresolved,
}

struct ImportPath<'a> {
    segments: Vec<&'a str>,
    absolute: bool,
}

#[derive(Clone, Copy)]
struct FileModule {
    source: usize,
    directory: usize,
    matches_textual: bool,
}

#[derive(Default)]
struct TypeScope<'a> {
    source: usize,
    parent: Option<usize>,
    module: usize,
    module_parent: Option<usize>,
    directory: Option<usize>,
    self_receiver: Option<ImplReceiver>,
    bindings: BTreeMap<&'a str, TypeBinding<'a>>,
    renamed: Vec<(&'a str, u32)>,
    glob_import: bool,
    matches_imported: MatchesImport<'a>,
    matches_textual: bool,
    matches_to_parent: bool,
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
        if macro_item_disabled(tokens, index) {
            return position;
        }
        let attributed = has_outer_attribute(tokens, index);
        let macro_attributed = attributed
            && has_macro_attribute(tokens, index, |attribute| !inert_lint_attribute(attribute));
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
            if name == "matches" {
                self.matches_imported =
                    self.matches_imported
                        .include(segments, absolute, macro_attributed);
            }
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

    fn bind_item(&mut self, tokens: &'a [Token], index: usize, file_module: Option<FileModule>) {
        let Some(name) = tokens.get(index + 1).and_then(Token::ident) else {
            return;
        };
        let binding = match tokens[index].ident() {
            Some("type") => TypeAlias::parse(tokens, index, self.source)
                .map_or(TypeBinding::Unresolved, TypeBinding::Alias),
            Some("struct" | "enum" | "union")
                if nominal_attributes_preserve_binding(tokens, index) =>
            {
                TypeBinding::Nominal
            }
            Some("struct" | "enum" | "union") => TypeBinding::Unresolved,
            Some("mod") if tokens.get(index + 2).is_some_and(|t| t.is_punct(';')) => {
                file_module.map_or(TypeBinding::Unresolved, TypeBinding::FileModule)
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

fn has_macro_attribute(
    tokens: &[Token],
    declaration: usize,
    recognizes: impl Fn(&[Token]) -> bool,
) -> bool {
    let mut boundary = before_visibility(tokens, declaration);
    while let Some(attribute) = outer_attribute_before(tokens, boundary) {
        if recognizes(&tokens[attribute.start + 2..attribute.end - 1]) {
            return true;
        }
        boundary = attribute.start;
    }
    false
}

fn imports_macro_rules(tokens: &[Token], declaration: usize) -> bool {
    has_macro_attribute(tokens, declaration, macro_use_attribute)
}

fn macro_use_attribute(attribute: &[Token]) -> bool {
    matches!(attribute, [name] if name.is_keyword("macro_use"))
}

fn inert_lint_attribute(attribute: &[Token]) -> bool {
    let [name, open, .., close] = attribute else {
        return false;
    };
    matches!(&name.kind, Tok::Ident(name) if matches!(name.as_str(), "allow" | "warn" | "deny" | "forbid" | "expect"))
        && open.is_punct('(')
        && close.is_punct(')')
}

fn macro_item_disabled(tokens: &[Token], declaration: usize) -> bool {
    has_macro_attribute(tokens, declaration, |attribute| {
        let [cfg, outer, any, inner, inner_close, outer_close] = attribute else {
            return false;
        };
        cfg.is_keyword("cfg")
            && outer.is_punct('(')
            && any.is_keyword("any")
            && inner.is_punct('(')
            && inner_close.is_punct(')')
            && outer_close.is_punct(')')
    })
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
    MatchesPattern(usize, MatchesMacro),
}

#[derive(Clone, Copy)]
enum MatchesMacro {
    Prelude,
    Absolute,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum MatchesImport<'a> {
    #[default]
    Absent,
    Standard,
    RelativeStandard(&'a str),
    Unknown,
}

impl<'a> MatchesImport<'a> {
    fn include(self, segments: &[&'a str], absolute: bool, attributed: bool) -> Self {
        let observed = match (segments, absolute, attributed) {
            (["core" | "std", "matches"], true, false) => Self::Standard,
            ([root @ ("core" | "std"), "matches"], false, false) => Self::RelativeStandard(root),
            _ => Self::Unknown,
        };
        if self == Self::Absent || self == observed {
            observed
        } else {
            Self::Unknown
        }
    }
}

fn file_may_supply_matches(tokens: &[Token], ends: &[Option<usize>]) -> bool {
    let mut index = 0;
    while index < tokens.len() {
        let Some(open) = opaque_group_open(tokens, index) else {
            index += 1;
            continue;
        };
        if tokens[index].is_punct('#') {
            let Some(end) = ends[open] else {
                return true;
            };
            let attribute = &tokens[open + 1..end - 1];
            if !attribute
                .first()
                .is_some_and(|token| token.is_keyword("cfg"))
                && !inert_lint_attribute(attribute)
            {
                return true;
            }
        } else if !tokens[index - 1].is_keyword("macro_rules")
            || tokens[index + 1].is_ident("matches")
        {
            return true;
        }
        index = ends[open].unwrap_or(tokens.len());
    }
    false
}

fn matches_macro(tokens: &[Token], bang: usize) -> Option<MatchesMacro> {
    let name = bang.checked_sub(1)?;
    if !tokens[bang].is_punct('!') || !tokens[name].is_ident("matches") {
        return None;
    }
    if !is_path_separator(tokens, name.wrapping_sub(2)) {
        return Some(MatchesMacro::Prelude);
    }
    let root = name.checked_sub(3)?;
    if !matches!(tokens[root].ident(), Some("core" | "std"))
        || !is_path_separator(tokens, root.wrapping_sub(2))
        || tokens.get(root.wrapping_sub(3)).is_some_and(|token| {
            token.ident().is_some()
                && !KEYWORDS_BEFORE_PATH
                    .iter()
                    .chain(&["if", "while", "match", "break", "yield"])
                    .any(|keyword| token.is_keyword(keyword))
        })
    {
        return None;
    }
    Some(MatchesMacro::Absolute)
}

fn matches_pattern(file: &ScreenFile<'_>, arguments: Range<usize>) -> Option<Range<usize>> {
    let tokens = &file.tokens;
    let close = arguments.end - 1;
    let mut index = arguments.start + 1;
    let mut top_level = std::iter::from_fn(|| {
        let current = index;
        let token = tokens.get(current).filter(|_| current < close)?;
        index = if matches!(token.kind, Tok::Punct('(' | '[' | '{')) {
            file.group_ends[current]
                .filter(|&end| end <= close)
                .unwrap_or(close)
        } else {
            matches_angle_end(file, current, close).unwrap_or(current + 1)
        };
        Some(current)
    });
    let separator = matches_scrutinee_end(tokens, &mut top_level)?;
    let start = separator + 1;
    let end = top_level
        .find(|&at| tokens[at].is_keyword("if") || tokens[at].is_punct(','))
        .unwrap_or(close);
    (start < end).then_some(start..end)
}

fn matches_scrutinee_end(
    tokens: &[Token],
    top_level: &mut impl Iterator<Item = usize>,
) -> Option<usize> {
    let mut starts_operand = true;
    while let Some(index) = top_level.next() {
        let token = &tokens[index];
        if token.is_punct(',') {
            return Some(index);
        }
        if token.joins_next_pipe {
            top_level.next()?;
            starts_operand = true;
            continue;
        }
        if starts_operand && token.is_punct('|') {
            top_level.find(|&at| tokens[at].is_punct('|'))?;
            continue;
        }
        starts_operand = matches!(
            token.kind,
            Tok::Punct('|' | '&' | '^' | '=' | '!' | '+' | '-' | '*' | '/' | '%')
        ) || [
            "if", "while", "match", "return", "break", "yield", "move", "async",
        ]
        .iter()
        .any(|keyword| token.is_keyword(keyword));
    }
    None
}

fn matches_angle_end(file: &ScreenFile<'_>, open: usize, limit: usize) -> Option<usize> {
    let tokens = &file.tokens;
    if !tokens[open].is_punct('<') {
        return None;
    }
    let end = file.group_ends[open]?;
    (end <= limit
        && (is_path_separator(tokens, open.wrapping_sub(2)) || is_path_separator(tokens, end)))
    .then_some(end)
}

fn mark_matches_pattern(
    file: &ScreenFile<'_>,
    arguments: Range<usize>,
    position: ScopePosition,
    enclosing: &mut [Option<ScopePosition>],
) {
    let Some(pattern) = matches_pattern(file, arguments) else {
        return;
    };
    let mut index = pattern.start;
    while index < pattern.end {
        if let Some(open) = opaque_group_open(&file.tokens, index) {
            index = file.group_ends[open].unwrap_or(pattern.end);
            continue;
        }
        if let Some(end) = matches_angle_end(file, index, pattern.end) {
            index = end;
            continue;
        }
        if file.tokens[index].is_ident("Self") {
            enclosing[index] = Some(position);
        }
        index += 1;
    }
}

#[derive(Clone, Copy)]
struct ImplReceiver {
    ident: usize,
    arguments: Option<usize>,
}

#[derive(Clone, Copy)]
enum SelfReceiver {
    Inherit,
    Mask,
    Concrete(ImplReceiver),
}

impl SelfReceiver {
    fn resolve(self, parent: Option<ImplReceiver>) -> Option<ImplReceiver> {
        match self {
            Self::Inherit => parent,
            Self::Mask => None,
            Self::Concrete(target) => Some(target),
        }
    }
}

fn self_receiver_scopes(file: &ScreenFile<'_>) -> Vec<SelfReceiver> {
    let tokens = &file.tokens;
    let mut scopes = vec![SelfReceiver::Inherit; tokens.len()];
    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        if let Some(open) = opaque_group_open(tokens, index) {
            index = file.group_ends[open].unwrap_or(tokens.len());
            continue;
        }
        let is_impl = token.is_ident("impl") && starts_impl_item(tokens, index);
        let is_trait =
            token.is_ident("trait") && tokens.get(index + 1).and_then(Token::ident).is_some();
        if (is_impl || is_trait)
            && let Some(body) = file.header_bodies[index + 1]
        {
            scopes[body] = if is_impl {
                concrete_impl_receiver(file, index, body)
                    .map_or(SelfReceiver::Mask, SelfReceiver::Concrete)
            } else {
                SelfReceiver::Mask
            };
            index = body;
        }
        index += 1;
    }
    scopes
}

fn starts_impl_item(tokens: &[Token], index: usize) -> bool {
    let start = index
        - usize::from(
            tokens
                .get(index.wrapping_sub(1))
                .is_some_and(|token| token.is_ident("unsafe")),
        );
    tokens
        .get(start.wrapping_sub(1))
        .is_none_or(|token| matches!(token.kind, Tok::Punct('{' | '}' | ';' | ']')))
}

fn concrete_impl_receiver(
    file: &ScreenFile<'_>,
    declaration: usize,
    body: usize,
) -> Option<ImplReceiver> {
    let tokens = &file.tokens;
    let ends = &file.group_ends;
    let generic_start = declaration + 1;
    let trait_start = if tokens.get(generic_start)?.is_punct('<') {
        ends[generic_start]?
    } else {
        generic_start
    };
    let (_, separator) = impl_path(&tokens[..body], ends, trait_start, true)?;
    if !tokens.get(separator)?.is_ident("for") {
        return None;
    }
    let receiver_start = separator + 1;
    let (receiver, boundary, bare) = impl_receiver_path(&tokens[..body], ends, receiver_start)?;
    if boundary != body && !tokens.get(boundary)?.is_ident("where") {
        return None;
    }
    if bare && trait_start != generic_start {
        let (parameters, _) = generic_slots(&tokens[..trait_start], generic_start)?;
        let name = tokens[receiver.ident].ident()?;
        if parameters.iter().any(|range| {
            impl_type_parameter(tokens, ends, range).is_some_and(|parameter| parameter == name)
        }) {
            return None;
        }
    }
    Some(receiver)
}

fn impl_receiver_path(
    tokens: &[Token],
    ends: &[Option<usize>],
    start: usize,
) -> Option<(ImplReceiver, usize, bool)> {
    if !tokens.get(start)?.is_punct('(') {
        let (receiver, end) = impl_receiver_type(tokens, ends, start)?;
        return Some((receiver, end, receiver.ident == start));
    }
    let boundary = ends[start]?;
    if boundary > tokens.len() {
        return None;
    }
    let mut inner = start;
    let mut end = boundary;
    while inner < end && tokens[inner].is_punct('(') && ends[inner] == Some(end) {
        inner += 1;
        end -= 1;
    }
    let (receiver, inner_end) = impl_receiver_type(&tokens[..end], ends, inner)?;
    (inner_end == end).then_some((receiver, boundary, receiver.ident == inner))
}

fn impl_receiver_type(
    tokens: &[Token],
    ends: &[Option<usize>],
    start: usize,
) -> Option<(ImplReceiver, usize)> {
    let (ident, end) = impl_path(tokens, ends, start, false)?;
    let angle = end + 2 * usize::from(is_path_separator(tokens, end));
    let arguments = tokens
        .get(angle)
        .filter(|token| token.is_punct('<'))
        .map(|_| angle);
    let boundary = arguments.map_or(Some(end), |open| ends[open])?;
    (boundary <= tokens.len()).then_some((ImplReceiver { ident, arguments }, boundary))
}

fn impl_path(
    tokens: &[Token],
    ends: &[Option<usize>],
    start: usize,
    type_arguments: bool,
) -> Option<(usize, usize)> {
    let mut position = start + 2 * usize::from(is_path_separator(tokens, start));
    loop {
        tokens.get(position)?.ident()?;
        let ident = position;
        position += 1;
        if type_arguments
            && tokens
                .get(position)
                .is_some_and(|token| token.is_punct('<'))
        {
            position = ends[position]?;
        }
        if !is_path_separator(tokens, position)
            || tokens.get(position + 2).and_then(Token::ident).is_none()
        {
            return Some((ident, position));
        }
        position += 2;
    }
}

fn impl_type_parameter<'a>(
    tokens: &'a [Token],
    ends: &[Option<usize>],
    range: &Range<usize>,
) -> Option<&'a str> {
    let mut position = range.start;
    while position < range.end && tokens[position].is_punct('#') {
        position = ends.get(position + 1).copied().flatten()?;
    }
    if position >= range.end || tokens[position].is_ident("const") {
        return None;
    }
    tokens[position].ident()
}

#[derive(Default)]
enum ModuleFile {
    #[default]
    Missing,
    Unique(usize),
    Ambiguous,
}

#[derive(Default)]
struct ModuleDirectory<'a> {
    children: BTreeMap<&'a str, usize>,
    file: ModuleFile,
}

struct ModulePaths<'a> {
    directories: Vec<ModuleDirectory<'a>>,
}

impl<'a> ModulePaths<'a> {
    fn include(&mut self, file: &'a Path, source: usize) -> Option<usize> {
        let relative = file.strip_prefix(SCREEN_SOURCE).ok()?;
        let mut directory = 0;
        for part in relative.parent()? {
            directory = self.include_child(directory, part.to_str()?);
        }
        if file == Path::new(SCREEN_LIB) {
            return Some(directory);
        }
        let direct = self.include_child(directory, relative.file_stem()?.to_str()?);
        self.claim(direct, source);
        if relative.file_name()?.to_str()? == "mod.rs" {
            self.claim(directory, source);
            Some(directory)
        } else {
            Some(direct)
        }
    }

    fn claim(&mut self, directory: usize, source: usize) {
        let candidate = &mut self.directories[directory].file;
        *candidate = match candidate {
            ModuleFile::Missing => ModuleFile::Unique(source),
            _ => ModuleFile::Ambiguous,
        };
    }

    fn include_child(&mut self, directory: usize, name: &'a str) -> usize {
        if let Some(child) = self.child(directory, name) {
            return child;
        }
        let child = self.directories.len();
        self.directories.push(ModuleDirectory::default());
        self.directories[directory].children.insert(name, child);
        child
    }

    fn child(&self, directory: usize, name: &str) -> Option<usize> {
        self.directories[directory].children.get(name).copied()
    }

    fn file(&self, directory: usize, name: &str) -> Option<FileModule> {
        let directory = self.child(directory, name)?;
        match self.directories[directory].file {
            ModuleFile::Unique(source) => Some(FileModule {
                source,
                directory,
                matches_textual: false,
            }),
            ModuleFile::Missing | ModuleFile::Ambiguous => None,
        }
    }
}

#[derive(Default)]
struct SourceScopes {
    enclosing: Vec<Option<ScopePosition>>,
    inner_scopes: Range<usize>,
    connected: bool,
}

struct TypeScopes<'a> {
    files: &'a [ScreenFile<'a>],
    scopes: Vec<TypeScope<'a>>,
    sources: Vec<SourceScopes>,
    crate_root: Option<usize>,
}

impl<'a> TypeScopes<'a> {
    fn collect(files: &'a [ScreenFile<'a>]) -> Self {
        let mut paths = ModulePaths {
            directories: vec![ModuleDirectory::default()],
        };
        let scopes = files
            .iter()
            .enumerate()
            .map(|(source, file)| TypeScope {
                source,
                module: source,
                directory: paths.include(file.path, source),
                ..TypeScope::default()
            })
            .collect();
        let mut graph = Self {
            files,
            scopes,
            sources: (0..files.len()).map(|_| SourceScopes::default()).collect(),
            crate_root: files
                .iter()
                .position(|file| file.path == Path::new(SCREEN_LIB)),
        };
        if let Some(root) = graph.crate_root {
            graph.sources[root].connected = true;
            let mut pending = VecDeque::from([root]);
            while let Some(source) = pending.pop_front() {
                graph.collect_file(source, &paths);
                graph.attach_children(source, &mut pending);
            }
        }
        for source in 0..files.len() {
            if !graph.sources[source].connected {
                graph.collect_file(source, &paths);
            }
        }
        graph
    }

    fn source_scopes(&self, source: usize) -> impl Iterator<Item = usize> + use<> {
        std::iter::once(source).chain(self.sources[source].inner_scopes.clone())
    }

    fn attach_children(&mut self, source: usize, pending: &mut VecDeque<usize>) {
        let links: Vec<_> = self
            .source_scopes(source)
            .flat_map(|scope| {
                self.scopes[scope]
                    .bindings
                    .values()
                    .filter_map(move |binding| match binding {
                        TypeBinding::FileModule(child) => Some((scope, *child)),
                        _ => None,
                    })
            })
            .collect();
        for (scope, child) in links {
            if !self.sources[child.source].connected {
                self.scopes[child.source].module_parent = Some(scope);
                self.scopes[child.source].directory = Some(child.directory);
                self.scopes[child.source].matches_textual = child.matches_textual;
                self.sources[child.source].connected = true;
                pending.push_back(child.source);
            }
        }
    }

    fn collect_file(&mut self, source: usize, paths: &ModulePaths<'a>) {
        let file = &self.files[source];
        let tokens = &file.tokens;
        let receiver_scopes = self_receiver_scopes(file);
        let inner_start = self.scopes.len();
        let mut enclosing = vec![None; tokens.len()];
        let mut stack = vec![source];
        let mut index = 0;
        while let Some(token) = tokens.get(index) {
            let scope = *stack.last().unwrap_or(&source);
            if let Some(open) = opaque_group_open(tokens, index) {
                let end = file.group_ends[open].unwrap_or(tokens.len());
                enclosing[index..end].fill(Some(ScopePosition::Opaque(scope)));
                let scope_state = &mut self.scopes[scope];
                scope_state.matches_textual |= token.is_punct('!')
                    && tokens[index - 1].is_ident("macro_rules")
                    && tokens[index + 1].is_ident("matches")
                    && !macro_item_disabled(tokens, index - 1);
                if scope_state.self_receiver.is_some()
                    && let Some(kind) = matches_macro(tokens, index)
                    && (matches!(kind, MatchesMacro::Absolute) || !scope_state.matches_textual)
                    && file.group_ends[open].is_some()
                {
                    mark_matches_pattern(
                        file,
                        open..end,
                        ScopePosition::MatchesPattern(scope, kind),
                        &mut enclosing,
                    );
                }
                index = end;
                continue;
            }
            enclosing[index] = Some(ScopePosition::Rust(scope));
            if token.is_punct('{') {
                stack.push(self.enter_scope(source, index, scope, paths, receiver_scopes[index]));
            } else if token.is_punct('}') && stack.len() > 1 {
                stack.pop();
                let child = &self.scopes[scope];
                if child.matches_to_parent && child.matches_textual {
                    let parent = *stack.last().unwrap_or(&source);
                    self.scopes[parent].matches_textual = true;
                }
            } else if token.is_ident("use") {
                index = self.scopes[scope].import(tokens, index);
                continue;
            } else {
                if token.is_keyword("mod")
                    && tokens.get(index + 2).is_some_and(|end| end.is_punct(';'))
                    && imports_macro_rules(tokens, index)
                    && !macro_item_disabled(tokens, index)
                    && self.file_may_import_matches(source, scope, index, paths)
                {
                    self.scopes[scope].matches_textual = true;
                }
                let file_module = self.file_module(source, scope, index, paths);
                self.scopes[scope].bind_item(tokens, index, file_module);
            }
            index += 1;
        }
        self.sources[source].enclosing = enclosing;
        self.sources[source].inner_scopes = inner_start..self.scopes.len();
    }

    fn file_module(
        &self,
        source: usize,
        scope: usize,
        declaration: usize,
        paths: &ModulePaths<'a>,
    ) -> Option<FileModule> {
        let tokens = &self.files[source].tokens;
        let module = &self.scopes[scope];
        if !self.sources[source].connected
            || scope != module.module
            || !tokens[declaration].is_ident("mod")
            || !tokens.get(declaration + 2)?.is_punct(';')
            || has_outer_attribute(tokens, declaration)
        {
            return None;
        }
        let mut child = paths.file(module.directory?, tokens.get(declaration + 1)?.ident()?)?;
        child.matches_textual = module.matches_textual;
        Some(child)
    }

    fn file_may_import_matches(
        &self,
        source: usize,
        scope: usize,
        declaration: usize,
        paths: &ModulePaths<'a>,
    ) -> bool {
        let module = &self.scopes[scope];
        let tokens = &self.files[source].tokens;
        if !self.sources[source].connected
            || scope != module.module
            || has_macro_attribute(tokens, declaration, |attribute| {
                !macro_use_attribute(attribute) && !inert_lint_attribute(attribute)
            })
        {
            return true;
        }
        module
            .directory
            .zip(tokens.get(declaration + 1).and_then(Token::ident))
            .and_then(|(directory, name)| paths.file(directory, name))
            .is_none_or(|child| self.files[child.source].may_supply_matches)
    }

    fn enter_scope(
        &mut self,
        source: usize,
        open: usize,
        parent: usize,
        paths: &ModulePaths<'a>,
        receiver: SelfReceiver,
    ) -> usize {
        let tokens = &self.files[source].tokens;
        let child = self.scopes.len();
        let enclosing_module = self.scopes[parent].module;
        let module = open
            .checked_sub(2)
            .filter(|&index| tokens[index].is_ident("mod"))
            .and_then(|index| tokens[index + 1].ident().map(|name| (index, name)));
        let directory = module.and_then(|(declaration, name)| {
            let attributed = has_outer_attribute(tokens, declaration);
            let binding = if attributed {
                TypeBinding::Unresolved
            } else {
                TypeBinding::InlineModule(child)
            };
            self.scopes[parent].bind(name, binding);
            if attributed {
                return None;
            }
            paths.child(self.scopes[enclosing_module].directory?, name)
        });
        let self_receiver = if module.is_some() {
            None
        } else {
            receiver.resolve(self.scopes[parent].self_receiver)
        };
        self.scopes.push(TypeScope {
            source,
            parent: module.is_none().then_some(parent),
            module: if module.is_some() {
                child
            } else {
                enclosing_module
            },
            module_parent: module.map(|_| enclosing_module),
            directory,
            self_receiver,
            matches_textual: self.scopes[parent].matches_textual,
            matches_to_parent: module.is_some_and(|(declaration, _)| {
                imports_macro_rules(tokens, declaration)
                    && !macro_item_disabled(tokens, declaration)
            }),
            ..TypeScope::default()
        });
        child
    }

    fn lexical_binding(
        &self,
        mut scope: usize,
        name: &str,
        work: &mut AliasWork,
    ) -> Result<Option<(usize, &TypeBinding<'a>)>, AliasWorkLimit> {
        loop {
            work.charge_name(name)?;
            let current = &self.scopes[scope];
            if let Some(binding) = current.bindings.get(name) {
                return Ok(Some((scope, binding)));
            }
            if current.glob_import {
                return Ok(None);
            }
            let Some(parent) = current.parent else {
                return Ok(None);
            };
            scope = parent;
        }
    }

    fn super_module(
        &self,
        scope: usize,
        path: &mut VecDeque<&str>,
        work: &mut AliasWork,
    ) -> Result<Option<usize>, AliasWorkLimit> {
        let mut module = self.scopes[scope].module;
        loop {
            work.charge(1)?;
            let Some(parent) = self.scopes[module].module_parent else {
                return Ok(None);
            };
            if self.scopes[module].source != self.scopes[parent].source
                && !work.permits_file_modules()
            {
                return Ok(None);
            }
            module = parent;
            if path.front() != Some(&"super") {
                return Ok(Some(module));
            }
            path.pop_front();
        }
    }

    fn resolve(
        &self,
        scope: usize,
        segments: impl IntoIterator<Item = &'a str>,
        absolute: bool,
        work: &mut AliasWork,
    ) -> Result<Option<&TypeBinding<'a>>, AliasWorkLimit> {
        let mut path = VecDeque::new();
        for segment in segments {
            work.charge(1)?;
            path.push_back(segment);
        }
        let mut state = Some((scope, absolute, path.len()));
        let mut current = scope;
        let mut visited = BTreeSet::new();
        loop {
            work.charge(1)?;
            let edge = state.take();
            if let Some((_, absolute, edge_len)) = edge {
                if edge_len == 3
                    && path
                        .iter()
                        .take(edge_len)
                        .copied()
                        .eq(["ratatui", "buffer", "Buffer"])
                {
                    return Ok((path.len() == edge_len).then_some(&TypeBinding::RatatuiBuffer));
                }
                if absolute {
                    return Ok(None);
                }
            }
            let Some(name) = path.pop_front() else {
                return Ok(None);
            };
            let start = edge.map(|(scope, _, _)| scope);
            let (owner, binding) = match (start, name) {
                (Some(start), "crate") => {
                    let source = self.scopes[start].source;
                    let Some(root) = self.crate_root.filter(|root| {
                        source == *root
                            || self.sources[source].connected && work.permits_file_modules()
                    }) else {
                        return Ok(None);
                    };
                    current = root;
                    continue;
                }
                (Some(start), "self") => {
                    current = self.scopes[start].module;
                    continue;
                }
                (Some(start), "super") => {
                    let Some(module) = self.super_module(start, &mut path, work)? else {
                        return Ok(None);
                    };
                    current = module;
                    continue;
                }
                (Some(start), name) => {
                    let Some((owner, binding)) = self.lexical_binding(start, name, work)? else {
                        return Ok(None);
                    };
                    ((owner, name), binding)
                }
                (None, name) => {
                    work.charge_name(name)?;
                    let Some(binding) = self.scopes[current].bindings.get(name) else {
                        return Ok(None);
                    };
                    ((current, name), binding)
                }
            };
            match binding {
                TypeBinding::Import(import) => {
                    work.charge_name(owner.1)?;
                    work.charge(import.segments.len())?;
                    if !visited.insert(owner) {
                        return Ok(None);
                    }
                    for segment in import.segments.iter().rev() {
                        path.push_front(segment);
                    }
                    state = Some((owner.0, import.absolute, import.segments.len()));
                }
                _ if path.is_empty() => return Ok(Some(binding)),
                TypeBinding::InlineModule(child) => current = *child,
                TypeBinding::FileModule(child)
                    if work.permits_file_modules()
                        && self.scopes[child.source].module_parent == Some(owner.0)
                        && self.scopes[child.source].directory == Some(child.directory) =>
                {
                    current = child.source;
                }
                _ => return Ok(None),
            }
        }
    }

    fn binding_at(
        &self,
        source: usize,
        end: usize,
        work: &mut AliasWork,
    ) -> Result<Option<&TypeBinding<'a>>, AliasWorkLimit> {
        work.charge(1)?;
        let tokens = &self.files[source].tokens;
        if tokens[end].ident().is_none() {
            return Ok(None);
        }
        let Some(position) = self.sources[source].enclosing[end] else {
            return Ok(None);
        };
        let scope = match position {
            ScopePosition::Rust(scope)
            | ScopePosition::Opaque(scope)
            | ScopePosition::MatchesPattern(scope, _) => scope,
        };
        let (start, absolute) = path_start(tokens, end, work)?;
        self.resolve(
            scope,
            tokens[start..=end]
                .iter()
                .step_by(3)
                .map_while(Token::ident),
            absolute,
            work,
        )
    }

    fn applied_alias_is_ratatui_buffer(
        &self,
        argument_source: usize,
        alias: &TypeAlias,
        arguments: Vec<Range<usize>>,
        work: &mut AliasWork,
    ) -> Result<bool, AliasWorkLimit> {
        let mut value = TypeSpan {
            source: alias.source,
            range: alias.value.clone(),
        };
        let Some(application) =
            AliasApplication::new(self.files, alias, argument_source, arguments, work)?
        else {
            return Ok(false);
        };
        let mut applications = vec![application];
        loop {
            let file = &self.files[value.source];
            let tokens = &file.tokens;
            let span = &mut value.range;
            work.charge(span.len().max(1))?;
            while !Range::is_empty(span)
                && tokens[span.start].is_punct('(')
                && file.group_ends[span.start] == Some(span.end)
            {
                span.start += 1;
                span.end -= 1;
            }
            if Range::is_empty(span) {
                return Ok(false);
            }
            let first = span.start + 2 * usize::from(is_path_separator(tokens, span.start));
            let mut end = first;
            let Some(name) = tokens.get(end).and_then(Token::ident) else {
                return Ok(false);
            };
            while end + 3 < span.end
                && is_path_separator(tokens, end + 1)
                && tokens[end + 3].ident().is_some()
            {
                end += 3;
            }
            let bare_path = end + 1 == span.end;
            let parameter = if let Some(current) = applications
                .last_mut()
                .filter(|_| bare_path && span.start == end)
            {
                work.charge(current.alias.parameters.len())?;
                let alias_tokens = &self.files[current.alias.source].tokens;
                current
                    .alias
                    .type_parameter_slot(alias_tokens, name)
                    .map(|slot| (current, slot))
            } else {
                None
            };
            if let Some((current, slot)) = parameter {
                if slot >= current.available {
                    return Ok(false);
                }
                let Some(argument_slot) = slot.checked_sub(current.elided_lifetimes) else {
                    return Ok(false);
                };
                if let Some(argument) = current.arguments.get(argument_slot) {
                    value = TypeSpan {
                        source: current.argument_source,
                        range: argument.clone(),
                    };
                    applications.truncate(applications.len() - 1);
                    continue;
                }
                work.charge(slot + 1 + current.alias.parameters[slot].len())?;
                let alias_tokens = &self.files[current.alias.source].tokens;
                let Some(default) = current.alias.type_parameter_default(alias_tokens, slot) else {
                    return Ok(false);
                };
                value = TypeSpan {
                    source: current.alias.source,
                    range: default,
                };
                current.available = slot;
                continue;
            }
            let alias = match self.binding_at(value.source, end, work)? {
                Some(TypeBinding::RatatuiBuffer) if bare_path => return Ok(true),
                Some(TypeBinding::Alias(alias)) => alias,
                _ => return Ok(false),
            };
            let slots = if bare_path {
                Vec::new()
            } else {
                let arguments = end + 1 + 2 * usize::from(is_path_separator(tokens, end + 1));
                let Some((slots, boundary)) = generic_slots(&tokens[..span.end], arguments) else {
                    return Ok(false);
                };
                if boundary != span.end {
                    return Ok(false);
                }
                slots
            };
            work.charge(applications.len())?;
            if applications.iter().any(|active| {
                active.alias.source == alias.source && active.alias.declaration == alias.declaration
            }) {
                return Ok(false);
            }
            let Some(application) =
                AliasApplication::new(self.files, alias, value.source, slots, work)?
            else {
                return Ok(false);
            };
            value = TypeSpan {
                source: alias.source,
                range: alias.value.clone(),
            };
            applications.push(application);
        }
    }

    fn may_be_ratatui_buffer(
        &self,
        source: usize,
        index: usize,
        work: &mut AliasWork,
    ) -> Result<bool, AliasWorkLimit> {
        let tokens = &self.files[source].tokens;
        Ok(match self.binding_at(source, index, work)? {
            Some(TypeBinding::RatatuiBuffer) => true,
            Some(TypeBinding::Nominal)
                if matches!(
                    self.sources[source].enclosing[index],
                    Some(ScopePosition::Rust(_))
                ) =>
            {
                false
            }
            _ => {
                tokens[index].is_ident("Buffer")
                    && !is_path_separator(tokens, index.wrapping_sub(2))
            }
        })
    }

    fn self_is_ratatui_buffer(
        &self,
        source: usize,
        index: usize,
        work: &mut AliasWork,
    ) -> Result<bool, AliasWorkLimit> {
        work.charge(1)?;
        let scope = match self.sources[source].enclosing[index] {
            Some(
                ScopePosition::Rust(scope)
                | ScopePosition::MatchesPattern(scope, MatchesMacro::Absolute),
            ) => scope,
            Some(ScopePosition::MatchesPattern(scope, MatchesMacro::Prelude))
                if self.matches_prelude_visible(scope, work)? =>
            {
                scope
            }
            _ => return Ok(false),
        };
        let Some(receiver) = self.scopes[scope].self_receiver else {
            return Ok(false);
        };
        let binding = self.binding_at(source, receiver.ident, work)?;
        match (binding, receiver.arguments) {
            (Some(TypeBinding::RatatuiBuffer), None) => Ok(true),
            (Some(TypeBinding::Alias(alias)), arguments) => {
                self.alias_is_ratatui_buffer(source, alias, arguments, work)
            }
            _ => Ok(false),
        }
    }

    fn matches_prelude_visible(
        &self,
        mut scope: usize,
        work: &mut AliasWork,
    ) -> Result<bool, AliasWorkLimit> {
        loop {
            work.charge(1)?;
            let current = &self.scopes[scope];
            match current.matches_imported {
                MatchesImport::Standard => return Ok(true),
                MatchesImport::RelativeStandard(root) => {
                    return self.matches_standard_root_visible(scope, root, work);
                }
                MatchesImport::Unknown => return Ok(false),
                MatchesImport::Absent if current.glob_import => return Ok(false),
                MatchesImport::Absent => {}
            }
            let Some(parent) = current.parent else {
                return Ok(true);
            };
            scope = parent;
        }
    }

    fn matches_standard_root_visible(
        &self,
        mut scope: usize,
        name: &str,
        work: &mut AliasWork,
    ) -> Result<bool, AliasWorkLimit> {
        loop {
            work.charge_name(name)?;
            let current = &self.scopes[scope];
            if current.glob_import || current.bindings.contains_key(name) {
                return Ok(false);
            }
            let Some(parent) = current.parent else {
                return Ok(true);
            };
            scope = parent;
        }
    }

    fn alias_is_ratatui_buffer(
        &self,
        source: usize,
        alias: &TypeAlias,
        start: Option<usize>,
        work: &mut AliasWork,
    ) -> Result<bool, AliasWorkLimit> {
        let file = &self.files[source];
        let slots = if let Some(start) = start {
            let Some(end) = file.group_ends[start] else {
                return Ok(false);
            };
            work.charge(end - start)?;
            let Some((slots, _)) = generic_slots(&file.tokens[..end], start) else {
                return Ok(false);
            };
            slots
        } else {
            Vec::new()
        };
        self.applied_alias_is_ratatui_buffer(source, alias, slots, work)
    }

    fn renamed_imports(&self, source: usize) -> Vec<Violation> {
        let path = self.files[source].path;
        self.source_scopes(source)
            .flat_map(|scope| {
                self.scopes[scope]
                    .renamed
                    .iter()
                    .filter(move |&&(name, _)| {
                        matches!(
                            self.resolve(scope, [name], false, &mut AliasWork::Unlimited),
                            Ok(Some(TypeBinding::RatatuiBuffer))
                        )
                    })
                    .map(move |&(_, line)| {
                        Violation::at_line(CiStep::Screen, path, line, BUFFER_RENAME)
                    })
            })
            .collect()
    }
}

fn path_start(
    tokens: &[Token],
    end: usize,
    work: &mut AliasWork,
) -> Result<(usize, bool), AliasWorkLimit> {
    let mut start = end;
    while is_path_separator(tokens, start.wrapping_sub(2)) {
        work.charge(1)?;
        let preceding = start.checked_sub(3).and_then(|at| tokens[at].ident());
        if preceding.is_none_or(|name| KEYWORDS_BEFORE_PATH.contains(&name)) {
            return Ok((start, true));
        }
        start -= 3;
    }
    Ok((start, false))
}

struct FieldPatternIndex<'a> {
    ends: &'a [Option<usize>],
    excluded: Vec<bool>,
    fields: Vec<bool>,
    type_prefixes: Vec<bool>,
}

impl<'a> FieldPatternIndex<'a> {
    fn new(file: &'a ScreenFile<'_>) -> Self {
        let tokens = &file.tokens;
        let ends = &file.group_ends;
        let excluded = field_pattern_exclusions(file);
        let fields = field_prefixes(tokens, ends);
        let type_prefixes = pattern_type_prefixes(tokens);
        Self {
            ends,
            excluded,
            fields,
            type_prefixes,
        }
    }

    fn is_field_pattern(&self, tokens: &[Token], index: usize, open: usize) -> bool {
        tokens.get(open).is_some_and(|t| t.is_punct('{'))
            && !self.excluded[index]
            && self.fields[open + 1]
            && (!self.type_prefixes[index]
                || self.ends[open]
                    .and_then(|end| tokens.get(end))
                    .is_some_and(|t| t.is_ident("in")))
    }
}

fn buffer_pattern_matches(
    source: usize,
    index: usize,
    types: &TypeScopes<'_>,
    patterns: &FieldPatternIndex<'_>,
    work_limit: usize,
) -> Result<bool, AliasWorkLimit> {
    let tokens = &types.files[source].tokens;
    let mut work = AliasWork::Limited(work_limit);
    if tokens.get(index + 1).is_some_and(|t| t.is_punct('{')) {
        if !patterns.is_field_pattern(tokens, index, index + 1) {
            return Ok(false);
        }
        if tokens[index].is_ident("Self") && !is_path_separator(tokens, index.wrapping_sub(2)) {
            return types.self_is_ratatui_buffer(source, index, &mut work);
        }
        return types.may_be_ratatui_buffer(source, index, &mut work);
    }
    let start = index + 3;
    if !is_path_separator(tokens, index + 1) || !tokens.get(start).is_some_and(|t| t.is_punct('<'))
    {
        return Ok(false);
    }
    let Some(open) = patterns.ends[start] else {
        return Ok(false);
    };
    if !patterns.is_field_pattern(tokens, index, open) {
        return Ok(false);
    }
    let Some(TypeBinding::Alias(alias)) = types.binding_at(source, index, &mut work)? else {
        return Ok(false);
    };
    types.alias_is_ratatui_buffer(source, alias, Some(start), &mut work)
}

fn pattern_violations(
    source: usize,
    aliases: &TypeScopes<'_>,
    work_limit: usize,
) -> Vec<Violation> {
    let file = &aliases.files[source];
    let tokens = &file.tokens;
    let patterns = FieldPatternIndex::new(file);
    (1..tokens.len())
        .filter_map(|index| {
            let message = match buffer_pattern_matches(source, index, aliases, &patterns, work_limit) {
                Ok(true) => "C1: destructures a ratatui Buffer; its cells are public fields, and jackin-tui draws nothing",
                Ok(false) => return None,
                Err(AliasWorkLimit) => "C1: Screen alias resolution exceeded its work limit for this pattern",
            };
            Some(Violation::at_line(CiStep::Screen, file.path, tokens[index].line, message))
        })
        .collect()
}

fn field_pattern_exclusions(file: &ScreenFile<'_>) -> Vec<bool> {
    let tokens = &file.tokens;
    let ends = &file.group_ends;
    let mut test_ends = vec![0; tokens.len()];
    for index in 0..tokens.len() {
        if let Some(test) =
            cfg_test_bounds(tokens, index, |open| ends[open].unwrap_or(tokens.len()))
        {
            test_ends[test.start] = test_ends[test.start].max(test.end);
        }
    }
    let mut excluded = Vec::with_capacity(tokens.len());
    let mut test_end = 0;
    let mut braces = Vec::new();
    let enum_bodies = enum_body_starts(file);
    for (index, token) in tokens.iter().enumerate() {
        test_end = test_end.max(test_ends[index]);
        excluded.push(index < test_end || braces.last().copied().unwrap_or(false));
        if token.is_punct('{') {
            braces.push(enum_bodies[index]);
        } else if token.is_punct('}') {
            braces.truncate(braces.len().saturating_sub(1));
        }
    }
    excluded
}

fn header_body_starts(tokens: &[Token], ends: &[Option<usize>]) -> Vec<Option<usize>> {
    let mut where_bodies = vec![None; tokens.len() + 1];
    for index in (0..tokens.len()).rev() {
        let token = &tokens[index];
        where_bodies[index] = match token.kind {
            Tok::Punct('{')
                if index
                    .checked_sub(1)
                    .and_then(|bang| opaque_group_open(tokens, bang))
                    != Some(index) =>
            {
                Some(index)
            }
            Tok::Punct(';' | '}' | ')' | ']') => None,
            Tok::Punct('{' | '<' | '(' | '[') => ends[index].and_then(|end| where_bodies[end]),
            _ => where_bodies[index + 1],
        };
    }
    where_bodies
}

fn enum_body_starts(file: &ScreenFile<'_>) -> Vec<bool> {
    let tokens = &file.tokens;
    let mut bodies = vec![false; tokens.len()];
    for (index, token) in tokens.iter().enumerate() {
        if token.is_ident("enum")
            && let Some(body) =
                enum_body_after_name(tokens, &file.group_ends, &file.header_bodies, index)
        {
            bodies[body] = true;
        }
    }
    bodies
}

fn enum_body_after_name(
    tokens: &[Token],
    ends: &[Option<usize>],
    where_bodies: &[Option<usize>],
    declaration: usize,
) -> Option<usize> {
    tokens.get(declaration + 1)?.ident()?;
    let after_name = declaration + 2;
    let boundary = if tokens.get(after_name)?.is_punct('<') {
        ends[after_name]?
    } else {
        after_name
    };
    let token = tokens.get(boundary)?;
    if token.is_ident("where") {
        where_bodies[boundary + 1]
    } else {
        token.is_punct('{').then_some(boundary)
    }
}

fn field_prefixes(tokens: &[Token], ends: &[Option<usize>]) -> Vec<bool> {
    let mut fields = vec![false; tokens.len() + 1];
    for index in (0..tokens.len()).rev() {
        let token = &tokens[index];
        let next = tokens.get(index + 1);
        fields[index] = if token.is_punct('#') && next.is_some_and(|t| t.is_punct('[')) {
            ends[index + 1].is_some_and(|end| fields[end])
        } else if token.is_ident("ref") || token.is_ident("mut") {
            fields[index + 1]
        } else {
            (token.ident().is_some()
                && next.is_some_and(|t| t.is_punct(',') || t.is_punct(':') || t.is_punct('}')))
                || (token.is_punct('.') && next.is_some_and(|t| t.is_punct('.')))
        };
    }
    fields
}

fn pattern_type_prefixes(tokens: &[Token]) -> Vec<bool> {
    let mut path_starts = Vec::with_capacity(tokens.len());
    let mut prefixes = Vec::with_capacity(tokens.len());
    let mut types = Vec::with_capacity(tokens.len());
    for index in 0..tokens.len() {
        let start = if index >= 3
            && is_path_separator(tokens, index - 2)
            && tokens[index - 3].ident().is_some()
        {
            path_starts[index - 3]
        } else {
            index
        };
        path_starts.push(start);
        let prefix = if index > 0 && is_pattern_type_qualifier(&tokens[index - 1]) {
            prefixes[index - 1]
        } else {
            index
        };
        prefixes.push(prefix);
        let start = if start >= 2 && is_path_separator(tokens, start - 2) {
            start - 2
        } else {
            start
        };
        let start = prefixes[start];
        types.push(start.checked_sub(1).is_some_and(|before| {
            let token = &tokens[before];
            (token.is_punct('>') && before > 0 && tokens[before - 1].is_punct('-'))
                || NON_PATTERN_KEYWORDS
                    .iter()
                    .any(|keyword| token.is_ident(keyword))
        }));
    }
    types
}

fn is_pattern_type_qualifier(token: &Token) -> bool {
    token.is_punct('&')
        || token.is_punct('*')
        || token.is_ident("mut")
        || token.is_ident("const")
        || matches!(token.kind, Tok::Other | Tok::Lifetime)
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
             pub type Identity<T> = T;\n\
             mod aliases { pub use super::Identity as Forward; }\n\
             fn view(buf: Buffer, imported: Buffer) {\n\
             \x20   let _: Option<Identity::<Buffer>> = None;\n\
             \x20   let Identity::<Buffer> { content, .. } = buf;\n\
             \x20   let aliases::Forward::<Buffer> { content, .. } = imported;\n\
             }\n",
        )?;
        let repo = Repo::open(root.to_owned())?;
        let normal = ScreenCheck::with_allowed(&[]);
        let import_limited = ScreenCheck {
            alias_work_limit: 48,
            ..normal
        };
        let capacity_limited = ScreenCheck {
            alias_work_limit: 3,
            ..normal
        };
        let path = Path::new(SCREEN_LIB);
        let destructures = |line| {
            Violation::at_line(
                CiStep::Screen,
                path,
                line,
                "C1: destructures a ratatui Buffer; its cells are public fields, and jackin-tui draws nothing",
            )
        };
        let exhausted = |line| {
            Violation::at_line(
                CiStep::Screen,
                path,
                line,
                "C1: Screen alias resolution exceeded its work limit for this pattern",
            )
        };
        assert_eq!(normal.run(&repo), [destructures(6), destructures(7)]);
        assert_eq!(import_limited.run(&repo), [destructures(6), exhausted(7)]);
        assert_eq!(capacity_limited.run(&repo), [exhausted(6), exhausted(7)]);
        Ok(())
    }
}
