//! Shared wire-format types for jackin's host CLI ↔ in-container
//! multiplexer control channel.
//!
//! Lives in its own crate so the host (`jackin`) and the
//! in-container binary (`jackin-container`) can both depend on it
//! without the host pulling in `jackin-container`'s tokio + PTY +
//! VT-parser stack. Anything declared here is on the wire between
//! the two processes; anything not on the wire belongs elsewhere.

pub mod control;
