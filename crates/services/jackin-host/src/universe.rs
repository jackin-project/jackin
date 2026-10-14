// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(crate) fn env_flag_enabled(value: Option<impl AsRef<std::ffi::OsStr>>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let Some(value) = value.as_ref().to_str() else {
        return true;
    };
    !matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "0" | "false" | "no" | "off"
    )
}
