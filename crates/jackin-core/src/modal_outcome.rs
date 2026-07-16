// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// Product-owned lifecycle result for jackin❯ modal workflows.
///
/// `TermRock` owns reusable widget interaction outcomes. Committing or closing a
/// product workflow is application policy, so this vocabulary lives in
/// jackin-core and can be shared across jackin❯ surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalOutcome<T> {
    /// Keep the current modal workflow open.
    Continue,
    /// Close the current modal workflow without applying a value.
    Cancel,
    /// Complete the current modal workflow with its product value.
    Commit(T),
}
