//! Console auth-tab UI vocabulary.

/// Which auth section the operator is currently editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthKind {
    Claude,
    Codex,
    Amp,
    Kimi,
    Opencode,
    Github,
    /// Z.AI / GLM Coding Plan: env-only auth kind.
    Zai,
}

impl AuthKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Amp => "Amp",
            Self::Kimi => "Kimi",
            Self::Opencode => "OpenCode",
            Self::Github => "GitHub CLI",
            Self::Zai => "Z.AI",
        }
    }

    #[must_use]
    pub const fn supported_modes(self) -> &'static [AuthMode] {
        match self {
            Self::Claude => &[
                AuthMode::Sync,
                AuthMode::ApiKey,
                AuthMode::OAuthToken,
                AuthMode::Ignore,
            ],
            Self::Codex | Self::Amp | Self::Kimi | Self::Opencode => {
                &[AuthMode::Sync, AuthMode::ApiKey, AuthMode::Ignore]
            }
            Self::Github => &[AuthMode::Sync, AuthMode::Token, AuthMode::Ignore],
            Self::Zai => &[AuthMode::ApiKey, AuthMode::Ignore],
        }
    }

    #[must_use]
    pub const fn required_env_var(self, mode: AuthMode) -> Option<&'static str> {
        match (self, mode) {
            (Self::Claude, AuthMode::ApiKey) => Some("ANTHROPIC_API_KEY"),
            (Self::Claude, AuthMode::OAuthToken) => Some("CLAUDE_CODE_OAUTH_TOKEN"),
            (Self::Codex, AuthMode::ApiKey) => Some("OPENAI_API_KEY"),
            (Self::Amp, AuthMode::ApiKey) => Some("AMP_API_KEY"),
            (Self::Kimi, AuthMode::ApiKey) => Some("KIMI_API_KEY"),
            (Self::Opencode, AuthMode::ApiKey) => Some("OPENCODE_API_KEY"),
            (Self::Github, AuthMode::Token) => Some("GH_TOKEN"),
            (Self::Zai, AuthMode::ApiKey) => Some("ZAI_API_KEY"),
            _ => None,
        }
    }
}

/// Unified mode enum for the auth-form modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthMode {
    Sync,
    ApiKey,
    OAuthToken,
    Token,
    Ignore,
}

impl AuthMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sync => "sync",
            Self::ApiKey => "api_key",
            Self::OAuthToken => "oauth_token",
            Self::Token => "token",
            Self::Ignore => "ignore",
        }
    }
}

#[must_use]
pub const fn can_generate_claude_oauth_token(kind: AuthKind, mode: Option<AuthMode>) -> bool {
    matches!((kind, mode), (AuthKind::Claude, Some(AuthMode::OAuthToken)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_matches_design_spec() {
        assert_eq!(AuthKind::Claude.label(), "Claude Code");
        assert_eq!(AuthKind::Codex.label(), "Codex");
        assert_eq!(AuthKind::Amp.label(), "Amp");
        assert_eq!(AuthKind::Kimi.label(), "Kimi");
        assert_eq!(AuthKind::Opencode.label(), "OpenCode");
        assert_eq!(AuthKind::Github.label(), "GitHub CLI");
    }

    #[test]
    fn github_supported_modes_are_sync_token_ignore() {
        let modes = AuthKind::Github.supported_modes();
        assert_eq!(modes, &[AuthMode::Sync, AuthMode::Token, AuthMode::Ignore]);
    }

    #[test]
    fn claude_supported_modes_include_oauth_token() {
        assert!(AuthKind::Claude
            .supported_modes()
            .contains(&AuthMode::OAuthToken));
    }

    #[test]
    fn non_claude_agent_modes_exclude_oauth_token_and_token() {
        for kind in [
            AuthKind::Codex,
            AuthKind::Amp,
            AuthKind::Kimi,
            AuthKind::Opencode,
        ] {
            let modes = kind.supported_modes();
            assert!(!modes.contains(&AuthMode::OAuthToken));
            assert!(!modes.contains(&AuthMode::Token));
        }
    }

    #[test]
    fn required_env_vars_match_auth_kind_table() {
        assert_eq!(
            AuthKind::Claude.required_env_var(AuthMode::ApiKey),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(
            AuthKind::Claude.required_env_var(AuthMode::OAuthToken),
            Some("CLAUDE_CODE_OAUTH_TOKEN")
        );
        assert_eq!(
            AuthKind::Github.required_env_var(AuthMode::Token),
            Some("GH_TOKEN")
        );
        assert_eq!(
            AuthKind::Zai.required_env_var(AuthMode::ApiKey),
            Some("ZAI_API_KEY")
        );
        assert_eq!(AuthKind::Github.required_env_var(AuthMode::Sync), None);
    }

    #[test]
    fn token_generation_gate_is_claude_oauth_only() {
        assert!(can_generate_claude_oauth_token(
            AuthKind::Claude,
            Some(AuthMode::OAuthToken),
        ));
        assert!(!can_generate_claude_oauth_token(
            AuthKind::Claude,
            Some(AuthMode::ApiKey),
        ));
        assert!(!can_generate_claude_oauth_token(
            AuthKind::Github,
            Some(AuthMode::Token),
        ));
        assert!(!can_generate_claude_oauth_token(AuthKind::Claude, None));
    }
}
