// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Workspace isolation drift detection: find mounts whose `src` changed
//! while containers hold preserved isolation state.
//!
//! Moved to [`jackin_runtime_drift::drift`]; this module keeps the
//! `jackin_runtime::runtime::drift::*` paths stable for existing callers.

pub use jackin_runtime_drift::drift::*;
