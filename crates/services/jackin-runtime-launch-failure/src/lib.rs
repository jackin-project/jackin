//! jackin-runtime-launch-failure: launch failure rendering and role-source resolve.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`failure::launch_failure_cli_error`] — pass-through CLI
//! error rendering for a failed launch stage.
//!
//! The failure title ([`failure::launch_failure_title`]), one-line
//! diagnosis ([`failure::short_launch_diagnosis`]), and CLI error
//! ([`failure::launch_failure_cli_error`]) for `jackin load`, plus
//! role-source resolution ([`failure::resolve_launch_role_source`]).
//! Split out of `jackin-runtime` (S7 split 90); the old
//! `jackin_runtime::runtime::launch::failure::*` paths keep working
//! through item re-exports in the hub shim (`launch.rs` is untouched).

pub mod failure;
