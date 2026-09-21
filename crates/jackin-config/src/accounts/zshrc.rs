// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Static literal-only `.zshrc`/shell-env importer.
//!
//! Parses `VAR=...` assignments (with optional `export`/`typeset`/`declare`
//! prefixes and simple single/double quoting) for account-relevant variables
//! only. Anything dynamic — command substitutions, function calls, `op read`
//! invocations, or expansions that cannot be resolved without running a shell —
//! is reported as a typed [`UnresolvedEntry`] instead of a value.
//!
//! This importer never executes anything: it spawns no shells, no helpers, and
//! reads no process state. Consumers resolve [`UnresolvedEntry`] values with
//! the operator. Parsing is total, so there is no `ConfigResult` surface here;
//! file loading stays with the caller (`ConfigError::Io` on failure).
//!
//! Later assignments win: a repeated literal overwrites the earlier value,
//! while unresolved entries accumulate in source order.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::{WrapperSpec, XdgRoots};
use jackin_core::{Agent, OpRef, parse_op_reference};

/// Why a shell assignment could not be resolved to a static literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnresolvedKind {
    /// `$(...)`, `` `...` ``, or `<(...)` command substitution.
    CommandSubstitution,
    /// Substitution invoking a shell function defined in the same source.
    FunctionCall,
    /// Substitution invoking the 1Password CLI (`op read ...`).
    OpRead,
    /// `$VAR`, `${VAR}`, `$((...))`, leading `~`, globs, appends, or arrays.
    UnresolvableExpansion,
}

/// One account-relevant assignment that needs operator attention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedEntry {
    /// 1-based number of the logical source line.
    pub line: usize,
    /// Assigned variable name.
    pub name: String,
    /// Dynamic construct class.
    pub kind: UnresolvedKind,
    /// Short source snippet of the offending construct (truncated).
    pub detail: String,
}

/// Result of [`parse_zshrc_source`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZshrcImport {
    /// Resolved literal values for account-relevant variables.
    pub values: BTreeMap<String, String>,
    /// Dynamic constructs found in account-relevant assignments, in order.
    pub unresolved: Vec<UnresolvedEntry>,
}

/// Whether a variable name is relevant to account import.
///
/// Matches config-dir overrides (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`), XDG
/// roots, `*_API_KEY`/`*_BASE_URL` families and their aliases, model
/// variables, profile names, and the Claude OAuth token.
pub fn is_account_relevant(name: &str) -> bool {
    const EXACT: &[&str] = &[
        "CLAUDE_CONFIG_DIR",
        "CODEX_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
        "CLAUDE_CODE_OAUTH_TOKEN",
    ];
    if EXACT.contains(&name) {
        return true;
    }
    if name.contains("MODEL") {
        return true;
    }
    [
        "_API_KEY",
        "_API_TOKEN",
        "_AUTH_TOKEN",
        "_OAUTH_TOKEN",
        "_BASE_URL",
        "_API_BASE",
        "_API_URL",
        "_PROFILE",
        "_PROFILE_NAME",
    ]
    .iter()
    .any(|suffix| name.ends_with(suffix))
}

/// Parse shell source into literal values plus typed unresolved entries.
///
/// Only account-relevant assignments (see [`is_account_relevant`]) are
/// reported; every other line is ignored, even when dynamic.
pub fn parse_zshrc_source(source: &str) -> ZshrcImport {
    let logicals = logical_lines(source);
    let functions = collect_function_names(&logicals);
    let mut import = ZshrcImport::default();
    for (line_no, logical) in &logicals {
        let code = strip_comment(logical);
        for statement in split_statements(&code) {
            parse_statement(&statement, *line_no, &functions, &mut import);
        }
    }
    import
}

/// One dynamic construct found inside an assignment value.
struct Detection {
    kind: UnresolvedKind,
    detail: String,
}

/// Join physical lines into backslash-continued logical lines with start numbers.
fn logical_lines(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut start = 1;
    let mut open = false;
    for (index, physical) in source.lines().enumerate() {
        let line = physical.strip_suffix('\r').unwrap_or(physical);
        if !open {
            open = true;
            start = index + 1;
        }
        if has_continuation(line) {
            let mut without = line.to_owned();
            without.pop();
            current.push_str(&without);
        } else {
            current.push_str(line);
            out.push((start, std::mem::take(&mut current)));
            open = false;
        }
    }
    if open {
        out.push((start, current));
    }
    out
}

/// Whether the line ends in an odd run of backslashes (line continuation).
fn has_continuation(line: &str) -> bool {
    let mut count = 0;
    for c in line.chars().rev() {
        if c == '\\' {
            count += 1;
        } else {
            break;
        }
    }
    count % 2 == 1
}

/// Collect `name() ...` / `function name ...` definitions for call classification.
fn collect_function_names(logicals: &[(usize, String)]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for (_, logical) in logicals {
        let code = strip_comment(logical);
        let trimmed = code.trim();
        let mut rest = trimmed;
        let mut had_keyword = false;
        if let Some(after) = trimmed
            .strip_prefix("function")
            .filter(|r| r.starts_with(char::is_whitespace))
        {
            rest = after.trim_start();
            had_keyword = true;
        }
        let mut ident = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(&c) = chars.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                ident.push(c);
                chars.next();
            } else {
                break;
            }
        }
        if ident.is_empty()
            || !ident
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            continue;
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let next = chars.peek().copied();
        if next == Some('(') || (had_keyword && (next == Some('{') || next.is_none())) {
            names.insert(ident);
        }
    }
    names
}

/// Strip a `#` comment; `#` is literal inside quotes and mid-word.
fn strip_comment(line: &str) -> String {
    let mut out = String::new();
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    let mut at_word_start = true;
    for c in line.chars() {
        if escaped {
            out.push(c);
            escaped = false;
            at_word_start = false;
            continue;
        }
        match c {
            '\\' if !single => {
                escaped = true;
                out.push(c);
                at_word_start = false;
            }
            '\'' if !double => {
                single = !single;
                out.push(c);
                at_word_start = false;
            }
            '"' if !single => {
                double = !double;
                out.push(c);
                at_word_start = false;
            }
            '#' if !single && !double && at_word_start => break,
            _ => {
                if single || double {
                    at_word_start = false;
                } else {
                    at_word_start = c.is_whitespace() || matches!(c, ';' | '(' | ')' | '|' | '&');
                }
                out.push(c);
            }
        }
    }
    out
}

/// Split a logical line on unquoted `;` statement separators.
fn split_statements(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut single = false;
    let mut double = false;
    let mut backtick = false;
    let mut escaped = false;
    let mut depth: u32 = 0;
    for c in code.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' if !single => {
                escaped = true;
                current.push(c);
            }
            '\'' if !double && !backtick => {
                single = !single;
                current.push(c);
            }
            '"' if !single && !backtick => {
                double = !double;
                current.push(c);
            }
            '`' if !single && !double => {
                backtick = !backtick;
                current.push(c);
            }
            ';' if !single && !double && !backtick && depth == 0 => {
                out.push(std::mem::take(&mut current));
            }
            '(' | '{' | '[' if !single && !double && !backtick => {
                depth += 1;
                current.push(c);
            }
            ')' | '}' | ']' if !single && !double && !backtick => {
                depth = depth.saturating_sub(1);
                current.push(c);
            }
            _ => current.push(c),
        }
    }
    out.push(current);
    out
}

/// Split a statement on whitespace outside quotes; quotes stay in the words.
///
/// Hand-rolled deliberately: the words feed the `UnresolvedKind`
/// classifier below, which needs the original quoting to tell substitution
/// from literal text. Quote-stripping tokenizers (`shell-words`) have an
/// API-awkward-for-this-call-site contract, and no crate classifies
/// `UnresolvedKind` — so this splitter stays local (ENGINEERING.md escape).
fn split_words(statement: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut single = false;
    let mut double = false;
    let mut backtick = false;
    let mut escaped = false;
    let mut depth: u32 = 0;
    let mut in_word = false;
    for c in statement.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' if !single => {
                escaped = true;
                in_word = true;
                current.push(c);
            }
            '\'' if !double && !backtick => {
                single = !single;
                in_word = true;
                current.push(c);
            }
            '"' if !single && !backtick => {
                double = !double;
                in_word = true;
                current.push(c);
            }
            '`' if !single && !double => {
                backtick = !backtick;
                in_word = true;
                current.push(c);
            }
            _ if c.is_whitespace() && !single && !double && !backtick && depth == 0 => {
                if in_word {
                    out.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            _ => {
                if !single && !double && !backtick {
                    match c {
                        '(' | '{' | '[' => depth += 1,
                        ')' | '}' | ']' => depth = depth.saturating_sub(1),
                        _ => {}
                    }
                }
                in_word = true;
                current.push(c);
            }
        }
    }
    if in_word {
        out.push(current);
    }
    out
}

/// Parse one statement's assignment words into the import result.
fn parse_statement(
    statement: &str,
    line: usize,
    functions: &BTreeSet<String>,
    import: &mut ZshrcImport,
) {
    let mut words = split_words(statement).into_iter().peekable();
    loop {
        match words.peek() {
            Some(word) if is_decl_keyword(word) || is_flag_word(word) => {
                words.next();
            }
            _ => break,
        }
    }
    for word in words {
        let Some((raw_name, raw_value)) = split_assignment(&word) else {
            continue;
        };
        let (name, append) = strip_append(raw_name);
        if !is_identifier(&name) || !is_account_relevant(&name) {
            continue;
        }
        if append {
            import.unresolved.push(UnresolvedEntry {
                line,
                name,
                kind: UnresolvedKind::UnresolvableExpansion,
                detail: "+= append".to_owned(),
            });
            continue;
        }
        if raw_value.starts_with('(') {
            import.unresolved.push(UnresolvedEntry {
                line,
                name,
                kind: UnresolvedKind::UnresolvableExpansion,
                detail: "array assignment".to_owned(),
            });
            continue;
        }
        let (literal, detections) = analyze_value(&raw_value, functions);
        if detections.is_empty() {
            if let Some(value) = literal {
                import.values.insert(name, value);
            }
        } else {
            for detection in detections {
                import.unresolved.push(UnresolvedEntry {
                    line,
                    name: name.clone(),
                    kind: detection.kind,
                    detail: detection.detail,
                });
            }
        }
    }
}

/// Declaration keywords skipped before assignments (`export FOO=...`).
fn is_decl_keyword(word: &str) -> bool {
    matches!(
        word,
        "export" | "typeset" | "declare" | "local" | "readonly"
    )
}

/// Flag words skipped before assignments (`typeset -x FOO=...`).
fn is_flag_word(word: &str) -> bool {
    (word.starts_with('-') || word.starts_with('+')) && word.len() > 1 && !word.contains('=')
}

/// Split a word on the first unquoted `=` into name and raw value.
fn split_assignment(word: &str) -> Option<(String, String)> {
    let mut name = String::new();
    let mut value: Option<String> = None;
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    for c in word.chars() {
        if let Some(current) = value.as_mut() {
            current.push(c);
            continue;
        }
        if escaped {
            name.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' if !single => {
                escaped = true;
                name.push(c);
            }
            '\'' if !double => {
                single = !single;
                name.push(c);
            }
            '"' if !single => {
                double = !double;
                name.push(c);
            }
            '=' if !single && !double => value = Some(String::new()),
            _ => name.push(c),
        }
    }
    value.map(|v| (name, v))
}

/// Strip a `+=` append marker from an assignment name.
fn strip_append(raw_name: String) -> (String, bool) {
    if raw_name.ends_with('+') {
        let mut name = raw_name;
        name.pop();
        (name, true)
    } else {
        (raw_name, false)
    }
}

/// Whether the name is a valid shell identifier.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Unquote a raw value; any dynamic construct voids the literal.
fn analyze_value(raw: &str, functions: &BTreeSet<String>) -> (Option<String>, Vec<Detection>) {
    let mut literal = String::new();
    let mut detections = Vec::new();
    let mut chars = raw.chars().peekable();
    let mut single = false;
    let mut double = false;
    let mut at_start = true;
    while let Some(c) = chars.next() {
        if single {
            if c == '\'' {
                single = false;
            } else {
                literal.push(c);
            }
            at_start = false;
            continue;
        }
        if double {
            if c == '"' {
                double = false;
                at_start = false;
                continue;
            }
            if c == '\\' {
                match chars.next() {
                    Some(next @ ('$' | '`' | '"' | '\\')) => literal.push(next),
                    Some(next) => {
                        literal.push('\\');
                        literal.push(next);
                    }
                    None => literal.push('\\'),
                }
                at_start = false;
                continue;
            }
            if c == '\'' {
                literal.push(c);
                at_start = false;
                continue;
            }
        } else {
            if c == '\'' {
                single = true;
                at_start = false;
                continue;
            }
            if c == '"' {
                double = true;
                at_start = false;
                continue;
            }
            if c == '\\' {
                if let Some(next) = chars.next() {
                    literal.push(next);
                }
                at_start = false;
                continue;
            }
            if c == '~' && at_start {
                detections.push(Detection {
                    kind: UnresolvedKind::UnresolvableExpansion,
                    detail: capture_tilde(&mut chars),
                });
                at_start = false;
                continue;
            }
            if matches!(c, '*' | '?' | '[') {
                detections.push(Detection {
                    kind: UnresolvedKind::UnresolvableExpansion,
                    detail: format!("glob {c:?}"),
                });
                at_start = false;
                continue;
            }
            if matches!(c, '<' | '>' | '=') && chars.peek() == Some(&'(') {
                chars.next();
                let mut prefix = String::new();
                prefix.push(c);
                prefix.push('(');
                let detail = capture_balanced(&mut chars, '(', ')', &prefix, 1);
                detections.push(Detection {
                    kind: UnresolvedKind::CommandSubstitution,
                    detail: truncate_detail(detail),
                });
                at_start = false;
                continue;
            }
        }
        if c == '$' {
            analyze_dollar(&mut chars, functions, &mut literal, &mut detections);
            at_start = false;
            continue;
        }
        if c == '`' {
            let captured = capture_backtick(&mut chars);
            let kind = classify_command(&captured, functions);
            detections.push(Detection {
                kind,
                detail: truncate_detail(captured),
            });
            at_start = false;
            continue;
        }
        literal.push(c);
        at_start = false;
    }
    if detections.is_empty() {
        (Some(literal), detections)
    } else {
        (None, detections)
    }
}

/// Handle one `$...` construct: substitutions and expansions void the
/// literal, while a bare `$` stays literal.
fn analyze_dollar(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    functions: &BTreeSet<String>,
    literal: &mut String,
    detections: &mut Vec<Detection>,
) {
    match chars.peek().copied() {
        Some('(') => {
            chars.next();
            if chars.peek() == Some(&'(') {
                chars.next();
                let detail = capture_balanced(chars, '(', ')', "$((", 2);
                detections.push(Detection {
                    kind: UnresolvedKind::UnresolvableExpansion,
                    detail: truncate_detail(detail),
                });
            } else {
                let captured = capture_balanced(chars, '(', ')', "$(", 1);
                let kind = classify_command(&captured, functions);
                detections.push(Detection {
                    kind,
                    detail: truncate_detail(captured),
                });
            }
        }
        Some('{') => {
            chars.next();
            let detail = capture_balanced(chars, '{', '}', "${", 1);
            detections.push(Detection {
                kind: UnresolvedKind::UnresolvableExpansion,
                detail: truncate_detail(detail),
            });
        }
        Some(next) if next.is_ascii_alphabetic() || next == '_' || next.is_ascii_digit() => {
            let detail = capture_dollar_name(chars);
            detections.push(Detection {
                kind: UnresolvedKind::UnresolvableExpansion,
                detail,
            });
        }
        Some(next) if matches!(next, '*' | '@' | '#' | '?' | '!' | '-') => {
            chars.next();
            let mut detail = String::from("$");
            detail.push(next);
            detections.push(Detection {
                kind: UnresolvedKind::UnresolvableExpansion,
                detail,
            });
        }
        _ => literal.push('$'),
    }
}

/// Capture a balanced bracket run into a detail snippet.
fn capture_balanced(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    open: char,
    close: char,
    prefix: &str,
    mut depth: u32,
) -> String {
    let mut detail = String::from(prefix);
    let mut single = false;
    let mut double = false;
    let mut escaped = false;
    for c in chars.by_ref() {
        detail.push(c);
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && !single {
            escaped = true;
            continue;
        }
        if c == '\'' && !double {
            single = !single;
            continue;
        }
        if c == '"' && !single {
            double = !double;
            continue;
        }
        if single || double {
            continue;
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
    }
    detail
}

/// Capture a backquote run (with backslash escapes) into a detail snippet.
fn capture_backtick(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut detail = String::from("`");
    let mut escaped = false;
    for c in chars.by_ref() {
        detail.push(c);
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == '`' {
            break;
        }
    }
    detail
}

/// Capture `$NAME` / `$9` into a detail snippet (`$` already consumed).
fn capture_dollar_name(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut detail = String::from("$");
    if chars.peek().is_some_and(char::is_ascii_digit) {
        if let Some(digit) = chars.next() {
            detail.push(digit);
        }
        return detail;
    }
    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphanumeric() || c == '_' {
            detail.push(c);
            chars.next();
        } else {
            break;
        }
    }
    detail
}

/// Capture a leading `~` / `~user` into a detail snippet (`~` already consumed).
fn capture_tilde(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut detail = String::from("~");
    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
            detail.push(c);
            chars.next();
        } else {
            break;
        }
    }
    detail
}

/// Classify a captured substitution by its invoked command.
fn classify_command(captured: &str, functions: &BTreeSet<String>) -> UnresolvedKind {
    let inner = captured
        .strip_prefix("$(")
        .or_else(|| captured.strip_prefix('`'))
        .unwrap_or(captured);
    let inner = inner
        .strip_suffix(')')
        .or_else(|| inner.strip_suffix('`'))
        .unwrap_or(inner);
    let mut words = inner.split_whitespace();
    let first = words.next().unwrap_or("");
    let second = words.next().unwrap_or("");
    if first == "op"
        || ((first == "sudo" || first == "env" || first == "command") && second == "op")
    {
        UnresolvedKind::OpRead
    } else if functions.contains(first) {
        UnresolvedKind::FunctionCall
    } else {
        UnresolvedKind::CommandSubstitution
    }
}

/// Truncate a detail snippet to a stable short length.
fn truncate_detail(detail: String) -> String {
    const MAX_CHARS: usize = 64;
    if detail.chars().count() <= MAX_CHARS {
        detail
    } else {
        let mut short: String = detail.chars().take(MAX_CHARS).collect();
        short.push('\u{2026}');
        short
    }
}

/// Agent-attributed configuration directory from a config-dir override.
///
/// Maps onto `AccountCredential::Profile { agent, directory }`: the consumer
/// pairs the candidate with a provider to seed a profile account. Only
/// absolute paths are extracted (relative values cannot seed a profile).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryCandidate {
    /// Agent owning this directory's storage format.
    pub agent: Agent,
    /// Absolute configuration directory.
    pub directory: PathBuf,
    /// Source variable (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`).
    pub source_var: String,
}

/// Parsed `op read ...` invocation behind one secret variable.
///
/// Maps onto `EnvValue::OpRef`: the consumer stores the reference as an
/// `ApiKey`/`OAuthToken` value. Carries the reference only, never the
/// resolved secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpReadCandidate {
    /// Assigned variable name.
    pub var: String,
    /// 1-based number of the logical source line.
    pub line: usize,
    /// Parsed 1Password reference (`op` URI, display `path`, account pin).
    pub reference: OpRef,
}

/// Shell-function call site behind one variable.
///
/// Maps onto [`WrapperSpec`]: the consumer stores the spec at the
/// agent-invoked-via-wrapper schema home on the agent configuration built
/// from this variable's account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapperCallSite {
    /// Assigned variable name.
    pub var: String,
    /// 1-based number of the logical source line.
    pub line: usize,
    /// Parsed wrapper identity plus call-site arguments.
    pub spec: WrapperSpec,
}

/// Model/endpoint group gathered from one variable stem.
///
/// Groups every `*MODEL*`, `*_PROFILE*`, and `*_BASE_URL` (plus `_API_BASE` /
/// `_API_URL`) literal by the lowercased first `_`-separated segment of the
/// variable name (`MOONSHOT_MODEL` + `MOONSHOT_BASE_URL` → `moonshot`). Maps
/// onto `AccountCredential::ApiKey { model, base_url }` defaults and the
/// matching `AgentConfiguration` overrides. Consumers match the lowercased
/// stem against the canonical provider slug; legacy agent aliases are not
/// normalized here. Model IDs and endpoint URLs are not secrets; key/token
/// literals never enter a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelProfile {
    /// Lowercased canonical provider slug (`moonshot`, `anthropic`, …).
    pub name: String,
    /// Explicit model identifier, if the group carries one.
    pub model: Option<String>,
    /// Endpoint override, if the group carries one.
    pub base_url: Option<String>,
}

/// Typed extraction over a [`ZshrcImport`] that a consumer can apply.
///
/// Built purely from parsed literals and unresolved snippets: no shell is
/// sourced, no substitution is executed, and no secret helper runs. The plan
/// carries references and paths only — never resolved secret values or key
/// literals. Variables that fail to parse (truncated snippets, relative
/// directories, unparseable `op://` URIs) are skipped here and stay visible
/// in [`ZshrcImport::unresolved`] for operator resolution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZshrcImportPlan {
    /// Config-dir overrides, sorted by source variable.
    pub directories: Vec<DirectoryCandidate>,
    /// Complete absolute XDG triple, mappable to `XdgRoots` on an Amp
    /// profile; `None` unless data/config/cache are all present.
    pub xdg_roots: Option<XdgRoots>,
    /// Parsed `op read` invocations, in source order.
    pub op_refs: Vec<OpReadCandidate>,
    /// Shell-function call sites, in source order.
    pub wrappers: Vec<WrapperCallSite>,
    /// Model/endpoint groups, sorted by name.
    pub models: Vec<ModelProfile>,
}

/// Build the typed [`ZshrcImportPlan`] for a parsed import.
///
/// Total and side-effect-free: every extraction is a pure function of
/// `import.values` and `import.unresolved`.
pub fn import_plan(import: &ZshrcImport) -> ZshrcImportPlan {
    ZshrcImportPlan {
        directories: extract_directories(&import.values),
        xdg_roots: extract_xdg_roots(&import.values),
        op_refs: import
            .unresolved
            .iter()
            .filter(|entry| entry.kind == UnresolvedKind::OpRead)
            .filter_map(|entry| {
                parse_op_read(&entry.detail).map(|reference| OpReadCandidate {
                    var: entry.name.clone(),
                    line: entry.line,
                    reference,
                })
            })
            .collect(),
        wrappers: import
            .unresolved
            .iter()
            .filter(|entry| entry.kind == UnresolvedKind::FunctionCall)
            .filter_map(|entry| {
                parse_wrapper_call(&entry.detail).map(|spec| WrapperCallSite {
                    var: entry.name.clone(),
                    line: entry.line,
                    spec,
                })
            })
            .collect(),
        models: extract_model_profiles(&import.values),
    }
}

/// Config-dir override variables attributed to their owning agent.
const CONFIG_DIR_VARS: &[(&str, Agent)] = &[
    ("CLAUDE_CONFIG_DIR", Agent::Claude),
    ("CODEX_HOME", Agent::Codex),
];

/// Collect absolute config-dir overrides as agent-attributed candidates.
fn extract_directories(values: &BTreeMap<String, String>) -> Vec<DirectoryCandidate> {
    let mut out: Vec<DirectoryCandidate> = CONFIG_DIR_VARS
        .iter()
        .filter_map(|(var, agent)| {
            let raw = values.get(*var)?;
            let directory = PathBuf::from(raw);
            if !directory.is_absolute() {
                return None;
            }
            Some(DirectoryCandidate {
                agent: *agent,
                directory,
                source_var: (*var).to_owned(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.source_var.cmp(&b.source_var));
    out
}

/// Collect the XDG triple when all three roots are present and absolute.
fn extract_xdg_roots(values: &BTreeMap<String, String>) -> Option<XdgRoots> {
    let data = PathBuf::from(values.get("XDG_DATA_HOME")?);
    let config = PathBuf::from(values.get("XDG_CONFIG_HOME")?);
    let cache = PathBuf::from(values.get("XDG_CACHE_HOME")?);
    if !data.is_absolute() || !config.is_absolute() || !cache.is_absolute() {
        return None;
    }
    Some(XdgRoots {
        data,
        config,
        cache,
    })
}

/// Endpoint suffixes that mark a variable as carrying a base URL.
const ENDPOINT_SUFFIXES: &[&str] = &["_BASE_URL", "_API_BASE", "_API_URL"];

/// Whether the variable can join a [`ModelProfile`] group.
fn is_modelish(name: &str) -> bool {
    name.contains("MODEL")
        || ENDPOINT_SUFFIXES.iter().any(|s| name.ends_with(s))
        || name.ends_with("_PROFILE")
        || name.ends_with("_PROFILE_NAME")
}

/// Group modelish literals by lowercased first-segment stem.
fn extract_model_profiles(values: &BTreeMap<String, String>) -> Vec<ModelProfile> {
    let mut groups: BTreeMap<String, Vec<(&String, &String)>> = BTreeMap::new();
    for (name, value) in values {
        if !is_modelish(name) {
            continue;
        }
        let stem = name.split('_').next().unwrap_or(name).to_lowercase();
        groups.entry(stem).or_default().push((name, value));
    }
    groups
        .into_iter()
        .filter_map(|(stem, mut vars)| {
            vars.sort_by(|a, b| a.0.cmp(b.0));
            let upper = stem.to_uppercase();
            let exact_model = format!("{upper}_MODEL");
            let exact_url = format!("{upper}_BASE_URL");
            let model = vars
                .iter()
                .find(|(name, _)| *name == &exact_model)
                .or_else(|| vars.iter().find(|(name, _)| name.contains("MODEL")))
                .map(|(_, value)| (*value).clone());
            let base_url = vars
                .iter()
                .find(|(name, _)| *name == &exact_url)
                .or_else(|| {
                    vars.iter()
                        .find(|(name, _)| ENDPOINT_SUFFIXES.iter().any(|s| name.ends_with(s)))
                })
                .map(|(_, value)| (*value).clone());
            if model.is_none() && base_url.is_none() {
                return None;
            }
            Some(ModelProfile {
                name: stem,
                model,
                base_url,
            })
        })
        .collect()
}

/// Parse an `OpRead` detail snippet into an [`OpRef`].
///
/// Accepts `$(op read ...)` / backquote forms with an optional `sudo` / `env`
/// / `command` prefix, global `--account <id>` / `--account=<id>` flags in any
/// position, and one `op://vault/item/[section/]field` argument (quoting
/// honored). The display `path` breadcrumb is rebuilt from the URI segments;
/// the URI may carry IDs rather than names, so the consumer treats it as a
/// snapshot. Returns `None` for truncated snippets, non-`read` invocations,
/// and URIs rejected by [`parse_op_reference`].
fn parse_op_read(detail: &str) -> Option<OpRef> {
    if detail.contains('\u{2026}') {
        return None;
    }
    let words = split_shell_words(strip_substitution(detail)?)?;
    let mut words = words.into_iter().peekable();
    if matches!(
        words.peek().map(String::as_str),
        Some("sudo" | "env" | "command")
    ) {
        words.next();
    }
    if words.next().as_deref() != Some("op") {
        return None;
    }
    let mut saw_read = false;
    let mut account = None;
    let mut uri = None;
    let mut pending_account = false;
    for word in words {
        if pending_account {
            account = Some(word);
            pending_account = false;
            continue;
        }
        if word == "read" {
            saw_read = true;
        } else if word == "--account" {
            pending_account = true;
        } else if let Some(id) = word.strip_prefix("--account=") {
            account = Some(id.to_owned());
        } else if word.starts_with("op://") && uri.is_none() {
            uri = Some(word);
        }
    }
    if pending_account || !saw_read {
        return None;
    }
    let op = uri?;
    let parts = parse_op_reference(&op)?;
    let vault = &parts.vault;
    let item = &parts.item;
    let mut path = format!("{vault}/{item}");
    if let Some(section) = &parts.section {
        path.push('/');
        path.push_str(section);
    }
    path.push('/');
    let field = &parts.field;
    path.push_str(field);
    Some(OpRef {
        op,
        path,
        account,
        on_demand: false,
    })
}

/// Parse a `FunctionCall` detail snippet into a [`WrapperSpec`].
///
/// The invoked function name becomes `identity`; remaining words (quoting
/// honored) become `args`. Returns `None` for truncated snippets and empty
/// invocations.
fn parse_wrapper_call(detail: &str) -> Option<WrapperSpec> {
    if detail.contains('\u{2026}') {
        return None;
    }
    let mut words = split_shell_words(strip_substitution(detail)?)?.into_iter();
    let identity = words.next()?;
    if identity.trim().is_empty() {
        return None;
    }
    Some(WrapperSpec {
        identity,
        args: words.collect(),
    })
}

/// Strip the outer `$(...)` / `` `...` `` substitution markers.
fn strip_substitution(detail: &str) -> Option<&str> {
    if let Some(inner) = detail.strip_prefix("$(").and_then(|s| s.strip_suffix(')')) {
        return Some(inner);
    }
    if let Some(inner) = detail
        .strip_prefix('`')
        .and_then(|stripped| stripped.strip_suffix('`'))
    {
        return Some(inner);
    }
    None
}

/// Split words honoring single/double quotes and backslash escapes.
/// Returns `None` for malformed snippets (e.g. unterminated quotes);
/// callers treat that as "not a parseable invocation".
fn split_shell_words(text: &str) -> Option<Vec<String>> {
    shell_words::split(text).ok()
}

#[cfg(test)]
mod tests;
