// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{
    CandidateAccumulator, CredentialSourceKey, UsageDiscoveryCatalog, UsageDiscoveryDiagnostic,
    UsageDiscoveryIssue, UsageDiscoveryScope, account_diagnostic, canonical_owner_for_account,
    canonical_usage_env_name, config_diagnostics, materialize_catalog, merge_env_candidate,
    provider_surface, resolve_profile_root,
};
use jackin_usage_host_credentials::{
    ForwardedUsageAccount, ProviderCredentialEnvOutcome, ProviderCredentialEnvResolver,
    UsageCredentialKind, governed_name_for_account_alias,
};
use std::collections::{BTreeMap, BTreeSet};

use jackin_config::{AccountCredential, AppConfig};
use jackin_core::{AuthForwardMode, JackinPaths, UsageCredentialEnvName, UsageCredentialOwner};
use jackin_usage_provider_core::dispatch_key_for_route;
use std::path::Path;

use jackin_usage_host_presentation::HostSurfaceId;

pub fn discover_usage_sources(
    scope: &UsageDiscoveryScope,
    env_resolver: &dyn ProviderCredentialEnvResolver,
) -> Result<UsageDiscoveryCatalog, String> {
    match scope {
        UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home,
        } => discover_host_sources(config_root, operator_home, env_resolver),
        UsageDiscoveryScope::Capsule { forwarded_accounts } => {
            Ok(discover_forwarded_sources(forwarded_accounts))
        }
    }
}

pub(crate) fn discover_host_sources(
    config_root: &Path,
    operator_home: &Path,
    env_resolver: &dyn ProviderCredentialEnvResolver,
) -> Result<UsageDiscoveryCatalog, String> {
    let paths = JackinPaths::resolve_with_env(operator_home, None, Some(config_root.as_os_str()));
    let snapshot = jackin_config::load_read_only_config_snapshot(&paths)
        .map_err(|_| "config snapshot unavailable".to_owned())?;
    let mut diagnostics = config_diagnostics(&snapshot);
    let mut candidates = BTreeMap::<CredentialSourceKey, CandidateAccumulator>::new();
    enumerate_registered_accounts(
        &snapshot.config,
        operator_home,
        env_resolver,
        &mut candidates,
        &mut diagnostics,
    );

    Ok(materialize_catalog(
        Some(snapshot.generation.as_str().to_owned()),
        candidates,
        diagnostics,
    ))
}

pub(crate) fn discover_forwarded_sources(
    accounts: &[ForwardedUsageAccount],
) -> UsageDiscoveryCatalog {
    let mut candidates = BTreeMap::<CredentialSourceKey, CandidateAccumulator>::new();
    for account in accounts {
        let Some(surface) = HostSurfaceId::from_id(&account.surface_id) else {
            continue;
        };
        if !HostSurfaceId::ALL.contains(&surface) {
            continue;
        }
        candidates
            .entry(CredentialSourceKey::Capability {
                surface,
                id: account.capability_id.clone(),
            })
            .or_insert_with(|| CandidateAccumulator {
                surface,
                kind: UsageCredentialKind::ForwardedCapability,
                provenance: BTreeSet::from(["forwarded to Capsule".to_owned()]),
                env_keys: BTreeSet::new(),
                account_label: account.account_label.clone(),
                operator_home: None,
            });
    }
    materialize_catalog(None, candidates, Vec::new())
}

pub(crate) fn usage_account_alias_entry(
    entry: UsageCredentialEnvName,
    canonical_owner: UsageCredentialOwner,
) -> UsageCredentialEnvName {
    let name = match entry.name {
        jackin_core::ANTHROPIC_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_ANTHROPIC_API_KEY",
        jackin_core::ANTHROPIC_AUTH_TOKEN_ENV_NAME => "JACKIN_USAGE_ACCOUNT_ANTHROPIC_AUTH_TOKEN",
        jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME => {
            "JACKIN_USAGE_ACCOUNT_CLAUDE_CODE_OAUTH_TOKEN"
        }
        jackin_core::OPENAI_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_OPENAI_API_KEY",
        jackin_core::AMP_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_AMP_API_KEY",
        jackin_core::KIMI_CODE_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_KIMI_CODE_API_KEY",
        jackin_core::KIMI_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_KIMI_API_KEY",
        jackin_core::MOONSHOT_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_MOONSHOT_API_KEY",
        jackin_core::XAI_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_XAI_API_KEY",
        jackin_core::GROK_DEPLOYMENT_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_GROK_DEPLOYMENT_KEY",
        jackin_core::ZAI_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_ZAI_API_KEY",
        jackin_core::ZHIPU_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_ZHIPU_API_KEY",
        "Z_AI_API_KEY" => "JACKIN_USAGE_ACCOUNT_Z_AI_API_KEY",
        jackin_core::MINIMAX_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_MINIMAX_API_KEY",
        jackin_core::OPENCODE_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_OPENCODE_API_KEY",
        jackin_core::GEMINI_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_GEMINI_API_KEY",
        jackin_core::GOOGLE_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_GOOGLE_API_KEY",
        jackin_core::CURSOR_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_CURSOR_API_KEY",
        jackin_core::META_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_META_API_KEY",
        jackin_core::OPENROUTER_API_KEY_ENV_NAME => "JACKIN_USAGE_ACCOUNT_OPENROUTER_API_KEY",
        _ => return entry,
    };
    UsageCredentialEnvName {
        name,
        owner: canonical_owner,
    }
}

pub(crate) fn enumerate_registered_accounts(
    config: &AppConfig,
    operator_home: &Path,
    resolver: &dyn ProviderCredentialEnvResolver,
    candidates: &mut BTreeMap<CredentialSourceKey, CandidateAccumulator>,
    diagnostics: &mut Vec<UsageDiscoveryDiagnostic>,
) {
    for (id, account) in &config.accounts {
        if !account.enabled {
            continue;
        }
        let surface = provider_surface(account.provider);
        let canonical_owner = canonical_owner_for_account(surface, account.provider);
        let provenance = registered_account_provenance(config, id);
        let label = if !account.name.trim().is_empty() {
            Some(account.name.trim().to_owned())
        } else if !id.trim().is_empty() {
            Some(id.trim().to_owned())
        } else {
            None
        };

        if let AccountCredential::Profile {
            agent, directory, ..
        } = &account.credential
        {
            let root = resolve_profile_root(operator_home, directory);
            candidates
                .entry(CredentialSourceKey::Profile {
                    agent: *agent,
                    root,
                })
                .and_modify(|candidate| {
                    candidate.provenance.extend(provenance.clone());
                    if candidate.account_label.is_none() {
                        candidate.account_label = label.clone();
                    }
                })
                .or_insert_with(|| CandidateAccumulator {
                    surface,
                    kind: UsageCredentialKind::Profile,
                    provenance,
                    env_keys: BTreeSet::new(),
                    account_label: label.clone(),
                    operator_home: Some(operator_home.to_path_buf()),
                });
            continue;
        }

        let (value, kind, expected_mode) = match &account.credential {
            AccountCredential::ApiKey { value, .. } => {
                (value, UsageCredentialKind::ApiKey, AuthForwardMode::ApiKey)
            }
            AccountCredential::OAuthToken { value, .. } => (
                value,
                UsageCredentialKind::OAuthToken,
                AuthForwardMode::OAuthToken,
            ),
            AccountCredential::Profile { .. } => continue,
        };
        let Ok(routes) = config.credential_descriptors_for_account(id) else {
            diagnostics.push(account_diagnostic(
                surface,
                id,
                None,
                u32::try_from(provenance.len()).unwrap_or(u32::MAX),
                UsageDiscoveryIssue::CredentialMalformed,
            ));
            continue;
        };
        for route in routes {
            if route.mode != expected_mode {
                diagnostics.push(account_diagnostic(
                    surface,
                    id,
                    Some(route.env_name),
                    u32::try_from(provenance.len()).unwrap_or(u32::MAX),
                    UsageDiscoveryIssue::CredentialMalformed,
                ));
                continue;
            }
            let Some(entry) = jackin_core::USAGE_CREDENTIAL_ENV_REGISTRY
                .iter()
                .copied()
                .find(|entry| entry.name == route.env_name)
            else {
                diagnostics.push(account_diagnostic(
                    surface,
                    id,
                    Some(route.env_name),
                    u32::try_from(provenance.len()).unwrap_or(u32::MAX),
                    UsageDiscoveryIssue::CredentialMalformed,
                ));
                continue;
            };
            let mut isolated = AppConfig {
                env: config.env.clone(),
                ..AppConfig::default()
            };
            let alias = usage_account_alias_entry(entry, canonical_owner);
            isolated.env.insert(alias.name.to_owned(), value.clone());
            let resolutions =
                resolver.resolve_provider_credentials(&isolated, None, None, &[alias]);
            let outcome = resolutions
                .into_iter()
                .find(|result| result.key == alias.name)
                .map_or(ProviderCredentialEnvOutcome::Missing, |result| {
                    result.outcome
                });
            let issue = match outcome {
                ProviderCredentialEnvOutcome::Resolved(handle) => {
                    candidates
                        .entry(CredentialSourceKey::Env {
                            surface,
                            handle,
                            key: canonical_usage_env_name(surface).to_owned(),
                            dispatch_key: dispatch_key_for_route(
                                canonical_owner,
                                governed_name_for_account_alias(entry.name),
                            )
                            .to_owned(),
                        })
                        .and_modify(|candidate| {
                            merge_env_candidate(
                                candidate,
                                &provenance,
                                entry.name,
                                label.as_deref(),
                            );
                        })
                        .or_insert_with(|| CandidateAccumulator {
                            surface,
                            kind,
                            provenance: provenance.clone(),
                            env_keys: BTreeSet::from([entry.name.to_owned()]),
                            account_label: label.clone(),
                            operator_home: None,
                        });
                    continue;
                }
                ProviderCredentialEnvOutcome::Missing => UsageDiscoveryIssue::CredentialMissing,
                ProviderCredentialEnvOutcome::Denied => UsageDiscoveryIssue::CredentialDenied,
                ProviderCredentialEnvOutcome::Malformed => UsageDiscoveryIssue::CredentialMalformed,
                ProviderCredentialEnvOutcome::InteractionRequired => {
                    UsageDiscoveryIssue::InteractionRequired
                }
            };
            diagnostics.push(account_diagnostic(
                surface,
                id,
                Some(entry.name),
                u32::try_from(provenance.len()).unwrap_or(u32::MAX),
                issue,
            ));
        }
    }
}

fn registered_account_provenance(config: &AppConfig, account_id: &str) -> BTreeSet<String> {
    let mut provenance = BTreeSet::from([format!("account {account_id}")]);
    for (workspace_name, workspace) in &config.workspaces {
        if workspace
            .accounts
            .iter()
            .any(|workspace_account_id| workspace_account_id.as_str() == account_id)
        {
            provenance.insert(format!("workspace {workspace_name}"));
        }
    }
    provenance
}
