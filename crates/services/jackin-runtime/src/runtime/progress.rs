// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host wiring for launch progress: re-exports presentation types from
//! `jackin-launch` and installs host-terminal/desktop adapters.
//!
//! Moved to [`jackin_runtime_progress::progress`]; this module keeps the
//! `jackin_runtime::runtime::progress::*` paths stable for existing callers.

pub use jackin_runtime_progress::progress::*;
