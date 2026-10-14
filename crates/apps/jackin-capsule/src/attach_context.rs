// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::session::SESSION_ENV_PASSTHROUGH;

#[must_use]
pub fn collect_session_env(include: bool) -> Vec<(String, String)> {
    if !include {
        return Vec::new();
    }
    SESSION_ENV_PASSTHROUGH
        .iter()
        .filter_map(|&key| std::env::var(key).ok().map(|value| (key.to_owned(), value)))
        .collect()
}
