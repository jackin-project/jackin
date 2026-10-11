//! jackin-runtime-apple-container-attach-outcome: post-attach outcome recording.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`attach_outcome::record_attach_outcome`] —
//! record whether a detached apple/container role is still running.
//!
//! Split out of `jackin-runtime` (S7 split 109): the outcome
//! recorder shared by the apple-container `launch` and `reconnect`
//! paths. The hub keeps a private import; the recorder had
//! no external callers.

pub mod attach_outcome;
