// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `validate`.
use super::*;
use crate::manifest::load_role_manifest;
use tempfile::tempdir;

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

/// Pin the orphan-codex-table warning: a manifest with `[codex]`
/// populated but codex absent from `agents` is dead config, and
/// the operator gets a warning that points at the fix instead of
/// having to debug "agent does not support codex" at load time.
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

/// Symmetric warning for the rare reverse case: `[claude]` populated
/// but claude absent from `agents`.
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

/// Property: parse+validate never panics; Ok/Err is total for any [`RoleManifest`].
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

/// Property: unknown top-level fields are rejected by serde (`deny_unknown_fields`).
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
