//! Tests for `agent` — auth table tests.
use super::*;
use crate::auth::AuthForwardMode;

#[test]
fn required_env_var_table() {
    assert_eq!(Agent::Claude.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Claude.required_env_var(AuthForwardMode::ApiKey),
        Some("ANTHROPIC_API_KEY")
    );
    assert_eq!(
        Agent::Claude.required_env_var(AuthForwardMode::OAuthToken),
        Some("CLAUDE_CODE_OAUTH_TOKEN")
    );
    assert_eq!(
        Agent::Claude.required_env_var(AuthForwardMode::Ignore),
        None
    );

    assert_eq!(Agent::Codex.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Codex.required_env_var(AuthForwardMode::ApiKey),
        Some("OPENAI_API_KEY")
    );
    assert_eq!(
        Agent::Codex.required_env_var(AuthForwardMode::OAuthToken),
        None
    );
    assert_eq!(Agent::Codex.required_env_var(AuthForwardMode::Ignore), None);

    assert_eq!(Agent::Amp.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Amp.required_env_var(AuthForwardMode::ApiKey),
        Some("AMP_API_KEY")
    );
    assert_eq!(
        Agent::Amp.required_env_var(AuthForwardMode::OAuthToken),
        None
    );
    assert_eq!(Agent::Amp.required_env_var(AuthForwardMode::Ignore), None);

    assert_eq!(Agent::Kimi.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Kimi.required_env_var(AuthForwardMode::ApiKey),
        Some("KIMI_API_KEY")
    );
    assert_eq!(
        Agent::Kimi.required_env_var(AuthForwardMode::OAuthToken),
        None
    );
    assert_eq!(Agent::Kimi.required_env_var(AuthForwardMode::Ignore), None);

    assert_eq!(
        Agent::Opencode.required_env_var(AuthForwardMode::Sync),
        None
    );
    assert_eq!(
        Agent::Opencode.required_env_var(AuthForwardMode::ApiKey),
        Some("OPENCODE_API_KEY")
    );
    assert_eq!(
        Agent::Opencode.required_env_var(AuthForwardMode::OAuthToken),
        None
    );
    assert_eq!(
        Agent::Opencode.required_env_var(AuthForwardMode::Ignore),
        None
    );
}

#[test]
fn supported_modes_claude_includes_oauth_token() {
    let modes = Agent::Claude.supported_modes();
    assert!(modes.contains(&AuthForwardMode::Sync));
    assert!(modes.contains(&AuthForwardMode::ApiKey));
    assert!(modes.contains(&AuthForwardMode::OAuthToken));
    assert!(modes.contains(&AuthForwardMode::Ignore));
}

#[test]
fn supported_modes_codex_excludes_oauth_token() {
    let modes = Agent::Codex.supported_modes();
    assert!(modes.contains(&AuthForwardMode::Sync));
    assert!(modes.contains(&AuthForwardMode::ApiKey));
    assert!(!modes.contains(&AuthForwardMode::OAuthToken));
    assert!(modes.contains(&AuthForwardMode::Ignore));
}

#[test]
fn supported_modes_amp_excludes_oauth_token() {
    let modes = Agent::Amp.supported_modes();
    assert!(modes.contains(&AuthForwardMode::Sync));
    assert!(modes.contains(&AuthForwardMode::ApiKey));
    assert!(!modes.contains(&AuthForwardMode::OAuthToken));
    assert!(modes.contains(&AuthForwardMode::Ignore));
}

#[test]
fn supported_modes_kimi_excludes_oauth_token() {
    let modes = Agent::Kimi.supported_modes();
    assert!(modes.contains(&AuthForwardMode::Sync));
    assert!(modes.contains(&AuthForwardMode::ApiKey));
    assert!(!modes.contains(&AuthForwardMode::OAuthToken));
    assert!(modes.contains(&AuthForwardMode::Ignore));
}

#[test]
fn supported_modes_opencode_excludes_oauth_token() {
    let modes = Agent::Opencode.supported_modes();
    assert!(modes.contains(&AuthForwardMode::Sync));
    assert!(modes.contains(&AuthForwardMode::ApiKey));
    assert!(!modes.contains(&AuthForwardMode::OAuthToken));
    assert!(modes.contains(&AuthForwardMode::Ignore));
}
