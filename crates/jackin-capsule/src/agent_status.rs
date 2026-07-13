//! Capsule-facing agent-status module.
//!
//! Detection, arbitration, policy, pack loading, and process sampling live in
//! `jackin-agent-status`. Capsule keeps only reporter installation because it
//! provisions files inside the role container.

pub use jackin_agent_status::*;

pub mod hook_installer;
