//! jackin-runtime-apple-container-running: running-state probe for apple-container attach.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`running::is_container_running`] —
//! report whether an apple/container container is running.
//!
//! Split out of `jackin-runtime` (S7 split 108): the probe
//! shared by the reconnect path and the post-attach outcome
//! recording. The hub keeps a private import; the probe had
//! no external callers.

pub mod running;
