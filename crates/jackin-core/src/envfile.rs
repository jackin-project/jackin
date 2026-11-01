// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use thiserror::Error;

use crate::ids::EnvName;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvFile(BTreeMap<EnvName, String>);

#[derive(Debug, Error)]
pub enum EnvFileError {
    #[error("invalid value for {name:?}")]
    InvalidValue { name: EnvName },
    #[error("syntax error at line {line}")]
    Syntax { line: usize },
}
