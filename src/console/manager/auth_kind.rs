//! Root-console adapters for auth-tab UI vocabulary.

use crate::agent::Agent;
use crate::config::{AuthForwardMode, GithubAuthMode, WorkspaceRoleOverride};

pub use jackin_console::tui::auth::{AuthKind, AuthMode};

#[must_use]
pub const fn auth_kind_for_agent(agent: Agent) -> AuthKind {
    match agent {
        Agent::Claude => AuthKind::Claude,
        Agent::Codex => AuthKind::Codex,
        Agent::Amp => AuthKind::Amp,
        Agent::Kimi => AuthKind::Kimi,
        Agent::Opencode => AuthKind::Opencode,
    }
}

#[must_use]
pub const fn auth_kind_agent(kind: AuthKind) -> Option<Agent> {
    match kind {
        AuthKind::Claude => Some(Agent::Claude),
        AuthKind::Codex => Some(Agent::Codex),
        AuthKind::Amp => Some(Agent::Amp),
        AuthKind::Kimi => Some(Agent::Kimi),
        AuthKind::Opencode => Some(Agent::Opencode),
        AuthKind::Github | AuthKind::Zai => None,
    }
}

#[must_use]
pub fn role_override_present(kind: AuthKind, ro: &WorkspaceRoleOverride) -> bool {
    match kind {
        AuthKind::Claude => ro.claude.is_some(),
        AuthKind::Codex => ro.codex.is_some(),
        AuthKind::Amp => ro.amp.is_some(),
        AuthKind::Kimi => ro.kimi.is_some(),
        AuthKind::Opencode => ro.opencode.is_some(),
        AuthKind::Github => ro.github.is_some(),
        AuthKind::Zai => ro.env.contains_key(crate::env_model::ZAI_API_KEY_ENV_NAME),
    }
}

#[must_use]
pub const fn auth_mode_to_auth_forward(mode: AuthMode) -> Option<AuthForwardMode> {
    match mode {
        AuthMode::Sync => Some(AuthForwardMode::Sync),
        AuthMode::ApiKey => Some(AuthForwardMode::ApiKey),
        AuthMode::OAuthToken => Some(AuthForwardMode::OAuthToken),
        AuthMode::Ignore => Some(AuthForwardMode::Ignore),
        AuthMode::Token => None,
    }
}

#[must_use]
pub const fn auth_mode_to_github(mode: AuthMode) -> Option<GithubAuthMode> {
    match mode {
        AuthMode::Sync => Some(GithubAuthMode::Sync),
        AuthMode::Token => Some(GithubAuthMode::Token),
        AuthMode::Ignore => Some(GithubAuthMode::Ignore),
        AuthMode::ApiKey | AuthMode::OAuthToken => None,
    }
}

#[must_use]
pub const fn auth_mode_from_auth_forward(mode: AuthForwardMode) -> AuthMode {
    match mode {
        AuthForwardMode::Sync => AuthMode::Sync,
        AuthForwardMode::ApiKey => AuthMode::ApiKey,
        AuthForwardMode::OAuthToken => AuthMode::OAuthToken,
        AuthForwardMode::Ignore => AuthMode::Ignore,
    }
}

#[must_use]
pub const fn auth_mode_from_github(mode: GithubAuthMode) -> AuthMode {
    match mode {
        GithubAuthMode::Sync => AuthMode::Sync,
        GithubAuthMode::Token => AuthMode::Token,
        GithubAuthMode::Ignore => AuthMode::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_agent_round_trip() {
        assert_eq!(auth_kind_for_agent(Agent::Claude), AuthKind::Claude);
        assert_eq!(auth_kind_for_agent(Agent::Codex), AuthKind::Codex);
        assert_eq!(auth_kind_for_agent(Agent::Amp), AuthKind::Amp);
        assert_eq!(auth_kind_for_agent(Agent::Kimi), AuthKind::Kimi);
        assert_eq!(auth_kind_for_agent(Agent::Opencode), AuthKind::Opencode);
    }

    #[test]
    fn agent_returns_none_for_github() {
        assert_eq!(auth_kind_agent(AuthKind::Github), None);
        assert_eq!(auth_kind_agent(AuthKind::Claude), Some(Agent::Claude));
        assert_eq!(auth_kind_agent(AuthKind::Codex), Some(Agent::Codex));
        assert_eq!(auth_kind_agent(AuthKind::Amp), Some(Agent::Amp));
        assert_eq!(auth_kind_agent(AuthKind::Kimi), Some(Agent::Kimi));
        assert_eq!(auth_kind_agent(AuthKind::Opencode), Some(Agent::Opencode));
    }

    #[test]
    fn auth_mode_to_auth_forward_round_trip() {
        for mode in [
            AuthForwardMode::Sync,
            AuthForwardMode::ApiKey,
            AuthForwardMode::OAuthToken,
            AuthForwardMode::Ignore,
        ] {
            assert_eq!(
                auth_mode_to_auth_forward(auth_mode_from_auth_forward(mode)),
                Some(mode)
            );
        }
    }

    #[test]
    fn auth_mode_to_github_round_trip() {
        for mode in [
            GithubAuthMode::Sync,
            GithubAuthMode::Token,
            GithubAuthMode::Ignore,
        ] {
            assert_eq!(
                auth_mode_to_github(auth_mode_from_github(mode)),
                Some(mode)
            );
        }
    }

    #[test]
    fn role_override_present_false_when_no_blocks_set() {
        let ro = WorkspaceRoleOverride::default();
        assert!(!role_override_present(AuthKind::Claude, &ro));
        assert!(!role_override_present(AuthKind::Codex, &ro));
        assert!(!role_override_present(AuthKind::Amp, &ro));
        assert!(!role_override_present(AuthKind::Kimi, &ro));
        assert!(!role_override_present(AuthKind::Opencode, &ro));
        assert!(!role_override_present(AuthKind::Github, &ro));
        assert!(!role_override_present(AuthKind::Zai, &ro));
    }

    #[test]
    fn role_override_present_zai_keys_off_env_var() {
        let mut ro = WorkspaceRoleOverride::default();
        assert!(!role_override_present(AuthKind::Zai, &ro));
        ro.env.insert(
            crate::env_model::ZAI_API_KEY_ENV_NAME.to_string(),
            crate::operator_env::EnvValue::Plain("k".into()),
        );
        assert!(role_override_present(AuthKind::Zai, &ro));
        assert!(!role_override_present(AuthKind::Claude, &ro));
        assert!(!role_override_present(AuthKind::Github, &ro));
    }
}
