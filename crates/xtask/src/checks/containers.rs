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

pub const COMPUTED_COMMAND_DIRS: &[&str] = &[
    "crates/xtask",
    "crates/jackin-session/src/host.rs",
    "crates/jackin/tests/process/main.rs",
    "crates/jackin-openshell/tests/gateway/exec_rpc.rs",
    "crates/jackin-openshell/tests/gateway/writer_bootstrap.rs",
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
            let aliases = command_aliases(&tokens);
            let scope = Scope {
                docker: under(file.path(), DOCKER_CRATE_DIRS),
                computed_command: under(file.path(), COMPUTED_COMMAND_DIRS),
            };
            for index in 0..tokens.len() {
                if let Some(message) = problem(&tokens, &aliases, index, scope) {
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

fn problem(tokens: &[Token], aliases: &[String], index: usize, scope: Scope) -> Option<String> {
    if !scope.docker
        && let Some(message) = docker_name_problem(tokens, index)
    {
        return Some(message);
    }
    if is_command(&tokens[index], aliases)
        && path_pair(tokens, index, tokens[index].ident()?, "new")
    {
        return match tokens.get(index + 4) {
            Some(t) if t.is_punct('(') => command_problem(tokens, index + 5, scope),
            _ => (!scope.computed_command).then(|| {
                "Command::new is used as a value; a function value can hide a computed program (D01)"
                    .to_owned()
            }),
        };
    }
    None
}

fn is_command(token: &Token, aliases: &[String]) -> bool {
    token.is_ident("Command")
        || token
            .ident()
            .is_some_and(|name| aliases.iter().any(|a| a == name))
}

fn command_aliases(tokens: &[Token]) -> Vec<String> {
    let mut aliases = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.is_ident("use") {
            let mut position = index + 1;
            use_tree(tokens, &mut position, false, &mut aliases);
        }
    }
    aliases
}

fn use_tree(
    tokens: &[Token],
    position: &mut usize,
    mut in_process: bool,
    aliases: &mut Vec<String>,
) {
    let mut last: Option<&str> = None;
    while let Some(token) = tokens.get(*position) {
        if token.is_punct('{') {
            *position += 1;
            loop {
                use_tree(tokens, position, in_process, aliases);
                if tokens.get(*position).is_some_and(|t| t.is_punct(',')) {
                    *position += 1;
                } else {
                    break;
                }
            }
            if tokens.get(*position).is_some_and(|t| t.is_punct('}')) {
                *position += 1;
            }
            return;
        }
        if token.is_punct(':') {
            *position += 1;
            continue;
        }
        let Some(name) = token.ident() else {
            return;
        };
        *position += 1;
        if name == "as" {
            if let Some(alias) = tokens.get(*position).and_then(Token::ident) {
                *position += 1;
                if last == Some("Command") && in_process && alias != "_" {
                    aliases.push(alias.to_owned());
                }
            }
            return;
        }
        in_process |= name == "process";
        last = Some(name);
    }
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
    if tokens[index].is_ident("type") {
        return type_alias_problem(tokens, index);
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

fn type_alias_problem(tokens: &[Token], index: usize) -> Option<String> {
    tokens.get(index + 1)?.ident()?;
    if !tokens.get(index + 2)?.is_punct('=') {
        return None;
    }
    let mut position = index + 3;
    let mut last = None;
    while let Some(token) = tokens.get(position) {
        if let Some(name) = token.ident() {
            last = Some(name);
        } else if !token.is_punct(':') {
            break;
        }
        position += 1;
    }
    (last == Some("Program") && tokens.get(position)?.is_punct(';'))
        .then(|| outside("`type X = Program` hides Program::Docker"))
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
