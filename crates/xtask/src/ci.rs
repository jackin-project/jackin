// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::Args;
use thiserror::Error;

use crate::violation::Violation;

#[derive(Args, Clone, Copy, Debug)]
pub struct CiArgs {
    #[arg(long, help = "Skip the docs build; same steps as --no-docs")]
    pub fast: bool,
    #[arg(long = "no-docs", help = "Skip the docs build")]
    pub no_docs: bool,
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
    #[error("cargo metadata failed: {0}")]
    Metadata(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
