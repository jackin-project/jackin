// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Feature-gated host-daemon spike.
//!
//! Moved to [`jackin_runtime_reactive_daemon::reactive_daemon`]; this module
//! keeps the `jackin_runtime::reactive_daemon::*` paths stable for existing
//! callers. The `daemon-spike` + unix gate still lives on the `mod`
//! declaration in `lib.rs`.

pub use jackin_runtime_reactive_daemon::reactive_daemon::*;
