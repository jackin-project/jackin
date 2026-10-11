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

pub(crate) mod amp;
pub(crate) mod antigravity;
pub(crate) mod claude;
pub(crate) mod codex;
pub(crate) mod cursor;
pub(crate) mod gemini;
pub(crate) mod grok;
pub(crate) mod hermes;
pub(crate) mod kimi;
pub(crate) mod muse;
pub(crate) mod omp;
pub(crate) mod opencode;

pub(crate) use amp::AmpRuntime;
pub(crate) use antigravity::AntigravityRuntime;

#[cfg(test)]
mod tests;
pub(crate) use claude::ClaudeRuntime;
pub(crate) use codex::CodexRuntime;
pub(crate) use cursor::CursorRuntime;
pub(crate) use gemini::GeminiRuntime;
pub(crate) use grok::GrokRuntime;
pub(crate) use hermes::HermesRuntime;
pub(crate) use kimi::KimiRuntime;
pub(crate) use muse::MuseRuntime;
pub(crate) use omp::OmpRuntime;
pub(crate) use opencode::OpencodeRuntime;

use super::runtime::AgentRuntime;

/// All twelve built-in adapters in the canonical declaration order.
///
/// Adding a new runtime is one new file + one line here.
pub(crate) const fn registry() -> &'static [&'static dyn AgentRuntime] {
    &[
        &ClaudeRuntime,
        &CodexRuntime,
        &AmpRuntime,
        &KimiRuntime,
        &OpencodeRuntime,
        &GrokRuntime,
        &AntigravityRuntime,
        &GeminiRuntime,
        &CursorRuntime,
        &MuseRuntime,
        &OmpRuntime,
        &HermesRuntime,
    ]
}
