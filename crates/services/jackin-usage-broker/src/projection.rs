// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use jackin_protocol::usage_broker::{
    UsageCatalogEntry, UsageCoordinationError, UsageIdentityKindV1, UsageProjectionRefreshStateV1,
    UsageProjectionSchemaV1, UsageProjectionV1,
};

use jackin_usage_coordinator::{FileProjectionStateStore, ProjectionStateEnvelope};
use jackin_usage_host_presentation::HostSurfaceId;

use crate::{UsageBrokerConfig, unavailable};

pub(crate) fn empty_projection(build_id: &str) -> UsageProjectionV1 {
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: format!("{build_id}:empty"),
        generated_at_epoch: chrono::Utc::now().timestamp(),
        discovery_revision: "empty".to_owned(),
        broker_instance_id: jackin_core::account_key_hash(
            "usage-broker-instance-v1",
            &format!("{}:{build_id}", std::process::id()),
        ),
        broker_generation: 0,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

pub(crate) struct LoadedProjection {
    pub(crate) projection: Arc<Mutex<UsageProjectionV1>>,
    pub(crate) catalog: Option<Vec<UsageCatalogEntry>>,
    pub(crate) catalog_revision: Option<String>,
}

pub(crate) fn load_projection(
    config: &UsageBrokerConfig,
) -> Result<LoadedProjection, UsageCoordinationError> {
    let store = FileProjectionStateStore::under_data_dir(&config.data_dir);
    let loaded = match store.load_for_broker_migration() {
        Ok(loaded) => loaded,
        Err(jackin_usage_coordinator::StateStoreError::Corrupt) => None,
        Err(
            jackin_usage_coordinator::StateStoreError::Unavailable
            | jackin_usage_coordinator::StateStoreError::SchemaMigrationRequired { .. },
        ) => return Err(unavailable()),
    };
    let (envelope, catalog, catalog_revision) = if let Some(mut envelope) = loaded {
        if envelope.schema_version == ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION {
            migrate_legacy_projection(&mut envelope.projection).map_err(|_| unavailable())?;
            envelope.schema_version = ProjectionStateEnvelope::SCHEMA_VERSION;
        }
        let catalog = Some(envelope.catalog.clone());
        let revision = Some(envelope.catalog_revision.clone());
        (envelope, catalog, revision)
    } else {
        let projection = empty_projection(&config.build_id);
        let envelope = ProjectionStateEnvelope {
            schema_version: ProjectionStateEnvelope::SCHEMA_VERSION,
            catalog_revision: projection.discovery_revision.clone(),
            catalog: Vec::new(),
            broker_instance_id: projection.broker_instance_id.clone(),
            projection,
            aliases: Vec::new(),
            retry_deadline_epoch: None,
            success_deadline_epoch: None,
        };
        (envelope, None, None)
    };
    store.store(&envelope).map_err(|_| unavailable())?;
    let projection = envelope.projection.clone();
    Ok(LoadedProjection {
        projection: Arc::new(Mutex::new(projection)),
        catalog,
        catalog_revision,
    })
}

fn migrate_legacy_projection(projection: &mut UsageProjectionV1) -> Result<(), String> {
    let mut provider_ids = BTreeSet::new();
    for provider in &mut projection.providers {
        let previous_id = provider.provider_id.clone();
        let surface = surface_for_stored_provider_id(&previous_id).ok_or_else(|| {
            format!("unknown provider ID in legacy usage projection: {previous_id}")
        })?;
        provider.provider_id = surface.provider_id().to_owned();
        if provider.display_name == previous_id {
            provider.display_name = surface.label().to_owned();
        }
        for account in &mut provider.accounts {
            account.identity_kind = UsageIdentityKindV1::UnverifiedHandle;
        }
        if !provider_ids.insert(provider.provider_id.clone()) {
            return Err(format!(
                "duplicate canonical provider in legacy usage projection: {}",
                provider.provider_id
            ));
        }
    }
    for unresolved in &mut projection.unresolved {
        let previous_id = unresolved.provider_id.clone();
        let surface = surface_for_stored_provider_id(&previous_id).ok_or_else(|| {
            format!("unknown provider ID in legacy unresolved row: {previous_id}")
        })?;
        unresolved.provider_id = surface.provider_id().to_owned();
    }
    projection.providers.sort_by(|left, right| {
        provider_order_rank(&left.provider_id).cmp(&provider_order_rank(&right.provider_id))
    });
    for (provider_rank, provider) in projection.providers.iter_mut().enumerate() {
        provider.rank = u32::try_from(provider_rank).unwrap_or(u32::MAX);
        for (account_rank, account) in provider.accounts.iter_mut().enumerate() {
            account.rank = u32::try_from(account_rank).unwrap_or(u32::MAX);
        }
    }
    projection.unresolved.sort_by(|left, right| {
        provider_order_rank(&left.provider_id)
            .cmp(&provider_order_rank(&right.provider_id))
            .then_with(|| left.provider_id.cmp(&right.provider_id))
            .then_with(|| left.capability_id.cmp(&right.capability_id))
    });
    projection.validate()
}

fn surface_for_stored_provider_id(provider_id: &str) -> Option<HostSurfaceId> {
    HostSurfaceId::from_id(provider_id).or_else(|| {
        HostSurfaceId::ALL
            .iter()
            .copied()
            .find(|surface| surface.provider_id() == provider_id)
    })
}

fn provider_order_rank(provider_id: &str) -> usize {
    HostSurfaceId::ALL
        .iter()
        .position(|surface| surface.provider_id() == provider_id)
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests;
