use super::{AppConfig, AuthForwardMode, ClaudeRoleConfig, RoleSource};
use crate::selector::RoleSelector;

pub const BUILTIN_ROLES: &[(&str, &str)] = &[
    (
        "agent-smith",
        "https://github.com/jackin-project/jackin-agent-smith.git",
    ),
    (
        "the-architect",
        "https://github.com/jackin-project/jackin-the-architect.git",
    ),
];

impl AppConfig {
    /// Resolve an existing role source or derive a new one from the selector.
    ///
    /// Returns `(source, is_new)`. When `is_new` is `true` the source has been
    /// inserted into the in-memory config but **not** persisted — the caller
    /// should call [`save`] after validating that the repository is reachable.
    pub fn resolve_role_source(
        &mut self,
        selector: &RoleSelector,
    ) -> anyhow::Result<(RoleSource, bool)> {
        if let Some(source) = self.roles.get(&selector.key()) {
            return Ok((source.clone(), false));
        }

        let namespace = selector
            .namespace
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("unknown selector {}", selector.key()))?;

        let source = RoleSource {
            git: format!(
                "https://github.com/{namespace}/jackin-{}.git",
                selector.name
            ),
            trusted: false,
            claude: None,
            env: std::collections::BTreeMap::new(),
        };
        self.roles.insert(selector.key(), source.clone());
        Ok((source, true))
    }

    /// Resolve the effective `AuthForwardMode` for a given role.
    ///
    /// Resolution order: per-role override → global default → `Sync`.
    pub fn resolve_auth_forward_mode(&self, agent_key: &str) -> AuthForwardMode {
        self.roles
            .get(agent_key)
            .and_then(|a| a.claude.as_ref())
            .and_then(|c| c.auth_forward)
            .unwrap_or(self.claude.auth_forward)
    }

    /// Set the per-role auth forward mode override.
    // pub(crate): test-only affordance; production callers use ConfigEditor.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn set_agent_auth_forward(&mut self, key: &str, mode: AuthForwardMode) {
        if let Some(source) = self.roles.get_mut(key) {
            let claude = source.claude.get_or_insert_with(ClaudeRoleConfig::default);
            claude.auth_forward = Some(mode);
        }
    }

    /// Mark an role source as trusted.  Returns `true` when the flag changed.
    // pub(crate): test-only affordance; production callers use ConfigEditor.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn trust_agent(&mut self, key: &str) -> bool {
        if let Some(source) = self.roles.get_mut(key)
            && !source.trusted
        {
            source.trusted = true;
            return true;
        }
        false
    }

    /// Revoke trust for an role source.  Returns `true` when the flag changed.
    /// Note: does not prevent revoking builtins — the caller should check
    /// [`is_builtin_agent`] first.
    // pub(crate): test-only affordance; production callers use ConfigEditor.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn untrust_agent(&mut self, key: &str) -> bool {
        if let Some(source) = self.roles.get_mut(key)
            && source.trusted
        {
            source.trusted = false;
            return true;
        }
        false
    }

    /// Returns `true` when `key` matches a built-in role shipped with the
    /// binary.  Built-in roles are always trusted and cannot be revoked.
    pub fn is_builtin_agent(key: &str) -> bool {
        BUILTIN_ROLES.iter().any(|&(name, _)| name == key)
    }

    /// Ensures all built-in role entries match the current binary version.
    /// Returns `true` if any entries were added or updated.
    pub(super) fn sync_builtin_agents(&mut self) -> bool {
        let mut changed = false;
        for &(name, git) in BUILTIN_ROLES {
            let existing_claude = self.roles.get(name).and_then(|a| a.claude.clone());
            let expected = RoleSource {
                git: git.to_string(),
                trusted: true,
                claude: existing_claude,
                env: std::collections::BTreeMap::new(),
            };
            match self.roles.get(name) {
                Some(existing) if existing.git == expected.git && existing.trusted => {}
                _ => {
                    self.roles.insert(name.to_string(), expected);
                    changed = true;
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::JackinPaths;
    use crate::selector::RoleSelector;
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
            "https://github.com/chainargos/jackin-the-architect.git"
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
    fn auth_forward_defaults_to_sync() {
        let config = AppConfig::default();
        assert_eq!(config.claude.auth_forward, AuthForwardMode::Sync);
    }

    #[test]
    fn deserializes_global_claude_auth_forward() {
        let toml_str = r#"
[claude]
auth_forward = "sync"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.claude.auth_forward, AuthForwardMode::Sync);
    }

    #[test]
    fn deserializes_per_agent_claude_auth_forward() {
        let toml_str = r#"
[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[roles.agent-smith.claude]
auth_forward = "ignore"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let role = config.roles.get("agent-smith").unwrap();
        assert_eq!(
            role.claude.as_ref().unwrap().auth_forward,
            Some(AuthForwardMode::Ignore)
        );
    }

    #[test]
    fn resolve_auth_forward_defaults_to_sync() {
        let config = AppConfig::default();
        assert_eq!(
            config.resolve_auth_forward_mode("nonexistent"),
            AuthForwardMode::Sync
        );
    }

    #[test]
    fn resolve_auth_forward_uses_global_setting() {
        let toml_str = r#"
[claude]
auth_forward = "sync"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.resolve_auth_forward_mode("agent-smith"),
            AuthForwardMode::Sync
        );
    }

    #[test]
    fn resolve_auth_forward_per_agent_overrides_global() {
        let toml_str = r#"
[claude]
auth_forward = "sync"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[roles.agent-smith.claude]
auth_forward = "ignore"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.resolve_auth_forward_mode("agent-smith"),
            AuthForwardMode::Ignore
        );
    }

    #[test]
    fn auth_forward_round_trips_through_toml() {
        let toml_str = r#"
[claude]
auth_forward = "sync"

[roles.agent-smith]
git = "https://github.com/jackin-project/jackin-agent-smith.git"

[roles.agent-smith.claude]
auth_forward = "ignore"
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let reloaded: AppConfig = toml::from_str(&serialized).unwrap();
        assert_eq!(reloaded.claude.auth_forward, AuthForwardMode::Sync);
        assert_eq!(
            reloaded.resolve_auth_forward_mode("agent-smith"),
            AuthForwardMode::Ignore
        );
    }
}
