pub mod branch;
pub mod cleanup;
mod error;
pub mod finalize;
pub mod git_inspect;
pub mod materialize;
mod ref_transaction;
pub mod safe_remove;
pub mod state;

pub use error::IsolationError;
pub use jackin_core::MountIsolation;
pub use jackin_core::ParseMountIsolationError;
pub use safe_remove::OwnedRemoval;
