//! jackin-runtime-universe-claims: construct-boundary claim types.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`claims::EntryClaim`] — pending entry lease.
//!
//! Claim objects for the operator-span boundary (`EntryClaim`,
//! `ExitClaim`, `StartKind`) plus the shared file machinery behind
//! them (`boundary`: lock, generation, state and pending files).
//! Split out of `jackin-runtime-universe` (S7 split 83); the
//! `jackin_runtime::runtime::universe::*` and
//! `jackin_runtime_universe::universe::*` paths keep working
//! through re-exports in `universe.rs`.

pub mod boundary;
pub mod claims;
