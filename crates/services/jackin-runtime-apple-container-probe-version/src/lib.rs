//! jackin-runtime-apple-container-probe-version: `container` CLI version probe.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`probe_version::probe_version`] —
//! probe the `container` CLI version, `None` when missing.
//!
//! Split out of `jackin-runtime` (S7 split 112): the version
//! probe at the head of the apple-container `launch` path.
//! The hub keeps a private import; the step had no external
//! callers.

pub mod probe_version;
