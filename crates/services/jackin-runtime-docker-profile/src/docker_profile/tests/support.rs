// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) fn tmpfs_paths_from_flags(flags: &[String]) -> Vec<&str> {
    flags
        .iter()
        .enumerate()
        .filter(|(i, _)| *i > 0 && flags.get(*i - 1).is_some_and(|f| f == "--tmpfs"))
        .map(|(_, v)| v.split(':').next().unwrap_or(""))
        .collect()
}
