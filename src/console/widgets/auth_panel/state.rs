//! Pure data types for the Auth panel.
//!
//! Provides [`CredentialBadge`], [`badge_for`], and
//! [`classify_env_value`]. These are consumed by the flat-row renderer in
//! `src/console/manager/render/editor.rs`.

use crate::agent::Agent;
use crate::config::{AppConfig, AuthForwardMode};

/// Status badge displayed on a row showing whether the credential resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialBadge {
    /// Required env var resolves to non-empty in the merged operator env.
    Resolves,
    /// Required env var is unset or empty.
    Unset,
    /// Mode does not require a credential (Sync, Ignore).
    NotApplicable,
}

/// Compute the credential badge for a given (workspace, role, agent, mode).
///
/// Walks the 4 env layers in launch-time precedence order (workspace × role
/// → workspace → role → global) and returns the badge for the first hit.
/// `OpRef` entries report `Resolves` optimistically — the picker validated
/// them at commit time, and verifying again here would require running
/// `op read`, which the panel deliberately avoids.
pub(crate) fn badge_for(
    cfg: &AppConfig,
    workspace: &str,
    role: &str,
    agent: Agent,
    mode: AuthForwardMode,
) -> CredentialBadge {
    let Some(env_var) = agent.required_env_var(mode) else {
        return CredentialBadge::NotApplicable;
    };

    if let Some(value) = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.roles.get(role))
        .and_then(|ro| ro.env.get(env_var))
    {
        return classify_env_value(value);
    }
    if let Some(value) = cfg
        .workspaces
        .get(workspace)
        .and_then(|ws| ws.env.get(env_var))
    {
        return classify_env_value(value);
    }
    if let Some(value) = cfg.roles.get(role).and_then(|r| r.env.get(env_var)) {
        return classify_env_value(value);
    }
    if let Some(value) = cfg.env.get(env_var) {
        return classify_env_value(value);
    }
    CredentialBadge::Unset
}

pub(crate) const fn classify_env_value(value: &crate::operator_env::EnvValue) -> CredentialBadge {
    use crate::operator_env::EnvValue;
    match value {
        // OpRef: assume the picker validated at commit time. The panel
        // does not run `op read` (would block render).
        EnvValue::OpRef(_) => CredentialBadge::Resolves,
        // Plain: resolves iff non-empty. The launch-time validator catches
        // `$VAR`/`${VAR}` expansion that ends up empty; for panel display
        // any non-empty persisted string is treated as `Resolves`.
        EnvValue::Plain(s) if !s.is_empty() => CredentialBadge::Resolves,
        EnvValue::Plain(_) => CredentialBadge::Unset,
    }
}

#[cfg(test)]
mod badge_for_tests {
    use super::*;
    use crate::config::{AgentAuthConfig, AppConfig};
    use crate::operator_env::EnvValue;
    use crate::workspace::WorkspaceConfig;

    fn base_cfg_with_ws() -> AppConfig {
        let mut cfg = AppConfig::default();
        let ws = WorkspaceConfig {
            workdir: "/tmp/proj".to_string(),
            allowed_roles: vec!["smith".to_string()],
            ..Default::default()
        };
        cfg.workspaces.insert("proj".into(), ws);
        cfg
    }

    /// Sync mode has no required env var, so badge must be NotApplicable
    /// regardless of what the operator env contains.
    #[test]
    fn credential_badge_not_applicable_for_sync() {
        let cfg = base_cfg_with_ws();
        let badge = badge_for(&cfg, "proj", "smith", Agent::Claude, AuthForwardMode::Sync);
        assert_eq!(badge, CredentialBadge::NotApplicable);
    }

    /// ApiKey mode requires ANTHROPIC_API_KEY, which is absent at all 4
    /// layers — badge must be Unset.
    #[test]
    fn credential_badge_unset_when_required_var_empty() {
        let mut cfg = base_cfg_with_ws();
        cfg.claude = Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
        });
        // ANTHROPIC_API_KEY is not set anywhere in cfg.
        let badge = badge_for(
            &cfg,
            "proj",
            "smith",
            Agent::Claude,
            AuthForwardMode::ApiKey,
        );
        assert_eq!(badge, CredentialBadge::Unset);
    }

    /// ApiKey mode with ANTHROPIC_API_KEY present at the workspace × role
    /// layer (most specific) must yield Resolves.
    #[test]
    fn credential_badge_resolves_when_env_var_present() {
        let mut cfg = base_cfg_with_ws();
        cfg.claude = Some(AgentAuthConfig {
            auth_forward: AuthForwardMode::ApiKey,
        });
        // Place the key at workspace × role — the most-specific layer.
        cfg.workspaces
            .get_mut("proj")
            .unwrap()
            .roles
            .entry("smith".into())
            .or_default()
            .env
            .insert(
                "ANTHROPIC_API_KEY".into(),
                EnvValue::Plain("sk-ant-test".into()),
            );
        let badge = badge_for(
            &cfg,
            "proj",
            "smith",
            Agent::Claude,
            AuthForwardMode::ApiKey,
        );
        assert_eq!(badge, CredentialBadge::Resolves);
    }
}
