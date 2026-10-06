// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn loads_manifest_with_agents_field() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "codex", "amp"]

[claude]
plugins = []

[codex]

[amp]
"#,
    )
    .unwrap();

    let m = load_role_manifest(temp.path()).unwrap();
    assert_eq!(
        m.supported_agents(),
        vec![
            jackin_core::Agent::Claude,
            jackin_core::Agent::Codex,
            jackin_core::Agent::Amp
        ]
    );
    assert!(m.codex.is_some());
    assert!(m.amp.is_some());
}

#[test]
fn loads_architect_manifest_from_immutable_ci_snapshot() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/jackin-xtask/tests/fixtures/architect");
    assert!(
        fixture.join(MANIFEST_FILENAME).is_file(),
        "Architect snapshot must provide Jackin's current manifest path"
    );
    let manifest = load_role_manifest(&fixture).unwrap();

    assert_eq!(manifest.version, jackin_core::CURRENT_MANIFEST_VERSION);
    assert_eq!(
        manifest.supported_agents(),
        vec![
            jackin_core::Agent::Claude,
            jackin_core::Agent::Codex,
            jackin_core::Agent::Amp,
            jackin_core::Agent::Opencode,
            jackin_core::Agent::Kimi,
            jackin_core::Agent::Grok,
        ]
    );
}

#[test]
fn legacy_manifest_without_agents_field_defaults_to_claude_only() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha2"
dockerfile = "Dockerfile"

[claude]
model = "sonnet"
plugins = []
"#,
    )
    .unwrap();

    let m = load_role_manifest(temp.path()).unwrap();
    assert_eq!(m.supported_agents(), vec![jackin_core::Agent::Claude]);
    assert_eq!(m.claude.as_ref().unwrap().model.as_deref(), Some("sonnet"));
}

#[test]
fn loads_codex_only_manifest() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["codex"]

[codex]
model = "gpt-5"
"#,
    )
    .unwrap();

    let m = load_role_manifest(temp.path()).unwrap();
    assert_eq!(m.supported_agents(), vec![jackin_core::Agent::Codex]);
    assert_eq!(m.codex.as_ref().unwrap().model.as_deref(), Some("gpt-5"));
}

#[test]
fn loads_opencode_manifest_with_model() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["opencode"]

[opencode]
model = "zai-coding-plan/glm-5.1"
"#,
    )
    .unwrap();

    let m = load_role_manifest(temp.path()).unwrap();
    assert_eq!(m.supported_agents(), vec![jackin_core::Agent::Opencode]);
    assert_eq!(
        m.opencode.as_ref().unwrap().model.as_deref(),
        Some("zai-coding-plan/glm-5.1")
    );
}

#[test]
fn rejects_removed_provider_model_overrides() {
    for agent in ["claude", "codex", "opencode"] {
        let manifest = format!(
            "version = \"v1alpha6\"\ndockerfile = \"Dockerfile\"\nagents = [\"{agent}\"]\n[{agent}.providers.minimax]\nmodel = \"old-model\"\n"
        );
        let error = toml::from_str::<RoleManifest>(&manifest).unwrap_err();
        assert!(error.to_string().contains("unknown field `providers`"));
    }
}

#[test]
fn rejects_unknown_agent_name() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["claude", "foo"]

[claude]
plugins = []
"#,
    )
    .unwrap();

    let err = load_role_manifest(temp.path()).unwrap_err();
    let chain = format!("{err:#}");
    assert!(
        chain.contains("foo") || chain.contains("unknown"),
        "{chain}"
    );
}

#[test]
fn loads_unversioned_manifest_without_newer_features() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();
    assert_eq!(
        manifest.supported_agents(),
        vec![jackin_core::Agent::Claude]
    );
}

#[test]
fn rejects_newer_manifest_version() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v2alpha1"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let err = load_role_manifest(temp.path()).unwrap_err();
    let chain = format!("{err:#}");
    assert!(chain.contains("only understands up to v1alpha7"), "{chain}");
}

#[test]
fn rejects_old_manifest_version_using_opencode_agent() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha2"
dockerfile = "Dockerfile"
agents = ["opencode"]

[opencode]
"#,
    )
    .unwrap();

    let err = load_role_manifest(temp.path()).unwrap_err();
    let chain = format!("{err:#}");
    assert!(chain.contains("requires v1alpha3"), "{chain}");
    assert!(chain.contains("jackin role migrate"), "{chain}");
}

#[test]
fn rejects_old_manifest_version_with_opencode_table() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha2"
dockerfile = "Dockerfile"

[claude]
plugins = []

[opencode]
"#,
    )
    .unwrap();

    let err = load_role_manifest(temp.path()).unwrap_err();
    let chain = format!("{err:#}");
    assert!(chain.contains("requires v1alpha3"), "{chain}");
}

#[test]
fn rejects_v1alpha3_manifest_using_kimi_agent() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"
agents = ["kimi"]

[kimi]
"#,
    )
    .unwrap();

    let err = load_role_manifest(temp.path()).unwrap_err();
    let chain = format!("{err:#}");
    assert!(chain.contains("requires v1alpha4"), "{chain}");
    assert!(chain.contains("jackin role migrate"), "{chain}");
}

#[test]
fn rejects_v1alpha3_manifest_with_kimi_table() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []

[kimi]
"#,
    )
    .unwrap();

    let err = load_role_manifest(temp.path()).unwrap_err();
    let chain = format!("{err:#}");
    assert!(chain.contains("requires v1alpha4"), "{chain}");
}

#[test]
fn rejects_old_manifest_using_provider_overrides() {
    // Removed provider settings are rejected regardless of manifest version.
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha4"
dockerfile = "Dockerfile"
agents = ["opencode"]

[opencode]
model = "zai-coding-plan/glm-5.1"

[opencode.providers.minimax]
model = "minimax/MiniMax-M3"
"#,
    )
    .unwrap();

    let err = load_role_manifest(temp.path()).unwrap_err();
    let chain = format!("{err:#}");
    assert!(chain.contains("unknown field `providers`"), "{chain}");
}

#[test]
fn rejects_old_manifest_using_docker_settings() {
    // A pre-v1alpha6 manifest that uses the role [docker] block is rejected
    // with a migrate hint, since the feature did not exist at that version.
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"

[docker]
min_profile = "hardened"
"#,
    )
    .unwrap();

    let err = load_role_manifest(temp.path()).unwrap_err();
    let chain = format!("{err:#}");
    assert!(chain.contains("requires v1alpha6"), "{chain}");
}

#[test]
fn loads_manifest_with_plugins() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = ["code-review@claude-plugins-official"]
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    assert_eq!(manifest.dockerfile, "Dockerfile");
    assert!(manifest.claude.as_ref().unwrap().marketplaces.is_empty());
    assert_eq!(manifest.claude.as_ref().unwrap().plugins.len(), 1);
    assert!(manifest.identity.is_none());
}

#[test]
fn loads_manifest_with_marketplaces_and_plugins() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = ["superpowers@superpowers-marketplace"]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
sparse = ["plugins", ".claude-plugin"]
"#,
    )
    .unwrap();

    let manifest = load_role_manifest(temp.path()).unwrap();

    assert_eq!(
        manifest.claude.as_ref().unwrap().plugins,
        vec!["superpowers@superpowers-marketplace"]
    );
    assert_eq!(manifest.claude.as_ref().unwrap().marketplaces.len(), 1);
    assert_eq!(
        manifest.claude.as_ref().unwrap().marketplaces[0],
        ClaudeMarketplaceConfig {
            source: "obra/superpowers-marketplace".to_owned(),
            sparse: vec!["plugins".to_owned(), ".claude-plugin".to_owned()],
        }
    );
}
