//! Shared host CLI ↔ in-container Capsule contracts.
//!
//! Lives in its own crate so the host (`jackin`) and the
//! in-container binary (`jackin-capsule`) can both depend on it
//! without the host pulling in `jackin-capsule`'s tokio + PTY +
//! VT-parser stack. Most declarations here are wire-format types;
//! small constants that name the host↔Capsule runtime contract live
//! here too so the two binaries cannot drift.

pub mod control;

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
}

impl Provider {
    /// Every provider variant, in picker/display order.
    pub const ALL: [Provider; 2] = [Provider::Anthropic, Provider::Zai];

    /// Display label, also used as the tab suffix and the string carried
    /// on the wire in `InitialProvider` / `AgentWithProvider`.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Provider::Anthropic => "Anthropic",
            Provider::Zai => "Z.AI",
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

    /// Env overrides that redirect the agent to this provider. Anthropic
    /// needs none. Z.AI always sets the base URL and, when `token` is a
    /// non-empty value, the auth token; callers without the resolved key
    /// (the host launch path) pass `None` and the daemon backfills the
    /// token from the container's `ZAI_API_KEY`.
    #[must_use]
    pub fn env_overrides(self, token: Option<&str>) -> Vec<(String, String)> {
        match self {
            Provider::Anthropic => Vec::new(),
            Provider::Zai => {
                let mut env = Vec::with_capacity(2);
                if let Some(token) = token.filter(|value| !value.is_empty()) {
                    env.push(("ANTHROPIC_AUTH_TOKEN".to_string(), token.to_string()));
                }
                env.push(("ANTHROPIC_BASE_URL".to_string(), ZAI_BASE_URL.to_string()));
                env
            }
        }
    }

    /// Providers selectable for `agent_slug`. Z.AI is offered only for
    /// Claude (its endpoint is Anthropic-compatible) and only when a key
    /// is available; every other case has one implicit provider and
    /// returns an empty list, which callers read as "no picker step".
    #[must_use]
    pub fn available_for(agent_slug: &str, zai_key_available: bool) -> Vec<Provider> {
        if agent_slug == "claude" && zai_key_available {
            vec![Provider::Anthropic, Provider::Zai]
        } else {
            Vec::new()
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
            ]
        );
        // None and empty both mean "daemon backfills the token from env":
        // emit the base-url redirect but no token entry.
        for absent in [None, Some("")] {
            assert_eq!(
                Provider::Zai.env_overrides(absent),
                vec![("ANTHROPIC_BASE_URL".to_string(), ZAI_BASE_URL.to_string())]
            );
        }
    }

    #[test]
    fn available_for_offers_zai_only_to_claude_with_key() {
        assert_eq!(
            Provider::available_for("claude", true),
            vec![Provider::Anthropic, Provider::Zai]
        );
        assert!(Provider::available_for("claude", false).is_empty());
        assert!(Provider::available_for("codex", true).is_empty());
    }
}
