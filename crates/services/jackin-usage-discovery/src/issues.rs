// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_host_credentials::UsageCredentialKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageDiscoveryIssue {
    ConfigUnreadable,

    ConfigInvalid,

    ConfigVersionUnsupported,

    ConfigTransientConflict,

    CredentialMissing,

    CredentialDenied,

    KeychainConsentRequired,

    CredentialMalformed,

    InteractionRequired,
}

impl UsageDiscoveryIssue {
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

#[derive(Clone, PartialEq, Eq)]
pub struct UsageDiscoveryDiagnostic {
    pub surface_id: Option<String>,

    pub scope_label: String,

    pub unresolved_source: Option<UsageDiscoveryUnresolvedSource>,

    pub issue: UsageDiscoveryIssue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageDiscoveryUnresolvedSource {
    pub capability_id: String,

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageSourceCandidateDescriptor {
    pub surface_id: String,

    pub credential_kind: UsageCredentialKind,

    pub source_id: String,

    pub capability_id: String,

    pub provenance: Vec<String>,
}
