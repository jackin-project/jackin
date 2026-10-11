// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn loads_manifest_with_env_depends_on() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
prompt = "Select:"
options = ["a", "b"]

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    let var = &manifest.env["BRANCH"];
    assert_eq!(var.depends_on, vec!["env.PROJECT"]);
}

#[test]
fn loads_manifest_with_skippable_env() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.API_KEY]
interactive = true
skippable = true
prompt = "API key (optional):"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    let var = &manifest.env["API_KEY"];
    assert!(var.skippable);
}

#[test]
fn loads_manifest_without_env() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    assert!(manifest.env.is_empty());
}

#[test]
fn rejects_unknown_env_field() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
typo = true
"#,
    )
    .unwrap();

    let error = load_role_manifest(temp.path()).unwrap_err();

    assert!(format!("{error:#}").contains("unknown field"));
}
