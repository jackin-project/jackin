//! jackin-runtime-reactive-daemon: feature-gated host-daemon spike.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`reactive_daemon::serve_one`] — single-connection serve.
//!
//! Captures the proposed host-side control-socket shape and proves the
//! smallest first adapter: attention notifications from the runtime
//! status authority. Gated on `daemon-spike` + unix, mirroring the
//! pre-split gate. Split out of `jackin-runtime` (S7 split 57); the
//! old `jackin_runtime::reactive_daemon::*` paths keep working
//! through a re-export shim.

#[cfg(all(feature = "daemon-spike", unix))]
pub mod reactive_daemon;
