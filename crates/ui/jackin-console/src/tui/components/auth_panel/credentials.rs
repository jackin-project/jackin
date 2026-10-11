// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Credential traits, states, and core impls.

use crate::tui::components::TextInputState;

use crate::tui::components::source_picker::SourcePickerState;
use crate::tui::screens::settings::model::AuthFormFocus;

pub trait AuthCredentialRef: Clone + std::fmt::Debug + PartialEq + Eq {
    fn path(&self) -> &str;

    fn is_empty(&self) -> bool {
        self.path().is_empty()
    }
}

pub trait AuthCredential: Clone + std::fmt::Debug + PartialEq + Eq {
    type Ref: AuthCredentialRef;

    fn into_credential_input(self) -> CredentialInput<Self::Ref>;
    fn from_plain(value: String) -> Self;
    fn from_op_ref(value: Self::Ref) -> Self;
}

/// What the user has supplied in the credential block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialInput<R> {
    None,
    Literal(String),
    OpRef(R),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFormKeyPlan {
    Stay,
    Focus(AuthFormFocus),
    CycleMode,
    OpenCredentialSource,
    OpenSourceFolderBrowser,
    Save,
    Cancel,
    Reset,
}

#[must_use]
pub fn auth_source_picker_state(
    env_var: impl Into<String>,
    op_available: bool,
) -> SourcePickerState {
    SourcePickerState::new(env_var.into(), op_available)
}

#[must_use]
pub fn auth_credential_input_state<'a>(literal: impl Into<String>) -> TextInputState<'a> {
    TextInputState::new_secret("Credential", literal)
}

#[must_use]
pub fn auth_panel_title(kind_label: &str) -> String {
    format!(" {kind_label} ")
}

/// `AuthCredentialRef` impl for `jackin_core::OpRef`.
///
/// Lives here (where the trait is defined) rather than in the binary crate
/// to satisfy the orphan rule — both the trait and the type are external to
/// the binary but this crate defines the trait.
impl AuthCredentialRef for jackin_core::OpRef {
    fn path(&self) -> &str {
        &self.path
    }

    fn is_empty(&self) -> bool {
        self.op.is_empty() || self.path.is_empty()
    }
}

/// `AuthCredential` impl for `jackin_core::EnvValue`.
impl AuthCredential for jackin_core::EnvValue {
    type Ref = jackin_core::OpRef;

    fn into_credential_input(self) -> CredentialInput<Self::Ref> {
        match self {
            Self::Plain(value) => CredentialInput::Literal(value),
            Self::Extended(e) => CredentialInput::Literal(e.value),
            Self::OpRef(value) => CredentialInput::OpRef(value),
        }
    }

    fn from_plain(value: String) -> Self {
        Self::Plain(value)
    }

    fn from_op_ref(value: Self::Ref) -> Self {
        Self::OpRef(value)
    }
}
