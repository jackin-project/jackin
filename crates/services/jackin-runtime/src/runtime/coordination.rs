// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Lock files and state directories for runtime coordination.
//!
//! Moved to [`jackin_runtime_coordination::coordination`]; this module keeps the
//! `jackin_runtime::runtime::coordination::*` paths stable for existing callers.

pub(crate) use jackin_runtime_coordination::coordination::*;
