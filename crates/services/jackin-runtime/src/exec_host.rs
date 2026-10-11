// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side credential resolver for `jackin-exec`.
//!
//! Moved to [`jackin_runtime_exec_host::exec_host`]; this module keeps the
//! `jackin_runtime::exec_host::*` paths stable for existing callers.

pub use jackin_runtime_exec_host::exec_host::*;
