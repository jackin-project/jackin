//! jackin-runtime-universe: construct-entry/exit boundary tracking.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`universe::claim_entry`] — entry boundary claim.
//!
//! Tracks how long the operator has been "in the construct": pending
//! entry claims, the start-instant marker, and the single-consumer
//! exit claim behind the boundary outro. Split out of
//! `jackin-runtime` (S7 split 58); the old
//! `jackin_runtime::runtime::universe::*` paths keep working
//! through a re-export shim.

pub mod universe;
