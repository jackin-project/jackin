// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::path::PathBuf;

use serde::Serialize;
use thiserror::Error;

use crate::boundary::FsError;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct HostPath(PathBuf);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SandboxPath(Cow<'static, str>);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct RepoPath(String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ProjectRoot(HostPath);

#[derive(Debug, Error)]
pub enum PathError {
    #[error("path is not absolute: {0}")]
    NotAbsolute(String),
    #[error("path is not relative: {0}")]
    NotRelative(String),
    #[error("path contains a parent segment: {0}")]
    ParentSegment(String),
    #[error("path contains an empty segment: {0}")]
    EmptySegment(String),
    #[error("not a directory: {0:?}")]
    NotADirectory(HostPath),
    #[error("not a git work tree: {0:?}")]
    NotAGitWorkTree(HostPath),
    #[error(transparent)]
    Fs(#[from] FsError),
}
