//! Tests for `agent` — auth table tests.
use super::*;
use crate::auth::AuthForwardMode;
use crate::env_model;

#[test]
fn required_env_var_table() {
    assert_eq!(Agent::Claude.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Claude.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::ANTHROPIC_API_KEY_ENV_NAME)
    );
    assert_eq!(
        Agent::Claude.required_env_var(AuthForwardMode::OAuthToken),
        Some(env_model::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME)
    );
    assert_eq!(
        Agent::Claude.required_env_var(AuthForwardMode::Ignore),
        None
    );

    assert_eq!(Agent::Codex.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Codex.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::OPENAI_API_KEY_ENV_NAME)
    );
    assert_eq!(
        Agent::Codex.required_env_var(AuthForwardMode::OAuthToken),
        None
    );
    assert_eq!(Agent::Codex.required_env_var(AuthForwardMode::Ignore), None);

    assert_eq!(Agent::Amp.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Amp.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::AMP_API_KEY_ENV_NAME)
    );
    assert_eq!(
        Agent::Amp.required_env_var(AuthForwardMode::OAuthToken),
        None
    );
    assert_eq!(Agent::Amp.required_env_var(AuthForwardMode::Ignore), None);

    assert_eq!(Agent::Kimi.required_env_var(AuthForwardMode::Sync), None);
    assert_eq!(
        Agent::Kimi.required_env_var(AuthForwardMode::ApiKey),
        Some(env_model::KIMI_API_KEY_ENV_NAME)
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
        Some(env_model::OPENCODE_API_KEY_ENV_NAME)
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
