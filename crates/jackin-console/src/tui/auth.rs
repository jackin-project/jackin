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
    pub const WORKSPACE_PANEL_KINDS: &'static [Self] = &[
        Self::Claude,
        Self::Codex,
        Self::Amp,
        Self::Opencode,
        Self::Github,
        Self::Zai,
    ];

    pub const SETTINGS_KINDS: &'static [Self] = &[
        Self::Claude,
        Self::Codex,
        Self::Amp,
        Self::Kimi,
        Self::Opencode,
        Self::Github,
        Self::Zai,
    ];

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
pub const fn auth_mode_requires_credential(kind: AuthKind, mode: AuthMode) -> bool {
    kind.required_env_var(mode).is_some()
}

#[must_use]
pub const fn can_generate_claude_oauth_token(kind: AuthKind, mode: Option<AuthMode>) -> bool {
    matches!((kind, mode), (AuthKind::Claude, Some(AuthMode::OAuthToken)))
}

#[cfg(test)]
mod tests;
