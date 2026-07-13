// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Static `AgentRuntime` adapter registry.
//!
//! Each built-in agent is a zero-sized unit struct that implements
//! `AgentRuntime`.  `registry()` returns a `&'static [&'static dyn
//! AgentRuntime]` so call sites can iterate or look up adapters without
//! touching the `Agent` enum directly.
//!
//! Module layout follows the self-named convention (crates/AGENTS.md):
//! - `agent/adapters.rs` — this file (registry + re-exports)
//! - `agent/adapters/claude.rs` — `ClaudeRuntime`
//! - `agent/adapters/codex.rs` — `CodexRuntime`
//! - `agent/adapters/amp.rs` — `AmpRuntime`
//! - `agent/adapters/kimi.rs` — `KimiRuntime`
//! - `agent/adapters/opencode.rs` — `OpencodeRuntime`

pub mod amp;
pub mod claude;
pub mod codex;
pub mod grok;
pub mod kimi;
pub mod opencode;

pub use amp::AmpRuntime;

#[cfg(test)]
mod tests;
pub use claude::ClaudeRuntime;
pub use codex::CodexRuntime;
pub use grok::GrokRuntime;
pub use kimi::KimiRuntime;
pub use opencode::OpencodeRuntime;

use super::runtime::AgentRuntime;

/// All six built-in adapters in the canonical declaration order.
///
/// Adding a new runtime is one new file + one line here.
pub const fn registry() -> &'static [&'static dyn AgentRuntime] {
    &[
        &ClaudeRuntime,
        &CodexRuntime,
        &AmpRuntime,
        &KimiRuntime,
        &OpencodeRuntime,
        &GrokRuntime,
    ]
}
