// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side client for the capsule's `session.send` and `events` control
//! surface: type text into a running agent session, and watch that session's
//! state transitions, exits, and activity.
//!
//! Moved to [`jackin_runtime_session_control::session_control`]; this module
//! keeps the `jackin_runtime::runtime::session_control::*` paths stable for
//! existing callers.

pub use jackin_runtime_session_control::session_control::*;
