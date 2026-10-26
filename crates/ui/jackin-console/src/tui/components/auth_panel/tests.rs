// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::screens::settings::model::AuthFormFocus;

#[test]
fn auth_form_key_plan_routes_shared_focus_model() {
    use crossterm::event::KeyCode;
    assert_eq!(
        auth_form_focus_chain(false, true),
        vec![
            AuthFormFocus::Mode,
            AuthFormFocus::CredentialSource,
            AuthFormFocus::Save,
            AuthFormFocus::Cancel,
            AuthFormFocus::Reset,
        ]
    );
    assert_eq!(
        auth_form_focus_chain(true, true),
        vec![
            AuthFormFocus::Mode,
            AuthFormFocus::SourceFolder,
            AuthFormFocus::CredentialSource,
            AuthFormFocus::Save,
            AuthFormFocus::Cancel,
            AuthFormFocus::Reset,
        ]
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Mode, KeyCode::Tab, true, false),
        AuthFormKeyPlan::Focus(AuthFormFocus::CredentialSource)
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Mode, KeyCode::Tab, false, false),
        AuthFormKeyPlan::Focus(AuthFormFocus::Save)
    );
    assert_eq!(
        auth_form_key_plan(
            AuthFormFocus::CredentialSource,
            KeyCode::BackTab,
            true,
            false
        ),
        AuthFormKeyPlan::Focus(AuthFormFocus::Mode)
    );
    let mut focus = AuthFormFocus::Mode;
    for _ in 0..5 {
        let AuthFormKeyPlan::Focus(next) = auth_form_key_plan(focus, KeyCode::Tab, true, false)
        else {
            panic!("tab did not advance focus");
        };
        focus = next;
    }
    assert_eq!(focus, AuthFormFocus::Mode);
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Save, KeyCode::Enter, true, true),
        AuthFormKeyPlan::Save
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Save, KeyCode::Enter, true, false),
        AuthFormKeyPlan::Stay
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Cancel, KeyCode::Enter, true, false),
        AuthFormKeyPlan::Cancel
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Reset, KeyCode::Enter, true, false),
        AuthFormKeyPlan::Reset
    );
    assert_eq!(
        auth_form_key_plan(AuthFormFocus::Mode, KeyCode::Char(' '), true, false),
        AuthFormKeyPlan::CycleMode
    );
}
