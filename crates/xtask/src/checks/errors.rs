// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::Check;
use crate::ci::CiStep;
use crate::lex::{Token, lex_rust};
use crate::repo::{FileKind, Repo};
use crate::violation::{Violation, sorted};

mod item;

#[derive(Clone, Copy, Debug, Default)]
pub struct ErrorsCheck;

impl Check for ErrorsCheck {
    fn step(&self) -> CiStep {
        CiStep::Errors
    }

    fn run(&self, repo: &Repo) -> Vec<Violation> {
        let mut violations = Vec::new();
        for file in repo.files() {
            if file.kind() == FileKind::Rust {
                let tokens = lex_rust(file.text()).tokens;
                violations.extend(
                    (0..tokens.len())
                        .filter_map(|index| error_attribute(&tokens, index))
                        .flat_map(|attribute| attribute_problems(&attribute))
                        .map(|(line, message)| {
                            Violation::at_line(self.step(), file.path(), line, message)
                        }),
                );
            }
        }
        sorted(violations)
    }
}

struct ErrorAttribute<'a> {
    arguments: &'a [Token],
    sources: Vec<String>,
}

fn error_attribute(tokens: &[Token], index: usize) -> Option<ErrorAttribute<'_>> {
    let opens = tokens[index].is_punct('#')
        && tokens.get(index + 1)?.is_punct('[')
        && tokens.get(index + 2)?.is_ident("error")
        && tokens.get(index + 3)?.is_punct('(');
    if !opens {
        return None;
    }
    let body = &tokens[index + 4..];
    let mut depth = 1usize;
    let mut close = body.len();
    for (offset, token) in body.iter().enumerate() {
        if token.is_punct('(') {
            depth += 1;
        } else if token.is_punct(')') {
            depth -= 1;
            if depth == 0 {
                close = offset;
                break;
            }
        }
    }
    Some(ErrorAttribute {
        arguments: &body[..close],
        sources: item::source_fields(tokens, index + 4 + close + 2),
    })
}

fn ends_expression(token: &Token) -> bool {
    token.ident().is_some()
        || token.number().is_some()
        || token.string().is_some()
        || token.is_punct(')')
        || token.is_punct(']')
}

fn attribute_problems(attribute: &ErrorAttribute<'_>) -> Vec<(u32, String)> {
    let arguments = attribute.arguments;
    let mut problems = Vec::new();
    for (index, token) in arguments.iter().enumerate() {
        if let Some(literal) = token.string() {
            if let Some(message) = literal_problem(literal, &attribute.sources) {
                problems.push((token.line, message));
            }
        } else if token.is_punct('.')
            && !index
                .checked_sub(1)
                .is_some_and(|before| ends_expression(&arguments[before]))
        {
            let field = arguments
                .get(index + 1)
                .and_then(|t| t.ident().or_else(|| t.number()));
            if let Some(field) = field.filter(|field| is_source(field, &attribute.sources)) {
                problems.push((token.line, source_message(&format!("`.{field}`"))));
            }
        }
    }
    problems
}

fn is_source(argument: &str, sources: &[String]) -> bool {
    argument == "source" || sources.iter().any(|name| name == argument)
}

fn literal_problem(literal: &str, sources: &[String]) -> Option<String> {
    let mut debug = Vec::new();
    let mut source = Vec::new();
    for inner in placeholders(literal) {
        let (argument, spec) = inner.split_once(':').unwrap_or((inner, ""));
        if spec.ends_with('?') {
            debug.push(format!("{{{inner}}}"));
        }
        if is_source(argument, sources) {
            source.push(format!("{{{inner}}}"));
        }
    }
    let mut parts = Vec::new();
    if !debug.is_empty() {
        parts.push(format!(
            "{} formats with Debug in an #[error] message; give the value a Display form and print that",
            debug.join(", ")
        ));
    }
    if !source.is_empty() {
        parts.push(source_message(&source.join(", ")));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

fn source_message(what: &str) -> String {
    format!(
        "{what} in an #[error] message prints the cause that source() already exposes, so the report shows it twice; drop it from the message"
    )
}

fn placeholders(literal: &str) -> Vec<&str> {
    let bytes = literal.as_bytes();
    let mut found = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'{' {
            index += 1;
        } else if bytes.get(index + 1) == Some(&b'{') {
            index += 2;
        } else if let Some(length) = literal[index + 1..].find('}') {
            found.push(&literal[index + 1..index + 1 + length]);
            index += length + 2;
        } else {
            break;
        }
    }
    found
}
