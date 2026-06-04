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
/// Z.AI's OpenAI-compatible API base URL (Codex / OpenCode).
pub const ZAI_OPENAI_BASE_URL: &str = "https://api.z.ai/api/coding/paas/v4";
/// Z.AI default model mapping: Opus tier → GLM-5.1.
pub const ZAI_DEFAULT_OPUS_MODEL: &str = "glm-5.1";
/// Z.AI default model mapping: Sonnet tier → GLM-5-Turbo.
pub const ZAI_DEFAULT_SONNET_MODEL: &str = "glm-5-turbo";
/// Z.AI default model mapping: Haiku tier → GLM-4.5-Air.
pub const ZAI_DEFAULT_HAIKU_MODEL: &str = "glm-4.5-air";
/// Z.AI recommended API timeout (50 minutes) for long-running agent operations through the proxy.
pub const ZAI_API_TIMEOUT_MS: &str = "3000000";

/// MiniMax Anthropic-compatible API base URL (Claude Code and OpenCode).
pub const MINIMAX_BASE_URL: &str = "https://api.minimax.io/anthropic";
/// MiniMax OpenAI-compatible API base URL (Codex Responses API).
pub const MINIMAX_OPENAI_BASE_URL: &str = "https://api.minimax.io/v1";
/// MiniMax Token Plan model — all three Claude tiers map to this single model.
pub const MINIMAX_DEFAULT_MODEL: &str = "MiniMax-M3";
/// MiniMax recommended API timeout, matching the Z.AI value.
pub const MINIMAX_API_TIMEOUT_MS: &str = "3000000";

/// Kimi Code Anthropic-compatible API base URL (Claude Code and OpenCode).
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// The agent's own Anthropic auth — no env redirection.
    Anthropic,
    /// Z.AI (GLM Coding Plan) via its Anthropic-compatible endpoint.
    Zai,
    /// MiniMax Token Plan via its Anthropic-compatible endpoint.
    Minimax,
    /// Kimi Code via its Anthropic-compatible endpoint.
    /// Distinct from the `kimi` agent runtime — this is the provider backend.
    Kimi,
}

impl Provider {
    /// Every provider variant, in picker/display order.
    pub const ALL: [Provider; 4] = [
        Provider::Anthropic,
        Provider::Zai,
        Provider::Minimax,
        Provider::Kimi,
    ];

    /// Display label, also used as the tab suffix and the string carried
    /// on the wire in `InitialProvider` / `AgentWithProvider`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic",
            Provider::Zai => "Z.AI",
            Provider::Minimax => "MiniMax",
            Provider::Kimi => "Kimi",
        }
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
    /// Codex and OpenCode route via config files generated at runtime-setup,
    /// not via this method.
    #[must_use]
    pub fn env_overrides(self, token: Option<&str>) -> Vec<(String, String)> {
        fn anthropic_surface(
            base_url: &str,
            opus: &str,
            sonnet: &str,
            haiku: &str,
            timeout: &str,
            token: Option<&str>,
        ) -> Vec<(String, String)> {
            let mut env = Vec::with_capacity(7);
            if let Some(token) = token.filter(|value| !value.is_empty()) {
                // Open question for Kimi: endpoint may honor ANTHROPIC_API_KEY
                // instead of ANTHROPIC_AUTH_TOKEN. Verify against the pinned
                // Kimi endpoint and update if needed.
                env.push(("ANTHROPIC_AUTH_TOKEN".to_string(), token.to_string()));
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
        match self {
            Provider::Anthropic => Vec::new(),
            Provider::Zai => anthropic_surface(
                ZAI_BASE_URL,
                ZAI_DEFAULT_OPUS_MODEL,
                ZAI_DEFAULT_SONNET_MODEL,
                ZAI_DEFAULT_HAIKU_MODEL,
                ZAI_API_TIMEOUT_MS,
                token,
            ),
            Provider::Minimax => anthropic_surface(
                MINIMAX_BASE_URL,
                MINIMAX_DEFAULT_MODEL,
                MINIMAX_DEFAULT_MODEL,
                MINIMAX_DEFAULT_MODEL,
                MINIMAX_API_TIMEOUT_MS,
                token,
            ),
            Provider::Kimi => anthropic_surface(
                KIMI_BASE_URL,
                KIMI_DEFAULT_MODEL,
                KIMI_DEFAULT_MODEL,
                KIMI_DEFAULT_MODEL,
                KIMI_API_TIMEOUT_MS,
                token,
            ),
        }
    }

    /// Providers selectable for `(agent_slug, present keys)`. Returns an
    /// empty list when no picker is needed (single implicit provider).
    ///
    /// - `claude`: Anthropic is always included — the subscription works
    ///   without an explicit API key. Alt providers added when their key is
    ///   present. Picker shown whenever at least one alt provider is available.
    /// - `opencode`: Anthropic included only when `anthropic_api_key` is
    ///   true — the subscription does not extend to OpenCode. Alt providers
    ///   added when their key is present.
    /// - `codex`: MiniMax only (GLM and Kimi expose Chat Completions only,
    ///   blocked until upstream ships a Responses-compatible endpoint).
    #[must_use]
    pub fn available_for(
        agent_slug: &str,
        anthropic_api_key: bool,
        zai_key: bool,
        minimax_key: bool,
        kimi_key: bool,
    ) -> Vec<Provider> {
        let mut providers = vec![];
        match agent_slug {
            "claude" => {
                // Subscription auth — Anthropic always available, no key needed.
                providers.push(Provider::Anthropic);
                if zai_key {
                    providers.push(Provider::Zai);
                }
                if minimax_key {
                    providers.push(Provider::Minimax);
                }
                if kimi_key {
                    providers.push(Provider::Kimi);
                }
            }
            "opencode" => {
                // Subscription does not extend to OpenCode — require explicit key.
                if anthropic_api_key {
                    providers.push(Provider::Anthropic);
                }
                if zai_key {
                    providers.push(Provider::Zai);
                }
                if minimax_key {
                    providers.push(Provider::Minimax);
                }
                if kimi_key {
                    providers.push(Provider::Kimi);
                }
            }
            // GLM and Kimi deferred: Chat-Completions-only, blocked on Codex Responses API.
            "codex" if minimax_key => {
                providers.push(Provider::Minimax);
            }
            _ => {}
        }
        // Collapse to "no choice" only when the sole option is the agent's
        // native Anthropic auth, which needs no redirect. A single alt
        // provider must survive so the caller routes the session through it
        // (a one-option picker is pointless, but the routing still has to
        // happen — dropping it would silently ignore a configured key).
        match providers.as_slice() {
            [] | [Provider::Anthropic] => Vec::new(),
            _ => providers,
        }
    }

    /// Model string in `provider/model` format for OpenCode's `-m` flag.
    /// `None` for Anthropic (use OpenCode's own default model selection).
    #[must_use]
    pub fn opencode_model(self) -> Option<&'static str> {
        match self {
            Provider::Anthropic => None,
            Provider::Zai => Some("zai/glm-5.1"),
            Provider::Minimax => Some("minimax/MiniMax-M3"),
            Provider::Kimi => Some("kimi/kimi-for-coding"),
        }
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

/// Container-name grammar shared by the host launcher and the
/// in-container capsule. The launcher constructs names of the shape
/// `jk-<id>[-<workspace>]-<role>`; both binaries must agree on how
/// to parse them.
pub const CONTAINER_PREFIX: &str = "jk";

/// Prefix with the trailing separator, used by [`instance_id_from_container_base`]
/// to strip the family marker before splitting.
pub const CONTAINER_PREFIX_DASH: &str = "jk-";

/// Extract the instance-ID component from a container base name.
///
/// Returns `None` when the name does not start with `jk-` or has no
/// `-` after the id component. Single parser shared by host
/// manifest construction (`JACKIN_INSTANCE_ID` injection) and the
/// capsule's status bar so the two surfaces cannot drift on what a
/// `jk-…` name means.
#[must_use]
pub fn instance_id_from_container_base(container_base: &str) -> Option<&str> {
    container_base
        .strip_prefix(CONTAINER_PREFIX_DASH)?
        .split_once('-')
        .map(|(id, _)| id)
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
                ("ANTHROPIC_AUTH_TOKEN".to_string(), "tok".to_string()),
                ("ANTHROPIC_BASE_URL".to_string(), ZAI_BASE_URL.to_string()),
                (
                    "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
                    ZAI_DEFAULT_OPUS_MODEL.to_string()
                ),
                (
                    "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
                    ZAI_DEFAULT_SONNET_MODEL.to_string()
                ),
                (
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
                    ZAI_DEFAULT_HAIKU_MODEL.to_string()
                ),
                ("API_TIMEOUT_MS".to_string(), ZAI_API_TIMEOUT_MS.to_string()),
                (
                    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(),
                    "1".to_string()
                ),
            ]
        );
        // None and empty both mean "daemon backfills the token from env":
        // emit the base-url redirect and model mapping but no token entry.
        for absent in [None, Some("")] {
            assert_eq!(
                Provider::Zai.env_overrides(absent),
                vec![
                    ("ANTHROPIC_BASE_URL".to_string(), ZAI_BASE_URL.to_string()),
                    (
                        "ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(),
                        ZAI_DEFAULT_OPUS_MODEL.to_string()
                    ),
                    (
                        "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
                        ZAI_DEFAULT_SONNET_MODEL.to_string()
                    ),
                    (
                        "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
                        ZAI_DEFAULT_HAIKU_MODEL.to_string()
                    ),
                    ("API_TIMEOUT_MS".to_string(), ZAI_API_TIMEOUT_MS.to_string()),
                    (
                        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(),
                        "1".to_string()
                    ),
                ]
            );
        }
    }

    #[test]
    fn available_for_provider_matrix() {
        // Claude: Anthropic always included (subscription auth, no key needed).
        assert_eq!(
            Provider::available_for("claude", false, true, false, false),
            vec![Provider::Anthropic, Provider::Zai]
        );
        assert_eq!(
            Provider::available_for("claude", false, false, true, false),
            vec![Provider::Anthropic, Provider::Minimax]
        );
        assert_eq!(
            Provider::available_for("claude", false, false, false, true),
            vec![Provider::Anthropic, Provider::Kimi]
        );
        assert_eq!(
            Provider::available_for("claude", false, true, true, true),
            vec![
                Provider::Anthropic,
                Provider::Zai,
                Provider::Minimax,
                Provider::Kimi
            ]
        );
        // No alt providers → no picker (Anthropic alone = single entry → empty).
        assert!(Provider::available_for("claude", false, false, false, false).is_empty());

        // Codex: MiniMax only (GLM/Kimi deferred). anthropic_api_key unused for codex.
        assert_eq!(
            Provider::available_for("codex", false, false, true, false),
            vec![Provider::Minimax]
        );
        assert!(Provider::available_for("codex", false, true, false, false).is_empty());
        assert!(Provider::available_for("codex", false, false, false, true).is_empty());

        // OpenCode: Anthropic only when ANTHROPIC_API_KEY is set (subscription not available).
        assert_eq!(
            Provider::available_for("opencode", true, true, true, false),
            vec![Provider::Anthropic, Provider::Zai, Provider::Minimax]
        );
        assert_eq!(
            Provider::available_for("opencode", false, true, true, false),
            vec![Provider::Zai, Provider::Minimax]
        );
        assert!(Provider::available_for("opencode", false, false, false, false).is_empty());
        // Only ANTHROPIC_API_KEY, no alts → sole entry is native Anthropic → no picker.
        assert!(Provider::available_for("opencode", true, false, false, false).is_empty());
        // A single alt provider survives so the caller auto-routes through it
        // (no picker, but the configured key must not be silently ignored).
        assert_eq!(
            Provider::available_for("opencode", false, true, false, false),
            vec![Provider::Zai]
        );
        assert_eq!(
            Provider::available_for("opencode", false, false, false, true),
            vec![Provider::Kimi]
        );

        // Unknown agent: always empty.
        assert!(Provider::available_for("amp", true, true, true, true).is_empty());
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
