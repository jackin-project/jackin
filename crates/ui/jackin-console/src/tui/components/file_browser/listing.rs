// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    pub name: String,

    pub path: PathBuf,

    pub is_parent: bool,

    pub is_git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderListing {
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub entries: Vec<FolderEntry>,
}
