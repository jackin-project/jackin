//! Auth panel: edit-form and supporting data types for the Auth tab.
//!
//!   - `state.rs`   : `ProvenanceTag`, `CredentialBadge`, `badge_for`, `classify_env_value`
//!   - `render.rs`  : `render_form`, `FormContext`, colour constants, `agent_display`, `mode_str`
//!   - `form.rs`    : `AuthForm`, `AuthFormOutcome`, `CredentialInput`
//!
//! Flat-row Auth tab rendering lives in `src/console/manager/render/editor.rs`.

pub mod form;
pub mod render;
pub mod state;

pub use form::{AuthForm, AuthFormOutcome, CredentialInput};
pub use render::{FormContext, render_form};
pub(crate) use render::{DANGER_RED, PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE, agent_display, mode_str};
pub use state::{CredentialBadge, ProvenanceTag};
pub(crate) use state::badge_for;
