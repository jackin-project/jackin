// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod credentials;
mod form;
mod key_plans;
mod lines;
mod render;
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
#[cfg(test)]
mod tests;

pub(crate) use lines::{action_buttons_line, credential_env_line, label_style, source_folder_line};
