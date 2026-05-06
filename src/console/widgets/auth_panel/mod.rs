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
pub use render::{FormContext, agent_display, mode_str, render_form};
pub use state::{CredentialBadge, ProvenanceTag, badge_for, classify_env_value};
