// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::AuthFormKeyPlan;

use crossterm::event::KeyCode;

use crate::tui::screens::settings::model::AuthFormFocus;

#[must_use]
pub fn auth_form_key_plan(
    focus: AuthFormFocus,
    key: KeyCode,
    shows_credential_block: bool,
    can_save: bool,
) -> AuthFormKeyPlan {
    auth_form_key_plan_with_source_folder(focus, key, false, shows_credential_block, can_save)
}

#[must_use]
pub fn auth_form_focus_chain(
    shows_source_folder: bool,
    shows_credential_block: bool,
) -> Vec<AuthFormFocus> {
    let mut chain = Vec::with_capacity(6);
    chain.push(AuthFormFocus::Mode);
    if shows_source_folder {
        chain.push(AuthFormFocus::SourceFolder);
    }
    if shows_credential_block {
        chain.push(AuthFormFocus::CredentialSource);
    }
    chain.extend([
        AuthFormFocus::Save,
        AuthFormFocus::Cancel,
        AuthFormFocus::Reset,
    ]);
    chain
}

#[must_use]
pub(crate) fn auth_form_focus_step(
    chain: &[AuthFormFocus],
    focus: AuthFormFocus,
    delta: isize,
) -> AuthFormFocus {
    let index = chain.iter().position(|field| *field == focus).unwrap_or(0);
    let len = isize::try_from(chain.len()).unwrap_or(isize::MAX);
    let next = (isize::try_from(index).unwrap_or(0) + delta).rem_euclid(len);
    chain[usize::try_from(next).unwrap_or(0)]
}

#[must_use]
pub fn auth_form_key_plan_with_source_folder(
    focus: AuthFormFocus,
    key: KeyCode,
    shows_source_folder: bool,
    shows_credential_block: bool,
    can_save: bool,
) -> AuthFormKeyPlan {
    let chain = auth_form_focus_chain(shows_source_folder, shows_credential_block);
    match focus {
        AuthFormFocus::Mode => match key {
            KeyCode::Char(' ') => AuthFormKeyPlan::CycleMode,
            KeyCode::Down | KeyCode::Char('j') if shows_source_folder || shows_credential_block => {
                AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, 1))
            }
            KeyCode::Tab => AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, 1)),
            KeyCode::BackTab => AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, -1)),
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::SourceFolder => match key {
            KeyCode::Enter => AuthFormKeyPlan::OpenSourceFolderBrowser,
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Tab => {
                AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, 1))
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, -1))
            }
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::CredentialSource => match key {
            KeyCode::Enter => AuthFormKeyPlan::OpenCredentialSource,
            KeyCode::Tab => AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, 1)),
            KeyCode::Up | KeyCode::Char('k') | KeyCode::BackTab => {
                AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, -1))
            }
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::Save => match key {
            KeyCode::Right | KeyCode::Tab => {
                AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, 1))
            }
            KeyCode::BackTab => AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, -1)),
            KeyCode::Enter if can_save => AuthFormKeyPlan::Save,
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::Cancel => match key {
            KeyCode::Left | KeyCode::BackTab => {
                AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, -1))
            }
            KeyCode::Right | KeyCode::Tab => {
                AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, 1))
            }
            KeyCode::Enter => AuthFormKeyPlan::Cancel,
            _ => AuthFormKeyPlan::Stay,
        },
        AuthFormFocus::Reset => match key {
            KeyCode::Left | KeyCode::BackTab => {
                AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, -1))
            }
            KeyCode::Tab => AuthFormKeyPlan::Focus(auth_form_focus_step(&chain, focus, 1)),
            KeyCode::Enter => AuthFormKeyPlan::Reset,
            _ => AuthFormKeyPlan::Stay,
        },
    }
}
