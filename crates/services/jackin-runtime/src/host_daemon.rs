// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host daemon backend over a Unix socket.
//!
//! Moved to [`jackin_runtime_host_daemon::host_daemon`]; this module keeps
//! the `jackin_runtime::host_daemon::*` paths stable for existing callers.

pub use jackin_runtime_host_daemon::host_daemon::*;
