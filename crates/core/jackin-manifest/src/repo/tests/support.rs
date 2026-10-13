// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn empty_hook_error(field: &str, path: &str) -> String {
    let temp = tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("hooks")).unwrap();
    std::fs::write(temp.path().join(path), "").unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        format!(
            r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
{field} = "{path}"
"#
        ),
    )
    .unwrap();

    validate_role_repo(temp.path()).unwrap_err().to_string()
}
