// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! List running and managed jackin role containers via Docker label queries.
//!
//! Moved to [`jackin_runtime_discovery::discovery`]; this module keeps the
//! `jackin_runtime::runtime::discovery::*` paths stable for existing callers.

pub use jackin_runtime_discovery::discovery::*;
