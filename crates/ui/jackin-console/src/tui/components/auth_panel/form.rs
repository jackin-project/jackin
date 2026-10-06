// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `AuthForm` state and mode helpers.

use super::{AuthCredential, AuthCredentialRef, CredentialInput};
use std::marker::PhantomData;
use std::path::PathBuf;

use crate::tui::auth::{
    AuthKind, AuthMode, auth_mode_requires_credential, auth_mode_supports_source_folder,
};

use crate::tui::components::editor_rows::AuthSourceFolderDisplay;

use crate::tui::screens::settings::model::AuthFormFocus;

/// The form's mutable state. Mode and credential are independently editable;
/// only the [`AuthForm::can_save`] invariant decides whether the parent should
/// allow the Save action.
#[derive(Debug)]
pub struct AuthForm<V: AuthCredential> {
    pub kind: AuthKind,
    pub mode: Option<AuthMode>,
    pub credential: CredentialInput<V::Ref>,
    pub source_folder: Option<PathBuf>,
    pub source_folder_fallback: Option<AuthSourceFolderDisplay>,
    _value: PhantomData<fn() -> V>,
}

/// Output of a successful commit. The parent uses these fields to write the
/// kind block and the env-var entry at the chosen layer.
#[derive(Debug, Clone)]
pub struct AuthFormOutcome<V> {
    pub mode: AuthMode,
    pub env_var_name: Option<&'static str>,
    pub env_value: Option<V>,
    pub source_folder: Option<PathBuf>,
}

impl<V: AuthCredential> AuthForm<V> {
    pub const fn new(kind: AuthKind) -> Self {
        Self {
            kind,
            mode: None,
            credential: CredentialInput::None,
            source_folder: None,
            source_folder_fallback: None,
            _value: PhantomData,
        }
    }

    /// Pre-populate the form from an existing row's mode and credential.
    pub fn from_existing(kind: AuthKind, mode: AuthMode, credential: Option<V>) -> Self {
        let credential = credential.map_or(CredentialInput::None, V::into_credential_input);
        Self {
            kind,
            mode: Some(mode),
            credential,
            source_folder: None,
            source_folder_fallback: None,
            _value: PhantomData,
        }
    }

    #[must_use]
    pub fn with_source_folder(
        mut self,
        source_folder: Option<PathBuf>,
        fallback: Option<AuthSourceFolderDisplay>,
    ) -> Self {
        self.source_folder = source_folder;
        self.source_folder_fallback = fallback;
        self
    }

    /// Set the mode. If switching to a mode that doesn't need a credential,
    /// clears the credential field automatically.
    pub fn set_mode(&mut self, mode: AuthMode) {
        debug_assert!(
            self.kind.supported_modes().contains(&mode),
            "AuthMode::{mode:?} not supported by AuthKind::{:?}",
            self.kind,
        );
        self.mode = Some(mode);
        if !mode_requires_credential(self.kind, mode) {
            self.credential = CredentialInput::None;
        }
    }

    pub fn set_literal(&mut self, value: String) {
        self.credential = CredentialInput::Literal(value);
    }

    pub fn literal_buffer(&self) -> String {
        match &self.credential {
            CredentialInput::Literal(value) => value.clone(),
            CredentialInput::None | CredentialInput::OpRef(_) => String::new(),
        }
    }

    pub fn set_op_ref(&mut self, value: V::Ref) {
        self.credential = CredentialInput::OpRef(value);
    }

    pub fn set_source_folder(&mut self, value: PathBuf) {
        self.source_folder = Some(value);
    }

    /// Whether the source-folder row should be shown.
    pub fn shows_source_folder(&self) -> bool {
        matches!(self.mode, Some(mode) if auth_mode_supports_source_folder(self.kind, mode))
    }

    /// Whether the credential input block should be shown.
    pub const fn shows_credential_block(&self) -> bool {
        matches!(self.mode, Some(mode) if mode_requires_credential(self.kind, mode))
    }

    pub fn cycle_mode(&mut self) {
        let modes = self.available_modes();
        if modes.is_empty() {
            return;
        }
        let next = self.mode.map_or(modes[0], |current| {
            let idx = modes.iter().position(|mode| *mode == current).unwrap_or(0);
            modes[(idx + 1) % modes.len()]
        });
        self.set_mode(next);
    }

    pub fn next_focus_after_mode(&self) -> AuthFormFocus {
        if self.shows_source_folder() {
            AuthFormFocus::SourceFolder
        } else if self.shows_credential_block() {
            AuthFormFocus::CredentialSource
        } else {
            AuthFormFocus::Save
        }
    }

    /// Modes the user can pick.
    pub const fn available_modes(&self) -> &'static [AuthMode] {
        self.kind.supported_modes()
    }

    /// Save invariant: mode is committed and, if needed, a non-empty credential.
    pub fn can_save(&self) -> bool {
        let Some(mode) = self.mode else { return false };
        if !self.available_modes().contains(&mode) {
            return false;
        }
        if auth_mode_supports_source_folder(self.kind, mode) {
            return self
                .source_folder
                .as_ref()
                .is_some_and(|path| !path.as_os_str().is_empty());
        }
        if !mode_requires_credential(self.kind, mode) {
            return self.kind == AuthKind::Github;
        }
        match &self.credential {
            CredentialInput::None => false,
            CredentialInput::Literal(value) => !value.trim().is_empty(),
            CredentialInput::OpRef(value) => !value.is_empty(),
        }
    }

    /// Build the outcome for the parent to persist. Returns None if `!can_save`.
    pub fn commit(&self) -> Option<AuthFormOutcome<V>> {
        if !self.can_save() {
            return None;
        }
        let mode = self.mode?;
        let env_var_name = self.kind.required_env_var(mode);
        let env_value = match &self.credential {
            CredentialInput::None => None,
            CredentialInput::Literal(value) => Some(V::from_plain(value.clone())),
            CredentialInput::OpRef(value) => Some(V::from_op_ref(value.clone())),
        };
        Some(AuthFormOutcome {
            mode,
            env_var_name,
            env_value,
            source_folder: self.source_folder.clone(),
        })
    }
}

pub(crate) const fn mode_requires_credential(kind: AuthKind, mode: AuthMode) -> bool {
    auth_mode_requires_credential(kind, mode)
}

/// Operator-facing slug for an [`AuthMode`].
pub const fn mode_str(mode: AuthMode) -> &'static str {
    mode.as_str()
}

pub(crate) const AUTH_FORM_MODE_LABEL_WIDTH: usize = 23;
pub(crate) const AUTH_FORM_CREDENTIAL_LABEL_WIDTH: usize = 23;
