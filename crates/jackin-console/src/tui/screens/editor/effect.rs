// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Editor-screen TUI effect vocabulary.
//!
//! Root-crate executors own side effects that need config paths, runtime
//! services, or credential adapters. Root-independent editor effects belong
//! here as they are introduced.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorEffect {}
