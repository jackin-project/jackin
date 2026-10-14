// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub fn branch_name(selector: &str, suffix: Option<&str>) -> String {
    suffix.map_or_else(
        || format!("jackin/scratch/{selector}"),
        |s| match selector.rsplit_once('/') {
            Some((ns, last)) => format!("jackin/scratch/{ns}/{last}-{s}"),
            None => format!("jackin/scratch/{selector}-{s}"),
        },
    )
}

#[cfg(test)]
mod tests;
