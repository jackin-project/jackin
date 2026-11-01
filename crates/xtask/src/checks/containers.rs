// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use super::Check;
use crate::ci::CiStep;
use crate::lex::{Token, lex_rust};
use crate::repo::{FileKind, Repo};
use crate::violation::{Violation, sorted};

pub const IMAGE_CRATE_DIR: &str = "crates/jackin-image";

pub const DOCKER_SUBCOMMANDS: &[&[&str]] = &[
    &["buildx", "build"],
    &["buildx", "create"],
    &["buildx", "inspect"],
    &["image", "inspect"],
    &["push"],
];

#[derive(Clone, Copy, Debug, Default)]
pub struct ContainersCheck;

impl Check for ContainersCheck {
    fn step(&self) -> CiStep {
        CiStep::Containers
    }

    fn run(&self, repo: &Repo) -> Vec<Violation> {
        let mut violations = Vec::new();
        for file in repo.files() {
            if file.kind() != FileKind::Rust {
                continue;
            }
            let tokens = lex_rust(file.text()).tokens;
            let in_image = file.path().starts_with(IMAGE_CRATE_DIR);
            for index in 0..tokens.len() {
                if let Some(message) = problem(&tokens, index, in_image) {
                    violations.push(Violation::at_line(
                        self.step(),
                        file.path(),
                        tokens[index].line,
                        message,
                    ));
                }
            }
        }
        sorted(violations)
    }
}

fn path_pair(tokens: &[Token], index: usize, head: &str, tail: &str) -> bool {
    tokens[index].is_ident(head)
        && tokens.get(index + 1).is_some_and(|t| t.is_punct(':'))
        && tokens.get(index + 2).is_some_and(|t| t.is_punct(':'))
        && tokens.get(index + 3).is_some_and(|t| t.is_ident(tail))
}

fn problem(tokens: &[Token], index: usize, in_image: bool) -> Option<String> {
    if path_pair(tokens, index, "Program", "Docker") {
        if !in_image {
            return Some(
                "Program::Docker is used outside crates/jackin-image; only jackin-image starts docker (D01)"
                    .to_owned(),
            );
        }
        return forbidden_subcommand(tokens, index);
    }
    if path_pair(tokens, index, "Command", "new") && tokens.get(index + 4)?.is_punct('(') {
        let program = tokens.get(index + 5)?.string()?;
        let name = Path::new(program).file_name()?.to_str()?;
        return match name {
            "podman" | "container" => Some(format!(
                "Command::new({name:?}) starts a container engine; jackin never runs podman or container (D01)"
            )),
            "docker" => Some(
                "Command::new(\"docker\") starts docker directly; use Program::Docker in jackin-image (D01)"
                    .to_owned(),
            ),
            _ => None,
        };
    }
    None
}

fn forbidden_subcommand(tokens: &[Token], index: usize) -> Option<String> {
    let literals = leading_args(tokens, index)?;
    if literals.is_empty() {
        return None;
    }
    let permitted = DOCKER_SUBCOMMANDS.iter().any(|allowed| {
        literals.len() >= allowed.len()
            && literals
                .iter()
                .zip(*allowed)
                .all(|(have, want)| have == want)
    });
    if permitted {
        return None;
    }
    let words = match literals[0].as_str() {
        "buildx" | "image" => &literals[..literals.len().min(2)],
        _ => &literals[..1],
    };
    Some(format!(
        "docker {} is not a permitted docker subcommand (D01)",
        words.join(" ")
    ))
}

fn leading_args(tokens: &[Token], index: usize) -> Option<Vec<String>> {
    if index < 2 || !tokens[index - 1].is_punct(':') || !tokens[index - 2].is_ident("program") {
        return None;
    }
    let open = enclosing_brace(tokens, index)?;
    let start = args_value(tokens, open)?;
    Some(literal_elements(tokens, start))
}

fn enclosing_brace(tokens: &[Token], index: usize) -> Option<usize> {
    let mut depth = 0usize;
    for position in (0..index).rev() {
        if tokens[position].is_punct('}') {
            depth += 1;
        } else if tokens[position].is_punct('{') {
            if depth == 0 {
                return Some(position);
            }
            depth -= 1;
        }
    }
    None
}

fn args_value(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for position in open + 1..tokens.len() {
        let token = &tokens[position];
        if token.is_punct('{') {
            depth += 1;
        } else if token.is_punct('}') {
            if depth == 0 {
                return None;
            }
            depth -= 1;
        } else if depth == 0
            && token.is_ident("args")
            && (tokens[position - 1].is_punct(',') || tokens[position - 1].is_punct('{'))
            && tokens.get(position + 1).is_some_and(|t| t.is_punct(':'))
            && tokens.get(position + 2).is_some_and(|t| !t.is_punct(':'))
        {
            return bracket_start(tokens, position + 2);
        }
    }
    None
}

fn bracket_start(tokens: &[Token], mut position: usize) -> Option<usize> {
    if tokens.get(position)?.is_punct('&') {
        position += 1;
    }
    if tokens.get(position)?.is_ident("vec") && tokens.get(position + 1)?.is_punct('!') {
        position += 2;
    }
    tokens.get(position)?.is_punct('[').then_some(position + 1)
}

fn literal_elements(tokens: &[Token], mut position: usize) -> Vec<String> {
    let mut literals = Vec::new();
    loop {
        let Some((text, next)) = string_element(tokens, position) else {
            return literals;
        };
        literals.push(text);
        position = next;
        if tokens.get(position).is_some_and(|t| t.is_punct(',')) {
            position += 1;
        } else {
            return literals;
        }
    }
}

fn string_element(tokens: &[Token], position: usize) -> Option<(String, usize)> {
    if path_pair(tokens, position, "String", "from") && tokens.get(position + 4)?.is_punct('(') {
        let text = tokens.get(position + 5)?.string()?;
        return tokens
            .get(position + 6)?
            .is_punct(')')
            .then(|| (text.to_owned(), position + 7));
    }
    let text = tokens.get(position)?.string()?;
    let mut next = position + 1;
    while tokens.get(next).is_some_and(|t| t.is_punct('.'))
        && tokens.get(next + 1).is_some_and(|t| t.ident().is_some())
        && tokens.get(next + 2).is_some_and(|t| t.is_punct('('))
        && tokens.get(next + 3).is_some_and(|t| t.is_punct(')'))
    {
        next += 4;
    }
    Some((text.to_owned(), next))
}
