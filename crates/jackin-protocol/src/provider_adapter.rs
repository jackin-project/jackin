// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `ProviderAdapter` trait: per-provider behavioral dispatch.
//!
//! Each of the five built-in providers (Anthropic, `OpenAI`, Zai, `MiniMax`,
//! Kimi) implements this trait in a zero-sized adapter struct.
//! `Provider::adapter()` returns the matching adapter as
//! `&'static dyn ProviderAdapter`.
//!
//! Design:
//! - Trait is **sealed** so external crates cannot add providers. The five
//!   built-in adapters are the only implementors.
//! - Zero-sized unit structs: no allocation, no per-call overhead.
//! - Adding a new provider requires exactly one registration here plus one
//!   variant in the `Provider` enum — no N closed matches scattered elsewhere.
//!
//! Sibling to `jackin_core::agent::runtime::AgentRuntime`.

/// Sealing module — prevents external crates from implementing `ProviderAdapter`.
pub(crate) mod private {
    pub(crate) trait Sealed {}
}

/// Behavioral contract each provider adapter satisfies.
///
/// Reach via `provider.adapter().<method>()`. Sealed: only the five built-in
/// adapters in this module implement it.
#[expect(
    private_bounds,
    reason = "sealed trait uses a private supertrait to block external implementations"
)]
pub trait ProviderAdapter: Send + Sync + 'static + private::Sealed {
    /// Display label (also the wire identifier in `InitialProvider`).
    fn label(&self) -> &'static str;

    /// Whether this provider requires an explicit API key for `agent_slug`.
    ///
    /// - Anthropic: `false` for `claude` (subscription auth); `true` for all
    ///   others where the subscription does not extend (e.g. `opencode`).
    /// - `OpenAI`: `false` for `codex` (the agent uses its own auth.json sync
    ///   or `OPENAI_API_KEY` from the host env, not a key stored in jackin
    ///   config); `true` for everything else (no other agent is supported).
    /// - All alt-providers: `true` always.
    fn needs_key_for_agent(&self, agent_slug: &str) -> bool;

    /// Whether this provider is wired for `agent_slug` at all.
    ///
    /// Returns `false` for agents whose API surface is not yet compatible
    /// (e.g. Zai/Kimi are blocked on Codex because they lack a Responses API).
    fn supports_agent(&self, agent_slug: &str) -> bool;

    /// Env overrides that redirect Claude Code to this provider.
    /// `token` is the provider's API key; `None` or empty → omit auth header.
    fn env_overrides(&self, token: Option<&str>) -> Vec<(String, String)>;

    /// Model string in `provider/model` format for `OpenCode`'s `-m` flag.
    /// `None` for Anthropic (use `OpenCode`'s own default selection).
    fn opencode_model(&self) -> Option<&'static str>;

    /// The environment variable that carries this provider's API key, or `None`
    /// if the provider has no key variable at all.
    ///
    /// This names the key variable unconditionally; whether a given agent
    /// *needs* that key (versus the agent's own subscription auth) is answered
    /// by `needs_key_for_agent`. Use this in `Provider::available_for` closures
    /// so callers map a provider to its key lookup without hardcoding the name.
    fn key_env_var(&self) -> Option<&'static str>;

    /// Codex v2 profile name for this provider, or `None` if this provider
    /// does not need a Codex profile (native `OpenAI` auth or unsupported).
    ///
    /// When `Some(name)`, the capsule passes `--profile <name>` to the Codex
    /// launch command and the runtime-setup step writes
    /// `~/.codex/<name>.config.toml` activating the provider.
    fn codex_profile(&self) -> Option<&'static str> {
        None
    }
}

// ── Shared helper ─────────────────────────────────────────────────────────────

/// Build the standard Anthropic-surface env block for an alt provider.
fn anthropic_surface(
    base_url: &str,
    opus: &str,
    sonnet: &str,
    haiku: &str,
    timeout: &str,
    token: Option<&str>,
) -> Vec<(String, String)> {
    let mut env = Vec::with_capacity(7);
    if let Some(t) = token.filter(|v| !v.is_empty()) {
        env.push(("ANTHROPIC_AUTH_TOKEN".to_owned(), t.to_owned()));
    }
    env.push(("ANTHROPIC_BASE_URL".to_owned(), base_url.to_owned()));
    env.push(("ANTHROPIC_DEFAULT_OPUS_MODEL".to_owned(), opus.to_owned()));
    env.push((
        "ANTHROPIC_DEFAULT_SONNET_MODEL".to_owned(),
        sonnet.to_owned(),
    ));
    env.push(("ANTHROPIC_DEFAULT_HAIKU_MODEL".to_owned(), haiku.to_owned()));
    env.push(("API_TIMEOUT_MS".to_owned(), timeout.to_owned()));
    env.push((
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_owned(),
        "1".to_owned(),
    ));
    env
}

// ── Adapter structs ───────────────────────────────────────────────────────────

/// Anthropic — the agent's native auth surface; no env redirection.
#[derive(Debug)]
pub struct AnthropicAdapter;
impl private::Sealed for AnthropicAdapter {}
impl ProviderAdapter for AnthropicAdapter {
    fn label(&self) -> &'static str {
        "Anthropic"
    }

    fn needs_key_for_agent(&self, agent_slug: &str) -> bool {
        // Claude Code subscription covers `claude`; other agents need an explicit key.
        agent_slug != "claude"
    }

    fn supports_agent(&self, agent_slug: &str) -> bool {
        // Anthropic works with every agent that has Anthropic-compatible auth.
        matches!(agent_slug, "claude" | "opencode")
    }

    fn env_overrides(&self, _token: Option<&str>) -> Vec<(String, String)> {
        // Native Anthropic auth — no redirection needed.
        Vec::new()
    }

    fn opencode_model(&self) -> Option<&'static str> {
        None
    }

    fn key_env_var(&self) -> Option<&'static str> {
        // Anthropic's API key variable. The subscription path (`claude`) does
        // not need it — see `needs_key_for_agent` — but agents that do (e.g.
        // `opencode`) authenticate with this key.
        Some(jackin_core::env_model::ANTHROPIC_API_KEY_ENV_NAME)
    }
}

/// `OpenAI` — Codex's native auth; no env redirection.
#[derive(Debug)]
pub struct OpenaiAdapter;
impl private::Sealed for OpenaiAdapter {}
impl ProviderAdapter for OpenaiAdapter {
    fn label(&self) -> &'static str {
        "OpenAI"
    }

    fn needs_key_for_agent(&self, agent_slug: &str) -> bool {
        // Codex supplies its own auth (auth.json sync or `OPENAI_API_KEY`).
        agent_slug != "codex"
    }

    fn supports_agent(&self, agent_slug: &str) -> bool {
        matches!(agent_slug, "codex")
    }

    fn env_overrides(&self, _token: Option<&str>) -> Vec<(String, String)> {
        // Native OpenAI auth — Codex routes via auth.json/config, not env.
        Vec::new()
    }

    fn opencode_model(&self) -> Option<&'static str> {
        None
    }

    fn key_env_var(&self) -> Option<&'static str> {
        Some(jackin_core::env_model::OPENAI_API_KEY_ENV_NAME)
    }
}

/// Z.AI — GLM Coding Plan via Anthropic-compatible endpoint.
#[derive(Debug)]
pub struct ZaiAdapter;
impl private::Sealed for ZaiAdapter {}
impl ProviderAdapter for ZaiAdapter {
    fn label(&self) -> &'static str {
        "Z.AI"
    }

    fn needs_key_for_agent(&self, _agent_slug: &str) -> bool {
        true
    }

    fn supports_agent(&self, agent_slug: &str) -> bool {
        // Codex blocked: Responses API not available via Z.AI yet.
        matches!(agent_slug, "claude" | "opencode")
    }

    fn env_overrides(&self, token: Option<&str>) -> Vec<(String, String)> {
        use crate::{
            ZAI_API_TIMEOUT_MS, ZAI_BASE_URL, ZAI_DEFAULT_HAIKU_MODEL, ZAI_DEFAULT_OPUS_MODEL,
            ZAI_DEFAULT_SONNET_MODEL,
        };
        anthropic_surface(
            ZAI_BASE_URL,
            ZAI_DEFAULT_OPUS_MODEL,
            ZAI_DEFAULT_SONNET_MODEL,
            ZAI_DEFAULT_HAIKU_MODEL,
            ZAI_API_TIMEOUT_MS,
            token,
        )
    }

    fn opencode_model(&self) -> Option<&'static str> {
        Some("zai/glm-5.1")
    }

    fn key_env_var(&self) -> Option<&'static str> {
        Some(jackin_core::env_model::ZAI_API_KEY_ENV_NAME)
    }
}

/// `MiniMax` Token Plan via Anthropic-compatible endpoint.
#[derive(Debug)]
pub struct MinimaxAdapter;
impl private::Sealed for MinimaxAdapter {}
impl ProviderAdapter for MinimaxAdapter {
    fn label(&self) -> &'static str {
        "MiniMax"
    }

    fn needs_key_for_agent(&self, _agent_slug: &str) -> bool {
        true
    }

    fn supports_agent(&self, agent_slug: &str) -> bool {
        // Codex Responses API works with MiniMax (OpenAI-compatible endpoint).
        matches!(agent_slug, "claude" | "opencode" | "codex")
    }

    fn env_overrides(&self, token: Option<&str>) -> Vec<(String, String)> {
        use crate::{MINIMAX_API_TIMEOUT_MS, MINIMAX_BASE_URL, MINIMAX_DEFAULT_MODEL};
        anthropic_surface(
            MINIMAX_BASE_URL,
            MINIMAX_DEFAULT_MODEL,
            MINIMAX_DEFAULT_MODEL,
            MINIMAX_DEFAULT_MODEL,
            MINIMAX_API_TIMEOUT_MS,
            token,
        )
    }

    fn opencode_model(&self) -> Option<&'static str> {
        Some("minimax/MiniMax-M3")
    }

    fn key_env_var(&self) -> Option<&'static str> {
        Some(jackin_core::env_model::MINIMAX_API_KEY_ENV_NAME)
    }

    fn codex_profile(&self) -> Option<&'static str> {
        Some("minimax")
    }
}

/// Kimi Code via Anthropic-compatible endpoint.
#[derive(Debug)]
pub struct KimiAdapter;
impl private::Sealed for KimiAdapter {}
impl ProviderAdapter for KimiAdapter {
    fn label(&self) -> &'static str {
        "Kimi"
    }

    fn needs_key_for_agent(&self, _agent_slug: &str) -> bool {
        true
    }

    fn supports_agent(&self, agent_slug: &str) -> bool {
        // Codex blocked: Chat-Completions-only; blocked until Responses API ships.
        matches!(agent_slug, "claude" | "opencode")
    }

    fn env_overrides(&self, token: Option<&str>) -> Vec<(String, String)> {
        use crate::{KIMI_API_TIMEOUT_MS, KIMI_BASE_URL, KIMI_DEFAULT_MODEL};
        anthropic_surface(
            KIMI_BASE_URL,
            KIMI_DEFAULT_MODEL,
            KIMI_DEFAULT_MODEL,
            KIMI_DEFAULT_MODEL,
            KIMI_API_TIMEOUT_MS,
            token,
        )
    }

    fn opencode_model(&self) -> Option<&'static str> {
        Some("kimi/kimi-for-coding")
    }

    fn key_env_var(&self) -> Option<&'static str> {
        Some(jackin_core::env_model::KIMI_CODE_API_KEY_ENV_NAME)
    }
}
