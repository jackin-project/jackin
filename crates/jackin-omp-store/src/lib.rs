// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded, source-bound snapshots of OMP's credential database.
//!
//! The crate reads the user's SQLite files as ordinary no-follow files, then
//! opens only a private captured copy with SQLite. Account identities are
//! exposed without secret values; materialization must use the exact selection
//! made from the same snapshot.

mod capture;
mod query;
mod wal;

pub use capture::{OmpSelectedAccount, OmpSnapshot};
pub use query::{OmpAccount, OmpSelector};

/// Maximum standalone SQLite image that the OMP snapshot can materialize.
pub const MAX_STANDALONE_DATABASE_BYTES: usize = 8 * 1024 * 1024;

/// Secret-free failure from capturing, validating, or materializing an OMP
/// account store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OmpError {
    /// The source is missing, changed, unsafe, malformed, or unavailable.
    #[error("OMP credential source is unavailable")]
    Unavailable,
    /// The source exceeds a physical or logical resource bound.
    #[error("OMP credential source exceeds a resource limit")]
    LimitExceeded,
    /// SQLite or the filesystem operation exceeded its deadline.
    #[error("OMP credential operation timed out")]
    Deadline,
    /// The requested provider/profile does not uniquely identify a current
    /// credential in this snapshot.
    #[error("OMP account selection is missing or ambiguous")]
    SelectionUnavailable,
}
