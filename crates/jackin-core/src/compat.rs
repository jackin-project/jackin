// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compatibility {
    pub openshell_version: String,
    pub openshell_tag: String,
    pub openshell_commit: String,
    pub codex_version: String,
}

#[derive(Debug, Error)]
pub enum CompatibilityError {
    #[error("syntax error in compatibility.lock.toml: {message}")]
    Syntax { message: String },
    #[error("missing key {key} in compatibility.lock.toml")]
    MissingField { key: String },
}
