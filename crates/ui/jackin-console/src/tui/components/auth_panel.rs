// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Auth edit-form state and rendering.
//!
//! Field composition copy-adapted from the upstream `patterns/auth_entry.rs`
//! recipe (composition reference, never a type dependency): labeled
//! credential field with validation feedback, mode switch, and a
//! submit/cancel action row — the form's focus cycle is one ordered field
//! chain (`auth_form_focus_chain`) that Tab/BackTab and the arrow keys walk,
//! mirroring the recipe's focus routing. The secret field stays on the
//! console's existing masked-input rows (plan 010 recorded the
//! `password_input` adoption as a behavior-preserving carve-out), and all
//! jackin❯ auth domain branches (op-refs, literals, source folders,
//! generated tokens) stay — the recipe covers field anatomy only.
mod credentials;
mod form;
mod key_plans;
mod lines;
mod render;
#[cfg(test)]
mod tests;
pub use credentials::{
    AuthCredential, AuthCredentialRef, AuthFormKeyPlan, CredentialInput,
    auth_credential_input_state, auth_panel_title, auth_source_picker_state,
};
pub(crate) use form::{AUTH_FORM_CREDENTIAL_LABEL_WIDTH, AUTH_FORM_MODE_LABEL_WIDTH};
pub use form::{AuthForm, AuthFormOutcome, mode_str};
pub use key_plans::{
    auth_form_focus_chain, auth_form_key_plan, auth_form_key_plan_with_source_folder,
};
pub use render::{render_form, required_height};

pub(crate) use lines::{action_buttons_line, credential_env_line, label_style, source_folder_line};

#[cfg(test)]
pub(crate) use crate::tui::auth::AuthKind;
#[cfg(test)]
pub(crate) use crate::tui::auth::AuthMode;
#[cfg(test)]
pub(crate) use crate::tui::components::editor_rows::AuthSourceFolderDisplay;
#[cfg(test)]
pub(crate) use crate::tui::components::editor_rows::AuthSourceFolderKind;
#[cfg(test)]
pub(crate) use crate::tui::screens::settings::model::AuthFormFocus;
#[cfg(test)]
pub(crate) use crossterm::event::KeyCode;
#[cfg(test)]
pub(crate) use std::path::PathBuf;
