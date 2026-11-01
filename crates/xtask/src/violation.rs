// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::ci::CiStep;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub step: CiStep,
    pub path: Option<PathBuf>,
    pub line: Option<u32>,
    pub message: String,
}
