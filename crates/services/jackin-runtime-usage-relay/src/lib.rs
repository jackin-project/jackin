//! jackin-runtime-usage-relay: usage relay tunnels for launches.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`usage_relay::prepare_for_stdio_tunnel`] — stdio tunnel setup.
//!
//! Relays host usage-broker traffic into role containers: tunnel setup for
//! the docker and Apple-container backends, launch usage-capability
//! inventory, and credential-scope resolution. Split out of
//! `jackin-runtime` (S7 split 70); the old
//! `jackin_runtime::usage_relay::*` paths keep working through a module
//! re-export in `lib.rs`.

pub mod usage_relay;
