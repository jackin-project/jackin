//! `ProviderAdapter` trait: per-provider behavioral dispatch.
//!
//! Each of the four built-in providers (Anthropic, Zai, MiniMax, Kimi)
//! implements this trait in a zero-sized adapter struct. `Provider::adapter()`
//! returns the matching adapter as `&'static dyn ProviderAdapter`.
//!
//! Design:
//! - Trait is **sealed** so external crates cannot add providers. The four
//!   built-in adapters are the only implementors.
//! - Zero-sized unit structs: no allocation, no per-call overhead.
//! - Adding a new provider requires exactly one registration here plus one
//!   variant in the `Provider` enum — no N closed matches scattered elsewhere.
//!
//! Sibling to `jackin_core::agent::runtime::AgentRuntime`.

/// Sealing module — prevents external crates from implementing `ProviderAdapter`.
pub(crate) mod private {
    pub trait Sealed {}
}

/// Behavioral contract each provider adapter satisfies.
///
/// Reach via `provider.adapter().<method>()`. Sealed: only the four built-in
/// adapters in this module implement it.
pub trait ProviderAdapter: Send + Sync + 'static + private::Sealed {
    /// Display label (also the wire identifier in `InitialProvider`).
    fn label(&self) -> &'static str;

    /// Whether this provider requires an explicit API key for `agent_slug`.
    ///
    /// - Anthropic: `false` for `claude` (subscription auth); `true` for all
    ///   others where the subscription does not extend (e.g. `opencode`).
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

    /// Model string in `provider/model` format for OpenCode's `-m` flag.
    /// `None` for Anthropic (use OpenCode's own default selection).
    fn opencode_model(&self) -> Option<&'static str>;
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
        env.push(("ANTHROPIC_AUTH_TOKEN".to_string(), t.to_string()));
    }
    env.push(("ANTHROPIC_BASE_URL".to_string(), base_url.to_string()));
    env.push(("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), opus.to_string()));
    env.push((
        "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
        sonnet.to_string(),
    ));
    env.push((
        "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
        haiku.to_string(),
    ));
    env.push(("API_TIMEOUT_MS".to_string(), timeout.to_string()));
    env.push((
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(),
        "1".to_string(),
    ));
    env
}

// ── Adapter structs ───────────────────────────────────────────────────────────

/// Anthropic — the agent's native auth surface; no env redirection.
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
}

/// Z.AI — GLM Coding Plan via Anthropic-compatible endpoint.
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
}

/// MiniMax Token Plan via Anthropic-compatible endpoint.
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
}

/// Kimi Code via Anthropic-compatible endpoint.
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
}
