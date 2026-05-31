//! Auth panel: edit-form and supporting data types for the Auth tab.
//!
//!   - `render.rs` : `render_form`, colour constants, `mode_str`
//!   - `form.rs`   : `AuthForm`, `AuthFormOutcome`, `CredentialInput`
//!   - `state.rs`  : test-only fixtures (`CredentialBadge` etc.);
//!     production rendering uses explicit `WorkspaceSource` /
//!     `RoleSource` rows on the Auth tab.
//!
//! Flat-row Auth tab rendering lives in `src/console/tui/render/editor.rs`.

pub mod form;
pub mod render;
#[cfg(test)]
pub mod state;

pub use form::{AuthForm, AuthFormOutcome, CredentialInput};
pub(crate) use render::mode_str;
pub use render::{render_form, required_height};
