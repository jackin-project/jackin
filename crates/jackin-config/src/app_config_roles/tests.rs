//! Tests for `roles` — tests.
use super::*;
use jackin_core::JackinPaths;
use jackin_core::RoleSelector;
use tempfile::tempdir;

#[test]
fn bootstrap_writes_builtin_agent_entries() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    let config = AppConfig::load_or_init(&paths).unwrap();

    assert_eq!(
        config.roles.get("agent-smith").unwrap().git,
        "https://github.com/jackin-project/jackin-agent-smith.git"
    );
    assert_eq!(
        config.roles.get("the-architect").unwrap().git,
        "https://github.com/jackin-project/jackin-the-architect.git"
    );
    assert!(paths.config_file.exists());
}

#[test]
fn sync_updates_stale_builtin_entries_and_preserves_user_agents() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    std::fs::write(
        &paths.config_file,
        r#"[roles.agent-smith]
git = "git@github.com:old/wrong-url.git"

[roles."chainargos/agent-brown"]
git = "git@github.com:chainargos/jackin-agent-brown.git"
"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();

    // Built-in entries are corrected
    assert_eq!(
        config.roles.get("agent-smith").unwrap().git,
        "https://github.com/jackin-project/jackin-agent-smith.git"
    );
    // Missing built-in entries are added
    assert_eq!(
        config.roles.get("the-architect").unwrap().git,
        "https://github.com/jackin-project/jackin-the-architect.git"
    );
    // User-added entries are preserved
    assert_eq!(
        config.roles.get("chainargos/agent-brown").unwrap().git,
        "git@github.com:chainargos/jackin-agent-brown.git"
    );

    // Config file is updated on disk
    let persisted = std::fs::read_to_string(&paths.config_file).unwrap();
    assert!(persisted.contains("jackin-project/jackin-agent-smith.git"));
    assert!(persisted.contains("jackin-project/jackin-the-architect.git"));
    assert!(persisted.contains("chainargos/jackin-agent-brown.git"));
}

#[test]
fn resolve_agent_source_adds_owner_repo_on_first_use() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");

    let (source, is_new) = config.resolve_role_source(&selector).unwrap();

    assert_eq!(
        source.git,
        "https://github.com/chainargos/the-architect.git"
    );
    assert!(is_new);

    // Not yet persisted — write via toml::to_string_pretty (AppConfig::save
    // was removed in Task 14; tests bootstrap the file directly).
    let contents = toml::to_string_pretty(&config).unwrap();
    std::fs::write(&paths.config_file, &contents).unwrap();
    assert!(
        std::fs::read_to_string(&paths.config_file)
            .unwrap()
            .contains("[roles.\"chainargos/the-architect\"]")
    );
}

// --- Trust model tests ---

#[test]
fn builtin_agents_are_trusted_on_bootstrap() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    let config = AppConfig::load_or_init(&paths).unwrap();

    assert!(config.roles.get("agent-smith").unwrap().trusted);
    assert!(config.roles.get("the-architect").unwrap().trusted);
}

#[test]
fn new_namespaced_agent_is_not_trusted() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");

    let (source, _) = config.resolve_role_source(&selector).unwrap();

    assert!(!source.trusted);
}

#[test]
fn trust_agent_marks_source_as_trusted() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");

    config.resolve_role_source(&selector).unwrap();
    assert!(
        !config
            .roles
            .get("chainargos/the-architect")
            .unwrap()
            .trusted
    );

    let changed = config.trust_agent("chainargos/the-architect");
    assert!(changed);
    assert!(
        config
            .roles
            .get("chainargos/the-architect")
            .unwrap()
            .trusted
    );

    // Second call is idempotent
    let changed_again = config.trust_agent("chainargos/the-architect");
    assert!(!changed_again);
}

#[test]
fn untrust_agent_revokes_trust() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");

    config.resolve_role_source(&selector).unwrap();
    config.trust_agent("chainargos/the-architect");
    assert!(
        config
            .roles
            .get("chainargos/the-architect")
            .unwrap()
            .trusted
    );

    let changed = config.untrust_agent("chainargos/the-architect");
    assert!(changed);
    assert!(
        !config
            .roles
            .get("chainargos/the-architect")
            .unwrap()
            .trusted
    );

    // Second call is idempotent
    let changed_again = config.untrust_agent("chainargos/the-architect");
    assert!(!changed_again);
}

#[test]
fn trusted_flag_round_trips_through_toml() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut config = AppConfig::load_or_init(&paths).unwrap();
    let selector = RoleSelector::new(Some("chainargos"), "the-architect");

    config.resolve_role_source(&selector).unwrap();
    config.trust_agent("chainargos/the-architect");
    // AppConfig::save removed in Task 14 — write the bootstrap file directly.
    let contents = toml::to_string_pretty(&config).unwrap();
    std::fs::write(&paths.config_file, &contents).unwrap();

    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    assert!(
        reloaded
            .roles
            .get("chainargos/the-architect")
            .unwrap()
            .trusted
    );
}

#[test]
fn sync_upgrades_untrusted_builtins() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    paths.ensure_base_dirs().unwrap();

    // Simulate a config from a pre-trust version (no trusted field)
    std::fs::write(
        &paths.config_file,
        r#"[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[roles.the-architect]
git = "https://github.com/jackin-project/jackin-the-architect.git"
"#,
    )
    .unwrap();

    let config = AppConfig::load_or_init(&paths).unwrap();

    // Builtins should be upgraded to trusted
    assert!(config.roles.get("agent-smith").unwrap().trusted);
    assert!(config.roles.get("the-architect").unwrap().trusted);
}

// ── Auth forwarding config tests ────────────────────────────────────

#[test]
fn deserializes_global_claude_auth_forward() {
    let toml_str = r#"
[claude]
auth_forward = "sync"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#;
    let config: AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.claude.as_ref().unwrap().auth_forward,
        AuthForwardMode::Sync
    );
}
