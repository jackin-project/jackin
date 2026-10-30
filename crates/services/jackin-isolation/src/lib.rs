pub mod backup;
pub mod branch;
pub mod capture;
pub mod cleanup;
pub mod dissociate;
mod error;
pub mod finalize;
pub mod git_inspect;
pub mod holds;
pub mod materialize;
mod ref_transaction;
pub mod safe_remove;
pub mod state;

pub use error::IsolationError;
pub use jackin_core::MountIsolation;
pub use jackin_core::ParseMountIsolationError;
pub use safe_remove::OwnedRemoval;

#[cfg(test)]
mod tests;
