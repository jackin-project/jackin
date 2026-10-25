// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
