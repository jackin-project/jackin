// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Formatted prune/cleanup terminal output shared by runtime and diagnostics.
//!
//! Moved to [`jackin_runtime_prune_output::prune_output`]; this module keeps
//! the `jackin_runtime::runtime::prune_output::*` paths stable for existing callers.

pub use jackin_runtime_prune_output::prune_output::*;
