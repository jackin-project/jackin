// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared credential form and safe environment display helpers.
use crate::tui::auth::AuthKind;
use crate::tui::components::auth_panel::{AuthCredential, AuthForm};
use jackin_config::EnvValue;
use jackin_core::Agent;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[must_use]
pub const fn auth_kind_agent(kind: AuthKind) -> Option<Agent> {
    match kind {
        AuthKind::Claude => Some(Agent::Claude),
        AuthKind::Codex => Some(Agent::Codex),
        AuthKind::Amp => Some(Agent::Amp),
        AuthKind::Kimi => Some(Agent::Kimi),
        AuthKind::Opencode => Some(Agent::Opencode),
        AuthKind::Grok => Some(Agent::Grok),
        AuthKind::Github | AuthKind::Zai | AuthKind::Minimax => None,
    }
}

pub trait ModalAuthFormFocusInspect<AuthFormFocus> {
    fn active_auth_form_focus(&self) -> Option<AuthFormFocus>;
}

pub trait ModalAuthFormParentInspect {
    fn is_auth_form_parent(&self) -> bool;
}

pub trait ModalAuthPlainSourceOpen<TextInputTarget, TextInputState, AuthFormFocus>: Sized {
    fn open_auth_plain_source_text_input(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        credential_focus: AuthFormFocus,
        text_input_target: TextInputTarget,
        make_text_input: impl FnOnce(String) -> TextInputState,
    ) -> bool;
}

pub trait ModalAuthOpPickerOpen<OpPickerState, AuthFormFocus>: Sized {
    fn open_auth_op_picker(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        credential_focus: AuthFormFocus,
        make_op_picker: impl FnOnce() -> OpPickerState,
    ) -> bool;
}

pub trait AuthFormCredentialEdit {
    type OpRef;

    fn set_auth_literal(&mut self, value: String);
    fn set_auth_source_folder(&mut self, value: PathBuf);
    fn set_auth_op_ref(&mut self, value: Self::OpRef);
}

impl<V: AuthCredential> AuthFormCredentialEdit for AuthForm<V> {
    type OpRef = V::Ref;

    fn set_auth_literal(&mut self, value: String) {
        self.set_literal(value);
    }

    fn set_auth_source_folder(&mut self, value: PathBuf) {
        self.set_source_folder(value);
    }

    fn set_auth_op_ref(&mut self, value: Self::OpRef) {
        self.set_op_ref(value);
    }
}

pub trait ModalAuthFormCredentialApply<AuthFormFocus>: Sized {
    fn apply_auth_plain_text(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        save_focus: AuthFormFocus,
        value: &str,
    ) -> bool;

    fn apply_auth_source_folder(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        save_focus: AuthFormFocus,
        value: PathBuf,
    ) -> bool;

    fn restore_auth_form_modal(modal: &mut Option<Self>, modal_parents: &mut Vec<Self>) -> bool;
}

pub trait ModalAuthFormOpRefApply<AuthFormFocus, OpRef>: Sized {
    fn apply_auth_op_ref(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        save_focus: AuthFormFocus,
        value: OpRef,
    ) -> bool;
}

pub trait AuthFormCredentialSourceState {
    fn required_credential_env_var(&self) -> Option<&'static str>;
}

impl<V: AuthCredential> AuthFormCredentialSourceState for AuthForm<V> {
    fn required_credential_env_var(&self) -> Option<&'static str> {
        self.mode.and_then(|mode| self.kind.required_env_var(mode))
    }
}

pub trait ModalAuthSourcePickerOpen<SourcePickerState>: Sized {
    fn open_auth_source_picker(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        make_source_picker: impl FnOnce(&'static str) -> SourcePickerState,
    ) -> bool;
}

pub trait AuthFormSourceFolderState {
    fn shows_auth_source_folder(&self) -> bool;
}

impl<V: AuthCredential> AuthFormSourceFolderState for AuthForm<V> {
    fn shows_auth_source_folder(&self) -> bool {
        self.shows_source_folder()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSourceFolderBrowserOpenResult<E> {
    Opened,
    NotAvailable,
    BrowserError(E),
}

pub trait ModalAuthSourceFolderBrowserOpen<FileBrowserTarget, FileBrowserState, AuthFormFocus>:
    Sized
{
    fn open_auth_source_folder_browser<E>(
        modal: &mut Option<Self>,
        modal_parents: &mut Vec<Self>,
        source_folder_focus: AuthFormFocus,
        file_browser_target: FileBrowserTarget,
        make_browser: impl FnOnce() -> Result<FileBrowserState, E>,
    ) -> AuthSourceFolderBrowserOpenResult<E>;
}

#[must_use]
pub fn env_display_map(values: &BTreeMap<String, EnvValue>) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (key.clone(), value.as_display_str().to_owned()))
        .collect()
}

#[must_use]
pub fn env_display_map_without_auth_credentials(
    values: &BTreeMap<String, EnvValue>,
) -> BTreeMap<String, String> {
    let credential_keys = auth_credential_env_keys();
    values
        .iter()
        .filter(|(key, _)| !credential_keys.contains(key.as_str()))
        .map(|(key, value)| (key.clone(), value.as_display_str().to_owned()))
        .collect()
}

#[must_use]
pub fn auth_credential_env_keys() -> BTreeSet<&'static str> {
    AuthKind::SETTINGS_KINDS
        .iter()
        .flat_map(|kind| {
            kind.supported_modes()
                .iter()
                .filter_map(|mode| kind.required_env_var(*mode))
        })
        .chain(jackin_core::account_env_names())
        .collect()
}

#[cfg(test)]
mod tests;
