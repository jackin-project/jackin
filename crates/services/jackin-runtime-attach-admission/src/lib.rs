//! jackin-runtime-attach-admission: reconnect admission.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`admission::require_current_account_admission`] — admission.
//!
//! Reconnect admission types plus current-account/instance
//! admission checks against live host policy. Split out of
//! `jackin-runtime` (S7 split 78); the old
//! `jackin_runtime::runtime::attach::admission::*` paths keep
//! working through a module re-export in `attach.rs`.

pub mod admission;
