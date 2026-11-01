// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ImageTag, ProjectRoot, SandboxSlug};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContractGroup {
    Core,
    Transfer,
    Terminal,
}

#[derive(Clone, Debug)]
pub struct ContractFixture {
    pub image: ImageTag,
    pub project: ProjectRoot,
    pub name_prefix: SandboxSlug,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractReport {
    pub checks: Vec<ContractCheck>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractCheck {
    pub name: &'static str,
    pub outcome: Result<(), String>,
}
