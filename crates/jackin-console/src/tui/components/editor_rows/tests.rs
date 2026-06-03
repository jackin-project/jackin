//! Tests for `editor_rows`.
use super::*;

#[test]
fn auth_source_display_maps_secret_value_state() {
    assert_eq!(
        auth_source_display(
            Some(AuthSourceValue::Plain("secret".to_string())),
            "API_KEY",
            "api-key",
        ),
        AuthSourceDisplay::MaskedPlain { chars: 6 },
    );
    assert_eq!(
        auth_source_display(
            Some(AuthSourceValue::OpRefPath("Vault/Item/key".to_string())),
            "API_KEY",
            "api-key",
        ),
        AuthSourceDisplay::OpRefPath("Vault/Item/key".to_string()),
    );
    assert_eq!(
        auth_source_display(
            Some(AuthSourceValue::Plain(String::new())),
            "API_KEY",
            "api-key",
        ),
        AuthSourceDisplay::Unset {
            env_name: "API_KEY".to_string(),
            mode_label: "api-key".to_string(),
        },
    );
}

#[test]
fn auth_source_display_returns_not_required_without_env() {
    assert_eq!(
        auth_source_display_for_required_env(
            None,
            Some(AuthSourceValue::Plain("secret".to_string())),
            "ignore",
        ),
        AuthSourceDisplay::NotRequired,
    );
}
