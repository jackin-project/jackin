// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! File-browser listing data shared between service adapters and TUI state.

use std::path::PathBuf;

/// One row in the folder listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    /// Display name, no trailing slash. `".."` for the synthetic parent link.
    pub name: String,
    /// Absolute path the row resolves to. For `..` this is the parent dir.
    pub path: PathBuf,
    /// True for the synthetic `..` parent-link row.
    pub is_parent: bool,
    /// True iff `path` contains a `.git` child (dir or submodule file).
    pub is_git: bool,
}

/// Fully-resolved directory listing handed to the TUI state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderListing {
    pub root: PathBuf,
    pub cwd: PathBuf,
    pub entries: Vec<FolderEntry>,
}
