// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::ci::CiStep;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub step: CiStep,
    pub path: Option<PathBuf>,
    pub line: Option<u32>,
    pub message: String,
}

impl Violation {
    pub(crate) fn at_line(
        step: CiStep,
        path: &Path,
        line: u32,
        message: impl Into<String>,
    ) -> Self {
        Self {
            step,
            path: Some(path.to_path_buf()),
            line: Some(line),
            message: message.into(),
        }
    }

    pub(crate) fn general(step: CiStep, message: impl Into<String>) -> Self {
        Self {
            step,
            path: None,
            line: None,
            message: message.into(),
        }
    }

    pub(crate) fn at_file(step: CiStep, path: &Path, message: impl Into<String>) -> Self {
        Self {
            step,
            path: Some(path.to_path_buf()),
            line: None,
            message: message.into(),
        }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(f, "{}", path.display())?;
            if let Some(line) = self.line {
                write!(f, ":{line}")?;
            }
            write!(f, ": ")?;
        }
        write!(f, "{}", self.message)
    }
}

pub(crate) fn sorted(mut violations: Vec<Violation>) -> Vec<Violation> {
    fn key(v: &Violation) -> (Option<&OsStr>, Option<u32>, &str) {
        (v.path.as_deref().map(Path::as_os_str), v.line, &v.message)
    }
    violations.sort_by(|a, b| key(a).cmp(&key(b)));
    violations
}
