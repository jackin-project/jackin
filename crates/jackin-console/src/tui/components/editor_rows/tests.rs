// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `editor_rows`.
use super::*;
use jackin_tui::theme::{ACTION_ACCENT, PHOSPHOR_GREEN};
use ratatui::style::{Color, Modifier};

#[test]
fn selected_action_row_uses_high_contrast_list_fill() {
    let style = action_row_style(true);

    assert_eq!(style.fg, Some(Color::Black));
    assert_eq!(style.bg, Some(PHOSPHOR_GREEN));
    assert!(style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn unselected_action_row_keeps_action_accent() {
    let style = action_row_style(false);

    assert_eq!(style.fg, Some(ACTION_ACCENT));
    assert_eq!(style.bg, None);
    assert!(!style.add_modifier.contains(Modifier::BOLD));
}

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
