//! Auth panel: lists role-agent auth modes for the active workspace.
//!
//! Splits state from rendering for unit-testability:
//!   - `state.rs`   : pure data types and `AuthPanelState::compute_for`
//!   - `render.rs`  : ratatui render path (Task 16)
//!   - `form.rs`    : edit-form state (Task 17)
//!
//! Mounted as a peer to the Secrets panel by Task 19.

pub mod form;
pub mod render;
pub mod state;

pub use form::{AuthForm, AuthFormOutcome, CredentialInput};
pub use render::{FormContext, render, render_form, render_with_selection};
pub use state::{AuthPanelState, AuthRow, CredentialBadge, ProvenanceTag};
