// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn is_valid_env_var_name_accepts_standard_names() {
    assert!(is_valid_env_var_name("FOO"));
    assert!(is_valid_env_var_name("_PRIVATE"));
    assert!(is_valid_env_var_name("FOO_BAR_123"));
    assert!(is_valid_env_var_name("mixedCase"));
}

#[test]
fn is_valid_env_var_name_rejects_invalid_names() {
    assert!(!is_valid_env_var_name(""));
    assert!(!is_valid_env_var_name("1FOO"));
    assert!(!is_valid_env_var_name("MY-VAR"));
    assert!(!is_valid_env_var_name("MY.VAR"));
    assert!(!is_valid_env_var_name("MY$VAR"));
    assert!(!is_valid_env_var_name("A}B"));
}

#[test]
fn rejects_empty_supported_list() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = []

[claude]
plugins = []
"#,
    )
    .unwrap();

    let err = load_role_manifest(temp.path()).unwrap_err();
    assert!(err.to_string().contains("must not be empty"));
}

#[test]
fn rejects_codex_supported_without_codex_table() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "codex"]

[claude]
plugins = []
"#,
    )
    .unwrap();

    let err = load_role_manifest(temp.path()).unwrap_err();
    assert!(err.to_string().contains("[codex]"));
}

#[test]
fn rejects_amp_supported_without_amp_table() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "amp"]

[claude]
plugins = []
"#,
    )
    .unwrap();

    let err = load_role_manifest(temp.path()).unwrap_err();
    assert!(err.to_string().contains("[amp]"));
}

#[test]
fn legacy_manifest_with_claude_passes() {
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
    let warnings = validate_role_manifest(&manifest).unwrap();
    assert!(warnings.is_empty());
}

#[test]
fn warns_when_codex_table_present_without_codex_in_supported() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude"]

[claude]
plugins = []

[codex]
model = "gpt-5"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let warnings = validate_role_manifest(&manifest).unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].message.contains("[codex]"));
    assert!(warnings[0].message.contains("ignored"));
}

#[test]
fn warns_when_amp_table_present_without_amp_in_supported() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude"]

[claude]
plugins = []

[amp]
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let warnings = validate_role_manifest(&manifest).unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].message.contains("[amp]"));
    assert!(warnings[0].message.contains("ignored"));
}

#[test]
fn warns_when_claude_table_present_without_claude_in_supported() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[claude]
plugins = []

[codex]
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let warnings = validate_role_manifest(&manifest).unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].message.contains("[claude]"));
}

#[test]
fn validate_rejects_non_interactive_without_default() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("FOO"));
}

#[test]
fn validate_rejects_options_without_interactive() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
options = ["a", "b"]
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("options"));
}

#[test]
fn validate_rejects_dangling_depends_on() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.BRANCH]
interactive = true
depends_on = ["env.NONEXISTENT"]
prompt = "Branch:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("NONEXISTENT"));
}

#[test]
fn validate_rejects_self_referencing_depends_on() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
depends_on = ["env.FOO"]
prompt = "Value:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("self"));
}

#[test]
fn validate_rejects_dependency_cycle() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.A]
interactive = true
depends_on = ["env.B"]
prompt = "A:"

[env.B]
interactive = true
depends_on = ["env.A"]
prompt = "B:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("cycle"));
}

#[test]
fn validate_rejects_depends_on_without_env_prefix() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
prompt = "Project:"

[env.BRANCH]
interactive = true
depends_on = ["PROJECT"]
prompt = "Branch:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("env."));
}

#[test]
fn validate_accepts_valid_manifest_with_env() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.RUNTIME]
default = "docker"

[env.PROJECT]
interactive = true
options = ["a", "b"]
prompt = "Pick:"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch:"
default = "main"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let warnings = validate_role_manifest(&manifest).unwrap();

    assert!(warnings.is_empty());
}

#[test]
fn validate_rejects_reserved_claude_env_name() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.JACKIN]
default = "docker"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("JACKIN"));
}
