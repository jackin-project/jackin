//! jackin-runtime-launch-supported-agents: supported agents for a role.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`supported_agents::resolve_supported_agents_for_console`] —
//! resolve the agent list the console role picker shows for one role.
//!
//! Split out of `jackin-runtime` (S7 split 96): a lookup-only query over
//! the cached role manifest (falling back to a non-interactive repo
//! resolve), decoupled from the launch pipeline core. The old
//! `jackin_runtime::runtime::launch::resolve_supported_agents_for_console`
//! path keeps working through the hub re-export.

pub mod supported_agents;
