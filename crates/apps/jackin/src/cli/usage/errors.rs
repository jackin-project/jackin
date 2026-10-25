// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

pub(super) fn validate_watch_monitor_identifier(identifier: &str) -> Result<()> {
    let valid = !identifier.is_empty()
        && identifier.len() <= 128
        && identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(super::usage_error(
            "invalid_argument",
            "monitor ID is empty or outside its accepted bounds",
            3,
        ))
    }
}
