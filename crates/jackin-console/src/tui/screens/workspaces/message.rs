// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspaces-screen TUI message vocabulary.
//!
//! Root-crate workspace-list messages still live in
//! `src/console/manager/message.rs` while they carry root-only runtime and
//! launch types. This module is the screen-local home for root-independent
//! workspace-list messages as the migration continues.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspacesMessage {
    CollapseSelectedTree,
    ExpandSelectedTree,
}
