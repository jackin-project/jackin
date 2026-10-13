//! jackin-runtime-launch-auth-error: credential source display and proxy helpers.
//!
//! **Architecture Invariant:** T0.
//! Entry point: [`auth_error::auth_token_source_reference`] — source reference.
//!
//! Credential source display (`"KEY ← value"` references) plus the proxy env
//! helpers launch uses when composing container env (`NO_PROXY` merge,
//! proxy-name detection, skip-if-blank push). Split out of `jackin-runtime`
//! (S7 split 67); the old `jackin_runtime::runtime::launch::auth_error::*`
//! paths keep working through a module re-export in `launch.rs`.

pub mod auth_error;
