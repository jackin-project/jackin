//! Auth panel: lists role-agent auth modes for the active workspace.
//!
//! Splits state from rendering for unit-testability:
//!   - `state.rs`   : pure data types and `AuthPanelState::compute_for`
//!   - `form.rs`    : edit-form state (Task 17)
//!   - rendering    : Task 16 will add a render method to `AuthPanel`
//!
//! Mounted as a peer to the Secrets panel by Task 19.

pub mod state;

pub use state::{AuthPanelState, AuthRow, CredentialBadge, ProvenanceTag};
