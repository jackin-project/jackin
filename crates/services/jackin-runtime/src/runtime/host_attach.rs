// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-owned attach client for running Capsule daemons.
//!
//! Moved to [`jackin_runtime_host_attach::host_attach`]; this module keeps the
//! `jackin_runtime::runtime::host_attach::*` paths stable for existing callers.

pub use jackin_runtime_host_attach::host_attach::*;
