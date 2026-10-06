// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Broker projection loading.

use std::sync::{Arc, Mutex};

use jackin_protocol::usage_broker::{
    UsageCatalogEntry, UsageCoordinationError, UsageProjectionRefreshStateV1,
    UsageProjectionSchemaV1, UsageProjectionV1,
};

use crate::coordinator::{FileProjectionStateStore, ProjectionStateEnvelope};

use super::{UsageBrokerConfig, unavailable};

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
    let loaded = match store.load() {
        Ok(loaded) => loaded,
        // v1 and invalid v2 envelopes are quarantined by the store. The
        // broker deliberately rebuilds an empty projection from current
        // discovery; unavailable state is not safe to overwrite.
        Err(crate::coordinator::StateStoreError::Corrupt) => None,
        Err(crate::coordinator::StateStoreError::Unavailable) => return Err(unavailable()),
    };
    let projection = loaded.as_ref().map_or_else(
        || empty_projection(&config.build_id),
        |envelope| envelope.projection.clone(),
    );
    let catalog = loaded.as_ref().map(|envelope| envelope.catalog.clone());
    let catalog_revision = loaded
        .as_ref()
        .map(|envelope| envelope.catalog_revision.clone());
    let envelope_catalog_revision = catalog_revision
        .clone()
        .unwrap_or_else(|| projection.discovery_revision.clone());
    let envelope = ProjectionStateEnvelope {
        schema_version: 2,
        catalog_revision: envelope_catalog_revision,
        catalog: catalog.clone().unwrap_or_default(),
        broker_instance_id: projection.broker_instance_id.clone(),
        projection: projection.clone(),
        aliases: Vec::new(),
        retry_deadline_epoch: None,
        success_deadline_epoch: None,
    };
    store.store(&envelope).map_err(|_| unavailable())?;
    Ok(LoadedProjection {
        projection: Arc::new(Mutex::new(projection)),
        catalog,
        catalog_revision,
    })
}
