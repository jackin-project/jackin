// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Exit "still running" summary data.
//!
//! Moved to [`jackin_runtime_exit_summary::exit_summary`]; this module keeps
//! the `jackin_runtime::runtime::exit_summary::*` paths stable for existing
//! callers.

pub use jackin_runtime_exit_summary::exit_summary::*;
