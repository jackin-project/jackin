//! jackin-usage-host-runtime: capsule-free host usage runtime for menu bar and CLI.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`host::HostUsageRuntime`] — presentation-state runtime over the broker.
//!
//! Holds presentation state only: provider work and shared state are owned by
//! the host usage broker. Split out of `jackin-usage` (S7 split 38); the old
//! `jackin_usage::host::*` paths keep working through a re-export shim.

pub mod host;
