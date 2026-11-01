// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use serde::Deserialize;
use toml::{Spanned, Value};

use super::Check;
use crate::ci::CiStep;
use crate::repo::Repo;
use crate::violation::{Violation, sorted};

const CARGO: &str = "Cargo.toml";
const TOOLCHAIN: &str = "rust-toolchain.toml";
const MISE: &str = "mise.toml";

#[derive(Clone, Copy, Debug, Default)]
pub struct PinsCheck;

impl Check for PinsCheck {
    fn step(&self) -> CiStep {
        CiStep::Pins
    }

    fn run(&self, repo: &Repo) -> Vec<Violation> {
        let authority = match authority(repo) {
            Ok(authority) => authority,
            Err(violation) => return vec![violation],
        };
        let mut violations = toolchain_pins(repo, &authority);
        violations.extend(mise_pins(repo, &authority));
        sorted(violations)
    }
}

fn at_file(path: &str, message: impl Into<String>) -> Violation {
    Violation::at_file(CiStep::Pins, Path::new(path), message)
}

fn disagree(path: &str, line: u32, found: &str, authority: &str) -> Violation {
    Violation::at_line(
        CiStep::Pins,
        Path::new(path),
        line,
        format!("{path} pins Rust {found}, but {CARGO} rust-version is {authority}"),
    )
}

fn read<'a, T: Deserialize<'a>>(repo: &'a Repo, path: &str) -> Result<(&'a str, T), Violation> {
    let file = repo
        .files()
        .iter()
        .find(|file| file.path() == Path::new(path))
        .ok_or_else(|| {
            at_file(
                path,
                format!("{path} is missing; the pins step compares it with {CARGO} rust-version"),
            )
        })?;
    let parsed = toml::from_str(file.text())
        .map_err(|error| at_file(path, format!("{path} does not parse: {error}")))?;
    Ok((file.text(), parsed))
}

fn line_at(text: &str, offset: usize) -> u32 {
    let newlines = text
        .char_indices()
        .take_while(|(index, _)| *index < offset)
        .filter(|(_, c)| *c == '\n')
        .count();
    u32::try_from(newlines + 1).unwrap_or(u32::MAX)
}

#[derive(Deserialize)]
struct CargoRoot {
    workspace: Option<CargoWorkspace>,
}

#[derive(Deserialize)]
struct CargoWorkspace {
    package: Option<CargoPackage>,
}

#[derive(Deserialize)]
struct CargoPackage {
    #[serde(rename = "rust-version")]
    rust_version: Option<String>,
}

fn authority(repo: &Repo) -> Result<String, Violation> {
    let (_, root): (_, CargoRoot) = read(repo, CARGO)?;
    root.workspace
        .and_then(|workspace| workspace.package)
        .and_then(|package| package.rust_version)
        .ok_or_else(|| {
            at_file(
                CARGO,
                format!("{CARGO} has no [workspace.package] rust-version; it is the Rust pin"),
            )
        })
}

#[derive(Deserialize)]
struct ToolchainRoot {
    toolchain: Option<ToolchainTable>,
}

#[derive(Deserialize)]
struct ToolchainTable {
    channel: Option<Spanned<String>>,
}

fn toolchain_pins(repo: &Repo, authority: &str) -> Vec<Violation> {
    let (text, root): (_, ToolchainRoot) = match read(repo, TOOLCHAIN) {
        Ok(read) => read,
        Err(violation) => return vec![violation],
    };
    let Some(channel) = root.toolchain.and_then(|table| table.channel) else {
        return vec![at_file(
            TOOLCHAIN,
            format!("{TOOLCHAIN} has no [toolchain] channel"),
        )];
    };
    if channel.get_ref() == authority {
        return Vec::new();
    }
    vec![disagree(
        TOOLCHAIN,
        line_at(text, channel.span().start),
        channel.get_ref(),
        authority,
    )]
}

#[derive(Deserialize)]
struct MiseRoot {
    tools: Option<MiseTools>,
}

#[derive(Deserialize)]
struct MiseTools {
    rust: Option<Spanned<Value>>,
}

fn mise_pins(repo: &Repo, authority: &str) -> Vec<Violation> {
    let (text, root): (_, MiseRoot) = match read(repo, MISE) {
        Ok(read) => read,
        Err(violation) => return vec![violation],
    };
    let mut violations = Vec::new();
    match root.tools.and_then(|tools| tools.rust) {
        None => violations.push(at_file(MISE, format!("{MISE} has no [tools] rust"))),
        Some(tool) => {
            let line = line_at(text, tool.span().start);
            let version = match tool.get_ref() {
                Value::String(version) => Some(version.as_str()),
                Value::Table(table) => table.get("version").and_then(Value::as_str),
                _ => None,
            };
            match version {
                Some(version) if version == authority => {}
                Some(version) => violations.push(disagree(MISE, line, version, authority)),
                None => violations.push(Violation::at_line(
                    CiStep::Pins,
                    Path::new(MISE),
                    line,
                    format!("{MISE} [tools] rust has no version"),
                )),
            }
        }
    }
    violations.extend(task_pins(text, authority));
    violations
}

fn task_pins(text: &str, authority: &str) -> Vec<Violation> {
    let mut in_tasks = false;
    let mut violations = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            in_tasks = trimmed.starts_with("[tasks");
        }
        if !in_tasks {
            continue;
        }
        let line_number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        for found in mbx_versions(line) {
            if found != authority {
                violations.push(disagree(MISE, line_number, found, authority));
            }
        }
    }
    violations
}

fn mbx_versions(line: &str) -> Vec<&str> {
    let words: Vec<&str> = line
        .split(|c: char| c.is_whitespace() || c == '"' || c == '\'')
        .filter(|word| !word.is_empty())
        .collect();
    words
        .windows(2)
        .filter(|pair| pair[0] == "mbx")
        .filter_map(|pair| pair[1].strip_prefix('+'))
        .filter(|version| version.starts_with(|c: char| c.is_ascii_digit()))
        .collect()
}
