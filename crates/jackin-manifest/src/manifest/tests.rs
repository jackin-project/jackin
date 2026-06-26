//! Tests for `manifest`.
use super::*;
use tempfile::tempdir;

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
fn loads_per_provider_model_overrides() {
    use jackin_core::Agent;
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha5"
dockerfile = "Dockerfile"
agents = ["claude", "opencode"]

[claude]
model = "claude-sonnet-4-6"

[claude.providers.minimax]
model = "MiniMax-M3"

[opencode]
model = "zai-coding-plan/glm-5.1"

[opencode.providers.minimax]
model = "minimax/MiniMax-M3"

[opencode.providers.zai]
model = "zai-coding-plan/glm-5.1"
"#,
    )
    .unwrap();

    let m = load_role_manifest(temp.path()).unwrap();
    // Per-(agent, provider) override resolves; the agent default is untouched.
    assert_eq!(
        m.agent_provider_model(Agent::Claude, "minimax"),
        Some("MiniMax-M3")
    );
    assert_eq!(m.agent_model(Agent::Claude), Some("claude-sonnet-4-6"));
    assert_eq!(
        m.agent_provider_model(Agent::Opencode, "minimax"),
        Some("minimax/MiniMax-M3")
    );
    assert_eq!(
        m.agent_provider_model(Agent::Opencode, "zai"),
        Some("zai-coding-plan/glm-5.1")
    );
    // Unset pairs and unset agents return None.
    assert_eq!(m.agent_provider_model(Agent::Opencode, "kimi"), None);
    assert_eq!(m.agent_provider_model(Agent::Codex, "minimax"), None);
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
    assert!(chain.contains("only understands up to v1alpha6"), "{chain}");
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
    // A pre-v1alpha5 manifest that uses [<agent>.providers.<id>] is rejected
    // with a migrate hint, since the feature did not exist at that version.
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
    assert!(chain.contains("requires v1alpha5"), "{chain}");
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
