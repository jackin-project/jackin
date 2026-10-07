// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Forwarded credential identity types.

use jackin_config::AppConfig;
use jackin_core::{UsageCredentialEnvName, WorkspaceName};
use jackin_protocol::control::FocusedUsageView;
use jackin_protocol::usage_broker::UsageCredentialSourceIdentity;

use jackin_usage_host_presentation::HostSurfaceId;

/// One account capability explicitly forwarded into a Capsule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardedUsageAccount {
    /// Stable provider surface id.
    pub surface_id: String,
    /// Opaque broker-issued capability id; never credential material.
    pub capability_id: String,
    /// Authenticated display label when already known.
    pub account_label: Option<String>,
}

/// Opaque process-local handle for a resolved environment credential.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OpaqueCredentialHandle(pub String);

impl OpaqueCredentialHandle {
    /// Construct from a non-secret adapter-owned identifier.
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

/// Secret-free outcome for one governed provider environment key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCredentialEnvOutcome {
    /// Protected value resolved and is retained behind this adapter handle.
    Resolved(OpaqueCredentialHandle),
    /// An explicitly required host value is missing.
    Missing,
    /// Protected source denied access or was unavailable.
    Denied,
    /// Configured source was structurally malformed.
    Malformed,
    /// Source requires an explicit operator action before retry.
    InteractionRequired,
}

/// One governed provider key result from a tier-4 env adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialEnvResolution {
    /// Exact requested key name.
    pub key: String,
    /// Secret-free outcome.
    pub outcome: ProviderCredentialEnvOutcome,
}

/// Secret-free source identity and material fingerprint retained by one
/// validated discovery binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentialSourceMaterial {
    /// Exact declaration identity observed by the resolver.
    pub source: UsageCredentialSourceIdentity,
    /// Fingerprint of the resolved material held behind the opaque handle.
    pub material_fingerprint: String,
}

/// Port from usage discovery to tier-4 env/1Password composition.
pub trait ProviderCredentialEnvResolver: Send + Sync {
    /// Begin one explicit manual retry action.
    ///
    /// Adapters may evict only prior non-success outcomes here. Background
    /// refresh never calls this method.
    fn begin_manual_retry(&self) {}

    /// Resolve only `keys` for the exact effective config scope.
    ///
    /// Absent declarations are omitted. Implementations retain resolved secret
    /// values internally and return opaque handles only.
    fn resolve_provider_credentials(
        &self,
        config: &AppConfig,
        workspace: Option<&WorkspaceName>,
        role: Option<&str>,
        keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution>;

    /// Resolve authenticated identity for an already resolved opaque handle.
    ///
    /// The default is anonymous because most API-key providers reveal identity
    /// only in the quota response. Implementations must never expose the key.
    fn identify_provider_credential(
        &self,
        _surface: HostSurfaceId,
        _handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialIdentityOutcome {
        ProviderCredentialIdentityOutcome::Anonymous
    }

    /// Probe quota for one already-resolved credential without exposing it.
    ///
    /// The adapter owns secret access; provider snapshot construction remains
    /// in `jackin-usage`. Background refresh may call this only for successful
    /// handles retained by the completed discovery generation.
    fn refresh_provider_credential(
        &self,
        _surface: HostSurfaceId,
        _key: &str,
        _handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialRefreshOutcome {
        ProviderCredentialRefreshOutcome::Malformed
    }

    /// Return the exact non-secret source material bound to one opaque handle.
    /// Implementations that cannot prove this must return `None`; the launch
    /// boundary then rejects scoped environment credentials.
    fn source_material(
        &self,
        _surface: HostSurfaceId,
        _key: &str,
        _handle: &OpaqueCredentialHandle,
    ) -> Option<ProviderCredentialSourceMaterial> {
        None
    }
}

/// Authenticated identity result for an opaque provider credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCredentialIdentityOutcome {
    /// Provider authenticated the source. Stable id wins over label for dedup.
    Authenticated {
        /// Provider-issued stable account/organization id when available.
        provider_id: Option<String>,
        /// Authenticated user-facing account label when available.
        account_label: Option<String>,
    },
    /// Source is usable but identity is unavailable until a quota request.
    Anonymous,
    /// Credential disappeared after enumeration.
    Missing,
    /// Protected source access was denied.
    Denied,
    /// Credential payload is malformed.
    Malformed,
}

/// Secret-free quota result returned by a protected-source adapter.
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderCredentialRefreshOutcome {
    /// Provider snapshot, including authenticated identity when supplied.
    Snapshot {
        /// Provider view with secret-free account/quota data.
        view: Box<FocusedUsageView>,
        /// Typed provider rate-limit metadata, when the provider returned HTTP 429.
        rate_limit: Option<jackin_usage_provider_core::ProviderRateLimit>,
    },
    /// Credential disappeared after discovery.
    Missing,
    /// Protected credential access is no longer authorized.
    Denied,
    /// Credential or provider response was malformed.
    Malformed,
    /// Source requires an explicit operator action before retry.
    InteractionRequired,
}

/// Credential form discovered before authenticated identity resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UsageCredentialKind {
    /// Agent-owned credential/config profile root.
    Profile,
    /// Provider API key.
    ApiKey,
    /// Provider OAuth token supplied through env.
    OAuthToken,
    /// Broker-issued Capsule capability.
    ForwardedCapability,
}

/// Recover the governed registry name for one discovery-isolated alias.
///
/// Unknown names pass through unchanged so direct governed-name callers keep
/// their existing cache identity.
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
