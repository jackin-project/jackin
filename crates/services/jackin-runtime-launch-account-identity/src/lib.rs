//! jackin-runtime-launch-account-identity: launch account admission identity.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`account_identity::AccountConfigRevision`] — persisted-config generation lease.
//!
//! Holds the immutable generation lease over the persisted config,
//! durable credential publication with crash recovery, and the
//! account-configuration fingerprints that gate restore admission.
//! Split out of `jackin-runtime` (S7 split 64); the old
//! `jackin_runtime::runtime::launch::account_identity::*` paths keep
//! working through a re-export shim.

pub mod account_identity;
