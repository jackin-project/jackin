// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn validate_rejects_when_one_of_multiple_refs_is_invalid() {
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

[env.LABEL]
interactive = true
depends_on = ["env.PROJECT"]
prompt = "Label for ${env.PROJECT} in ${env.MISSING}:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("MISSING"));
}

#[test]
fn validate_rejects_empty_env_ref_in_prompt() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
prompt = "Value: ${env.}"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("empty"));
}

#[test]
fn validate_rejects_invalid_var_name_in_interpolation_ref() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
depends_on = []
prompt = "Value: ${env.MY-VAR}"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("invalid env var name")
    );
}

#[test]
fn validate_rejects_empty_env_ref_in_default() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
prompt = "Value:"
default = "prefix-${env.}"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("empty"));
}

#[test]
fn validate_rejects_empty_depends_on_name() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
depends_on = ["env."]
prompt = "Value:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("empty"));
}

#[test]
fn validate_rejects_invalid_depends_on_name() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.FOO]
interactive = true
depends_on = ["env.MY-VAR"]
prompt = "Value:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("invalid env var name")
    );
}

#[test]
fn validate_rejects_duplicate_depends_on() {
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
depends_on = ["env.PROJECT", "env.PROJECT"]
prompt = "Branch:"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    let result = validate_role_manifest(&manifest);

    assert!(result.unwrap_err().to_string().contains("duplicate"));
}

#[test]
fn prop_validate_never_panics_on_parsed_manifest() {
    use proptest::prelude::*;

    proptest!(|(dockerfile in "[A-Za-z0-9_./-]{1,64}", junk in ".{0,48}")| {
        let text = format!(
            "version = \"v1alpha6\"\ndockerfile = \"{dockerfile}\"\n{junk}\n"
        );
        // Parsing may fail (deny_unknown / invalid TOML) — never panic either path.
        if let Ok(manifest) = toml::from_str::<RoleManifest>(&text) {
            // Validation is total: Ok(warnings) or Err; never panic.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                drop(validate_role_manifest(&manifest));
                drop(validate_agent_consistency(&manifest));
            }));
            prop_assert!(result.is_ok(), "validate must not panic");
        }
    });
}

#[test]
fn prop_unknown_fields_rejected() {
    use proptest::prelude::*;

    proptest!(|(field in "[a-z][a-z0-9_]{0,12}", value in "[A-Za-z0-9_-]{0,32}")| {
        prop_assume!(
            field != "version"
                && field != "dockerfile"
                && field != "published_image"
                && field != "identity"
                && field != "agents"
                && field != "claude"
                && field != "codex"
                && field != "amp"
                && field != "kimi"
                && field != "opencode"
                && field != "grok"
                && field != "hooks"
                && field != "env"
                && field != "docker"
        );
        let text = format!(
            "version = \"v1alpha6\"\ndockerfile = \"Dockerfile\"\n{field} = \"{value}\"\n"
        );
        let err = toml::from_str::<RoleManifest>(&text).expect_err("unknown field must fail");
        let msg = err.to_string();
        prop_assert!(
            msg.contains("unknown") || msg.contains(&field) || msg.contains("did you mean"),
            "unexpected parse error for unknown field: {msg}"
        );
    });
}
