//! Tests for `editor_rows`.
use super::*;

#[test]
fn auth_source_display_maps_secret_value_state() {
    assert_eq!(
        auth_source_display(
            Some(AuthSourceValue::Plain("secret".to_owned())),
            "API_KEY",
            "api-key",
        ),
        AuthSourceDisplay::MaskedPlain { chars: 6 },
    );
    assert_eq!(
        auth_source_display(
            Some(AuthSourceValue::OpRefPath("Vault/Item/key".to_owned())),
            "API_KEY",
            "api-key",
        ),
        AuthSourceDisplay::OpRefPath("Vault/Item/key".to_owned()),
    );
    assert_eq!(
        auth_source_display(
            Some(AuthSourceValue::Plain(String::new())),
            "API_KEY",
            "api-key",
        ),
        AuthSourceDisplay::Unset {
            env_name: "API_KEY".to_owned(),
            mode_label: "api-key".to_owned(),
        },
    );
}

#[test]
fn auth_source_display_returns_not_required_without_env() {
    assert_eq!(
        auth_source_display_for_required_env(
            None,
            Some(AuthSourceValue::Plain("secret".to_owned())),
            "ignore",
        ),
        AuthSourceDisplay::NotRequired,
    );
}
