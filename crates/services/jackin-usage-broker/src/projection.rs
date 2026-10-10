// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker projection loading.

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
        // v1 and invalid envelopes are quarantined by the store. The broker
        // deliberately rebuilds an empty projection from current discovery.
        Err(jackin_usage_coordinator::StateStoreError::Corrupt) => None,
        Err(
            jackin_usage_coordinator::StateStoreError::Unavailable
            | jackin_usage_coordinator::StateStoreError::SchemaMigrationRequired { .. },
        ) => return Err(unavailable()),
    };
    let (envelope, catalog, catalog_revision) = match loaded {
        Some(mut envelope) => {
            if envelope.schema_version == ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION {
                migrate_legacy_projection(&mut envelope.projection).map_err(|_| unavailable())?;
                envelope.schema_version = ProjectionStateEnvelope::SCHEMA_VERSION;
            }
            let catalog = Some(envelope.catalog.clone());
            let revision = Some(envelope.catalog_revision.clone());
            (envelope, catalog, revision)
        }
        None => {
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
        }
    };
    store.store(&envelope).map_err(|_| unavailable())?;
    let projection = envelope.projection.clone();
    Ok(LoadedProjection {
        projection: Arc::new(Mutex::new(projection)),
        catalog,
        catalog_revision,
    })
}

/// Normalize the previous publication schema before its contents are exposed.
/// Old surface IDs are converted to canonical provider IDs, while every old
/// identity label is downgraded to unverified because schema v2 cannot prove
/// which values came from discovery versus a display-label fallback.
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
mod tests {
    use super::*;

    use jackin_usage_coordinator::FileProjectionStateStore;

    fn legacy_projection() -> UsageProjectionV1 {
        let mut projection: UsageProjectionV1 = serde_json::from_str(include_str!(
            "../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
        ))
        .unwrap();
        let provider = &mut projection.providers[0];
        provider.provider_id = "codex".to_owned();
        provider.display_name = "codex".to_owned();
        provider.accounts[0].identity_kind = UsageIdentityKindV1::ProviderAccountId;
        projection
    }

    fn legacy_envelope(projection: UsageProjectionV1) -> ProjectionStateEnvelope {
        ProjectionStateEnvelope {
            schema_version: ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION,
            catalog_revision: "catalog-1".to_owned(),
            catalog: Vec::new(),
            broker_instance_id: projection.broker_instance_id.clone(),
            projection,
            aliases: Vec::new(),
            retry_deadline_epoch: None,
            success_deadline_epoch: None,
        }
    }

    #[test]
    fn startup_migrates_projection_v2_before_exposing_identity() {
        let temp = tempfile::tempdir().unwrap();
        let config = UsageBrokerConfig::for_data_dir(temp.path().join("data"));
        let store = FileProjectionStateStore::under_data_dir(&config.data_dir);
        let legacy = legacy_envelope(legacy_projection());
        let path = config.data_dir.join("usage-broker/projection.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

        let loaded = load_projection(&config).unwrap();
        let projection = loaded.projection.lock().unwrap();
        assert_eq!(projection.providers[0].provider_id, "openai");
        assert_eq!(projection.providers[0].display_name, "OpenAI");
        assert_eq!(
            projection.providers[0].accounts[0].identity_kind,
            UsageIdentityKindV1::UnverifiedHandle
        );
        let persisted = store.load().unwrap().unwrap();
        assert_eq!(
            persisted.schema_version,
            ProjectionStateEnvelope::SCHEMA_VERSION
        );
        assert_eq!(persisted.projection, *projection);
    }

    #[test]
    fn unknown_legacy_provider_fails_without_replacing_projection_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let config = UsageBrokerConfig::for_data_dir(temp.path().join("data"));
        let mut projection = legacy_projection();
        projection.providers[0].provider_id = "unknown-provider".to_owned();
        let legacy = legacy_envelope(projection);
        let path = config.data_dir.join("usage-broker/projection.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = serde_json::to_vec(&legacy).unwrap();
        std::fs::write(&path, &bytes).unwrap();

        assert!(load_projection(&config).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}
