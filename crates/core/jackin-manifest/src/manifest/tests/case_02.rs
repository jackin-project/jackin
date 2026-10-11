// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn loads_manifest_marketplace_without_sparse() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[[claude.marketplaces]]
source = "jackin-project/jackin-marketplace"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    assert_eq!(manifest.claude.as_ref().unwrap().marketplaces.len(), 1);
    assert_eq!(
        manifest.claude.as_ref().unwrap().marketplaces[0],
        ClaudeMarketplaceConfig {
            source: "jackin-project/jackin-marketplace".to_owned(),
            sparse: vec![],
        }
    );
    assert!(manifest.claude.as_ref().unwrap().plugins.is_empty());
}

#[test]
fn loads_manifest_without_plugins_defaults_to_empty() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    assert!(manifest.claude.as_ref().unwrap().plugins.is_empty());
    assert_eq!(manifest.claude.as_ref().unwrap().marketplaces.len(), 1);
}

#[test]
fn loads_manifest_with_identity() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    assert_eq!(manifest.identity.as_ref().unwrap().name, "Agent Smith");
}

#[test]
fn display_name_uses_identity_when_present() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    assert_eq!(manifest.display_name("agent-smith"), "Agent Smith");
}

#[test]
fn display_name_falls_back_to_role_name() {
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

    assert_eq!(manifest.display_name("agent-smith"), "agent-smith");
}

#[test]
fn loads_manifest_with_published_image() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
published_image = "docker.io/myorg/my-role:latest"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    assert_eq!(
        manifest.published_image.as_deref(),
        Some("docker.io/myorg/my-role:latest")
    );
}

#[test]
fn loads_manifest_without_published_image() {
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

    assert!(manifest.published_image.is_none());
}

#[test]
fn rejects_unknown_top_level_field() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
unknown_field = true

[claude]
plugins = []
"#,
    )
    .unwrap();

    let error = load_role_manifest(temp.path()).unwrap_err();

    assert!(format!("{error:#}").contains("unknown field"));
}

#[test]
fn rejects_unknown_claude_field() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
typo = "oops"
"#,
    )
    .unwrap();

    let error = load_role_manifest(temp.path()).unwrap_err();

    assert!(format!("{error:#}").contains("unknown field"));
}

#[test]
fn rejects_unknown_identity_field() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[identity]
name = "Smith"
typo = true

[claude]
plugins = []
"#,
    )
    .unwrap();

    let error = load_role_manifest(temp.path()).unwrap_err();

    assert!(format!("{error:#}").contains("unknown field"));
}

#[test]
fn hook_entries_yield_runtime_contract_order() {
    let hooks = HooksConfig {
        setup_once: Some("a.sh".to_owned()),
        source: Some("b.sh".to_owned()),
        preflight: Some("c.sh".to_owned()),
    };
    let triples: Vec<_> = hooks
        .entries()
        .map(|e| (e.label, e.filename, e.path))
        .collect();
    assert_eq!(
        triples,
        [
            ("setup_once hook", "setup-once.sh", "a.sh"),
            ("source hook", "source.sh", "b.sh"),
            ("preflight hook", "preflight.sh", "c.sh"),
        ]
    );
}

#[test]
fn hook_entries_skip_absent_and_preserve_order() {
    // Mixed presence: only source + preflight. Order must follow
    // the canonical sequence, not the order fields are populated.
    let hooks = HooksConfig {
        setup_once: None,
        source: Some("b.sh".to_owned()),
        preflight: Some("c.sh".to_owned()),
    };
    let labels: Vec<_> = hooks.entries().map(|e| e.label).collect();
    assert_eq!(labels, ["source hook", "preflight hook"]);
}

#[test]
fn loads_manifest_with_hooks() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
setup_once = "hooks/setup-once.sh"
source = "hooks/source.sh"
preflight = "hooks/preflight.sh"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    let hooks = manifest.hooks.as_ref().unwrap();
    assert_eq!(hooks.setup_once.as_deref(), Some("hooks/setup-once.sh"));
    assert_eq!(hooks.source.as_deref(), Some("hooks/source.sh"));
    assert_eq!(hooks.preflight.as_deref(), Some("hooks/preflight.sh"));
}

#[test]
fn loads_manifest_without_hooks() {
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

    assert!(manifest.hooks.is_none());
}

#[test]
fn rejects_unknown_hooks_field() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[hooks]
post_launch = "bad"
"#,
    )
    .unwrap();

    let error = load_role_manifest(temp.path()).unwrap_err();

    assert!(format!("{error:#}").contains("unknown field"));
}

#[test]
fn loads_manifest_with_static_env() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.RUNTIME]
default = "docker"
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    assert_eq!(manifest.env.len(), 1);
    let var = &manifest.env["RUNTIME"];
    assert_eq!(var.default_value.as_deref(), Some("docker"));
    assert!(!var.interactive);
}

#[test]
fn loads_manifest_with_interactive_env() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[env.PROJECT]
interactive = true
prompt = "Select a project:"
options = ["project1", "project2"]
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    let var = &manifest.env["PROJECT"];
    assert!(var.interactive);
    assert_eq!(var.prompt.as_deref(), Some("Select a project:"));
    assert_eq!(var.options, vec!["project1", "project2"]);
}
