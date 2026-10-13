//! jackin-runtime-launch-account-config: launch account configuration.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`account_config::configure_accounts`] — provider account publication.
//!
//! Publishes per-instance provider account files, model catalogs,
//! and configuration fingerprints with durable atomic writes.
//! Split out of `jackin-runtime` (S7 split 63); the old
//! `jackin_runtime::runtime::launch::account_config::*` paths keep
//! working through a re-export shim.

pub mod account_config;
