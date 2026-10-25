// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

pub(super) struct MonitorId(String);

impl MonitorId {
    pub(super) fn parse(identifier: &str) -> Result<Self> {
        let valid = !identifier.is_empty()
            && identifier.len() <= 128
            && identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        if valid {
            Ok(Self(identifier.to_owned()))
        } else {
            Err(super::usage_error(
                "invalid_argument",
                "monitor ID is empty or outside its accepted bounds",
                3,
            ))
        }
    }

    pub(super) fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn into_string(self) -> String {
        self.0
    }
}
