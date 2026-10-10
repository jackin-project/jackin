// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Credential-free presentation state over broker-canonical publications.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageCalendarPeriodV1, UsageMetricGroupKindV1, UsageMetricGroupV1,
    UsageMetricPeriodV1, UsageMetricValueV1, UsageProjectionV1, UsageProviderV1, UsageUnresolvedV1,
};
use serde::{Deserialize, Serialize};

use crate::usage::atomic_write_usage_json;

use super::{HOST_USAGE_STATE_REL, HostSurfaceId};

const SELECTED_PROJECTION_ACCOUNTS_FILE: &str = "selected-projection-accounts.json";

/// Inputs for a presentation runtime that consumes broker publications only.
#[derive(Debug, Clone)]
pub struct HostUsageProjectionConfig {
    /// jackin data dir (`~/.jackin/data` or a test root).
    pub data_dir: PathBuf,
    /// Initially enabled host surface ids; empty means every host surface.
    pub enabled_surface_ids: Vec<String>,
}

impl HostUsageProjectionConfig {
    /// Build config under one data directory with every host surface enabled.
    #[must_use]
    pub fn under_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            enabled_surface_ids: Vec::new(),
        }
    }
}

/// One account entry exposed to a native consumer without changing its
/// canonical identity or flattening its typed metric groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostUsageProjectionAccountPresentation<'a> {
    /// Host surface id used by native settings and display code.
    pub surface_id: &'static str,
    /// Broker-canonical provider record, including provider freshness/issues.
    pub provider: &'a UsageProviderV1,
    /// Broker-canonical account record, including windows and typed groups.
    pub account: &'a UsageAccountV1,
    /// Whether this exact canonical account id is selected for the surface.
    pub selected: bool,
}

/// Resolution of persisted operator intent against the current publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostUsageProjectionSelectedAccount<'a> {
    /// No account is selected for the surface.
    Unselected,
    /// The selected canonical id is present in this provider publication.
    Available {
        canonical_account_id: &'a str,
        account: &'a UsageAccountV1,
    },
    /// The selected canonical id is not present in this provider publication.
    /// It remains selected so a sibling account is never chosen implicitly.
    Unavailable { canonical_account_id: &'a str },
}

/// Current broker-owned provider status and selected-account presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostUsageProjectionProviderPresentation<'a> {
    /// Host surface id used by native settings and display code.
    pub surface_id: &'static str,
    /// Canonical provider id used by the broker publication.
    pub provider_id: &'static str,
    /// Current broker provider row, if it exists in this publication.
    /// Its membership, freshness, issues, and ranked accounts remain typed.
    pub provider: Option<&'a UsageProviderV1>,
    /// Broker unresolved-capability rows for this provider, when identity is
    /// not yet available. They remain separate from canonical accounts.
    pub unresolved_capabilities: Vec<&'a UsageUnresolvedV1>,
    /// Exact persisted selection resolved against the current provider row.
    pub selected_account: HostUsageProjectionSelectedAccount<'a>,
    /// First rank-ordered typed weekly group (daily for Amp), if published.
    pub glance_metric_group: Option<&'a UsageMetricGroupV1>,
    /// All typed detail groups for the selected current account.
    pub detail_metric_groups: &'a [UsageMetricGroupV1],
}

/// Broker-publication consumer for account inventory and native presentation.
///
/// This runtime owns no discovery, credentials, cache, or provider client. Its
/// only usage input is a complete `UsageProjectionV1` publication. Selected
/// account values are canonical broker account ids, persisted independently
/// of the discovery-backed host runtime's account-key store.
#[derive(Debug, Clone)]
pub struct HostUsageProjectionRuntime {
    projection: UsageProjectionV1,
    data_dir: PathBuf,
    enabled_surfaces: BTreeSet<HostSurfaceId>,
    selected_accounts: BTreeMap<HostSurfaceId, String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct PersistedProjectionSelections {
    #[serde(default)]
    selected: BTreeMap<String, String>,
}

impl HostUsageProjectionRuntime {
    /// Open presentation state from one complete broker publication.
    ///
    /// This validates the publication and reads only the dedicated selection
    /// preferences file under `data_dir`; it performs no discovery, credential
    /// resolution, or provider access.
    pub fn open(
        projection: UsageProjectionV1,
        config: HostUsageProjectionConfig,
    ) -> Result<Self, String> {
        validate_projection(&projection)?;
        let enabled_surfaces = enabled_surfaces(&config.enabled_surface_ids)?;
        let mut runtime = Self {
            selected_accounts: load_projection_selections(&config.data_dir)?,
            projection,
            data_dir: config.data_dir,
            enabled_surfaces,
        };
        let mut selections = runtime.selected_accounts.clone();
        Self::add_default_selections_for(
            &runtime.projection,
            &runtime.enabled_surfaces,
            &mut selections,
        );
        if selections != runtime.selected_accounts {
            runtime.persist_selections(&selections)?;
            runtime.selected_accounts = selections;
        }
        Ok(runtime)
    }

    /// Current immutable broker publication.
    #[must_use]
    pub fn projection(&self) -> &UsageProjectionV1 {
        &self.projection
    }

    /// Apply one later whole publication.
    ///
    /// For one broker incarnation, generations cannot move backwards and a
    /// generation cannot be reused for different content. A new broker
    /// incarnation may restart its generation. Invalid or stale publications
    /// leave the current publication and selections untouched.
    pub fn apply_publication(&mut self, projection: UsageProjectionV1) -> Result<(), String> {
        validate_projection(&projection)?;
        if projection.broker_instance_id == self.projection.broker_instance_id {
            if projection.broker_generation < self.projection.broker_generation {
                return Err("stale usage projection publication".to_owned());
            }
            if projection.broker_generation == self.projection.broker_generation {
                if projection == self.projection {
                    return Ok(());
                }
                return Err("conflicting usage projection generation".to_owned());
            }
            if projection.projection_id == self.projection.projection_id {
                return Err("usage projection id reused for a newer generation".to_owned());
            }
        }

        let mut selections = self.selected_accounts.clone();
        Self::add_default_selections_for(&projection, &self.enabled_surfaces, &mut selections);
        if selections != self.selected_accounts {
            self.persist_selections(&selections)?;
        }
        self.projection = projection;
        self.selected_accounts = selections;
        Ok(())
    }

    /// Replace the enabled host surface set. An empty list enables all known
    /// surfaces. Existing selections for disabled surfaces remain persisted.
    pub fn set_enabled_surface_ids(&mut self, ids: &[String]) -> Result<(), String> {
        let enabled_surfaces = enabled_surfaces(ids)?;
        let mut selections = self.selected_accounts.clone();
        Self::add_default_selections_for(&self.projection, &enabled_surfaces, &mut selections);
        if selections != self.selected_accounts {
            self.persist_selections(&selections)?;
        }
        self.enabled_surfaces = enabled_surfaces;
        self.selected_accounts = selections;
        Ok(())
    }

    /// Select one exact broker `canonical_account_id`, or clear selection.
    ///
    /// A selection is persisted even if a later publication removes that
    /// account. The resulting provider presentation reports `Unavailable`
    /// until the same canonical identity returns or the user selects another.
    pub fn set_selected_account(
        &mut self,
        surface_id: &str,
        canonical_account_id: Option<&str>,
    ) -> Result<(), String> {
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if !self.enabled_surfaces.contains(&surface) {
            return Err(format!("surface disabled: {surface_id}"));
        }
        let mut selections = self.selected_accounts.clone();
        if let Some(canonical_account_id) = canonical_account_id {
            if canonical_account_id.is_empty() {
                return Err("canonical account id must not be empty".to_owned());
            }
            let provider = Self::provider_for_surface(&self.projection, surface);
            if !provider.is_some_and(|provider| {
                provider
                    .accounts
                    .iter()
                    .any(|account| account.canonical_account_id == canonical_account_id)
            }) {
                return Err(format!(
                    "canonical account id does not belong to surface {surface_id}"
                ));
            }
            selections.insert(surface, canonical_account_id.to_owned());
        } else {
            selections.remove(&surface);
        }
        if selections != self.selected_accounts {
            self.persist_selections(&selections)?;
            self.selected_accounts = selections;
        }
        Ok(())
    }

    /// List broker-canonical account rows in publication order, filtered by
    /// enabled surfaces and optionally one host surface id.
    pub fn account_inventory(
        &self,
        surface_id: Option<&str>,
    ) -> Result<Vec<HostUsageProjectionAccountPresentation<'_>>, String> {
        let selected_surface = surface_id
            .map(|id| HostSurfaceId::from_id(id).ok_or_else(|| format!("unknown surface: {id}")))
            .transpose()?;
        let mut inventory = Vec::new();
        for provider in &self.projection.providers {
            let Some(surface) = surface_for_provider_id(&provider.provider_id) else {
                // `open` and `apply_publication` already reject this shape.
                continue;
            };
            if !self.enabled_surfaces.contains(&surface)
                || selected_surface.is_some_and(|selected| selected != surface)
            {
                continue;
            }
            let selected_id = self.selected_accounts.get(&surface).map(String::as_str);
            inventory.extend(provider.accounts.iter().map(|account| {
                HostUsageProjectionAccountPresentation {
                    surface_id: surface.id(),
                    provider,
                    account,
                    selected: selected_id == Some(account.canonical_account_id.as_str()),
                }
            }));
        }
        Ok(inventory)
    }

    /// Current provider status and selected account, retaining broker types.
    pub fn provider_presentation(
        &self,
        surface_id: &str,
    ) -> Result<HostUsageProjectionProviderPresentation<'_>, String> {
        let surface = HostSurfaceId::from_id(surface_id)
            .ok_or_else(|| format!("unknown surface: {surface_id}"))?;
        if !self.enabled_surfaces.contains(&surface) {
            return Err(format!("surface disabled: {surface_id}"));
        }
        let provider = Self::provider_for_surface(&self.projection, surface);
        let provider_id = surface.provider_id();
        let unresolved_capabilities = self
            .projection
            .unresolved
            .iter()
            .filter(|unresolved| unresolved.provider_id == provider_id)
            .collect();
        let (selected_account, detail_metric_groups) = match self.selected_accounts.get(&surface) {
            None => (HostUsageProjectionSelectedAccount::Unselected, &[][..]),
            Some(canonical_account_id) => {
                let account = provider.and_then(|provider| {
                    provider
                        .accounts
                        .iter()
                        .find(|account| account.canonical_account_id == *canonical_account_id)
                });
                match account {
                    Some(account) => (
                        HostUsageProjectionSelectedAccount::Available {
                            canonical_account_id: &account.canonical_account_id,
                            account,
                        },
                        account.metric_groups.as_slice(),
                    ),
                    None => (
                        HostUsageProjectionSelectedAccount::Unavailable {
                            canonical_account_id,
                        },
                        &[][..],
                    ),
                }
            }
        };
        let glance_metric_group = match selected_account {
            HostUsageProjectionSelectedAccount::Available { account, .. } => {
                glance_metric_group(surface, account)
            }
            HostUsageProjectionSelectedAccount::Unselected
            | HostUsageProjectionSelectedAccount::Unavailable { .. } => None,
        };
        Ok(HostUsageProjectionProviderPresentation {
            surface_id: surface.id(),
            provider_id,
            provider,
            unresolved_capabilities,
            selected_account,
            glance_metric_group,
            detail_metric_groups,
        })
    }

    fn provider_for_surface(
        projection: &UsageProjectionV1,
        surface: HostSurfaceId,
    ) -> Option<&UsageProviderV1> {
        projection
            .providers
            .iter()
            .find(|provider| provider.provider_id == surface.provider_id())
    }

    fn add_default_selections_for(
        projection: &UsageProjectionV1,
        surfaces: &BTreeSet<HostSurfaceId>,
        selections: &mut BTreeMap<HostSurfaceId, String>,
    ) {
        for surface in surfaces {
            if selections.contains_key(surface) {
                continue;
            }
            if let Some(account) = Self::provider_for_surface(projection, *surface)
                .and_then(|provider| provider.accounts.first())
            {
                selections.insert(*surface, account.canonical_account_id.clone());
            }
        }
    }

    fn persist_selections(
        &self,
        selections: &BTreeMap<HostSurfaceId, String>,
    ) -> Result<(), String> {
        let persisted = PersistedProjectionSelections {
            selected: selections
                .iter()
                .map(|(surface, account_id)| (surface.id().to_owned(), account_id.clone()))
                .collect(),
        };
        let contents = serde_json::to_string_pretty(&persisted)
            .map_err(|error| format!("encode projection selections: {error}"))?;
        atomic_write_usage_json(&projection_selections_path(&self.data_dir), &contents)
    }
}

fn validate_projection(projection: &UsageProjectionV1) -> Result<(), String> {
    projection.validate()?;
    let mut provider_ids = BTreeSet::new();
    let mut account_ids = BTreeSet::new();
    for provider in &projection.providers {
        if surface_for_provider_id(&provider.provider_id).is_none() {
            return Err(format!(
                "unknown provider in usage projection: {}",
                provider.provider_id
            ));
        }
        if !provider_ids.insert(provider.provider_id.as_str()) {
            return Err(format!(
                "duplicate provider in usage projection: {}",
                provider.provider_id
            ));
        }
        for account in &provider.accounts {
            if account.canonical_account_id.is_empty() {
                return Err("usage projection account has an empty canonical id".to_owned());
            }
            if !account_ids.insert(account.canonical_account_id.as_str()) {
                return Err(format!(
                    "duplicate canonical account id in usage projection: {}",
                    account.canonical_account_id
                ));
            }
        }
    }
    Ok(())
}

fn surface_for_provider_id(provider_id: &str) -> Option<HostSurfaceId> {
    HostSurfaceId::ALL
        .iter()
        .copied()
        .find(|surface| surface.provider_id() == provider_id)
}

fn enabled_surfaces(ids: &[String]) -> Result<BTreeSet<HostSurfaceId>, String> {
    if ids.is_empty() {
        return Ok(HostSurfaceId::ALL.iter().copied().collect());
    }
    let unknown = ids
        .iter()
        .filter(|id| HostSurfaceId::from_id(id).is_none())
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(format!(
            "unknown enabled surface ids: {}",
            unknown.join(", ")
        ));
    }
    Ok(ids
        .iter()
        .filter_map(|id| HostSurfaceId::from_id(id))
        .collect())
}

fn glance_metric_group(
    surface: HostSurfaceId,
    account: &UsageAccountV1,
) -> Option<&UsageMetricGroupV1> {
    let desired_period = if surface == HostSurfaceId::Amp {
        UsageCalendarPeriodV1::Daily
    } else {
        UsageCalendarPeriodV1::Weekly
    };
    account.metric_groups.iter().find(|group| {
        group.kind == UsageMetricGroupKindV1::Window
            && matches!(
                &group.value,
                UsageMetricValueV1::Window {
                    period: UsageMetricPeriodV1::Calendar { granularity },
                    ..
                } if *granularity == desired_period
            )
    })
}

fn projection_selections_path(data_dir: &Path) -> PathBuf {
    data_dir
        .join(HOST_USAGE_STATE_REL)
        .join(SELECTED_PROJECTION_ACCOUNTS_FILE)
}

fn load_projection_selections(data_dir: &Path) -> Result<BTreeMap<HostSurfaceId, String>, String> {
    let path = projection_selections_path(data_dir);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(format!("read projection selections: {error}")),
    };
    let persisted = serde_json::from_str::<PersistedProjectionSelections>(&contents)
        .map_err(|error| format!("decode projection selections: {error}"))?;
    let mut selections = BTreeMap::new();
    for (surface_id, canonical_account_id) in persisted.selected {
        let surface = HostSurfaceId::from_id(&surface_id)
            .ok_or_else(|| format!("unknown surface in projection selections: {surface_id}"))?;
        if canonical_account_id.is_empty() {
            return Err(format!(
                "empty canonical account id in projection selections for {surface_id}"
            ));
        }
        selections.insert(surface, canonical_account_id);
    }
    Ok(selections)
}
