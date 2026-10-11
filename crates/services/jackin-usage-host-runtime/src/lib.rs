//! jackin-usage-host-runtime: credential-free host projection presentation.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`host::HostUsageProjectionRuntime`] — presentation state over
//! complete broker-owned publications.
//!
//! Provider work, credential resolution, discovery, and shared state are owned
//! by the host usage broker. This crate validates and presents broker
//! projections; it does not discover credentials or invoke providers.

pub mod host;
