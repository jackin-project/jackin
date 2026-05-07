//! Auth panel: edit-form and supporting data types for the Auth tab.
//!
//!   - `render.rs`  : `render_form`, `FormContext`, colour constants, `mode_str`
//!   - `form.rs`    : `AuthForm`, `AuthFormOutcome`, `CredentialInput`
//!
//! Flat-row Auth tab rendering lives in `src/console/manager/render/editor.rs`.

pub mod form;
pub mod render;
#[cfg(test)]
pub mod state;

pub use form::{AuthForm, AuthFormOutcome, CredentialInput};
pub(crate) use render::{DANGER_RED, PHOSPHOR_DARK, WHITE, mode_str};
pub use render::{FormContext, render_form};
