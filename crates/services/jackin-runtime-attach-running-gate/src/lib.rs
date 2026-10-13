//! jackin-runtime-attach-running-gate: running-state gate for attach spawn.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`running_gate::require_container_running`] —
//! verify only the Docker lifecycle state and return the
//! prevalidated container handle.
//!
//! Split out of `jackin-runtime` (S7 split 107): the gate
//! shared by the shell-spawn path (through
//! `require_container_reachable`) and the agent-spawn path.
//! The old
//! `jackin_runtime::runtime::attach::require_container_running`
//! path keeps working through the hub re-export.

pub mod running_gate;
