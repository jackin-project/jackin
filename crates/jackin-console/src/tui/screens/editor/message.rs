// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Editor-screen TUI message vocabulary.
//!
//! Root-crate editor messages still live in `src/console/manager/message.rs`
//! while they carry root-only config and workspace types. This module is the
//! screen-local home for root-independent editor messages as the migration
//! continues.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMessage {
    FocusTabBar,
    FocusContent,
}
