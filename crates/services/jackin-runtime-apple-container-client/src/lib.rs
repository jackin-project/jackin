//! jackin-runtime-apple-container-client: Apple Container backend client.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`apple_container_client::AppleContainerClient`] — `container` CLI lifecycle.
//!
//! Defines the `AppleContainerApi` trait and its production
//! implementation, which shells out to the `container` CLI via the
//! shared process transport (`jackin-process` + process telemetry).
//! Split out of `jackin-runtime` (S7 split 59); the old
//! `jackin_runtime::apple_container_client::*` paths keep working
//! through a re-export shim.

pub mod apple_container_client;
