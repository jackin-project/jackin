// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::AppConfig;
use jackin_core::{UsageCredentialEnvName, WorkspaceName};
use jackin_protocol::control::FocusedUsageView;
use jackin_protocol::usage_broker::UsageCredentialSourceIdentity;
use jackin_usage_provider_core::ProviderFailureMetadata;

use jackin_usage_host_presentation::HostSurfaceId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardedUsageAccount {
    pub surface_id: String,
    pub capability_id: String,
    pub account_label: Option<String>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpaqueCredentialHandle(pub String);

impl OpaqueCredentialHandle {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Debug for OpaqueCredentialHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OpaqueCredentialHandle(REDACTED)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCredentialEnvOutcome {
    Resolved(OpaqueCredentialHandle),
    Missing,
    Denied,
    Malformed,
    InteractionRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialEnvResolution {
    pub key: String,
    pub outcome: ProviderCredentialEnvOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialSourceMaterial {
    pub source: UsageCredentialSourceIdentity,
    pub material_fingerprint: String,
}

pub trait ProviderCredentialEnvResolver: Send + Sync {
    fn begin_manual_retry(&self) {}

    fn resolve_provider_credentials(
        &self,
        config: &AppConfig,
        workspace: Option<&WorkspaceName>,
        role: Option<&str>,
        keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution>;

    fn identify_provider_credential(
        &self,
        _surface: HostSurfaceId,
        _handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialIdentityOutcome {
        ProviderCredentialIdentityOutcome::Anonymous
    }

    fn refresh_provider_credential(
        &self,
        _surface: HostSurfaceId,
        _key: &str,
        _handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialRefreshOutcome {
        ProviderCredentialRefreshOutcome::Malformed
    }

    fn source_material(
        &self,
        _surface: HostSurfaceId,
        _key: &str,
        _handle: &OpaqueCredentialHandle,
    ) -> Option<ProviderCredentialSourceMaterial> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCredentialIdentityOutcome {
    Authenticated {
        provider_id: Option<String>,
        account_label: Option<String>,
    },
    Anonymous,
    Missing,
    Denied,
    Malformed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderCredentialRefreshOutcome {
    Snapshot {
        view: Box<FocusedUsageView>,
        rate_limit: Option<jackin_usage_provider_core::ProviderRateLimit>,
        provider_error: Option<ProviderFailureMetadata>,
    },
    Missing,
    Denied,
    Malformed,
    InteractionRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UsageCredentialKind {
    Profile,
    ApiKey,
    OAuthToken,
    ForwardedCapability,
}

pub fn governed_name_for_account_alias(name: &str) -> &str {
    match name {
        "JACKIN_USAGE_ACCOUNT_ANTHROPIC_API_KEY" => jackin_core::ANTHROPIC_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_ANTHROPIC_AUTH_TOKEN" => jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_CLAUDE_CODE_OAUTH_TOKEN" => {
            jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME
        }
        "JACKIN_USAGE_ACCOUNT_OPENAI_API_KEY" => jackin_core::OPENAI_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_AMP_API_KEY" => jackin_core::AMP_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_KIMI_CODE_API_KEY" => jackin_core::KIMI_CODE_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_KIMI_API_KEY" => jackin_core::KIMI_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_MOONSHOT_API_KEY" => jackin_core::MOONSHOT_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_XAI_API_KEY" => jackin_core::XAI_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_GROK_DEPLOYMENT_KEY" => jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_ZAI_API_KEY" => jackin_core::ZAI_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_ZHIPU_API_KEY" => jackin_core::ZHIPU_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_Z_AI_API_KEY" => "Z_AI_API_KEY",
        "JACKIN_USAGE_ACCOUNT_MINIMAX_API_KEY" => jackin_core::MINIMAX_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_OPENCODE_API_KEY" => jackin_core::OPENCODE_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_GEMINI_API_KEY" => jackin_core::GEMINI_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_GOOGLE_API_KEY" => jackin_core::GOOGLE_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_CURSOR_API_KEY" => jackin_core::CURSOR_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_META_API_KEY" => jackin_core::META_API_KEY_ENV_NAME,
        "JACKIN_USAGE_ACCOUNT_OPENROUTER_API_KEY" => jackin_core::OPENROUTER_API_KEY_ENV_NAME,
        _ => name,
    }
}
