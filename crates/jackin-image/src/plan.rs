// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{FileMode, HostPath, ImageTag};

use crate::key::ImageKey;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildStage {
    Construct,
    Role,
    Derived,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedBuild {
    pub stage: BuildStage,
    pub tag: ImageTag,
    pub args: Vec<String>,
    pub context: ContextSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextSource {
    RoleRepository(HostPath),
    Materialized(Vec<MaterializedFile>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub mode: FileMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalBuilds {
    pub key: ImageKey,
    pub role: PlannedBuild,
    pub derived: PlannedBuild,
}

#[derive(Clone, Debug)]
pub struct ImagePlan {}
