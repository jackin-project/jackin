//! jackin-runtime-apple-container-wait: capsule readiness wait.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`wait::wait_for_capsule`] —
//! wait until the capsule daemon negotiates the protocol major.
//!
//! Split out of `jackin-runtime` (S7 split 111): the readiness
//! poll shared by the apple-container `launch` and `reconnect`
//! paths. The hub keeps a private import; the step had no
//! external callers.

pub mod wait;
