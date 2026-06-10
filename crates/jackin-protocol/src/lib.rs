//! Shared host CLI ↔ in-container Capsule contracts.
//!
//! Lives in its own crate so the host (`jackin`) and the
//! in-container binary (`jackin-capsule`) can both depend on it
//! without the host pulling in `jackin-capsule`'s tokio + PTY +
//! VT-parser stack. Most declarations here are wire-format types;
//! small constants that name the host↔Capsule runtime contract live
//! here too so the two binaries cannot drift.

pub mod control;
pub mod provider_adapter;

pub use provider_adapter::ProviderAdapter;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Filename written under `/jackin/run/` by the host launcher.
pub const CAPSULE_CONFIG_FILENAME: &str = "agent.toml";

/// Normalized runtime config path read by Capsule PID 1.
pub const CAPSULE_CONFIG_PATH: &str = "/jackin/run/agent.toml";

/// Host-validated role/session facts Capsule needs to spawn panes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapsuleConfig {
    pub role: String,
    pub workdir: String,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, String>,
    /// When the operator picked a specific provider in the console's
    /// launch flow (before the container existed), this field tells the
    /// capsule's initial spawn to use that provider and env overrides
    /// instead of defaulting to Anthropic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_provider: Option<InitialProvider>,
}

/// Provider selection for the capsule's initial session spawn. Carries
/// only the label; the daemon re-derives the env redirection from it (and
/// the container's `ZAI_API_KEY`) at spawn time, so there is a single
/// source of truth for the provider's overrides.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitialProvider {
    pub label: String,
}

/// Z.AI's Anthropic-compatible API base URL.
pub const ZAI_BASE_URL: &str = "https://api.z.ai/api/anthropic";
/// Z.AI's OpenAI-compatible API base URL (Codex / `OpenCode`).
pub const ZAI_OPENAI_BASE_URL: &str = "https://api.z.ai/api/coding/paas/v4";
/// Z.AI default model mapping: Opus tier → GLM-5.1.
pub const ZAI_DEFAULT_OPUS_MODEL: &str = "glm-5.1";
/// Z.AI default model mapping: Sonnet tier → GLM-5-Turbo.
pub const ZAI_DEFAULT_SONNET_MODEL: &str = "glm-5-turbo";
/// Z.AI default model mapping: Haiku tier → GLM-4.5-Air.
pub const ZAI_DEFAULT_HAIKU_MODEL: &str = "glm-4.5-air";
/// Z.AI recommended API timeout (50 minutes) for long-running agent operations through the proxy.
pub const ZAI_API_TIMEOUT_MS: &str = "3000000";

/// `MiniMax` Anthropic-compatible API base URL (Claude Code and `OpenCode`).
pub const MINIMAX_BASE_URL: &str = "https://api.minimax.io/anthropic";
/// `MiniMax` OpenAI-compatible API base URL (Codex Responses API).
pub const MINIMAX_OPENAI_BASE_URL: &str = "https://api.minimax.io/v1";
/// `MiniMax` Token Plan model — all three Claude tiers map to this single model.
pub const MINIMAX_DEFAULT_MODEL: &str = "MiniMax-M3";
/// `MiniMax-M3` context window (tokens). Codex ships no metadata for this custom
/// model, so jackin' registers it via a Codex model catalog; the value cannot be
/// raised through a profile-scoped `model_context_window` (Codex clamps that to
/// the model's fallback cap).
pub const MINIMAX_CONTEXT_WINDOW: u64 = 512_000;
/// `MiniMax` recommended API timeout, matching the Z.AI value.
pub const MINIMAX_API_TIMEOUT_MS: &str = "3000000";

/// Kimi Code Anthropic-compatible API base URL (Claude Code and `OpenCode`).
pub const KIMI_BASE_URL: &str = "https://api.kimi.com/coding";
/// Kimi Code model — all three Claude tiers map to this single model.
pub const KIMI_DEFAULT_MODEL: &str = "kimi-for-coding";
/// Kimi Code recommended API timeout, matching the Z.AI value.
pub const KIMI_API_TIMEOUT_MS: &str = "3000000";

/// API provider a Claude-compatible agent can be routed through. The
/// single source of truth for provider labels, endpoints, and env
/// redirection — the host console, the wire (`InitialProvider` /
/// `SpawnRequest::AgentWithProvider`), and the in-container daemon all
/// match on this enum so the provider catalog cannot drift across sites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Provider {
    /// The agent's own Anthropic auth — no env redirection.
    Anthropic,
    /// The agent's own `OpenAI` auth — no env redirection. Native to Codex.
    Openai,
    /// Z.AI (GLM Coding Plan) via its Anthropic-compatible endpoint.
    Zai,
    /// `MiniMax` Token Plan via its Anthropic-compatible endpoint.
    Minimax,
    /// Kimi Code via its Anthropic-compatible endpoint.
    /// Distinct from the `kimi` agent runtime — this is the provider backend.
    Kimi,
}

impl Provider {
    /// Every provider variant, in picker/display order. Native providers
    /// (Anthropic for `claude`, `OpenAI` for `codex`) lead the catalog.
    pub const ALL: [Provider; 5] = [
        Provider::Anthropic,
        Provider::Openai,
        Provider::Zai,
        Provider::Minimax,
        Provider::Kimi,
    ];

    /// The adapter for this provider. Single dispatch point for all
    /// provider-specific behavior — adding a new provider requires one
    /// adapter struct + one match arm here + one variant in `ALL`, not N
    /// scattered match arms.
    #[must_use]
    pub fn adapter(self) -> &'static dyn ProviderAdapter {
        use provider_adapter::{
            AnthropicAdapter, KimiAdapter, MinimaxAdapter, OpenaiAdapter, ZaiAdapter,
        };
        match self {
            Self::Anthropic => &AnthropicAdapter,
            Self::Openai => &OpenaiAdapter,
            Self::Zai => &ZaiAdapter,
            Self::Minimax => &MinimaxAdapter,
            Self::Kimi => &KimiAdapter,
        }
    }

    /// Display label, also used as the tab suffix and the string carried
    /// on the wire in `InitialProvider` / `AgentWithProvider`.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.adapter().label()
    }

    /// Inverse of [`Provider::label`], derived from the same labels so the
    /// two cannot drift. `None` for an unrecognized label (a stale or
    /// hostile peer naming a provider this build does not know).
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.label() == label)
    }

    /// Env overrides that redirect Claude Code to this provider via the
    /// Anthropic-compatible surface. Anthropic needs none. Each alt provider
    /// sets the base URL, auth token (when present), model-tier mapping vars,
    /// a generous API timeout, and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`.
    /// Codex and `OpenCode` route via config files generated at runtime-setup,
    /// not via this method.
    #[must_use]
    pub fn env_overrides(self, token: Option<&str>) -> Vec<(String, String)> {
        self.adapter().env_overrides(token)
    }

    /// Codex v2 profile name for this provider, or `None` if no profile is
    /// needed (native `OpenAI` auth or provider unsupported for Codex).
    #[must_use]
    pub fn codex_profile(self) -> Option<&'static str> {
        self.adapter().codex_profile()
    }

    /// Providers selectable for `(agent_slug, has_key)`. Returns an empty
    /// list when no picker is needed (the agent's native auth is the
    /// implicit choice).
    ///
    /// `has_key(p)` returns `true` when the operator has configured a key for
    /// provider `p`. Each adapter's `needs_key_for_agent` + `supports_agent`
    /// determine membership — no closed match required to add a new provider.
    /// A non-native sole option (e.g. only `Zai` for `opencode`) is still
    /// returned so the caller can auto-route through it without a picker.
    #[must_use]
    pub fn available_for(agent_slug: &str, has_key: impl Fn(Provider) -> bool) -> Vec<Provider> {
        let providers: Vec<Provider> = Self::ALL
            .iter()
            .filter(|&&p| {
                let a = p.adapter();
                a.supports_agent(agent_slug) && (!a.needs_key_for_agent(agent_slug) || has_key(p))
            })
            .copied()
            .collect();
        match providers.as_slice() {
            [] | [Provider::Anthropic | Provider::Openai] => Vec::new(),
            _ => providers,
        }
    }

    /// Model string in `provider/model` format for `OpenCode`'s `-m` flag.
    /// `None` for Anthropic (use `OpenCode`'s own default selection).
    #[must_use]
    pub fn opencode_model(self) -> Option<&'static str> {
        self.adapter().opencode_model()
    }

    /// Env var that holds the API key for this provider, if any.
    ///
    /// Convenience wrapper around `self.adapter().key_env_var()` so callers
    /// do not need to import the `ProviderAdapter` trait.
    #[must_use]
    pub fn key_env_var(self) -> Option<&'static str> {
        self.adapter().key_env_var()
    }
}

impl CapsuleConfig {
    pub fn supported_agents(&self) -> Vec<String> {
        self.agents.clone()
    }

    pub fn model_for_agent(&self, agent: &str) -> Option<&str> {
        self.models.get(agent).map(String::as_str)
    }
}

#[cfg(test)]
mod provider_tests {
    use super::*;

    #[test]
    fn label_round_trips_through_from_label() {
        for provider in Provider::ALL {
            assert_eq!(Provider::from_label(provider.label()), Some(provider));
        }
        assert_eq!(Provider::from_label("Gemini"), None);
    }

    #[test]
    fn anthropic_needs_no_env_overrides() {
        assert!(Provider::Anthropic.env_overrides(Some("tok")).is_empty());
    }

    #[test]
    fn zai_injects_token_only_when_present() {
        assert_eq!(
            Provider::Zai.env_overrides(Some("tok")),
            vec![
                ("ANTHROPIC_AUTH_TOKEN".to_owned(), "tok".to_owned()),
                ("ANTHROPIC_BASE_URL".to_owned(), ZAI_BASE_URL.to_owned()),
                (
                    "ANTHROPIC_DEFAULT_OPUS_MODEL".to_owned(),
                    ZAI_DEFAULT_OPUS_MODEL.to_owned()
                ),
                (
                    "ANTHROPIC_DEFAULT_SONNET_MODEL".to_owned(),
                    ZAI_DEFAULT_SONNET_MODEL.to_owned()
                ),
                (
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_owned(),
                    ZAI_DEFAULT_HAIKU_MODEL.to_owned()
                ),
                ("API_TIMEOUT_MS".to_owned(), ZAI_API_TIMEOUT_MS.to_owned()),
                (
                    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_owned(),
                    "1".to_owned()
                ),
            ]
        );
        // None and empty both mean "daemon backfills the token from env":
        // emit the base-url redirect and model mapping but no token entry.
        for absent in [None, Some("")] {
            assert_eq!(
                Provider::Zai.env_overrides(absent),
                vec![
                    ("ANTHROPIC_BASE_URL".to_owned(), ZAI_BASE_URL.to_owned()),
                    (
                        "ANTHROPIC_DEFAULT_OPUS_MODEL".to_owned(),
                        ZAI_DEFAULT_OPUS_MODEL.to_owned()
                    ),
                    (
                        "ANTHROPIC_DEFAULT_SONNET_MODEL".to_owned(),
                        ZAI_DEFAULT_SONNET_MODEL.to_owned()
                    ),
                    (
                        "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_owned(),
                        ZAI_DEFAULT_HAIKU_MODEL.to_owned()
                    ),
                    ("API_TIMEOUT_MS".to_owned(), ZAI_API_TIMEOUT_MS.to_owned()),
                    (
                        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_owned(),
                        "1".to_owned()
                    ),
                ]
            );
        }
    }

    #[test]
    fn available_for_provider_matrix() {
        // Claude: Anthropic always included (subscription auth, no key needed).
        assert_eq!(
            Provider::available_for("claude", |p| matches!(p, Provider::Zai)),
            vec![Provider::Anthropic, Provider::Zai]
        );
        assert_eq!(
            Provider::available_for("claude", |p| matches!(p, Provider::Minimax)),
            vec![Provider::Anthropic, Provider::Minimax]
        );
        assert_eq!(
            Provider::available_for("claude", |p| matches!(p, Provider::Kimi)),
            vec![Provider::Anthropic, Provider::Kimi]
        );
        assert_eq!(
            Provider::available_for("claude", |p| {
                matches!(p, Provider::Zai | Provider::Minimax | Provider::Kimi)
            }),
            vec![
                Provider::Anthropic,
                Provider::Zai,
                Provider::Minimax,
                Provider::Kimi
            ]
        );
        // No alt providers → no picker (Anthropic alone = native sole → empty).
        assert!(Provider::available_for("claude", |_| false).is_empty());

        // Codex: OpenAI always included (native). Only MiniMax supports it today
        // (GLM/Kimi deferred), and Zai/Kimi are filtered out by `supports_agent`.
        assert_eq!(
            Provider::available_for("codex", |p| matches!(p, Provider::Minimax)),
            vec![Provider::Openai, Provider::Minimax]
        );
        assert!(Provider::available_for("codex", |_| false).is_empty());
        assert!(Provider::available_for("codex", |p| matches!(p, Provider::Zai)).is_empty());
        assert!(Provider::available_for("codex", |p| matches!(p, Provider::Kimi)).is_empty());

        // OpenCode: Anthropic only when anthropic_api_key is set (subscription not available).
        assert_eq!(
            Provider::available_for("opencode", |p| {
                matches!(p, Provider::Anthropic | Provider::Zai | Provider::Minimax)
            }),
            vec![Provider::Anthropic, Provider::Zai, Provider::Minimax]
        );
        assert_eq!(
            Provider::available_for("opencode", |p| {
                matches!(p, Provider::Zai | Provider::Minimax)
            }),
            vec![Provider::Zai, Provider::Minimax]
        );
        assert!(Provider::available_for("opencode", |_| false).is_empty());
        // Only ANTHROPIC_API_KEY, no alts → sole entry is native Anthropic → no picker.
        assert!(
            Provider::available_for("opencode", |p| matches!(p, Provider::Anthropic)).is_empty()
        );
        // A single alt provider survives so the caller auto-routes through it.
        assert_eq!(
            Provider::available_for("opencode", |p| matches!(p, Provider::Zai)),
            vec![Provider::Zai]
        );
        assert_eq!(
            Provider::available_for("opencode", |p| matches!(p, Provider::Kimi)),
            vec![Provider::Kimi]
        );

        // Unknown agent (amp): always empty — no adapters support it.
        assert!(Provider::available_for("amp", |_| true).is_empty());
    }

    #[test]
    fn codex_profile_is_some_only_for_minimax() {
        assert_eq!(Provider::Minimax.codex_profile(), Some("minimax"));
        for p in [
            Provider::Anthropic,
            Provider::Openai,
            Provider::Zai,
            Provider::Kimi,
        ] {
            assert_eq!(
                p.codex_profile(),
                None,
                "{p:?} must not declare a Codex profile"
            );
        }
    }

    #[test]
    fn minimax_env_overrides_map_all_tiers_to_same_model() {
        let env = Provider::Minimax.env_overrides(Some("mk"));
        assert!(
            env.iter()
                .any(|(k, v)| k == "ANTHROPIC_BASE_URL" && v == MINIMAX_BASE_URL)
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "ANTHROPIC_DEFAULT_OPUS_MODEL" && v == MINIMAX_DEFAULT_MODEL)
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "ANTHROPIC_DEFAULT_SONNET_MODEL" && v == MINIMAX_DEFAULT_MODEL)
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "ANTHROPIC_DEFAULT_HAIKU_MODEL" && v == MINIMAX_DEFAULT_MODEL)
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "ANTHROPIC_AUTH_TOKEN" && v == "mk")
        );
    }

    #[test]
    fn kimi_env_overrides_map_all_tiers_to_same_model() {
        let env = Provider::Kimi.env_overrides(Some("kk"));
        assert!(
            env.iter()
                .any(|(k, v)| k == "ANTHROPIC_BASE_URL" && v == KIMI_BASE_URL)
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "ANTHROPIC_DEFAULT_OPUS_MODEL" && v == KIMI_DEFAULT_MODEL)
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "ANTHROPIC_DEFAULT_SONNET_MODEL" && v == KIMI_DEFAULT_MODEL)
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "ANTHROPIC_DEFAULT_HAIKU_MODEL" && v == KIMI_DEFAULT_MODEL)
        );
        assert!(
            env.iter()
                .any(|(k, v)| k == "ANTHROPIC_AUTH_TOKEN" && v == "kk")
        );
    }
}
