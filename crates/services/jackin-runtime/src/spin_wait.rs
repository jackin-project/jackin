// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Async spinner-wait helper for polling operations.
//!
//! Moved to [`jackin_runtime_spin_wait::spin_wait`]; this module keeps the
//! `jackin_runtime::spin_wait::*` paths stable for existing callers.

pub use jackin_runtime_spin_wait::spin_wait::*;
