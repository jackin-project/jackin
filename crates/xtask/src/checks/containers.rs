// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use super::Check;
use crate::ci::CiStep;
use crate::lex::{Token, lex_rust};
use crate::repo::{FileKind, Repo};
use crate::violation::{Violation, sorted};

pub const DOCKER_CRATE_DIRS: &[&str] = &[
    "crates/jackin-image",
    "crates/jackin-core",
    "crates/jackin-testkit",
];

pub const COMPUTED_COMMAND_DIRS: &[&str] = &["crates/xtask"];

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
            let scope = Scope {
                docker: under(file.path(), DOCKER_CRATE_DIRS),
                computed_command: under(file.path(), COMPUTED_COMMAND_DIRS),
            };
            for index in 0..tokens.len() {
                if let Some(message) = problem(&tokens, index, scope) {
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

#[derive(Clone, Copy)]
struct Scope {
    docker: bool,
    computed_command: bool,
}

fn under(path: &Path, dirs: &[&str]) -> bool {
    dirs.iter().any(|dir| path.starts_with(dir))
}

fn path_pair(tokens: &[Token], index: usize, head: &str, tail: &str) -> bool {
    tokens[index].is_ident(head)
        && tokens.get(index + 1).is_some_and(|t| t.is_punct(':'))
        && tokens.get(index + 2).is_some_and(|t| t.is_punct(':'))
        && tokens.get(index + 3).is_some_and(|t| t.is_ident(tail))
}

fn problem(tokens: &[Token], index: usize, scope: Scope) -> Option<String> {
    if !scope.docker
        && let Some(message) = docker_name_problem(tokens, index)
    {
        return Some(message);
    }
    if path_pair(tokens, index, "Command", "new") && tokens.get(index + 4)?.is_punct('(') {
        return command_problem(tokens, index + 5, scope);
    }
    None
}

fn outside(what: &str) -> String {
    format!(
        "{what} outside jackin-image, jackin-core, and jackin-testkit; only jackin-image starts docker (D01)"
    )
}

fn docker_name_problem(tokens: &[Token], index: usize) -> Option<String> {
    if tokens[index].is_ident("DockerSubcommand") {
        return Some(outside("DockerSubcommand is used"));
    }
    if !tokens[index].is_ident("Program") {
        return None;
    }
    if tokens.get(index + 1).is_some_and(|t| t.is_ident("as")) {
        return Some(outside("`Program as` hides Program::Docker"));
    }
    if path_pair(tokens, index, "Program", "Docker") {
        return Some(outside("Program::Docker is used"));
    }
    let after_path = tokens.get(index + 1).is_some_and(|t| t.is_punct(':'))
        && tokens.get(index + 2).is_some_and(|t| t.is_punct(':'));
    match tokens.get(index + 3) {
        Some(t) if after_path && t.is_punct('*') => {
            Some(outside("`Program::*` hides Program::Docker"))
        }
        Some(t) if after_path && t.is_punct('{') && group_names_docker(tokens, index + 3) => {
            Some(outside("a `Program::{..}` group imports Program::Docker"))
        }
        _ => None,
    }
}

fn group_names_docker(tokens: &[Token], open: usize) -> bool {
    let mut depth = 0usize;
    for token in &tokens[open..] {
        if token.is_punct('{') {
            depth += 1;
        } else if token.is_punct('}') {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return false;
            }
        } else if token.is_ident("Docker") {
            return true;
        }
    }
    false
}

fn command_problem(tokens: &[Token], argument: usize, scope: Scope) -> Option<String> {
    if let Some(program) = tokens.get(argument).and_then(Token::string)
        && let Some(message) = container_program(program)
    {
        return Some(message);
    }
    let single_literal = tokens.get(argument).is_some_and(|t| t.string().is_some())
        && (tokens.get(argument + 1).is_some_and(|t| t.is_punct(')'))
            || (tokens.get(argument + 1).is_some_and(|t| t.is_punct(','))
                && tokens.get(argument + 2).is_some_and(|t| t.is_punct(')'))));
    (!single_literal && !scope.computed_command).then(|| {
        "Command::new takes a program that is not a string literal; a computed program can hide docker (D01)"
            .to_owned()
    })
}

fn container_program(program: &str) -> Option<String> {
    let name = Path::new(program).file_name()?.to_str()?;
    match name {
        "podman" | "container" => Some(format!(
            "Command::new({name:?}) starts a container engine; jackin never runs podman or container (D01)"
        )),
        "docker" => Some(
            "Command::new(\"docker\") starts docker directly; use Program::Docker in jackin-image (D01)"
                .to_owned(),
        ),
        _ => None,
    }
}
