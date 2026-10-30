// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn extract_block<'a>(haystack: &'a str, start: &str, end: &str) -> &'a str {
    haystack
        .split_once(start)
        .unwrap_or_else(|| panic!("missing block start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing block end: {end}"))
        .0
}

pub(super) fn minimal_role_repo(repo: &Path) {
    std::fs::write(
        repo.join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
}
