// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsString;
use std::io::{self, Write};
use std::process::Command;

use clap::Args;
use thiserror::Error;

use crate::checks::{Check, CommentsCheck, ContainersCheck, LayersCheck};
use crate::repo::{Repo, cargo_program};
use crate::violation::Violation;

#[derive(Args, Clone, Copy, Debug)]
pub struct CiArgs {
    #[arg(long, help = "Skip the docs build; same steps as --no-docs")]
    pub fast: bool,
    #[arg(long = "no-docs", help = "Skip the docs build")]
    pub no_docs: bool,
}

impl CiArgs {
    #[must_use]
    pub fn profile(self) -> CiProfile {
        if self.fast || self.no_docs {
            CiProfile::SkipDocs
        } else {
            CiProfile::Full
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CiProfile {
    Full,
    SkipDocs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CiStep {
    Fmt,
    Clippy,
    Nextest,
    Deny,
    Shear,
    Comments,
    Layers,
    Containers,
    Reuse,
    Docs,
}

impl CiStep {
    pub const ALL: [CiStep; 10] = [
        CiStep::Fmt,
        CiStep::Clippy,
        CiStep::Nextest,
        CiStep::Deny,
        CiStep::Shear,
        CiStep::Comments,
        CiStep::Layers,
        CiStep::Containers,
        CiStep::Reuse,
        CiStep::Docs,
    ];

    #[must_use]
    pub fn runs_in(self, profile: CiProfile) -> bool {
        !matches!((self, profile), (CiStep::Docs, CiProfile::SkipDocs))
    }

    fn action(self) -> Action {
        match self {
            CiStep::Fmt => Action::Tools(vec![ToolCall::cargo(&["fmt", "--all", "--check"])]),
            CiStep::Clippy => Action::Tools(vec![ToolCall::cargo(&[
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--locked",
                "--",
                "-D",
                "warnings",
            ])]),
            CiStep::Nextest => Action::Tools(vec![ToolCall::cargo(&[
                "nextest",
                "run",
                "--workspace",
                "--all-features",
                "--locked",
            ])]),
            CiStep::Deny => Action::Tools(vec![ToolCall::cargo(&["deny", "--locked", "check"])]),
            CiStep::Shear => Action::Tools(vec![ToolCall::cargo(&["shear", "--deny-warnings"])]),
            CiStep::Comments => Action::Check(&CommentsCheck),
            CiStep::Layers => Action::Check(&LayersCheck),
            CiStep::Containers => Action::Check(&ContainersCheck),
            CiStep::Reuse => Action::Tools(vec![ToolCall::named("reuse", &["lint"])]),
            CiStep::Docs => Action::Tools(vec![
                ToolCall::named("bun", &["install", "--frozen-lockfile"]).in_dir("docs"),
                ToolCall::named("bun", &["run", "build"]).in_dir("docs"),
            ]),
        }
    }
}

enum Action {
    Tools(Vec<ToolCall>),
    Check(&'static dyn Check),
}

struct ToolCall {
    program: Option<&'static str>,
    args: &'static [&'static str],
    dir: &'static str,
}

impl ToolCall {
    fn cargo(args: &'static [&'static str]) -> Self {
        Self {
            program: None,
            args,
            dir: "",
        }
    }

    fn named(program: &'static str, args: &'static [&'static str]) -> Self {
        Self {
            program: Some(program),
            args,
            dir: "",
        }
    }

    fn in_dir(mut self, dir: &'static str) -> Self {
        self.dir = dir;
        self
    }

    fn program(&self) -> OsString {
        self.program.map_or_else(cargo_program, OsString::from)
    }
}

#[derive(Debug, Error)]
pub enum CiFailure {
    #[error("{step:?} reported {} violations", violations.len())]
    Violations {
        step: CiStep,
        violations: Vec<Violation>,
    },
    #[error(transparent)]
    Tool(#[from] CiError),
}

#[derive(Debug, Error)]
pub enum CiError {
    #[error("{step:?} failed with status {status}")]
    ToolFailed { step: CiStep, status: i32 },
    #[error("cannot start {program}: {source}")]
    Spawn { program: String, source: io::Error },
    #[error("cargo metadata failed: {0}")]
    Metadata(String),
    #[error("git ls-files failed: {0}")]
    ListFiles(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub fn run_ci(profile: CiProfile, repo: &Repo) -> Result<(), CiFailure> {
    let mut stderr = io::stderr().lock();
    for step in CiStep::ALL.into_iter().filter(|step| step.runs_in(profile)) {
        writeln!(stderr, "xtask: {step:?}").map_err(CiError::from)?;
        match step.action() {
            Action::Tools(calls) => {
                for call in &calls {
                    run_tool(step, call, repo)?;
                }
            }
            Action::Check(check) => {
                let violations = check.run(repo);
                if !violations.is_empty() {
                    return Err(CiFailure::Violations { step, violations });
                }
            }
        }
    }
    Ok(())
}

fn run_tool(step: CiStep, call: &ToolCall, repo: &Repo) -> Result<(), CiError> {
    let program = call.program();
    let status = Command::new(&program)
        .args(call.args)
        .current_dir(repo.root().join(call.dir))
        .status()
        .map_err(|source| CiError::Spawn {
            program: program.to_string_lossy().into_owned(),
            source,
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(CiError::ToolFailed {
            step,
            status: status.code().unwrap_or(-1),
        })
    }
}
