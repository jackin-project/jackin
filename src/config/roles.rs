use super::{AppConfig, AuthForwardMode, GithubAuthMode, RoleSource};
use crate::agent::Agent;
use crate::operator_env::EnvValue;
use crate::selector::RoleSelector;
use std::collections::BTreeMap;

/// Resolve the effective auth-forward mode for an agent in a (workspace, role) scope.
///
/// Walks three layers, most-specific wins:
///
/// 1. `workspaces[ws].roles[role].<agent>.auth_forward`
/// 2. `workspaces[ws].<agent>.auth_forward`
/// 3. `<agent>.auth_forward` (global)
///
/// Returns [`AuthForwardMode::Sync`] if no layer is set. The `<agent>`
/// selector picks the matching per-agent field at each layer.
///
/// Passing `workspace = ""` (or any name not present in the config)
/// naturally falls through to the global layer; this is the supported
/// way for non-workspace-scoped callers (e.g. `jackin config auth show`)
/// to read the global default through the same code path.
pub fn resolve_mode(cfg: &AppConfig, agent: Agent, workspace: &str, role: &str) -> AuthForwardMode {
    // Layer 3 (most specific): workspace × role × agent
    if let Some(m) = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.roles.get(role))
        .and_then(|ro| match agent {
            Agent::Claude => ro.claude.as_ref().map(|c| c.auth_forward),
            Agent::Codex => ro.codex.as_ref().map(|c| c.auth_forward),
            Agent::Amp => ro.amp.as_ref().map(|c| c.auth_forward),
        })
    {
        return m;
    }

    // Layer 2: workspace × agent
    if let Some(m) = cfg.workspaces.get(workspace).and_then(|ws| match agent {
        Agent::Claude => ws.claude.as_ref().map(|c| c.auth_forward),
        Agent::Codex => ws.codex.as_ref().map(|c| c.auth_forward),
        Agent::Amp => ws.amp.as_ref().map(|c| c.auth_forward),
    }) {
        return m;
    }

    // Layer 1: global agent
    match agent {
        Agent::Claude => cfg.claude.as_ref().map(|c| c.auth_forward),
        Agent::Codex => cfg.codex.as_ref().map(|c| c.auth_forward),
        Agent::Amp => cfg.amp.as_ref().map(|c| c.auth_forward),
    }
    .unwrap_or_default()
}

/// Resolve the effective GitHub CLI auth-forward mode for a
/// (workspace, role) scope.
///
/// Walks three layers, most-specific wins:
///
/// 1. `workspaces[ws].roles[role].github`
/// 2. `workspaces[ws].github`
/// 3. `github` (global)
///
/// Returns [`GithubAuthMode::Sync`] when no layer is set. Unlike
/// Claude / Codex, the GitHub axis has no agent dimension because
/// `.config/gh/` is shared by every agent in the container.
pub fn resolve_github_mode(cfg: &AppConfig, workspace: &str, role: &str) -> GithubAuthMode {
    if let Some(m) = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.roles.get(role))
        .and_then(|ro| ro.github.as_ref().map(|g| g.auth_forward))
    {
        return m;
    }

    if let Some(m) = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.github.as_ref().map(|g| g.auth_forward))
    {
        return m;
    }

    cfg.github
        .as_ref()
        .map_or_else(GithubAuthMode::default, |g| g.auth_forward)
}

/// Walk the three `[…github.env]` layers for the given pair.
///
/// Merges later layers over earlier ones. Used by the launcher to
/// discover `GH_TOKEN` / `GH_HOST` / `GH_ENTERPRISE_TOKEN` declarations
/// specific to GitHub auth without requiring operators to also list
/// them under the regular role/workspace `[*.env]` blocks.
pub fn build_github_env_layers(
    cfg: &AppConfig,
    workspace: &str,
    role: &str,
) -> BTreeMap<String, EnvValue> {
    let ws = cfg.workspaces.get(workspace);
    let layers = [
        cfg.github.as_ref().map(|g| &g.env),
        ws.and_then(|w| w.github.as_ref()).map(|g| &g.env),
        ws.and_then(|w| w.roles.get(role))
            .and_then(|ro| ro.github.as_ref())
            .map(|g| &g.env),
    ];
    let mut merged: BTreeMap<String, EnvValue> = BTreeMap::new();
    for env in layers.into_iter().flatten() {
        for (k, v) in env {
            merged.insert(k.clone(), v.clone());
        }
    }
    merged
}

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
            env: std::collections::BTreeMap::new(),
        };
        self.roles.insert(selector.key(), source.clone());
        Ok((source, true))
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
            let expected = RoleSource {
                git: git.to_string(),
                trusted: true,
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
}

#[cfg(test)]
mod resolve_mode_tests {
    use super::*;
    use crate::agent::Agent;
    use crate::config::{
        AgentAuthConfig, AmpAuthConfig, AppConfig, AuthForwardMode, CodexAuthConfig,
        GithubAuthConfig, GithubAuthMode,
    };
    use crate::operator_env::EnvValue;
    use crate::workspace::{WorkspaceConfig, WorkspaceRoleOverride};
    use std::collections::BTreeMap;

    /// Build an `AppConfig` with optionally-set Claude modes at each of
    /// the 3 layers: global, workspace, workspace × role.
    fn cfg_claude(
        global: Option<AuthForwardMode>,
        ws: Option<AuthForwardMode>,
        ws_role: Option<AuthForwardMode>,
    ) -> AppConfig {
        let mut cfg = AppConfig::default();
        if let Some(m) = global {
            cfg.claude = Some(AgentAuthConfig {
                auth_forward: m,
            });
        }
        let mut ws_cfg = WorkspaceConfig::default();
        if let Some(m) = ws {
            ws_cfg.claude = Some(AgentAuthConfig {
                auth_forward: m,
            });
        }
        if let Some(m) = ws_role {
            let over = WorkspaceRoleOverride {
                claude: Some(AgentAuthConfig {
                    auth_forward: m,
                }),
                ..Default::default()
            };
            ws_cfg.roles.insert("smith".to_string(), over);
        }
        cfg.workspaces.insert("proj".to_string(), ws_cfg);
        cfg
    }

    #[test]
    fn default_is_sync_when_nothing_set() {
        let cfg = cfg_claude(None, None, None);
        assert_eq!(
            resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
            AuthForwardMode::Sync
        );
    }

    #[test]
    fn global_used_when_others_unset() {
        let cfg = cfg_claude(Some(AuthForwardMode::ApiKey), None, None);
        assert_eq!(
            resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
            AuthForwardMode::ApiKey
        );
    }

    #[test]
    fn workspace_overrides_global() {
        let cfg = cfg_claude(
            Some(AuthForwardMode::ApiKey),
            Some(AuthForwardMode::OAuthToken),
            None,
        );
        assert_eq!(
            resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
            AuthForwardMode::OAuthToken
        );
    }

    #[test]
    fn role_override_wins() {
        let cfg = cfg_claude(
            Some(AuthForwardMode::ApiKey),
            Some(AuthForwardMode::OAuthToken),
            Some(AuthForwardMode::Ignore),
        );
        assert_eq!(
            resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
            AuthForwardMode::Ignore
        );
    }

    #[test]
    fn workspace_only_when_global_unset() {
        let cfg = cfg_claude(None, Some(AuthForwardMode::ApiKey), None);
        assert_eq!(
            resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
            AuthForwardMode::ApiKey
        );
    }

    #[test]
    fn role_only_when_global_and_workspace_unset() {
        let cfg = cfg_claude(None, None, Some(AuthForwardMode::OAuthToken));
        assert_eq!(
            resolve_mode(&cfg, Agent::Claude, "proj", "smith"),
            AuthForwardMode::OAuthToken
        );
    }

    #[test]
    fn unknown_workspace_falls_back_to_global() {
        let cfg = cfg_claude(Some(AuthForwardMode::ApiKey), None, None);
        assert_eq!(
            resolve_mode(&cfg, Agent::Claude, "nonexistent", "smith"),
            AuthForwardMode::ApiKey
        );
    }

    #[test]
    fn unknown_role_falls_back_to_workspace_or_global() {
        let cfg = cfg_claude(
            Some(AuthForwardMode::ApiKey),
            Some(AuthForwardMode::OAuthToken),
            None,
        );
        assert_eq!(
            resolve_mode(&cfg, Agent::Claude, "proj", "ghost"),
            AuthForwardMode::OAuthToken
        );
    }

    #[test]
    fn codex_isolated_from_claude_global() {
        let cfg = AppConfig {
            claude: Some(AgentAuthConfig {
                auth_forward: AuthForwardMode::ApiKey,
            }),
            // codex unset
            ..AppConfig::default()
        };
        assert_eq!(
            resolve_mode(&cfg, Agent::Codex, "proj", "smith"),
            AuthForwardMode::Sync
        );
    }

    #[test]
    fn codex_uses_codex_layer() {
        let cfg = AppConfig {
            codex: Some(CodexAuthConfig(AgentAuthConfig {
                auth_forward: AuthForwardMode::ApiKey,
            })),
            ..AppConfig::default()
        };
        assert_eq!(
            resolve_mode(&cfg, Agent::Codex, "proj", "smith"),
            AuthForwardMode::ApiKey
        );
    }

    #[test]
    fn amp_uses_amp_layer() {
        let cfg = AppConfig {
            amp: Some(AmpAuthConfig(AgentAuthConfig {
                auth_forward: AuthForwardMode::ApiKey,
            })),
            ..AppConfig::default()
        };
        assert_eq!(
            resolve_mode(&cfg, Agent::Amp, "proj", "smith"),
            AuthForwardMode::ApiKey
        );
    }

    // ── build_github_env_layers — 3-layer merge precedence ──────

    fn ws_with_github_env(env: BTreeMap<String, EnvValue>) -> WorkspaceConfig {
        WorkspaceConfig {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Sync,
                env,
            }),
            ..WorkspaceConfig::default()
        }
    }

    #[test]
    fn build_github_env_layers_global_only() {
        let mut global_env = BTreeMap::new();
        global_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_global".into()));
        let cfg = AppConfig {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Sync,
                env: global_env,
            }),
            ..AppConfig::default()
        };
        let merged = build_github_env_layers(&cfg, "proj", "smith");
        assert_eq!(
            merged.get("GH_TOKEN"),
            Some(&EnvValue::Plain("ghp_global".into()))
        );
    }

    #[test]
    fn build_github_env_layers_workspace_overrides_global() {
        let mut global_env = BTreeMap::new();
        global_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_global".into()));
        let mut ws_env = BTreeMap::new();
        ws_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_ws".into()));
        let mut workspaces = BTreeMap::new();
        workspaces.insert("proj".into(), ws_with_github_env(ws_env));
        let cfg = AppConfig {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Sync,
                env: global_env,
            }),
            workspaces,
            ..AppConfig::default()
        };
        let merged = build_github_env_layers(&cfg, "proj", "smith");
        assert_eq!(
            merged.get("GH_TOKEN"),
            Some(&EnvValue::Plain("ghp_ws".into()))
        );
    }

    #[test]
    fn build_github_env_layers_role_overrides_workspace_and_global() {
        let mut global_env = BTreeMap::new();
        global_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_global".into()));
        let mut ws_env = BTreeMap::new();
        ws_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_ws".into()));
        let mut role_env = BTreeMap::new();
        role_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_role".into()));
        let mut ws = ws_with_github_env(ws_env);
        ws.roles.insert(
            "smith".into(),
            crate::workspace::WorkspaceRoleOverride {
                github: Some(GithubAuthConfig {
                    auth_forward: GithubAuthMode::Token,
                    env: role_env,
                }),
                ..crate::workspace::WorkspaceRoleOverride::default()
            },
        );
        let mut workspaces = BTreeMap::new();
        workspaces.insert("proj".into(), ws);
        let cfg = AppConfig {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Sync,
                env: global_env,
            }),
            workspaces,
            ..AppConfig::default()
        };
        let merged = build_github_env_layers(&cfg, "proj", "smith");
        assert_eq!(
            merged.get("GH_TOKEN"),
            Some(&EnvValue::Plain("ghp_role".into()))
        );
    }

    #[test]
    fn build_github_env_layers_preserves_distinct_keys_across_layers() {
        let mut global_env = BTreeMap::new();
        global_env.insert("GH_TOKEN".into(), EnvValue::Plain("ghp_global".into()));
        let mut ws_env = BTreeMap::new();
        ws_env.insert("GH_HOST".into(), EnvValue::Plain("ghe.acme.com".into()));
        let mut role_env = BTreeMap::new();
        role_env.insert(
            "GH_ENTERPRISE_TOKEN".into(),
            EnvValue::Plain("ent_token".into()),
        );
        let mut ws = ws_with_github_env(ws_env);
        ws.roles.insert(
            "smith".into(),
            crate::workspace::WorkspaceRoleOverride {
                github: Some(GithubAuthConfig {
                    auth_forward: GithubAuthMode::Token,
                    env: role_env,
                }),
                ..crate::workspace::WorkspaceRoleOverride::default()
            },
        );
        let mut workspaces = BTreeMap::new();
        workspaces.insert("proj".into(), ws);
        let cfg = AppConfig {
            github: Some(GithubAuthConfig {
                auth_forward: GithubAuthMode::Sync,
                env: global_env,
            }),
            workspaces,
            ..AppConfig::default()
        };
        let merged = build_github_env_layers(&cfg, "proj", "smith");
        // All 3 keys survive — different keys at different layers.
        assert_eq!(merged.len(), 3);
        assert!(merged.contains_key("GH_TOKEN"));
        assert!(merged.contains_key("GH_HOST"));
        assert!(merged.contains_key("GH_ENTERPRISE_TOKEN"));
    }

    #[test]
    fn build_github_env_layers_empty_when_no_layers_set() {
        let cfg = AppConfig::default();
        let merged = build_github_env_layers(&cfg, "proj", "smith");
        assert!(merged.is_empty());
    }
}
