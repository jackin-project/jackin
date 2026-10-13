// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn validate_rejects_reserved_dind_hostname_env_name() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.JACKIN_DIND_HOSTNAME]
default = "sidecar"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("JACKIN_DIND_HOSTNAME")
    );
}

#[test]
fn validate_rejects_reserved_docker_tls_env_vars() {
    for var in ["DOCKER_HOST", "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH"] {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.role.toml"),
            format!(
                r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.{var}]
default = "override"
"#
            ),
        )
        .unwrap();

        let manifest = load_role_manifest(temp.path()).unwrap();
        let result = validate_role_manifest(&manifest);

        assert!(
            result.unwrap_err().to_string().contains(var),
            "error message should mention {var}"
        );
    }
}

#[test]
fn validate_warns_on_prompt_without_interactive() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
prompt = "This is ignored"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let warnings = validate_role_manifest(&manifest).unwrap();

    assert!(!warnings.is_empty());
    assert!(warnings[0].message.contains("prompt"));
}

#[test]
fn validate_warns_on_skippable_without_interactive() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
default = "bar"
skippable = true
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let warnings = validate_role_manifest(&manifest).unwrap();

    assert!(!warnings.is_empty());
    assert!(warnings[0].message.contains("skippable"));
}

#[test]
fn validate_accepts_interpolation_in_prompt_and_default() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
options = ["project1", "project2"]
prompt = "Select a project:"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Branch name for ${env.PROJECT}:"
default = "feature/${env.PROJECT}"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let warnings = validate_role_manifest(&manifest).unwrap();

    assert!(warnings.is_empty());
}

#[test]
fn validate_rejects_interpolation_referencing_unknown_var() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.BRANCH]
interactive = true
depends_on = []
prompt = "Branch for ${env.NONEXISTENT}:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("NONEXISTENT"));
}

#[test]
fn validate_rejects_interpolation_not_in_depends_on() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
options = ["a", "b"]
prompt = "Select:"

[env.BRANCH]
interactive = true
prompt = "Branch for ${env.PROJECT}:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("PROJECT"));
    assert!(msg.contains("depends_on"));
}

#[test]
fn validate_rejects_interpolation_in_default_referencing_unknown_var() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.BRANCH]
interactive = true
depends_on = []
default = "feature/${env.GHOST}"
prompt = "Branch:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("GHOST"));
}

#[test]
fn validate_rejects_invalid_env_var_name() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env."MY-VAR"]
default = "value"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("MY-VAR"));
}

#[test]
fn validate_rejects_env_var_name_starting_with_digit() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env."1FOO"]
default = "value"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("1FOO"));
}

#[test]
fn validate_accepts_valid_env_var_names() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env._PRIVATE]
default = "a"

[env.UPPER_CASE_123]
default = "b"

[env.mixedCase]
default = "c"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    result.unwrap();
}

#[test]
fn validate_rejects_interpolation_in_options() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
options = ["a", "b"]
prompt = "Pick:"

[env.BRANCH]
interactive = true
depends_on = ["env.PROJECT"]
options = ["${env.PROJECT}-main", "${env.PROJECT}-dev"]
prompt = "Branch:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("options cannot contain interpolation")
    );
}

#[test]
fn validate_ignores_non_env_namespace_in_interpolation() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
prompt = "Value (use ${other.THING} for other):"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let warnings = validate_role_manifest(&manifest).unwrap();

    // ${other.THING} is not an env. ref, so no error or warning
    assert!(warnings.is_empty());
}

#[test]
fn validate_rejects_interpolation_in_default_not_in_depends_on() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
options = ["a", "b"]
prompt = "Select:"

[env.BRANCH]
interactive = true
prompt = "Branch:"
default = "feature/${env.PROJECT}"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("PROJECT"));
    assert!(msg.contains("depends_on"));
}
