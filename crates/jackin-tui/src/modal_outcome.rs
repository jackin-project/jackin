// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// Lifecycle result shared by jackin❯ modal workflows.
///
/// `TermRock` owns reusable widget interaction outcomes. This type represents
/// product workflow policy shared across jackin❯ surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalOutcome<T> {
    /// Keep the current modal workflow open.
    Continue,
    /// Close the current modal workflow without applying a value.
    Cancel,
    /// Complete the current modal workflow with its product value.
    Commit(T),
}
