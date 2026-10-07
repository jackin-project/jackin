//! jackin-runtime-attach-sessions: session inventory.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`sessions::inspect_agent_sessions`] — inventory.
//!
//! Agent session inventory inspection plus the shared
//! `docker inspect`-failure message builders. Split out of
//! `jackin-runtime` (S7 split 73); the old
//! `jackin_runtime::runtime::attach::sessions::*` paths keep
//! working through a module re-export in `attach.rs`.

pub mod sessions;
