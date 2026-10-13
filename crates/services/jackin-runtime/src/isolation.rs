// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Mount isolation: `MountIsolation` enum and the sub-modules that implement
//! the three isolation strategies.
//!
//! Moved to [`jackin_runtime_isolation::isolation`]; this module keeps the
//! `jackin_runtime::isolation::*` paths stable for existing callers.

pub use jackin_runtime_isolation::isolation::*;
