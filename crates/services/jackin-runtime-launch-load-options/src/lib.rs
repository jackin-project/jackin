//! jackin-runtime-launch-load-options: load options bag.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`load_options::LoadOptions`] — launch options.
//!
//! Caller-supplied options for `jackin load` ([`load_options::LoadOptions`]),
//! the programmatic-launch validation over them
//! ([`load_options::LoadOptions::validate_programmatic`]), and the identity
//! the launch reports back ([`load_options::LaunchedInstance`]). Split out
//! of `jackin-runtime` (S7 split 87); the old
//! `jackin_runtime::runtime::launch::LoadOptions` and
//! `jackin_runtime::runtime::launch::programmatic::*` paths keep working
//! through item re-exports in `launch.rs` and `programmatic.rs`.

pub mod load_options;
