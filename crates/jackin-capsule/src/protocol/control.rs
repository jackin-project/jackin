//! Re-export of `jackin_protocol::control` so existing in-crate
//! `crate::protocol::control::...` import paths keep resolving.
//!
//! The authoritative copy of the control-channel wire types lives in
//! the shared `jackin-protocol` crate so the host (`jackin`) and the
//! in-container binary (`jackin-capsule`) can talk to each other
//! without `jackin` pulling in `jackin-capsule`'s tokio + PTY +
//! VT-parser stack. Anything added to the control channel goes there;
//! this module exists only as a path alias.

pub use jackin_protocol::control::*;
