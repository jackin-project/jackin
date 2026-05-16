//! Auth-tab kind axis.
//!
//! Panel-layer enum that widens beyond the runtime `Agent` enum so the
//! GitHub CLI row can sit alongside Claude / Codex without forcing a
//! fake `Agent::Github` variant on the runtime layer.
//!
//! `AuthKind` names which auth section the operator is editing in the
//! workspace-manager Auth tab. Claude, Codex, and Amp map 1:1 onto the
//! `Agent` enum (they are runtimes); Github is a non-runtime kind —
//! no install block, no launch entrypoint, no version probe — but it
//! still has its own `auth_forward` axis at three layers (global,
//! workspace, workspace × role) per the github-cli-auth-strategy
//! roadmap design.
//!
//! `AuthMode` unifies the union of modes across all three kinds.
//! `AuthForwardMode` (Claude / Codex) and `GithubAuthMode` are the
//! disjoint persistence types in `crate::config`; the form widget
//! carries this unified mode and converts at the persistence boundary
//! through [`AuthMode::to_auth_forward`] / [`AuthMode::to_github`].

use crate::agent::Agent;
use crate::config::{AuthForwardMode, GithubAuthMode};

/// Which auth section the operator is currently editing.
///
/// See module docs for why this is wider than `Agent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthKind {
    Claude,
    Codex,
    Amp,
    Opencode,
    Github,
}

impl AuthKind {
    /// Operator-facing label rendered in the root-view row and the
    /// detail-view title bar.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Amp => "Amp",
            Self::Opencode => "OpenCode",
            Self::Github => "GitHub CLI",
        }
    }

    /// Modes the form's mode picker offers for this kind. The order is
    /// deliberate — it matches the cycle order of the Space-cycle
    /// keystroke and the order rendered in the wireframe
    /// (`sync` → `api_key` / `token` → `oauth_token` → `ignore`).
    #[must_use]
    pub const fn supported_modes(self) -> &'static [AuthMode] {
        match self {
            Self::Claude => &[
                AuthMode::Sync,
                AuthMode::ApiKey,
                AuthMode::OAuthToken,
                AuthMode::Ignore,
            ],
            Self::Codex | Self::Amp | Self::Opencode => {
                &[AuthMode::Sync, AuthMode::ApiKey, AuthMode::Ignore]
            }
            Self::Github => &[AuthMode::Sync, AuthMode::Token, AuthMode::Ignore],
        }
    }

    /// Well-known env var that carries the auth credential for this
    /// (kind, mode) combination, if any. Returns `None` for modes that
    /// don't inject a credential (sync, ignore) or for combinations
    /// that don't make sense for the kind.
    #[must_use]
    pub const fn required_env_var(self, mode: AuthMode) -> Option<&'static str> {
        match (self, mode) {
            (Self::Claude, AuthMode::ApiKey) => Some("ANTHROPIC_API_KEY"),
            (Self::Claude, AuthMode::OAuthToken) => Some("CLAUDE_CODE_OAUTH_TOKEN"),
            (Self::Codex, AuthMode::ApiKey) => Some("OPENAI_API_KEY"),
            (Self::Amp, AuthMode::ApiKey) => Some("AMP_API_KEY"),
            (Self::Opencode, AuthMode::ApiKey) => Some("OPENCODE_API_KEY"),
            (Self::Github, AuthMode::Token) => Some(crate::env_model::GH_TOKEN_ENV_NAME),
            _ => None,
        }
    }

    /// Map a runtime [`Agent`] onto its [`AuthKind`] counterpart. The
    /// inverse direction (`Agent::for_auth_kind`) is intentionally
    /// absent — `AuthKind::Github` has no `Agent` partner.
    #[must_use]
    pub const fn for_agent(agent: Agent) -> Self {
        match agent {
            Agent::Claude => Self::Claude,
            Agent::Codex => Self::Codex,
            Agent::Amp => Self::Amp,
            Agent::Opencode => Self::Opencode,
        }
    }

    /// Convert back to the runtime [`Agent`] axis for code paths that
    /// remain agent-keyed (Claude / Codex persistence diffs, the
    /// shared `resolve_mode` helper that takes an `Agent`). Returns
    /// `None` for `Self::Github`, which has no runtime peer.
    #[must_use]
    pub const fn agent(self) -> Option<Agent> {
        match self {
            Self::Claude => Some(Agent::Claude),
            Self::Codex => Some(Agent::Codex),
            Self::Amp => Some(Agent::Amp),
            Self::Opencode => Some(Agent::Opencode),
            Self::Github => None,
        }
    }

    /// Whether the role-override entry already carries a block for
    /// this kind. Used by the auth-role-override picker (filter out
    /// roles that already have an override) and the render-side row
    /// builder (decide whether to draw a `RoleHeader`).
    #[must_use]
    pub const fn role_override_present(self, ro: &crate::config::WorkspaceRoleOverride) -> bool {
        match self {
            Self::Claude => ro.claude.is_some(),
            Self::Codex => ro.codex.is_some(),
            Self::Amp => ro.amp.is_some(),
            Self::Opencode => ro.opencode.is_some(),
            Self::Github => ro.github.is_some(),
        }
    }
}

/// Unified mode enum for the auth-form modal.
///
/// Subsumes [`AuthForwardMode`] (Claude / Codex) and [`GithubAuthMode`]
/// so a single form widget can drive every kind. The form is kind-
/// keyed; each kind exposes only the modes it supports through
/// [`AuthKind::supported_modes`].
///
/// `Token` is the Github-only mode; `OAuthToken` is the Claude-only
/// mode. The other variants are shared across at least two kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthMode {
    Sync,
    ApiKey,
    OAuthToken,
    Token,
    Ignore,
}

impl AuthMode {
    /// Operator-facing slug (matches the on-disk TOML serialization
    /// for both [`AuthForwardMode`] and [`GithubAuthMode`]).
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

    /// Map to the Claude / Codex persistence enum. Returns `None` for
    /// `Token` (Github-only).
    #[must_use]
    pub const fn to_auth_forward(self) -> Option<AuthForwardMode> {
        match self {
            Self::Sync => Some(AuthForwardMode::Sync),
            Self::ApiKey => Some(AuthForwardMode::ApiKey),
            Self::OAuthToken => Some(AuthForwardMode::OAuthToken),
            Self::Ignore => Some(AuthForwardMode::Ignore),
            Self::Token => None,
        }
    }

    /// Map to the Github persistence enum. Returns `None` for
    /// `ApiKey` / `OAuthToken` (Claude / Codex-only).
    #[must_use]
    pub const fn to_github(self) -> Option<GithubAuthMode> {
        match self {
            Self::Sync => Some(GithubAuthMode::Sync),
            Self::Token => Some(GithubAuthMode::Token),
            Self::Ignore => Some(GithubAuthMode::Ignore),
            Self::ApiKey | Self::OAuthToken => None,
        }
    }

    /// Lift an [`AuthForwardMode`] (Claude / Codex) into the unified
    /// auth-mode enum.
    #[must_use]
    pub const fn from_auth_forward(mode: AuthForwardMode) -> Self {
        match mode {
            AuthForwardMode::Sync => Self::Sync,
            AuthForwardMode::ApiKey => Self::ApiKey,
            AuthForwardMode::OAuthToken => Self::OAuthToken,
            AuthForwardMode::Ignore => Self::Ignore,
        }
    }

    /// Lift a [`GithubAuthMode`] into the unified auth-mode enum.
    #[must_use]
    pub const fn from_github(mode: GithubAuthMode) -> Self {
        match mode {
            GithubAuthMode::Sync => Self::Sync,
            GithubAuthMode::Token => Self::Token,
            GithubAuthMode::Ignore => Self::Ignore,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_matches_design_spec() {
        assert_eq!(AuthKind::Claude.label(), "Claude Code");
        assert_eq!(AuthKind::Codex.label(), "Codex");
        assert_eq!(AuthKind::Amp.label(), "Amp");
        assert_eq!(AuthKind::Github.label(), "GitHub CLI");
    }

    #[test]
    fn github_supported_modes_are_sync_token_ignore() {
        let modes = AuthKind::Github.supported_modes();
        assert_eq!(modes, &[AuthMode::Sync, AuthMode::Token, AuthMode::Ignore]);
    }

    #[test]
    fn claude_supported_modes_include_oauth_token() {
        let modes = AuthKind::Claude.supported_modes();
        assert!(modes.contains(&AuthMode::OAuthToken));
    }

    #[test]
    fn codex_supported_modes_exclude_oauth_token_and_token() {
        let modes = AuthKind::Codex.supported_modes();
        assert!(!modes.contains(&AuthMode::OAuthToken));
        assert!(!modes.contains(&AuthMode::Token));
    }

    #[test]
    fn amp_supported_modes_exclude_oauth_token_and_token() {
        let modes = AuthKind::Amp.supported_modes();
        assert!(!modes.contains(&AuthMode::OAuthToken));
        assert!(!modes.contains(&AuthMode::Token));
    }

    #[test]
    fn github_token_mode_requires_gh_token_env_var() {
        assert_eq!(
            AuthKind::Github.required_env_var(AuthMode::Token),
            Some("GH_TOKEN")
        );
    }

    #[test]
    fn github_sync_and_ignore_have_no_required_env_var() {
        assert_eq!(AuthKind::Github.required_env_var(AuthMode::Sync), None);
        assert_eq!(AuthKind::Github.required_env_var(AuthMode::Ignore), None);
    }

    #[test]
    fn claude_required_env_vars_match_runtime_table() {
        assert_eq!(
            AuthKind::Claude.required_env_var(AuthMode::ApiKey),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(
            AuthKind::Claude.required_env_var(AuthMode::OAuthToken),
            Some("CLAUDE_CODE_OAUTH_TOKEN")
        );
        assert_eq!(AuthKind::Claude.required_env_var(AuthMode::Sync), None);
        assert_eq!(AuthKind::Claude.required_env_var(AuthMode::Ignore), None);
    }

    #[test]
    fn amp_required_env_vars_match_runtime_table() {
        assert_eq!(
            AuthKind::Amp.required_env_var(AuthMode::ApiKey),
            Some("AMP_API_KEY")
        );
        assert_eq!(AuthKind::Amp.required_env_var(AuthMode::Sync), None);
        assert_eq!(AuthKind::Amp.required_env_var(AuthMode::Ignore), None);
        assert_eq!(AuthKind::Amp.required_env_var(AuthMode::OAuthToken), None);
    }

    #[test]
    fn for_agent_round_trip() {
        assert_eq!(AuthKind::for_agent(Agent::Claude), AuthKind::Claude);
        assert_eq!(AuthKind::for_agent(Agent::Codex), AuthKind::Codex);
        assert_eq!(AuthKind::for_agent(Agent::Amp), AuthKind::Amp);
    }

    #[test]
    fn agent_returns_none_for_github() {
        assert_eq!(AuthKind::Github.agent(), None);
        assert_eq!(AuthKind::Claude.agent(), Some(Agent::Claude));
        assert_eq!(AuthKind::Codex.agent(), Some(Agent::Codex));
        assert_eq!(AuthKind::Amp.agent(), Some(Agent::Amp));
    }

    #[test]
    fn auth_mode_to_auth_forward_round_trip() {
        for m in [
            AuthForwardMode::Sync,
            AuthForwardMode::ApiKey,
            AuthForwardMode::OAuthToken,
            AuthForwardMode::Ignore,
        ] {
            assert_eq!(AuthMode::from_auth_forward(m).to_auth_forward(), Some(m));
        }
    }

    #[test]
    fn auth_mode_to_github_round_trip() {
        for m in [
            GithubAuthMode::Sync,
            GithubAuthMode::Token,
            GithubAuthMode::Ignore,
        ] {
            assert_eq!(AuthMode::from_github(m).to_github(), Some(m));
        }
    }

    #[test]
    fn auth_mode_token_has_no_auth_forward_partner() {
        assert!(AuthMode::Token.to_auth_forward().is_none());
    }

    #[test]
    fn auth_mode_oauth_token_has_no_github_partner() {
        assert!(AuthMode::OAuthToken.to_github().is_none());
    }

    // ── role_override_present per-kind table ─────────────────────

    /// Empty role override returns false for every kind.
    #[test]
    fn role_override_present_false_when_no_blocks_set() {
        let ro = crate::config::WorkspaceRoleOverride::default();
        assert!(!AuthKind::Claude.role_override_present(&ro));
        assert!(!AuthKind::Codex.role_override_present(&ro));
        assert!(!AuthKind::Amp.role_override_present(&ro));
        assert!(!AuthKind::Github.role_override_present(&ro));
    }

    #[test]
    fn role_override_present_isolated_per_kind() {
        // Claude-only override → only Claude returns true.
        let ro = crate::config::WorkspaceRoleOverride {
            claude: Some(crate::config::AgentAuthConfig {
                auth_forward: crate::config::AuthForwardMode::Ignore,
            }),
            ..crate::config::WorkspaceRoleOverride::default()
        };
        assert!(AuthKind::Claude.role_override_present(&ro));
        assert!(!AuthKind::Codex.role_override_present(&ro));
        assert!(!AuthKind::Amp.role_override_present(&ro));
        assert!(!AuthKind::Github.role_override_present(&ro));

        // Codex-only override → only Codex returns true.
        let ro = crate::config::WorkspaceRoleOverride {
            codex: Some(crate::config::CodexAuthConfig(
                crate::config::AgentAuthConfig {
                    auth_forward: crate::config::AuthForwardMode::ApiKey,
                },
            )),
            ..crate::config::WorkspaceRoleOverride::default()
        };
        assert!(!AuthKind::Claude.role_override_present(&ro));
        assert!(AuthKind::Codex.role_override_present(&ro));
        assert!(!AuthKind::Amp.role_override_present(&ro));
        assert!(!AuthKind::Github.role_override_present(&ro));

        // Amp-only override → only Amp returns true.
        let ro = crate::config::WorkspaceRoleOverride {
            amp: Some(crate::config::AmpAuthConfig(
                crate::config::AgentAuthConfig {
                    auth_forward: crate::config::AuthForwardMode::ApiKey,
                },
            )),
            ..crate::config::WorkspaceRoleOverride::default()
        };
        assert!(!AuthKind::Claude.role_override_present(&ro));
        assert!(!AuthKind::Codex.role_override_present(&ro));
        assert!(AuthKind::Amp.role_override_present(&ro));
        assert!(!AuthKind::Github.role_override_present(&ro));

        // Github-only override → only Github returns true.
        let ro = crate::config::WorkspaceRoleOverride {
            github: Some(crate::config::GithubAuthConfig {
                auth_forward: crate::config::GithubAuthMode::Ignore,
                ..Default::default()
            }),
            ..crate::config::WorkspaceRoleOverride::default()
        };
        assert!(!AuthKind::Claude.role_override_present(&ro));
        assert!(!AuthKind::Codex.role_override_present(&ro));
        assert!(!AuthKind::Amp.role_override_present(&ro));
        assert!(AuthKind::Github.role_override_present(&ro));
    }

    #[test]
    fn role_override_present_all_kinds_set() {
        let ro = crate::config::WorkspaceRoleOverride {
            claude: Some(crate::config::AgentAuthConfig {
                auth_forward: crate::config::AuthForwardMode::Sync,
            }),
            codex: Some(crate::config::CodexAuthConfig(
                crate::config::AgentAuthConfig {
                    auth_forward: crate::config::AuthForwardMode::Sync,
                },
            )),
            amp: Some(crate::config::AmpAuthConfig(
                crate::config::AgentAuthConfig {
                    auth_forward: crate::config::AuthForwardMode::Sync,
                },
            )),
            github: Some(crate::config::GithubAuthConfig {
                auth_forward: crate::config::GithubAuthMode::Sync,
                ..Default::default()
            }),
            ..crate::config::WorkspaceRoleOverride::default()
        };
        assert!(AuthKind::Claude.role_override_present(&ro));
        assert!(AuthKind::Codex.role_override_present(&ro));
        assert!(AuthKind::Amp.role_override_present(&ro));
        assert!(AuthKind::Github.role_override_present(&ro));
    }
}
