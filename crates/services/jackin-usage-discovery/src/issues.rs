// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Discovery diagnostics and candidates.

use jackin_usage_host_credentials::UsageCredentialKind;

/// Sanitized source-level failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageDiscoveryIssue {
    /// Config source was unreadable.
    ConfigUnreadable,
    /// Config source was malformed or invalid.
    ConfigInvalid,
    /// Config schema is newer than supported.
    ConfigVersionUnsupported,
    /// Config changed repeatedly during discovery.
    ConfigTransientConflict,
    /// Required credential source is absent.
    CredentialMissing,
    /// Protected credential access was denied/unavailable.
    CredentialDenied,
    /// A Keychain item exists but the operator has not approved access.
    KeychainConsentRequired,
    /// Credential source is malformed.
    CredentialMalformed,
    /// Credential source requires explicit interaction.
    InteractionRequired,
}

impl UsageDiscoveryIssue {
    /// Stable machine-readable identifier exported through sanitized adapters.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::ConfigUnreadable => "config_unreadable",
            Self::ConfigInvalid => "config_invalid",
            Self::ConfigVersionUnsupported => "config_version_unsupported",
            Self::ConfigTransientConflict => "config_transient_conflict",
            Self::CredentialMissing => "credential_missing",
            Self::CredentialDenied => "credential_denied",
            Self::KeychainConsentRequired => "keychain_consent_required",
            Self::CredentialMalformed => "credential_malformed",
            Self::InteractionRequired => "interaction_required",
        }
    }

    /// Rust-owned operator copy. It deliberately contains no source location.
    #[must_use]
    pub const fn display_message(self) -> &'static str {
        match self {
            Self::ConfigUnreadable => "Configuration could not be read",
            Self::ConfigInvalid => "Configuration is invalid",
            Self::ConfigVersionUnsupported => "Configuration version is not supported",
            Self::ConfigTransientConflict => "Configuration changed while it was being read",
            Self::CredentialMissing => "Credentials are missing",
            Self::CredentialDenied => "Credential access was denied",
            Self::KeychainConsentRequired => {
                "Keychain consent required; approve jackin in Keychain Access"
            }
            Self::CredentialMalformed => "Credentials are malformed",
            Self::InteractionRequired => "Credential access requires interaction",
        }
    }
}

/// Sanitized provider/scope diagnostic. No path, secret, or 1Password coordinate.
#[derive(Clone, PartialEq, Eq)]
pub struct UsageDiscoveryDiagnostic {
    /// Provider surface when the failure is provider-specific.
    pub surface_id: Option<String>,
    /// Rust-composed scope label (`account …`, `workspace …`).
    pub scope_label: String,
    /// Opaque configured source that failed before it could become a binding.
    /// This is a domain-separated hash, never an account alias, path, or secret.
    pub unresolved_source: Option<UsageDiscoveryUnresolvedSource>,
    /// Stable machine-readable category.
    pub issue: UsageDiscoveryIssue,
}

/// Secret-free identity for one configured source that could not be validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageDiscoveryUnresolvedSource {
    /// Stable opaque source identifier derived from non-secret config identity.
    pub capability_id: String,
    /// Number of current config observations contributing this source.
    pub configuration_count: u32,
}

impl std::fmt::Debug for UsageDiscoveryDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UsageDiscoveryDiagnostic")
            .field("surface_id", &self.surface_id)
            .field("scope_label", &"[REDACTED]")
            .field("unresolved_source", &self.unresolved_source)
            .field("issue", &self.issue)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_scope_labels() {
        let diagnostic = UsageDiscoveryDiagnostic {
            surface_id: Some("claude".to_owned()),
            scope_label: "/private/config/private-account".to_owned(),
            unresolved_source: Some(UsageDiscoveryUnresolvedSource {
                capability_id: "opaque-source-id".to_owned(),
                configuration_count: 1,
            }),
            issue: UsageDiscoveryIssue::InteractionRequired,
        };
        let debug = format!("{diagnostic:?}");

        assert!(debug.contains("opaque-source-id"));
        assert!(!debug.contains("/private/config/private-account"));
    }
}

/// Sanitized candidate source descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSourceCandidateDescriptor {
    /// Provider surface id.
    pub surface_id: String,
    /// Credential form.
    pub credential_kind: UsageCredentialKind,
    /// Opaque process-local source identifier.
    pub source_id: String,
    /// Stable opaque capability identity; never a source ordinal or credential hash.
    pub capability_id: String,
    /// Every config scope that resolved to this source.
    pub provenance: Vec<String>,
}
