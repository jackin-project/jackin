// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;

use super::Check;
use super::baseline::demo_violations;
use crate::ci::CiStep;
use crate::repo::{Repo, capture, cargo_program, without_git_env};
use crate::text::plain;
use crate::violation::{Violation, sorted};

pub const DEMO_PATHS: [&str; 2] = ["jackin_console::fixtures::", "jackin_console::sim::"];

pub const DEMO_CANARIES: [&str; 5] = [
    "jackin-preview",
    "payments-platform",
    "Claude Code v2.1.14",
    "tty004",
    "sentinel-demo",
];

const CONSOLE: &str = "jackin-console";
const CONSOLE_LIB: &str = "jackin_console";
const SYMBOL_EXCERPT: usize = 160;

const NO_MATCH: &str = "did not match any packages";
const TREE_MEMBERS: [&str; 2] = ["jackin", "jackin-tui"];
const BUILD_MEMBER: &str = "jackin-tui";
const ARTIFACT_MEMBER: &str = "jackin";

#[derive(Clone, Debug)]
pub struct ToolOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str], dir: &Path) -> io::Result<ToolOutput>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str], dir: &Path) -> io::Result<ToolOutput> {
        let program = if program == "cargo" {
            cargo_program()
        } else {
            program.into()
        };
        let output = capture(
            without_git_env(&mut Command::new(program))
                .args(args)
                .current_dir(dir),
        )?;
        Ok(ToolOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DemoGateCheck<R = SystemRunner> {
    runner: R,
}

impl DemoGateCheck {
    #[must_use]
    pub const fn production() -> Self {
        Self::new(SystemRunner)
    }
}

impl<R> DemoGateCheck<R> {
    #[must_use]
    pub const fn new(runner: R) -> Self {
        Self { runner }
    }
}

enum Reply {
    Text(String),
    NoMatch,
}

impl<R: CommandRunner> DemoGateCheck<R> {
    fn call(&self, repo: &Repo, program: &str, args: &[&str]) -> Result<Reply, Violation> {
        let command = format!("{program} {}", args.join(" "));
        let output = self
            .runner
            .run(program, args, repo.root())
            .map_err(|error| {
                Violation::general(
                    CiStep::DemoGate,
                    format!("cannot start {program}: {}", plain(error)),
                )
            })?;
        if output.success {
            return Ok(Reply::Text(output.stdout));
        }
        if output.stderr.contains(NO_MATCH) {
            return Ok(Reply::NoMatch);
        }
        Err(Violation::general(
            CiStep::DemoGate,
            format!("{command} failed: {}", plain(output.stderr.trim())),
        ))
    }

    fn feature_graph(&self, repo: &Repo, violations: &mut Vec<Violation>) -> bool {
        let mut console_present = false;
        for member in TREE_MEMBERS {
            let args = [
                "tree",
                "-p",
                member,
                "-e",
                "normal,build",
                "-f",
                "{p} {f}",
                "-i",
                CONSOLE,
            ];
            match self.call(repo, "cargo", &args) {
                Ok(Reply::Text(text)) => {
                    let lines: Vec<TreeLine<'_>> = console_lines(&text).collect();
                    console_present |= !lines.is_empty();
                    if lines.iter().any(|line| line.features.contains(&"demo")) {
                        violations.push(Violation::general(
                            CiStep::DemoGate,
                            format!(
                                "cargo tree -p {member} builds {CONSOLE} with the feature demo"
                            ),
                        ));
                    }
                }
                Ok(Reply::NoMatch) => {}
                Err(violation) => violations.push(violation),
            }
        }
        let args = ["tree", "-p", "jackin", "-e", "normal", "-i", "clap"];
        match self.call(repo, "cargo", &args) {
            Ok(Reply::Text(text)) => {
                if console_lines(&text).next().is_some() {
                    violations.push(Violation::general(
                        CiStep::DemoGate,
                        format!(
                            "cargo tree -p jackin -e normal -i clap shows clap below {CONSOLE}"
                        ),
                    ));
                }
            }
            Ok(Reply::NoMatch) => {}
            Err(violation) => violations.push(violation),
        }
        console_present
    }

    fn symbols(&self, repo: &Repo, violations: &mut Vec<Violation>) {
        let args = ["build", "-p", BUILD_MEMBER, "--message-format=json"];
        let text = match self.call(repo, "cargo", &args) {
            Ok(Reply::Text(text)) => text,
            Ok(Reply::NoMatch) => return,
            Err(violation) => {
                violations.push(violation);
                return;
            }
        };
        let rlibs = console_rlibs(&text);
        if rlibs.is_empty() {
            violations.push(Violation::general(
                CiStep::DemoGate,
                format!("cargo build -p {BUILD_MEMBER} names no rlib for {CONSOLE}"),
            ));
        }
        for rlib in rlibs {
            match self.call(repo, "nm", &["-C", &rlib]) {
                Ok(Reply::Text(symbols)) => {
                    if let Some(symbol) = symbols
                        .lines()
                        .find(|line| DEMO_PATHS.iter().any(|path| line.contains(path)))
                    {
                        violations.push(Violation::general(
                            CiStep::DemoGate,
                            format!(
                                "{rlib} holds a demo symbol: {}",
                                plain(
                                    symbol
                                        .trim()
                                        .chars()
                                        .take(SYMBOL_EXCERPT)
                                        .collect::<String>()
                                )
                            ),
                        ));
                    }
                }
                Ok(Reply::NoMatch) => {}
                Err(violation) => violations.push(violation),
            }
            match self.call(repo, "strings", &["-a", &rlib]) {
                Ok(Reply::Text(strings)) => {
                    for canary in DEMO_CANARIES
                        .iter()
                        .filter(|canary| strings.contains(**canary))
                    {
                        violations.push(Violation::general(
                            CiStep::DemoGate,
                            format!("{rlib} holds the demo canary {canary:?}"),
                        ));
                    }
                }
                Ok(Reply::NoMatch) => {}
                Err(violation) => violations.push(violation),
            }
        }
    }
}

impl<R: CommandRunner + Sync> Check for DemoGateCheck<R> {
    fn step(&self) -> CiStep {
        CiStep::DemoGate
    }

    fn run(&self, repo: &Repo) -> Vec<Violation> {
        let mut violations = demo_violations(repo, self.step());
        if self.feature_graph(repo, &mut violations) {
            self.symbols(repo, &mut violations);
        }
        sorted(violations)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProductCheck<R = SystemRunner> {
    gate: DemoGateCheck<R>,
}

impl ProductCheck {
    #[must_use]
    pub const fn production() -> Self {
        Self::new(SystemRunner)
    }
}

impl<R> ProductCheck<R> {
    #[must_use]
    pub const fn new(runner: R) -> Self {
        Self {
            gate: DemoGateCheck::new(runner),
        }
    }
}

impl<R: CommandRunner> ProductCheck<R> {
    fn artifact(&self, repo: &Repo, violations: &mut Vec<Violation>) {
        let args = [
            "build",
            "--release",
            "-p",
            ARTIFACT_MEMBER,
            "--message-format=json",
        ];
        let text = match self.gate.call(repo, "cargo", &args) {
            Ok(Reply::Text(text)) => text,
            Ok(Reply::NoMatch) => {
                violations.push(Violation::general(
                    CiStep::DemoGate,
                    format!("cargo build --release -p {ARTIFACT_MEMBER} matched no package"),
                ));
                return;
            }
            Err(violation) => {
                violations.push(violation);
                return;
            }
        };
        let executables = executables(&text);
        if executables.is_empty() {
            violations.push(Violation::general(
                CiStep::DemoGate,
                format!("cargo build --release -p {ARTIFACT_MEMBER} names no executable"),
            ));
        }
        for executable in executables {
            match std::fs::read(&executable) {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for canary in DEMO_CANARIES
                        .iter()
                        .filter(|canary| text.contains(**canary))
                    {
                        violations.push(Violation::general(
                            CiStep::DemoGate,
                            format!("{executable} holds the demo canary {canary:?}"),
                        ));
                    }
                }
                Err(error) => violations.push(Violation::general(
                    CiStep::DemoGate,
                    format!("cannot read {executable}: {}", plain(error)),
                )),
            }
        }
    }
}

impl<R: CommandRunner + Sync> Check for ProductCheck<R> {
    fn step(&self) -> CiStep {
        CiStep::DemoGate
    }

    fn run(&self, repo: &Repo) -> Vec<Violation> {
        let violations = self.gate.run(repo);
        if !violations.is_empty() {
            return violations;
        }
        let mut violations = Vec::new();
        self.artifact(repo, &mut violations);
        sorted(violations)
    }
}

fn console_lines(text: &str) -> impl Iterator<Item = TreeLine<'_>> {
    text.lines()
        .filter_map(tree_line)
        .filter(|line| line.name == CONSOLE)
}

struct TreeLine<'a> {
    name: &'a str,
    features: Vec<&'a str>,
}

fn tree_line(line: &str) -> Option<TreeLine<'_>> {
    let body = line.trim_start_matches(['│', '├', '└', '─', ' ']);
    let name = body.split_whitespace().next()?;
    let body = body.trim_end();
    let body = body.strip_suffix("(*)").map_or(body, str::trim_end);
    let features = body
        .rfind(") ")
        .map_or("", |index| &body[index + 2..])
        .split(',')
        .filter(|feature| !feature.is_empty())
        .collect();
    Some(TreeLine { name, features })
}

#[derive(Deserialize)]
struct BuildMessage {
    reason: String,
    target: Option<BuildTarget>,
    #[serde(default)]
    filenames: Vec<String>,
    executable: Option<String>,
}

#[derive(Deserialize)]
struct BuildTarget {
    name: String,
    kind: Vec<String>,
}

fn executables(json: &str) -> Vec<String> {
    json.lines()
        .filter_map(|line| serde_json::from_str::<BuildMessage>(line).ok())
        .filter(|message| message.reason == "compiler-artifact")
        .filter(|message| {
            message.target.as_ref().is_some_and(|target| {
                target.name == ARTIFACT_MEMBER && target.kind.iter().any(|kind| kind == "bin")
            })
        })
        .filter_map(|message| message.executable)
        .collect()
}

fn console_rlibs(json: &str) -> Vec<String> {
    json.lines()
        .filter_map(|line| serde_json::from_str::<BuildMessage>(line).ok())
        .filter(|message| message.reason == "compiler-artifact")
        .filter(|message| {
            message.target.as_ref().is_some_and(|target| {
                target.name == CONSOLE_LIB && target.kind.iter().any(|kind| kind == "lib")
            })
        })
        .flat_map(|message| message.filenames)
        .filter(|name| {
            Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("rlib"))
        })
        .collect()
}
