// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-owned read authority, separate from credentials admitted to agents.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result};
use jackin_config::{ConfigSourceScope, ReadOnlyConfigSnapshot, load_read_only_config_snapshot};
use jackin_core::JackinPaths;
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageFreshnessPhaseV2, UsageFreshnessV2, UsageIssueRecoverabilityV2, UsageIssueScopeV2,
    UsageIssueV2, UsageLifecycleV2, UsageProjectionRefreshStateV2, UsageProjectionV2,
    UsageUnresolvedGrantV2,
};
use jackin_usage::host::{
    HostSurfaceId, UsageBrokerClient, UsageDiscoveryIssue, ValidatedUsageDiscovery,
    usage_capability_for_selected_account, usage_projection_account_for_capability,
    usage_projection_source_ids_for_capability,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InventoryAccountAuthority {
    pub(super) surface_id: String,
    pub(super) canonical_account_id: String,
    pub(super) capability: UsageAccountCapability,
    pub(super) provenance_count: u32,
}

/// Immutable inventory fence captured from a stable host configuration.
#[derive(Debug, Clone)]
pub(super) struct RelayUsageInventory {
    paths: JackinPaths,
    config_generation: String,
    capabilities: BTreeSet<UsageAccountCapability>,
    workspace: Option<String>,
    selected_account_ids: BTreeSet<String>,
    projection_accounts: BTreeMap<(String, String), UsageAccountCapability>,
    provenance_counts: BTreeMap<(String, String), u32>,
    unresolved_grants: Vec<UsageUnresolvedGrantV2>,
}

impl RelayUsageInventory {
    pub(super) fn unresolved_grants(&self) -> &[UsageUnresolvedGrantV2] {
        &self.unresolved_grants
    }

    pub(super) fn authority(&self) -> Vec<InventoryAccountAuthority> {
        self.projection_accounts
            .iter()
            .map(
                |((surface_id, canonical_account_id), capability)| InventoryAccountAuthority {
                    surface_id: surface_id.clone(),
                    canonical_account_id: canonical_account_id.clone(),
                    capability: capability.clone(),
                    provenance_count: self
                        .provenance_counts
                        .get(&(surface_id.clone(), canonical_account_id.clone()))
                        .copied()
                        .unwrap_or(0),
                },
            )
            .collect()
    }

    pub(super) fn restore(
        paths: &JackinPaths,
        workspace: Option<&str>,
        selected_account_ids: &BTreeSet<String>,
        config_generation: &str,
        authorities: &[InventoryAccountAuthority],
        unresolved_grants: &[UsageUnresolvedGrantV2],
    ) -> Result<Self> {
        let mut capabilities = BTreeSet::new();
        let mut projection_accounts = BTreeMap::new();
        let mut provenance_counts = BTreeMap::new();
        for authority in authorities {
            anyhow::ensure!(
                HostSurfaceId::from_id(&authority.surface_id).is_some()
                    && authority.surface_id == authority.capability.surface_id
                    && !authority.canonical_account_id.is_empty()
                    && !authority.capability.account_id.is_empty()
                    && authority.provenance_count > 0,
                "invalid saved usage inventory authority"
            );
            let key = (
                authority.surface_id.clone(),
                authority.canonical_account_id.clone(),
            );
            anyhow::ensure!(
                projection_accounts
                    .insert(key.clone(), authority.capability.clone())
                    .is_none(),
                "duplicate saved usage inventory authority"
            );
            provenance_counts.insert(key, authority.provenance_count);
            capabilities.insert(authority.capability.clone());
        }
        validate_saved_grants(paths, workspace, selected_account_ids, unresolved_grants)?;
        let inventory = Self {
            unresolved_grants: unresolved_grants.to_vec(),
            paths: paths.clone(),
            config_generation: config_generation.to_owned(),
            capabilities,
            workspace: workspace.map(str::to_owned),
            selected_account_ids: selected_account_ids.clone(),
            projection_accounts,
            provenance_counts,
        };
        inventory
            .validate_current_config()
            .map_err(|_| anyhow::anyhow!("usage inventory authority was revoked"))?;
        Ok(inventory)
    }

    pub(super) fn config_generation(&self) -> &str {
        &self.config_generation
    }

    pub(super) fn prepare(
        paths: &JackinPaths,
        workspace: Option<&str>,
        selected_account_ids: &BTreeSet<String>,
        discovery: &ValidatedUsageDiscovery,
    ) -> Result<Self> {
        let snapshot = load_read_only_config_snapshot(paths)
            .context("reading usage inventory configuration")?;
        validate_diagnostics(&snapshot, workspace)?;
        let config_generation = snapshot.generation.as_str().to_owned();
        anyhow::ensure!(
            discovery.config_generation.as_deref() == Some(config_generation.as_str()),
            "usage inventory configuration changed during discovery"
        );
        let accounts = match workspace {
            Some(name) => snapshot
                .config
                .workspaces
                .get(name)
                .with_context(|| format!("usage inventory workspace {name:?} is unavailable"))?
                .accounts
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            None => selected_account_ids.clone(),
        };
        let mut capabilities = BTreeSet::new();
        let mut projection_accounts = BTreeMap::new();
        let authorized_configured_ids = accounts.clone();
        let mut admitted_source_ids = BTreeMap::<(String, String), BTreeSet<String>>::new();
        let mut unresolved_grants = Vec::new();
        for account_id in accounts {
            anyhow::ensure!(
                !account_id.trim().is_empty() && !account_id.chars().any(char::is_control),
                "usage inventory configured account reference is invalid"
            );
            let account = snapshot.config.accounts.get(&account_id).with_context(|| {
                format!("usage inventory account {account_id:?} is unavailable")
            })?;
            if !account.enabled {
                continue;
            }
            let Some(surface) = HostSurfaceId::from_provider_alias(account.provider.slug()) else {
                continue;
            };
            let mut issues = discovery
                .diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.surface_id.as_deref() == Some(surface.id())
                        && diagnostic.configured_account_ids.contains(&account_id)
                })
                .map(|diagnostic| grant_issue(diagnostic.issue))
                .collect::<Vec<_>>();
            issues.sort_by(|left, right| left.code.cmp(&right.code));
            issues.dedup();
            let capability =
                usage_capability_for_selected_account(discovery, &account_id, surface.id());
            let canonical_id = capability.as_ref().and_then(|capability| {
                usage_projection_account_for_capability(discovery, capability)
            });
            if !issues.is_empty() || canonical_id.is_none() {
                unresolved_grants.push(UsageUnresolvedGrantV2 {
                    configured_account_id: account_id.clone(),
                    surface_id: surface.id().to_owned(),
                    issues: if issues.is_empty() {
                        vec![unresolved_grant_issue()]
                    } else {
                        issues
                    },
                });
            }
            let Some(capability) = capability else {
                // Diagnostics grant no identity, source, or refresh authority.
                continue;
            };
            capabilities.insert(capability.clone());
            if let Some(canonical_id) = canonical_id {
                let key = (surface.id().to_owned(), canonical_id);
                admitted_source_ids.entry(key.clone()).or_default().extend(
                    usage_projection_source_ids_for_capability(
                        discovery,
                        &capability,
                        &authorized_configured_ids,
                    ),
                );
                projection_accounts.insert(key, capability);
            }
        }
        let provenance_counts = admitted_source_ids
            .into_iter()
            .map(|(key, source_ids)| {
                (
                    key,
                    u32::try_from(source_ids.len()).expect("admitted source count fits u32"),
                )
            })
            .collect();
        Ok(Self {
            unresolved_grants,
            paths: paths.clone(),
            config_generation,
            capabilities,
            workspace: workspace.map(str::to_owned),
            selected_account_ids: selected_account_ids.clone(),
            projection_accounts,
            provenance_counts,
        })
    }

    /// Read one publication with cancellable broker IPC and charged file validation.
    pub(super) async fn read_async(
        self,
        broker: &UsageBrokerClient,
        permit: Option<tokio::sync::OwnedSemaphorePermit>,
        deadline: Option<tokio::time::Instant>,
    ) -> std::result::Result<UsageProjectionV2, UsageCoordinationError> {
        let (inventory, permit) = self.validate_admitted(permit, deadline).await?;
        let projection = broker
            .current_projection_async(deadline.unwrap_or_else(|| {
                tokio::time::Instant::now() + std::time::Duration::from_secs(30)
            }))
            .await?;
        projection.validate().map_err(|_| UsageCoordinationError {
            kind: UsageCoordinationErrorKind::CorruptState,
            message: "usage inventory publication failed validation".to_owned(),
        })?;
        let (inventory, _permit) = inventory.validate_admitted(permit, deadline).await?;
        Ok(inventory.filter_projection(projection))
    }

    async fn validate_admitted(
        self,
        permit: Option<tokio::sync::OwnedSemaphorePermit>,
        deadline: Option<tokio::time::Instant>,
    ) -> std::result::Result<
        (Self, Option<tokio::sync::OwnedSemaphorePermit>),
        UsageCoordinationError,
    > {
        let unavailable = || UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message: "usage inventory request expired or was cancelled".to_owned(),
        };
        if deadline.is_some_and(|deadline| tokio::time::Instant::now() >= deadline) {
            return Err(unavailable());
        }
        let validation = jackin_telemetry::spawn::joined_blocking(move || {
            // The admission lease stays in the actual filesystem work even
            // if its asynchronous owner disappears before completion.
            let result = self.validate_current_config();
            (self, permit, result)
        });
        let (inventory, permit, result) = match deadline {
            Some(deadline) => tokio::time::timeout_at(deadline, validation)
                .await
                .map_err(|_| unavailable())?
                .map_err(|_| unavailable())?,
            None => validation.await.map_err(|_| unavailable())?,
        };
        result?;
        Ok((inventory, permit))
    }

    fn validate_current_config(&self) -> std::result::Result<(), UsageCoordinationError> {
        let admitted = load_read_only_config_snapshot(&self.paths).is_ok_and(|snapshot| {
            snapshot.generation.as_str() == self.config_generation
                && validate_diagnostics(&snapshot, self.workspace.as_deref()).is_ok()
                && self
                    .workspace
                    .as_ref()
                    .is_none_or(|name| snapshot.config.workspaces.contains_key(name))
                && (self.workspace.is_some()
                    || self
                        .selected_account_ids
                        .iter()
                        .all(|id| snapshot.config.accounts.contains_key(id)))
        });
        if admitted {
            Ok(())
        } else {
            Err(UsageCoordinationError {
                kind: UsageCoordinationErrorKind::Unauthorized,
                message: "usage inventory configuration authority was revoked".to_owned(),
            })
        }
    }

    pub(super) fn projection_matches_current_issuer(
        &self,
        projection: &UsageProjectionV2,
        issuer: &UsageProjectionV2,
    ) -> bool {
        if jackin_protocol::control::UsageAccountMembershipV1::validate_current_projection(issuer).is_err() {
            return false;
        }
        let current = self.filter_projection(issuer.clone());
        self.projection_is_complete_and_scoped(&current) && current == *projection
    }

    pub(super) fn projection_is_complete_and_scoped(&self, projection: &UsageProjectionV2) -> bool {
        let expected = self.projection_accounts.keys().filter_map(|(surface_id, canonical_id)| {
            HostSurfaceId::from_id(surface_id)
                .map(|surface| (surface.provider_id().to_owned(), canonical_id.clone()))
        }).collect::<BTreeSet<_>>();
        let actual = projection.providers.iter().flat_map(|provider| {
            provider.accounts.iter().map(|account| {
                (provider.provider_id.clone(), account.canonical_account_id.clone())
            })
        }).collect::<BTreeSet<_>>();
        expected == actual && self.filter_projection(projection.clone()) == *projection
    }

    pub(super) fn filter_projection(&self, mut projection: UsageProjectionV2) -> UsageProjectionV2 {
        projection.providers.retain_mut(|provider| {
            provider.accounts.retain(|account| {
                self.projection_accounts
                    .iter()
                    .any(|((surface_id, canonical_id), capability)| {
                        canonical_id == &account.canonical_account_id
                            && self.capabilities.contains(capability)
                            && HostSurfaceId::from_id(surface_id).is_some_and(|surface| {
                                surface.provider_id() == provider.provider_id
                            })
                    })
            });
            for (rank, account) in provider.accounts.iter_mut().enumerate() {
                account.rank = u32::try_from(rank).expect("broker account rank fits u32");
                account
                    .refresh_capabilities
                    .retain(|capability| self.capabilities.contains(capability));
                account
                    .issues
                    .retain(|issue| issue.scope == UsageIssueScopeV2::Account);
                if account.refresh_capabilities.is_empty() {
                    // Identity membership cannot authorize quota from an older
                    // issuing catalog's route, even for the same principal.
                    account.lifecycle = UsageLifecycleV2::Unavailable;
                    account.freshness.phase = UsageFreshnessPhaseV2::Failed;
                    account.freshness.last_good_at_epoch = None;
                    account.freshness.retry_at_epoch = None;
                    account.freshness.is_stale = false;
                    account.windows.clear();
                    account.metric_groups.clear();
                    account.username = None;
                    account.auth_origin = None;
                    account.plan_label = None;
                    account.status_label = None;
                    account.credential_expires_at_epoch = None;
                    account.issues = vec![UsageIssueV2 {
                        code: "scoped_usage_route_unavailable".to_owned(),
                        scope: UsageIssueScopeV2::Account,
                        recoverability: UsageIssueRecoverabilityV2::ActionRequired,
                        message: "Current usage publication has no authorized account route."
                            .to_owned(),
                        retry_at_epoch: None,
                    }];
                }
                account.provenance_count = self
                    .provenance_counts
                    .iter()
                    .filter(|((surface_id, canonical_id), _)| {
                        canonical_id == &account.canonical_account_id
                            && HostSurfaceId::from_id(surface_id).is_some_and(|surface| {
                                surface.provider_id() == provider.provider_id
                            })
                    })
                    .map(|(_, count)| *count)
                    .fold(0_u32, u32::saturating_add);
                for group in &mut account.metric_groups {
                    group
                        .issues
                        .retain(|issue| issue.scope == UsageIssueScopeV2::Group);
                }
            }
            provider.issues.clear();
            // Provider freshness originally aggregates accounts outside this fence.
            provider.freshness =
                scoped_provider_freshness(&provider.accounts, projection.broker_generation);
            !provider.accounts.is_empty()
        });
        for (rank, provider) in projection.providers.iter_mut().enumerate() {
            provider.rank = u32::try_from(rank).expect("broker provider rank fits u32");
        }
        // Unresolved source ids and global issues have no exact account ownership.
        projection.unresolved.clear();
        projection.unresolved_grants = self.unresolved_grants.clone();
        projection.issues.clear();
        projection.refresh_state = if projection.providers.iter().any(|provider| {
            provider
                .accounts
                .iter()
                .any(|account| account.freshness.phase == UsageFreshnessPhaseV2::Refreshing)
        }) {
            UsageProjectionRefreshStateV2::Refreshing
        } else {
            UsageProjectionRefreshStateV2::Idle
        };
        projection
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jackin_config::{ConfigSourceDiagnostic, ConfigSourceIssue};
    use jackin_protocol::usage_broker::{
        UsageAccountV2, UsageIdentityKindV2, UsageIssueRecoverabilityV2, UsageIssueV2,
        UsageLifecycleV2, UsageMembershipStateV2, UsageProjectionSchemaV2, UsageProviderV2,
        UsageUnresolvedV2,
    };

    fn granted_accounts_fixture(
        root: &std::path::Path,
    ) -> Result<(
        JackinPaths,
        jackin_config::AppConfig,
        ValidatedUsageDiscovery,
    )> {
        use jackin_config::{AccountConfig, AccountCredential, AiProvider, WorkspaceConfig};
        let paths = JackinPaths::for_tests(root);
        std::fs::create_dir_all(&paths.config_dir)?;
        std::fs::create_dir_all(&paths.home_dir)?;
        let mut config = jackin_config::AppConfig::default();
        for id in ["personal", "work", "outside"] {
            let profile = root.join(format!("profile-{id}"));
            std::fs::create_dir_all(&profile)?;
            std::fs::write(
                profile.join("auth.json"),
                format!(
                    r#"{{"tokens":{{"access_token":"fixture-{id}","account_id":"provider-{id}"}}}}"#
                ),
            )?;
            config.accounts.insert(
                id.to_owned(),
                AccountConfig {
                    enabled: true,
                    name: id.to_owned(),
                    provider: AiProvider::OpenAi,
                    credential: AccountCredential::Profile {
                        agent: jackin_core::Agent::Codex,
                        directory: profile,
                        xdg_roots: None,
                        source_selector: None,
                    },
                },
            );
        }
        for (name, ids) in [
            ("mine", vec!["personal", "work"]),
            ("other", vec!["outside"]),
        ] {
            config.workspaces.insert(
                name.to_owned(),
                WorkspaceConfig {
                    accounts: ids.into_iter().map(str::to_owned).collect(),
                    workdir: root.to_string_lossy().into_owned(),
                    ..WorkspaceConfig::default()
                },
            );
        }
        std::fs::write(&paths.config_file, toml::to_string(&config)?)?;
        let discovery = discover_fixture(&paths)?;
        Ok((paths, config, discovery))
    }

    fn discover_fixture(paths: &JackinPaths) -> Result<ValidatedUsageDiscovery> {
        use jackin_usage::host::{
            CachedProviderCredentialResolver, UsageDiscoveryScope, discover_usage_sources,
            validate_usage_sources,
        };
        let resolver = CachedProviderCredentialResolver::new(super::super::RuntimeSecretSource);
        let catalog = discover_usage_sources(
            &UsageDiscoveryScope::HostDesktop {
                config_root: paths.config_dir.clone(),
                operator_home: paths.home_dir.clone(),
            },
            &resolver,
        )
        .map_err(anyhow::Error::msg)?;
        Ok(validate_usage_sources(catalog, &resolver))
    }

    fn expected_projection_account(id: &str) -> (String, String) {
        (
            "codex".to_owned(),
            jackin_core::account_key_hash(
                "openai",
                &format!("canonical-account-v1:provider-id:provider-{id}"),
            ),
        )
    }

    #[test]
    fn workspace_constructor_uses_all_grants_without_launch_membership() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (paths, _, discovery) = granted_accounts_fixture(root.path())?;
        let inventory =
            RelayUsageInventory::prepare(&paths, Some("mine"), &BTreeSet::new(), &discovery)?;
        assert_eq!(inventory.capabilities.len(), 2);
        assert_eq!(
            inventory
                .projection_accounts
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                expected_projection_account("personal"),
                expected_projection_account("work"),
            ])
        );
        assert!(
            !inventory
                .projection_accounts
                .contains_key(&expected_projection_account("outside"))
        );
        inventory.validate_current_config().unwrap();
        Ok(())
    }

    #[test]
    fn saved_authority_restores_without_discovery_and_rejects_revocation_or_forgery() -> Result<()>
    {
        let root = tempfile::tempdir()?;
        let (paths, mut config, discovery) = granted_accounts_fixture(root.path())?;
        let selected = BTreeSet::new();
        let prepared = RelayUsageInventory::prepare(&paths, Some("mine"), &selected, &discovery)?;
        let authorities = prepared.authority();
        assert_eq!(authorities.len(), 2);
        let encoded = serde_json::to_vec(&authorities)?;
        let saved: Vec<InventoryAccountAuthority> = serde_json::from_slice(&encoded)?;

        // Profiles disappear: restore must use saved host authority, never read credentials.
        for id in ["personal", "work", "outside"] {
            std::fs::remove_dir_all(root.path().join(format!("profile-{id}")))?;
        }
        let restored = RelayUsageInventory::restore(
            &paths,
            Some("mine"),
            &selected,
            prepared.config_generation(),
            &saved,
            &[],
        )?;
        assert_eq!(restored.authority(), authorities);
        assert_eq!(restored.capabilities, prepared.capabilities);
        assert_eq!(restored.projection_accounts, prepared.projection_accounts);
        assert_eq!(restored.provenance_counts, prepared.provenance_counts);

        let mut malformed = Vec::new();
        let mut cross_surface = saved[0].clone();
        cross_surface.capability.surface_id = "claude".to_owned();
        malformed.push(vec![cross_surface]);
        let mut empty_canonical_id = saved[0].clone();
        empty_canonical_id.canonical_account_id.clear();
        malformed.push(vec![empty_canonical_id]);
        let mut empty_capability_id = saved[0].clone();
        empty_capability_id.capability.account_id.clear();
        malformed.push(vec![empty_capability_id]);
        let mut unknown_surface = saved[0].clone();
        unknown_surface.surface_id = "unknown".to_owned();
        unknown_surface.capability.surface_id = "unknown".to_owned();
        malformed.push(vec![unknown_surface]);
        let mut zero_provenance = saved[0].clone();
        zero_provenance.provenance_count = 0;
        malformed.push(vec![zero_provenance]);
        malformed.push(vec![saved[0].clone(), saved[0].clone()]);
        for forged in malformed {
            assert!(
                RelayUsageInventory::restore(
                    &paths,
                    Some("mine"),
                    &selected,
                    prepared.config_generation(),
                    &forged,
                    &[],
                )
                .is_err()
            );
        }

        config.workspaces.get_mut("mine").unwrap().accounts.clear();
        std::fs::write(&paths.config_file, toml::to_string(&config)?)?;
        assert!(
            RelayUsageInventory::restore(
                &paths,
                Some("mine"),
                &selected,
                prepared.config_generation(),
                &saved,
                &[],
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn unresolved_grants_preserve_exact_configuration_scope_without_authority() -> Result<()> {
        use jackin_usage::host::UsageDiscoveryDiagnostic;
        let root = tempfile::tempdir()?;
        let (paths, mut config, _) = granted_accounts_fixture(root.path())?;
        let mut disabled = config.accounts["outside"].clone();
        disabled.enabled = false;
        config.accounts.insert("disabled".to_owned(), disabled);
        config
            .workspaces
            .get_mut("mine")
            .unwrap()
            .accounts
            .push("disabled".to_owned());
        std::fs::write(&paths.config_file, toml::to_string(&config)?)?;
        for id in ["personal", "work"] {
            std::fs::remove_dir_all(root.path().join(format!("profile-{id}")))?;
        }
        let mut discovery = discover_fixture(&paths)?;
        discovery.diagnostics = [
            ("personal", UsageDiscoveryIssue::CredentialMissing),
            ("personal", UsageDiscoveryIssue::CredentialMissing),
            ("personal", UsageDiscoveryIssue::CredentialDenied),
            ("work", UsageDiscoveryIssue::CredentialMalformed),
            ("outside", UsageDiscoveryIssue::CredentialDenied),
            ("disabled", UsageDiscoveryIssue::CredentialMissing),
        ]
        .into_iter()
        .map(|(id, issue)| UsageDiscoveryDiagnostic {
            surface_id: Some("codex".to_owned()),
            scope_label: format!("account {id}"),
            issue,
        })
        .collect();
        let selected = BTreeSet::new();
        let inventory = RelayUsageInventory::prepare(&paths, Some("mine"), &selected, &discovery)?;
        assert!(inventory.capabilities.is_empty());
        assert!(inventory.authority().is_empty());
        assert_eq!(inventory.unresolved_grants.len(), 2);
        assert_eq!(
            inventory.unresolved_grants[0].configured_account_id,
            "personal"
        );
        assert_eq!(
            inventory.unresolved_grants[0]
                .issues
                .iter()
                .map(|issue| issue.code.as_str())
                .collect::<Vec<_>>(),
            ["credential_denied", "credential_missing"]
        );
        assert_eq!(inventory.unresolved_grants[1].configured_account_id, "work");
        assert_eq!(
            inventory.unresolved_grants[1].issues[0].code,
            "credential_malformed"
        );
        let grants: Vec<UsageUnresolvedGrantV2> =
            serde_json::from_slice(&serde_json::to_vec(inventory.unresolved_grants())?)?;
        std::fs::remove_dir_all(root.path().join("profile-outside"))?;
        let restored = RelayUsageInventory::restore(
            &paths,
            Some("mine"),
            &selected,
            inventory.config_generation(),
            &[],
            &grants,
        )?;
        assert_eq!(restored.unresolved_grants(), grants);
        assert!(restored.capabilities.is_empty());
        let publication = UsageProjectionV2 {
            schema_version: UsageProjectionSchemaV2,
            projection_id: "scoped-unresolved".to_owned(),
            generated_at_epoch: 123,
            discovery_revision: "revision".to_owned(),
            broker_instance_id: "broker".to_owned(),
            broker_generation: 9,
            refresh_state: UsageProjectionRefreshStateV2::Idle,
            providers: Vec::new(),
            unresolved: Vec::new(),
            unresolved_grants: Vec::new(),
            issues: Vec::new(),
        };
        let filtered = restored.filter_projection(publication);
        filtered.validate().unwrap();
        assert!(filtered.providers.is_empty());
        assert!(filtered.unresolved.is_empty());
        assert_eq!(filtered.unresolved_grants, grants);
        let ad_hoc = RelayUsageInventory::prepare(
            &paths,
            None,
            &BTreeSet::from(["personal".to_owned()]),
            &discovery,
        )?;
        assert_eq!(ad_hoc.unresolved_grants().len(), 1);
        assert_eq!(
            ad_hoc.unresolved_grants()[0].configured_account_id,
            "personal"
        );
        let mut without_diagnostics = discovery.clone();
        without_diagnostics.diagnostics.clear();
        let ambiguous =
            RelayUsageInventory::prepare(&paths, Some("mine"), &selected, &without_diagnostics)?;
        assert!(ambiguous.capabilities.is_empty());
        assert_eq!(ambiguous.unresolved_grants().len(), 2);
        assert!(
            ambiguous
                .unresolved_grants()
                .iter()
                .all(|grant| grant.issues == vec![unresolved_grant_issue()])
        );
        for forged in ["outside", "disabled"] {
            let mut altered = grants.clone();
            altered[0].configured_account_id = forged.to_owned();
            assert!(
                RelayUsageInventory::restore(
                    &paths,
                    Some("mine"),
                    &selected,
                    inventory.config_generation(),
                    &[],
                    &altered
                )
                .is_err()
            );
        }
        let mut malformed = Vec::new();
        let mut reordered = grants.clone();
        reordered[0].issues.reverse();
        malformed.push(reordered);
        let mut repeated = grants.clone();
        repeated[0].issues.push(grants[0].issues[0].clone());
        malformed.push(repeated);
        let mut wrong_surface = grants.clone();
        wrong_surface[0].surface_id = "claude".to_owned();
        malformed.push(wrong_surface);
        let mut wrong_code = grants.clone();
        wrong_code[0].issues[0].code = "arbitrary".to_owned();
        malformed.push(wrong_code);
        let mut wrong_copy = grants.clone();
        wrong_copy[0].issues[0].message = "untrusted source location".to_owned();
        malformed.push(wrong_copy);
        let mut empty_issues = grants.clone();
        empty_issues[0].issues.clear();
        malformed.push(empty_issues);
        let mut duplicate = grants.clone();
        duplicate.push(grants[0].clone());
        malformed.push(duplicate);
        for altered in malformed {
            assert!(
                RelayUsageInventory::restore(
                    &paths,
                    Some("mine"),
                    &selected,
                    inventory.config_generation(),
                    &[],
                    &altered
                )
                .is_err()
            );
        }
        config.workspaces.get_mut("mine").unwrap().accounts.clear();
        std::fs::write(&paths.config_file, toml::to_string(&config)?)?;
        assert!(restored.validate_current_config().is_err());
        assert!(
            RelayUsageInventory::restore(
                &paths,
                Some("mine"),
                &selected,
                inventory.config_generation(),
                &[],
                &grants
            )
            .is_err()
        );
        config.workspaces.get_mut("mine").unwrap().accounts = vec![
            "personal".to_owned(),
            "work".to_owned(),
            "outside".to_owned(),
        ];
        std::fs::write(&paths.config_file, toml::to_string(&config)?)?;
        let current_discovery = discover_fixture(&paths)?;
        let expanded =
            RelayUsageInventory::prepare(&paths, Some("mine"), &selected, &current_discovery)?;
        assert_eq!(
            expanded
                .unresolved_grants()
                .iter()
                .map(|grant| grant.configured_account_id.as_str())
                .collect::<Vec<_>>(),
            ["outside", "personal", "work"]
        );
        config
            .workspaces
            .get_mut("mine")
            .unwrap()
            .accounts
            .retain(|id| id != "personal");
        std::fs::write(&paths.config_file, toml::to_string(&config)?)?;
        assert!(expanded.validate_current_config().is_err());
        let current_discovery = discover_fixture(&paths)?;
        let reduced =
            RelayUsageInventory::prepare(&paths, Some("mine"), &selected, &current_discovery)?;
        assert_eq!(
            reduced
                .unresolved_grants()
                .iter()
                .map(|grant| grant.configured_account_id.as_str())
                .collect::<Vec<_>>(),
            ["outside", "work"]
        );
        Ok(())
    }

    #[test]
    fn ad_hoc_constructor_uses_only_selected_accounts() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (paths, _, discovery) = granted_accounts_fixture(root.path())?;
        let selected = BTreeSet::from(["work".to_owned()]);
        let inventory = RelayUsageInventory::prepare(&paths, None, &selected, &discovery)?;
        assert_eq!(inventory.selected_account_ids, selected);
        assert_eq!(inventory.capabilities.len(), 1);
        assert_eq!(
            inventory
                .projection_accounts
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![expected_projection_account("work")]
        );
        let empty = RelayUsageInventory::prepare(&paths, None, &BTreeSet::new(), &discovery)?;
        assert!(empty.capabilities.is_empty());
        empty.validate_current_config().unwrap();
        Ok(())
    }

    #[test]
    fn grant_removal_revokes_prepared_workspace_authority() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (paths, mut config, discovery) = granted_accounts_fixture(root.path())?;
        let inventory =
            RelayUsageInventory::prepare(&paths, Some("mine"), &BTreeSet::new(), &discovery)?;
        config
            .workspaces
            .get_mut("mine")
            .unwrap()
            .accounts
            .retain(|id| id != "work");
        std::fs::write(&paths.config_file, toml::to_string(&config)?)?;
        assert_eq!(
            inventory.validate_current_config().unwrap_err().kind,
            UsageCoordinationErrorKind::Unauthorized
        );
        assert!(
            RelayUsageInventory::prepare(&paths, Some("mine"), &BTreeSet::new(), &discovery)
                .is_err()
        );
        let current = discover_fixture(&paths)?;
        let replacement =
            RelayUsageInventory::prepare(&paths, Some("mine"), &BTreeSet::new(), &current)?;
        assert_eq!(
            replacement
                .projection_accounts
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![expected_projection_account("personal")]
        );
        Ok(())
    }

    #[test]
    fn disabled_accounts_grant_nothing_and_unknown_selections_fail_closed() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (paths, mut config, _) = granted_accounts_fixture(root.path())?;
        config.accounts.get_mut("work").unwrap().enabled = false;
        std::fs::write(&paths.config_file, toml::to_string(&config)?)?;
        let discovery = discover_fixture(&paths)?;
        let inventory =
            RelayUsageInventory::prepare(&paths, Some("mine"), &BTreeSet::new(), &discovery)?;
        assert_eq!(
            inventory
                .projection_accounts
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![expected_projection_account("personal")]
        );
        let disabled = RelayUsageInventory::prepare(
            &paths,
            None,
            &BTreeSet::from(["work".to_owned()]),
            &discovery,
        )?;
        assert!(disabled.capabilities.is_empty());
        assert!(
            RelayUsageInventory::prepare(
                &paths,
                None,
                &BTreeSet::from(["unknown".to_owned()]),
                &discovery
            )
            .is_err()
        );
        assert!(
            RelayUsageInventory::prepare(&paths, Some("unknown"), &BTreeSet::new(), &discovery)
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn actual_read_dispatch_accepts_empty_refresh_authority_and_never_probes() -> Result<()> {
        use jackin_protocol::usage_broker::{
            UsageBrokerOperation, UsageBrokerResponse, UsageCredentialScope,
        };
        use jackin_usage::coordinator::{
            ProviderProbeOutcome, UsageCapabilitySet, UsageProviderExecutor,
        };
        use jackin_usage::host::{UsageBrokerConfig, ensure_usage_broker_with_executor};
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct ReadOnlyExecutor(AtomicUsize);
        impl UsageProviderExecutor for ReadOnlyExecutor {
            fn probe(&self, _: &UsageAccountCapability, _: u64) -> ProviderProbeOutcome {
                self.0.fetch_add(1, Ordering::SeqCst);
                ProviderProbeOutcome::Failure {
                    kind: UsageCoordinationErrorKind::Unavailable,
                    message: "unexpected provider probe".to_owned(),
                    retry_at_epoch: None,
                }
            }
        }
        let root = tempfile::tempdir()?;
        let (paths, mut config, discovery) = granted_accounts_fixture(root.path())?;
        let inventory =
            RelayUsageInventory::prepare(&paths, Some("mine"), &BTreeSet::new(), &discovery)?;
        let empty = RelayUsageInventory::prepare(&paths, None, &BTreeSet::new(), &discovery)?;
        let executor = Arc::new(ReadOnlyExecutor(AtomicUsize::new(0)));
        let broker = ensure_usage_broker_with_executor(
            UsageBrokerConfig::for_data_dir(paths.data_dir.clone()),
            executor.clone(),
        )
        .map_err(|error| anyhow::anyhow!("broker fixture failed: {}", error.message))?;
        for fence in [inventory.clone(), empty] {
            let response = super::super::dispatch(
                UsageBrokerOperation::CurrentProjectionForSurface,
                broker.clone(),
                UsageCapabilitySet::new([]),
                UsageCredentialScope::default(),
                Some(fence),
                BTreeMap::new(),
                None,
            )
            .await;
            let UsageBrokerResponse::Projection { projection } = response else {
                panic!("inventory read requires no refresh capability: {response:?}");
            };
            projection.validate().unwrap();
            assert_eq!(
                projection.refresh_state,
                UsageProjectionRefreshStateV2::Idle
            );
            assert_eq!(executor.0.load(Ordering::SeqCst), 0);
        }
        config.workspaces.get_mut("mine").unwrap().accounts.clear();
        std::fs::write(&paths.config_file, toml::to_string(&config)?)?;
        let response = super::super::dispatch(
            UsageBrokerOperation::CurrentProjectionForSurface,
            broker,
            UsageCapabilitySet::new([]),
            UsageCredentialScope::default(),
            Some(inventory),
            BTreeMap::new(),
            None,
        )
        .await;
        let UsageBrokerResponse::Error { error } = response else {
            panic!("revoked inventory still served projection");
        };
        assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
        assert_eq!(executor.0.load(Ordering::SeqCst), 0);
        Ok(())
    }

    fn inventory(root: &std::path::Path, capabilities: &[(&str, &str)]) -> RelayUsageInventory {
        let paths = JackinPaths::for_tests(root);
        let snapshot = load_read_only_config_snapshot(&paths).unwrap();
        RelayUsageInventory {
            unresolved_grants: Vec::new(),
            paths,
            config_generation: snapshot.generation.as_str().to_owned(),
            capabilities: capabilities
                .iter()
                .map(|(account, surface)| UsageAccountCapability {
                    account_id: format!("authority:{account}"),
                    surface_id: (*surface).to_owned(),
                })
                .collect(),
            workspace: None,
            selected_account_ids: BTreeSet::new(),
            provenance_counts: capabilities
                .iter()
                .map(|(account, surface)| (((*surface).to_owned(), (*account).to_owned()), 1))
                .collect(),
            projection_accounts: capabilities
                .iter()
                .map(|(account, surface)| {
                    (
                        ((*surface).to_owned(), (*account).to_owned()),
                        UsageAccountCapability {
                            account_id: format!("authority:{account}"),
                            surface_id: (*surface).to_owned(),
                        },
                    )
                })
                .collect(),
        }
    }

    fn freshness() -> UsageFreshnessV2 {
        UsageFreshnessV2 {
            generation: 9,
            phase: UsageFreshnessPhaseV2::Current,
            last_good_at_epoch: Some(100),
            retry_at_epoch: None,
            is_stale: false,
        }
    }

    fn issue(scope: UsageIssueScopeV2) -> UsageIssueV2 {
        UsageIssueV2 {
            code: "scope-fixture".to_owned(),
            scope,
            recoverability: UsageIssueRecoverabilityV2::Unsupported,
            message: "scope fixture".to_owned(),
            retry_at_epoch: None,
        }
    }

    fn account(id: &str, rank: u32) -> UsageAccountV2 {
        UsageAccountV2 {
            canonical_account_id: id.to_owned(),
            refresh_capabilities: Vec::new(),
            identity_kind: UsageIdentityKindV2::ProviderAccountId,
            rank,
            display_label: id.to_owned(),
            username: None,
            auth_origin: None,
            plan_label: None,
            status_label: None,
            lifecycle: UsageLifecycleV2::Unsupported,
            freshness: freshness(),
            provenance_count: 1,
            windows: Vec::new(),
            metric_groups: Vec::new(),
            credential_expires_at_epoch: None,
            issues: vec![
                issue(UsageIssueScopeV2::Account),
                issue(UsageIssueScopeV2::Projection),
            ],
        }
    }

    fn provider(id: &str, rank: u32, accounts: Vec<UsageAccountV2>) -> UsageProviderV2 {
        UsageProviderV2 {
            provider_id: id.to_owned(),
            display_name: id.to_owned(),
            rank,
            membership_state: UsageMembershipStateV2::Current,
            freshness: freshness(),
            accounts,
            issues: vec![issue(UsageIssueScopeV2::Provider)],
        }
    }

    #[test]
    fn current_membership_requires_all_admitted_accounts_and_allows_genuinely_empty_scope() {
        let root = tempfile::tempdir().unwrap();
        let admitted = inventory(root.path(), &[("first", "codex"), ("second", "codex")]);
        let publication = UsageProjectionV2 {
            schema_version: UsageProjectionSchemaV2,
            projection_id: "complete-publication".to_owned(),
            generated_at_epoch: 123,
            discovery_revision: "current-configured-sources".to_owned(),
            broker_instance_id: "current-broker".to_owned(),
            broker_generation: 9,
            refresh_state: UsageProjectionRefreshStateV2::Refreshing,
            providers: vec![provider("openai", 0, vec![account("first", 0), account("second", 1)])],
            unresolved_grants: Vec::new(),
            unresolved: Vec::new(),
            issues: Vec::new(),
        };
        let complete = admitted.filter_projection(publication);
        jackin_protocol::control::UsageAccountMembershipV1::validate_current_projection(&complete).unwrap();
        assert!(admitted.projection_is_complete_and_scoped(&complete));

        let mut drop_one = complete.clone();
        drop_one.providers[0].accounts.pop();
        // Old filter-equality admission accepts this controlled omission.
        assert_eq!(admitted.filter_projection(drop_one.clone()), drop_one);
        assert!(!admitted.projection_is_complete_and_scoped(&drop_one));
        let mut drop_all = complete.clone();
        drop_all.providers.clear();
        assert_eq!(admitted.filter_projection(drop_all.clone()), drop_all);
        assert!(!admitted.projection_is_complete_and_scoped(&drop_all));

        let empty_inventory = inventory(root.path(), &[]);
        let genuinely_empty = empty_inventory.filter_projection(complete);
        assert!(genuinely_empty.providers.is_empty());
        jackin_protocol::control::UsageAccountMembershipV1::validate_current_projection(&genuinely_empty).unwrap();
        assert!(empty_inventory.projection_is_complete_and_scoped(&genuinely_empty));
    }

    #[test]
    fn inventory_filters_exact_account_and_provider_without_requiring_quota() {
        let root = tempfile::tempdir().unwrap();
        let inventory = inventory(root.path(), &[("shared", "claude"), ("mine", "codex")]);
        let mut projection = UsageProjectionV2 {
            schema_version: UsageProjectionSchemaV2,
            projection_id: "publication".to_owned(),
            generated_at_epoch: 123,
            discovery_revision: "opaque-revision".to_owned(),
            broker_instance_id: "broker".to_owned(),
            broker_generation: 9,
            refresh_state: UsageProjectionRefreshStateV2::Refreshing,
            providers: vec![
                provider("amp", 0, vec![account("shared", 0)]),
                provider(
                    "anthropic",
                    1,
                    vec![account("other", 0), account("shared", 1)],
                ),
                provider("openai", 2, vec![account("mine", 0), account("shared", 1)]),
            ],
            unresolved_grants: Vec::new(),
            unresolved: vec![UsageUnresolvedV2 {
                provider_id: "amp".to_owned(),
                capability_id: "outside".to_owned(),
                configuration_count: 1,
                state: UsageLifecycleV2::NeedsLogin,
                issues: vec![issue(UsageIssueScopeV2::Account)],
            }],
            issues: vec![issue(UsageIssueScopeV2::Projection)],
        };
        let granted_route = UsageAccountCapability {
            account_id: "authority:shared".to_owned(),
            surface_id: "claude".to_owned(),
        };
        projection.providers[2].accounts[0].refresh_capabilities = vec![UsageAccountCapability {
            account_id: "authority:mine".to_owned(),
            surface_id: "codex".to_owned(),
        }];
        projection.providers[1].accounts[1].refresh_capabilities = vec![
            granted_route.clone(),
            UsageAccountCapability {
                account_id: "foreign-credential-same-canonical-account".to_owned(),
                surface_id: "claude".to_owned(),
            },
        ];
        let filtered = inventory.filter_projection(projection);
        filtered.validate().unwrap();
        assert_eq!(filtered.providers.len(), 2);
        assert_eq!(
            filtered.providers[0].accounts[0].canonical_account_id,
            "shared"
        );
        assert_eq!(
            filtered.providers[1].accounts[0].canonical_account_id,
            "mine"
        );
        assert!(
            filtered
                .providers
                .iter()
                .all(|provider| provider.accounts.len() == 1)
        );
        assert_eq!(filtered.providers[1].rank, 1);
        assert_eq!(
            filtered.providers[0].accounts[0].refresh_capabilities,
            vec![granted_route]
        );
        assert_eq!(filtered.refresh_state, UsageProjectionRefreshStateV2::Idle);
        assert_eq!(filtered.projection_id, "publication");
        assert_eq!(filtered.generated_at_epoch, 123);
        assert_eq!(filtered.broker_generation, 9);
        assert!(filtered.issues.is_empty());
        assert!(filtered.unresolved.is_empty());
        for provider in &filtered.providers {
            assert!(provider.issues.is_empty());
            assert_eq!(
                provider.accounts[0].issues,
                vec![issue(UsageIssueScopeV2::Account)]
            );
        }
    }

    #[test]
    fn configured_aliases_count_one_admitted_source() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (paths, mut config, _) = granted_accounts_fixture(root.path())?;
        let personal = config
            .accounts
            .get("personal")
            .context("fixture personal account")?
            .clone();
        config
            .accounts
            .insert("personal_alias".to_owned(), personal);
        config
            .workspaces
            .get_mut("mine")
            .context("fixture workspace")?
            .accounts = vec!["personal".to_owned(), "personal_alias".to_owned()];
        std::fs::write(&paths.config_file, toml::to_string(&config)?)?;
        let discovery = discover_fixture(&paths)?;
        let admitted =
            RelayUsageInventory::prepare(&paths, Some("mine"), &BTreeSet::new(), &discovery)?;
        assert_eq!(
            admitted.capabilities.len(),
            1,
            "same source aliases share exact authority"
        );
        assert_eq!(
            admitted
                .provenance_counts
                .values()
                .copied()
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(
            admitted.authority()[0].provenance_count,
            1,
            "restored proof preserves source count"
        );
        Ok(())
    }

    #[test]
    fn distinct_profile_roots_for_one_principal_count_two_sources() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (paths, config, _) = granted_accounts_fixture(root.path())?;
        let work = config
            .accounts
            .get("work")
            .context("fixture work account")?;
        let jackin_config::AccountCredential::Profile { directory, .. } = &work.credential else {
            anyhow::bail!("fixture work profile expected");
        };
        std::fs::write(
            directory.join("auth.json"),
            r#"{"tokens":{"access_token":"fixture-independent-work-source","account_id":"provider-personal"}}"#,
        )?;
        let discovery = discover_fixture(&paths)?;
        let admitted =
            RelayUsageInventory::prepare(&paths, Some("mine"), &BTreeSet::new(), &discovery)?;
        assert_eq!(
            admitted.capabilities.len(),
            1,
            "same principal shares broker routing identity"
        );
        assert_eq!(admitted.projection_accounts.len(), 1);
        assert_eq!(
            admitted
                .provenance_counts
                .values()
                .copied()
                .collect::<Vec<_>>(),
            vec![2]
        );
        assert_eq!(
            admitted.authority()[0].provenance_count,
            2,
            "distinct source roots survive proof persistence"
        );
        Ok(())
    }

    #[test]
    fn older_issuer_route_cannot_publish_fresh_usage_for_same_canonical_account() {
        let root = tempfile::tempdir().unwrap();
        let admitted = inventory(root.path(), &[("same-principal", "codex")]);
        let mut row = account("same-principal", 0);
        row.lifecycle = UsageLifecycleV2::Available;
        row.refresh_capabilities = vec![UsageAccountCapability {
            account_id: "older-catalog-route".to_owned(),
            surface_id: "codex".to_owned(),
        }];
        row.windows
            .push(jackin_protocol::usage_broker::UsageLimitWindowV2 {
                window_id: "old-route-window".to_owned(),
                rank: 0,
                category: jackin_protocol::usage_broker::UsageWindowCategoryV2::Session,
                label: "Session".to_owned(),
                value_label: "75% remaining".to_owned(),
                reset_label: "In one hour".to_owned(),
                remaining_percent: Some(
                    jackin_protocol::usage_broker::UsagePercent::new(75).unwrap(),
                ),
                remaining_raw_percent: Some(75),
                used_percent: Some(jackin_protocol::usage_broker::UsagePercent::new(25).unwrap()),
                used_raw_percent: Some(25),
                reset_at_epoch: Some(999),
                quota_state: jackin_protocol::usage_broker::UsageQuotaStateV2::Available,
                count_quota: None,
                pace_label: None,
                runs_out_label: None,
            });
        row.username = Some("old-route-user".to_owned());
        row.auth_origin = Some("old-route-origin".to_owned());
        row.plan_label = Some("old-route-plan".to_owned());
        row.credential_expires_at_epoch = Some(999);
        let projection = UsageProjectionV2 {
            schema_version: UsageProjectionSchemaV2,
            projection_id: "older-issuer".to_owned(),
            generated_at_epoch: 123,
            discovery_revision: "older-catalog".to_owned(),
            broker_instance_id: "broker".to_owned(),
            broker_generation: 9,
            refresh_state: UsageProjectionRefreshStateV2::Idle,
            providers: vec![provider("openai", 0, vec![row])],
            unresolved_grants: Vec::new(),
            unresolved: Vec::new(),
            issues: Vec::new(),
        };
        let filtered = admitted.filter_projection(projection);
        let visible = &filtered.providers[0].accounts[0];
        assert_eq!(
            visible.canonical_account_id, "same-principal",
            "authorized inventory remains visible"
        );
        assert_eq!(visible.lifecycle, UsageLifecycleV2::Unavailable);
        assert_eq!(visible.freshness.phase, UsageFreshnessPhaseV2::Failed);
        assert!(visible.refresh_capabilities.is_empty());
        assert!(visible.windows.is_empty() && visible.metric_groups.is_empty());
        assert!(visible.plan_label.is_none() && visible.credential_expires_at_epoch.is_none());
        assert!(visible.username.is_none() && visible.auth_origin.is_none());
        assert_eq!(visible.issues[0].code, "scoped_usage_route_unavailable");
    }

    #[test]
    fn configuration_changes_revoke_an_existing_inventory() {
        let root = tempfile::tempdir().unwrap();
        let inventory = inventory(root.path(), &[]);
        inventory.validate_current_config().unwrap();
        std::fs::create_dir_all(&inventory.paths.config_dir).unwrap();
        std::fs::write(&inventory.paths.config_file, "invalid config bytes").unwrap();
        assert_eq!(
            inventory.validate_current_config().unwrap_err().kind,
            UsageCoordinationErrorKind::Unauthorized
        );
    }

    #[test]
    fn diagnostics_fail_only_the_relevant_workspace_and_shared_sources() {
        let root = tempfile::tempdir().unwrap();
        let mut snapshot =
            load_read_only_config_snapshot(&JackinPaths::for_tests(root.path())).unwrap();
        snapshot.diagnostics.push(ConfigSourceDiagnostic {
            scope: ConfigSourceScope::Workspace("other".to_owned()),
            issue: ConfigSourceIssue::Malformed,
        });
        validate_diagnostics(&snapshot, Some("mine")).unwrap();
        assert!(validate_diagnostics(&snapshot, Some("other")).is_err());
        snapshot.diagnostics.push(ConfigSourceDiagnostic {
            scope: ConfigSourceScope::Workspaces,
            issue: ConfigSourceIssue::TransientConflict,
        });
        assert!(validate_diagnostics(&snapshot, None).is_err());
    }
}

fn grant_issue(issue: UsageDiscoveryIssue) -> UsageIssueV2 {
    let recoverability = match issue {
        UsageDiscoveryIssue::ConfigVersionUnsupported => UsageIssueRecoverabilityV2::Unsupported,
        UsageDiscoveryIssue::ConfigTransientConflict
        | UsageDiscoveryIssue::CredentialUnavailable => UsageIssueRecoverabilityV2::Retryable,
        UsageDiscoveryIssue::ConfigUnreadable
        | UsageDiscoveryIssue::ConfigInvalid
        | UsageDiscoveryIssue::CredentialMissing
        | UsageDiscoveryIssue::CredentialDenied
        | UsageDiscoveryIssue::KeychainConsentRequired
        | UsageDiscoveryIssue::CredentialMalformed
        | UsageDiscoveryIssue::InteractionRequired => UsageIssueRecoverabilityV2::ActionRequired,
    };
    UsageIssueV2 {
        code: issue.id().to_owned(),
        scope: UsageIssueScopeV2::Account,
        recoverability,
        message: issue.display_message().to_owned(),
        retry_at_epoch: None,
    }
}

fn unresolved_grant_issue() -> UsageIssueV2 {
    UsageIssueV2 {
        code: "account_source_unresolved".to_owned(),
        scope: UsageIssueScopeV2::Account,
        recoverability: UsageIssueRecoverabilityV2::ActionRequired,
        message: "Configured account source could not be resolved".to_owned(),
        retry_at_epoch: None,
    }
}

fn saved_grant_issue(code: &str) -> Option<UsageIssueV2> {
    let issue = match code {
        "config_unreadable" => UsageDiscoveryIssue::ConfigUnreadable,
        "config_invalid" => UsageDiscoveryIssue::ConfigInvalid,
        "config_version_unsupported" => UsageDiscoveryIssue::ConfigVersionUnsupported,
        "config_transient_conflict" => UsageDiscoveryIssue::ConfigTransientConflict,
        "credential_unavailable" => UsageDiscoveryIssue::CredentialUnavailable,
        "credential_missing" => UsageDiscoveryIssue::CredentialMissing,
        "credential_denied" => UsageDiscoveryIssue::CredentialDenied,
        "keychain_consent_required" => UsageDiscoveryIssue::KeychainConsentRequired,
        "credential_malformed" => UsageDiscoveryIssue::CredentialMalformed,
        "interaction_required" => UsageDiscoveryIssue::InteractionRequired,
        "account_source_unresolved" => return Some(unresolved_grant_issue()),
        _ => return None,
    };
    Some(grant_issue(issue))
}

fn validate_saved_grants(
    paths: &JackinPaths,
    workspace: Option<&str>,
    selected_account_ids: &BTreeSet<String>,
    grants: &[UsageUnresolvedGrantV2],
) -> Result<()> {
    let snapshot = load_read_only_config_snapshot(paths)?;
    let authorized = match workspace {
        Some(name) => snapshot
            .config
            .workspaces
            .get(name)
            .context("saved usage inventory workspace is unavailable")?
            .accounts
            .iter()
            .cloned()
            .collect(),
        None => selected_account_ids.clone(),
    };
    let mut seen = BTreeSet::new();
    for grant in grants {
        let account = snapshot
            .config
            .accounts
            .get(&grant.configured_account_id)
            .context("invalid saved unresolved usage grant")?;
        anyhow::ensure!(
            authorized.contains(&grant.configured_account_id)
                && !grant.configured_account_id.trim().is_empty()
                && !grant.configured_account_id.chars().any(char::is_control)
                && account.enabled
                && HostSurfaceId::from_provider_alias(account.provider.slug())
                    .is_some_and(|surface| surface.id() == grant.surface_id)
                && !grant.issues.is_empty()
                && grant
                    .issues
                    .windows(2)
                    .all(|pair| pair[0].code < pair[1].code)
                && seen.insert((&grant.configured_account_id, &grant.surface_id))
                && grant
                    .issues
                    .iter()
                    .all(|issue| { saved_grant_issue(&issue.code).as_ref() == Some(issue) }),
            "invalid saved unresolved usage grant"
        );
    }
    Ok(())
}

fn validate_diagnostics(snapshot: &ReadOnlyConfigSnapshot, workspace: Option<&str>) -> Result<()> {
    anyhow::ensure!(
        !snapshot
            .diagnostics
            .iter()
            .any(|diagnostic| match &diagnostic.scope {
                ConfigSourceScope::Global | ConfigSourceScope::Workspaces => true,
                ConfigSourceScope::Workspace(name) => workspace == Some(name.as_str()),
            }),
        "usage inventory configuration source is unavailable"
    );
    Ok(())
}

fn scoped_provider_freshness(
    accounts: &[jackin_protocol::usage_broker::UsageAccountV2],
    generation: u64,
) -> UsageFreshnessV2 {
    let is_stale = accounts.iter().any(|account| account.freshness.is_stale);
    let phase = if accounts
        .iter()
        .any(|account| account.freshness.phase == UsageFreshnessPhaseV2::Refreshing)
    {
        UsageFreshnessPhaseV2::Refreshing
    } else if is_stale {
        UsageFreshnessPhaseV2::Stale
    } else if accounts
        .iter()
        .all(|account| account.freshness.phase == UsageFreshnessPhaseV2::Failed)
    {
        UsageFreshnessPhaseV2::Failed
    } else {
        UsageFreshnessPhaseV2::Current
    };
    UsageFreshnessV2 {
        generation,
        phase,
        last_good_at_epoch: accounts
            .iter()
            .filter_map(|account| account.freshness.last_good_at_epoch)
            .max(),
        retry_at_epoch: accounts
            .iter()
            .filter_map(|account| account.freshness.retry_at_epoch)
            .min(),
        is_stale,
    }
}
