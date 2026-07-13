// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Attach-session context: state for a single host connection to the daemon,
//! including PTY ownership, focus, and environment passthrough.
//!
//! Not responsible for: persistent container identity (see
//! `container_context`) or socket framing (see `socket`).

use crate::session::SESSION_ENV_PASSTHROUGH;

pub fn collect_session_env(include: bool) -> Vec<(String, String)> {
    if !include {
        return Vec::new();
    }
    SESSION_ENV_PASSTHROUGH
        .iter()
        .filter_map(|&key| std::env::var(key).ok().map(|value| (key.to_owned(), value)))
        .collect()
}
