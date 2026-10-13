//! jackin-runtime-launch-programmatic-selection: launch selection.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`selection::with_account_selection`] —
//! account selection.
//!
//! Ephemeral account/configuration selection for programmatic launches,
//! validated through launch admission. Split out of `jackin-runtime`
//! (S7 split 86); the old
//! `jackin_runtime::runtime::launch::programmatic::*` paths keep
//! working through item re-exports in `programmatic.rs`.

pub mod selection;
