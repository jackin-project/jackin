// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Provider ownership and catalog materialization.

use crate::{
    CandidateAccumulator, CredentialSourceKey, DiscoveredCredentialSource, UsageDiscoveryCatalog,
    UsageDiscoveryDiagnostic, UsageDiscoveryIssue, UsageSourceCandidateDescriptor,
};
use std::collections::BTreeMap;

use jackin_config::{AiProvider, ConfigSourceIssue, ReadOnlyConfigSnapshot};
use jackin_core::UsageCredentialOwner;
use std::path::{Component, Path, PathBuf};

use jackin_usage_host_presentation::HostSurfaceId;

pub(crate) fn provider_surface(provider: AiProvider) -> HostSurfaceId {
    match provider {
        AiProvider::Anthropic => HostSurfaceId::Claude,
        AiProvider::OpenAi => HostSurfaceId::Codex,
        AiProvider::Amp => HostSurfaceId::Amp,
        AiProvider::Xai => HostSurfaceId::Grok,
        AiProvider::Opencode => HostSurfaceId::OpenCode,
        AiProvider::Moonshot => HostSurfaceId::Kimi,
        AiProvider::Zai => HostSurfaceId::Zai,
        AiProvider::Minimax => HostSurfaceId::Minimax,
        AiProvider::Google => HostSurfaceId::Google,
        AiProvider::Cursor => HostSurfaceId::Cursor,
        AiProvider::Meta => HostSurfaceId::Meta,
        AiProvider::OpenRouter => HostSurfaceId::OpenRouter,
    }
}

pub(crate) fn canonical_owner_for_account(
    surface: HostSurfaceId,
    provider: AiProvider,
) -> UsageCredentialOwner {
    let owner = match provider {
        AiProvider::Anthropic => UsageCredentialOwner::Claude,
        AiProvider::OpenAi => UsageCredentialOwner::Codex,
        AiProvider::Amp => UsageCredentialOwner::Amp,
        AiProvider::Moonshot => UsageCredentialOwner::Kimi,
        AiProvider::Xai => UsageCredentialOwner::Grok,
        AiProvider::Zai => UsageCredentialOwner::Zai,
        AiProvider::Minimax => UsageCredentialOwner::Minimax,
        AiProvider::Opencode => UsageCredentialOwner::OpenCode,
        AiProvider::Google => UsageCredentialOwner::Google,
        AiProvider::Cursor => UsageCredentialOwner::Cursor,
        AiProvider::Meta => UsageCredentialOwner::Meta,
        AiProvider::OpenRouter => UsageCredentialOwner::OpenRouter,
    };
    debug_assert_eq!(provider_surface(provider), surface);
    owner
}

/// Canonical provider usage key. Launch routes may use provider-compatible
/// aliases, but cache/refresh/capability identity always uses this key.
pub(crate) fn canonical_usage_env_name(surface: HostSurfaceId) -> &'static str {
    match surface {
        HostSurfaceId::Claude => jackin_core::ANTHROPIC_API_KEY_ENV_NAME,
        HostSurfaceId::Codex => jackin_core::OPENAI_API_KEY_ENV_NAME,
        HostSurfaceId::Amp => jackin_core::AMP_API_KEY_ENV_NAME,
        HostSurfaceId::Kimi => jackin_core::KIMI_CODE_API_KEY_ENV_NAME,
        HostSurfaceId::Grok => jackin_core::XAI_API_KEY_ENV_NAME,
        HostSurfaceId::Zai => jackin_core::ZAI_API_KEY_ENV_NAME,
        HostSurfaceId::Minimax => jackin_core::MINIMAX_API_KEY_ENV_NAME,
        HostSurfaceId::OpenCode => jackin_core::OPENCODE_API_KEY_ENV_NAME,
        HostSurfaceId::Google => jackin_core::GEMINI_API_KEY_ENV_NAME,
        HostSurfaceId::Cursor => jackin_core::CURSOR_API_KEY_ENV_NAME,
        HostSurfaceId::Meta => jackin_core::META_API_KEY_ENV_NAME,
        HostSurfaceId::OpenRouter => jackin_core::OPENROUTER_API_KEY_ENV_NAME,
    }
}

pub(crate) fn resolve_profile_root(operator_home: &Path, configured: &Path) -> PathBuf {
    let mut components = configured.components();
    match components.next() {
        Some(Component::Normal(first)) if first == "~" => operator_home.join(components),
        Some(_) if configured.is_absolute() => configured.to_path_buf(),
        _ => operator_home.join(configured),
    }
}

pub(crate) fn account_diagnostic(
    surface: HostSurfaceId,
    account_id: &str,
    source_key: Option<&str>,
    configuration_count: u32,
    issue: UsageDiscoveryIssue,
) -> UsageDiscoveryDiagnostic {
    let source_key = source_key.unwrap_or("account-credential");
    let material = format!(
        "surface:{}:account:{}:source:{}",
        surface.id(),
        account_id,
        source_key
    );
    let capability_id =
        jackin_core::account_key_hash("usage-discovery-unresolved-source-v1", &material);
    UsageDiscoveryDiagnostic {
        surface_id: Some(surface.id().to_owned()),
        // Do not retain the declared account alias in an operator diagnostic.
        scope_label: "account".to_owned(),
        unresolved_source: Some(crate::UsageDiscoveryUnresolvedSource {
            capability_id: capability_id
                .strip_prefix("sha256:")
                .unwrap_or(&capability_id)
                .to_owned(),
            configuration_count,
        }),
        issue,
    }
}

pub(crate) fn config_diagnostics(
    snapshot: &ReadOnlyConfigSnapshot,
) -> Vec<UsageDiscoveryDiagnostic> {
    snapshot
        .diagnostics
        .iter()
        .map(|diagnostic| UsageDiscoveryDiagnostic {
            surface_id: None,
            scope_label: match &diagnostic.scope {
                jackin_config::ConfigSourceScope::Global => "global config".to_owned(),
                jackin_config::ConfigSourceScope::Workspaces => "workspace configs".to_owned(),
                jackin_config::ConfigSourceScope::Workspace(name) => {
                    format!("workspace {name}")
                }
            },
            unresolved_source: None,
            issue: match diagnostic.issue {
                ConfigSourceIssue::Unreadable => UsageDiscoveryIssue::ConfigUnreadable,
                ConfigSourceIssue::UnsupportedVersion => {
                    UsageDiscoveryIssue::ConfigVersionUnsupported
                }
                ConfigSourceIssue::TransientConflict => {
                    UsageDiscoveryIssue::ConfigTransientConflict
                }
                ConfigSourceIssue::Malformed
                | ConfigSourceIssue::Invalid
                | ConfigSourceIssue::InvalidWorkspaceName
                | ConfigSourceIssue::ConflictingWorkspaceDefinitions => {
                    UsageDiscoveryIssue::ConfigInvalid
                }
            },
        })
        .collect()
}

pub(crate) fn materialize_catalog(
    config_generation: Option<String>,
    candidates: BTreeMap<CredentialSourceKey, CandidateAccumulator>,
    diagnostics: Vec<UsageDiscoveryDiagnostic>,
) -> UsageDiscoveryCatalog {
    let mut descriptors = Vec::with_capacity(candidates.len());
    let mut sources = Vec::with_capacity(candidates.len());
    for (index, (key, candidate)) in candidates.into_iter().enumerate() {
        let source_id = format!("source-{:04}", index + 1);
        let capability_id = source_capability_id(candidate.surface, &key);
        let provenance = candidate.provenance.iter().cloned().collect::<Vec<_>>();
        descriptors.push(UsageSourceCandidateDescriptor {
            surface_id: candidate.surface.id().to_owned(),
            credential_kind: candidate.kind,
            source_id: source_id.clone(),
            capability_id: capability_id.clone(),
            provenance,
        });
        let source = match key {
            CredentialSourceKey::Profile { agent, root } => DiscoveredCredentialSource::Profile {
                surface: candidate.surface,
                agent,
                root,
                operator_home: candidate.operator_home.unwrap_or_default(),
                account_label: candidate.account_label,
                source_id,
                capability_id,
                provenance: candidate.provenance,
            },
            CredentialSourceKey::Env {
                surface,
                handle,
                key,
                dispatch_key,
            } => DiscoveredCredentialSource::Env {
                surface,
                handle,
                key,
                dispatch_key,
                launch_keys: candidate.env_keys,
                kind: candidate.kind,
                account_label: candidate.account_label,
                source_id,
                capability_id,
                provenance: candidate.provenance,
            },
            CredentialSourceKey::Capability { surface, id } => {
                DiscoveredCredentialSource::Capability {
                    surface,
                    account_label: candidate.account_label,
                    source_id,
                    capability_id: id,
                }
            }
        };
        sources.push(source);
    }
    UsageDiscoveryCatalog {
        config_generation,
        candidates: descriptors,
        diagnostics,
        sources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_diagnostic_keeps_only_opaque_declared_source_identity() {
        let account_id = "private-account-alias";
        let route_key = "ANTHROPIC_API_KEY";
        let diagnostic = account_diagnostic(
            HostSurfaceId::Claude,
            account_id,
            Some(route_key),
            2,
            UsageDiscoveryIssue::InteractionRequired,
        );
        let debug = format!("{diagnostic:?}");
        let source = diagnostic.unresolved_source.as_ref().unwrap();

        assert_eq!(diagnostic.scope_label, "account");
        assert_eq!(source.configuration_count, 2);
        assert!(!source.capability_id.contains(account_id));
        assert!(!source.capability_id.contains(route_key));
        assert!(!debug.contains(account_id));
        assert!(!debug.contains(route_key));
    }
}

pub(crate) fn source_capability_id(surface: HostSurfaceId, key: &CredentialSourceKey) -> String {
    if let CredentialSourceKey::Capability { id, .. } = key {
        return id.clone();
    }
    let evidence = match key {
        CredentialSourceKey::Profile { agent, root } => {
            format!("profile-v1:{}:{}", agent.slug(), root.to_string_lossy())
        }
        CredentialSourceKey::Env {
            surface,
            handle,
            key,
            dispatch_key,
        } => {
            pub(crate) fn segment(value: &str) -> String {
                format!("{}:{value}", value.len())
            }
            format!(
                "env-v3:{}:{}:{}:{}",
                surface.id(),
                segment(key),
                segment(dispatch_key),
                segment(&handle.0)
            )
        }
        CredentialSourceKey::Capability { .. } => unreachable!("returned above"),
    };
    let hashed = jackin_core::account_key_hash(surface.id(), &evidence);
    hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned()
}
