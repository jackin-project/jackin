//! `Agent` enum: the set of AI agents jackin❯ can provision inside a role
//! container.
//!
//! Single source of truth for agent identity — variant ordering, display
//! labels, CLI slug parsing, and serde shape. Every match arm across the
//! codebase that keys on agent identity should use this enum rather than
//! string comparisons.
//!
//! The type definition lives in `jackin-core`; this module re-exports it.
//! The `AgentChoice` trait impl for `Agent` lives in `jackin-console`
//! (where the trait is defined), satisfying the orphan rule.

pub use jackin_core::Agent;
pub use jackin_core::ParseAgentError;

pub type AgentChoiceState = jackin_console::tui::components::agent_choice::AgentChoiceState<Agent>;
