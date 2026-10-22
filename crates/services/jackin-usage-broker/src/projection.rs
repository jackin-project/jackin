// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use jackin_protocol::control::{PercentObservationOrigin, RawPercentPoints, UsageMeterSide};
use jackin_protocol::usage_broker::{
    UsageCatalogEntry, UsageCoordinationError, UsageIdentityKindV1, UsageProjectionRefreshStateV1,
    UsageProjectionSchemaV2, UsageProjectionV1,
};

use jackin_usage_coordinator::{FileProjectionStateStore, ProjectionStateEnvelope};
use jackin_usage_host_presentation::HostSurfaceId;

use crate::{UsageBrokerConfig, unavailable};

pub(crate) fn empty_projection(build_id: &str) -> UsageProjectionV1 {
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV2,
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
        Err(
            jackin_usage_coordinator::StateStoreError::Corrupt
            | jackin_usage_coordinator::StateStoreError::Unavailable
            | jackin_usage_coordinator::StateStoreError::SchemaMigrationRequired { .. },
        ) => return Err(unavailable()),
    };
    let (envelope, catalog, catalog_revision, migrated) = if let Some(value) = loaded {
        let schema_version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|version| u32::try_from(version).ok())
            .ok_or_else(unavailable)?;
        if schema_version == ProjectionStateEnvelope::SCHEMA_VERSION {
            let envelope: ProjectionStateEnvelope =
                serde_json::from_value(value).map_err(|_| unavailable())?;
            envelope.projection.validate().map_err(|_| unavailable())?;
            let catalog = Some(envelope.catalog.clone());
            let revision = Some(envelope.catalog_revision.clone());
            (envelope, catalog, revision, false)
        } else if schema_version == ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION
            || schema_version == ProjectionStateEnvelope::OLDER_MIGRATABLE_SCHEMA_VERSION
        {
            let envelope =
                migrate_projection_envelope(value, schema_version).map_err(|_| unavailable())?;
            let catalog = Some(envelope.catalog.clone());
            let revision = Some(envelope.catalog_revision.clone());
            (envelope, catalog, revision, true)
        } else {
            return Err(unavailable());
        }
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
        (envelope, None, None, false)
    };
    if migrated {
        store.store_migrated(&envelope).map_err(|_| unavailable())?;
    } else {
        store.store(&envelope).map_err(|_| unavailable())?;
    }
    let projection = envelope.projection.clone();
    Ok(LoadedProjection {
        projection: Arc::new(Mutex::new(projection)),
        catalog,
        catalog_revision,
    })
}

fn migrate_projection_envelope(
    mut value: serde_json::Value,
    previous_schema: u32,
) -> Result<ProjectionStateEnvelope, String> {
    migrate_projection_precision(&mut value)?;
    let mut envelope: ProjectionStateEnvelope = serde_json::from_value(value)
        .map_err(|error| format!("decode migrated usage projection envelope failed: {error}"))?;
    if previous_schema == ProjectionStateEnvelope::OLDER_MIGRATABLE_SCHEMA_VERSION {
        migrate_legacy_projection_identity(&mut envelope.projection)?;
    }
    envelope.schema_version = ProjectionStateEnvelope::SCHEMA_VERSION;
    envelope
        .projection
        .validate()
        .map_err(|error| format!("validate migrated usage projection failed: {error}"))?;
    Ok(envelope)
}

fn migrate_projection_precision(value: &mut serde_json::Value) -> Result<(), String> {
    let envelope = value
        .as_object_mut()
        .ok_or_else(|| "projection state envelope is not an object".to_owned())?;
    let projection = envelope
        .get_mut("projection")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| "projection state is missing its projection object".to_owned())?;
    if projection
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        return Err("legacy projection public schema is not version 1".to_owned());
    }
    let providers = projection
        .get_mut("providers")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| "legacy projection providers are malformed".to_owned())?;
    for provider in providers {
        let provider = provider
            .as_object_mut()
            .ok_or_else(|| "legacy projection provider is malformed".to_owned())?;
        let accounts = provider
            .get_mut("accounts")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(|| "legacy projection accounts are malformed".to_owned())?;
        for account in accounts {
            let account = account
                .as_object_mut()
                .ok_or_else(|| "legacy projection account is malformed".to_owned())?;
            let windows = account
                .get_mut("windows")
                .and_then(serde_json::Value::as_array_mut)
                .ok_or_else(|| "legacy projection windows are malformed".to_owned())?;
            for window in windows {
                migrate_percent_window(window)?;
            }
            if let Some(groups) = account.get_mut("metric_groups") {
                let groups = groups
                    .as_array_mut()
                    .ok_or_else(|| "legacy projection metric groups are malformed".to_owned())?;
                for group in groups {
                    migrate_projection_metric_group(group)?;
                }
            }
        }
    }
    projection.insert("schema_version".to_owned(), serde_json::json!(2));
    Ok(())
}

fn migrate_projection_metric_group(group: &mut serde_json::Value) -> Result<(), String> {
    let group = group
        .as_object_mut()
        .ok_or_else(|| "legacy projection metric group is malformed".to_owned())?;
    let Some(value) = group.get_mut("value") else {
        return Err("legacy projection metric group has no value".to_owned());
    };
    if value.get("type").and_then(serde_json::Value::as_str) == Some("window") {
        migrate_percent_window(value)?;
    }
    Ok(())
}

fn migrate_percent_window(value: &mut serde_json::Value) -> Result<(), String> {
    let window = value
        .as_object_mut()
        .ok_or_else(|| "legacy projection window is malformed".to_owned())?;
    if [
        "remaining_percent_origin",
        "used_percent_origin",
        "meter_percent",
        "meter_side",
        "count_quota",
        "money_used_percent",
        "provider_quota_state",
    ]
    .iter()
    .any(|field| window.contains_key(*field))
    {
        return Err("legacy projection window contains partial precision fields".to_owned());
    }
    let remaining_display = window
        .remove("remaining_percent")
        .unwrap_or(serde_json::Value::Null);
    let used_display = window
        .remove("used_percent")
        .unwrap_or(serde_json::Value::Null);
    let remaining_raw = window
        .remove("remaining_raw_percent")
        .unwrap_or(serde_json::Value::Null);
    let used_raw = window
        .remove("used_raw_percent")
        .unwrap_or(serde_json::Value::Null);
    let remaining_raw = migrate_raw_percent(remaining_raw, "remaining")?;
    let used_raw = migrate_raw_percent(used_raw, "used")?;
    let remaining_selected = migrate_display_percent(remaining_display, "remaining")?;
    let used_selected = migrate_display_percent(used_display, "used")?;
    if remaining_selected.is_some() && used_selected.is_some() {
        return Err("legacy projection window selected both meter sides".to_owned());
    }
    let (meter_percent, meter_side) = if let Some(value) = used_selected {
        (Some(value), Some(UsageMeterSide::Used))
    } else if let Some(value) = remaining_selected {
        (Some(value), Some(UsageMeterSide::Remaining))
    } else {
        (None, None)
    };
    let historical = serde_json::to_value(PercentObservationOrigin::HistoricalUnknown)
        .map_err(|error| error.to_string())?;
    window.insert("remaining_percent".to_owned(), remaining_raw);
    window.insert(
        "remaining_percent_origin".to_owned(),
        if window["remaining_percent"].is_null() {
            serde_json::Value::Null
        } else {
            historical.clone()
        },
    );
    window.insert("used_percent".to_owned(), used_raw);
    window.insert(
        "used_percent_origin".to_owned(),
        if window["used_percent"].is_null() {
            serde_json::Value::Null
        } else {
            historical
        },
    );
    window.insert(
        "meter_percent".to_owned(),
        meter_percent.unwrap_or(serde_json::Value::Null),
    );
    window.insert(
        "meter_side".to_owned(),
        meter_side
            .map(|side| serde_json::to_value(side).map_err(|error| error.to_string()))
            .transpose()?
            .unwrap_or(serde_json::Value::Null),
    );
    window.insert("count_quota".to_owned(), serde_json::Value::Null);
    window.insert("money_used_percent".to_owned(), serde_json::Value::Null);
    window.insert("provider_quota_state".to_owned(), serde_json::Value::Null);
    Ok(())
}

fn migrate_raw_percent(raw: serde_json::Value, side: &str) -> Result<serde_json::Value, String> {
    if raw.is_null() {
        return Ok(serde_json::Value::Null);
    }
    let integer = raw
        .as_i64()
        .ok_or_else(|| format!("legacy {side} raw percent is not a signed integer"))?;
    let integer = i32::try_from(integer)
        .map_err(|_| format!("legacy {side} raw percent exceeds its historical i32 range"))?;
    let raw = RawPercentPoints::new(f64::from(integer))?;
    serde_json::Number::from_f64(raw.get())
        .map(serde_json::Value::Number)
        .ok_or_else(|| format!("legacy {side} raw percent is not representable"))
}

fn migrate_display_percent(
    display: serde_json::Value,
    side: &str,
) -> Result<Option<serde_json::Value>, String> {
    if display.is_null() {
        return Ok(None);
    }
    let value = display
        .as_u64()
        .filter(|value| *value <= 100)
        .ok_or_else(|| format!("legacy {side} selected percent is outside 0..=100"))?;
    Ok(Some(serde_json::json!(value as f64)))
}

fn migrate_legacy_projection_identity(projection: &mut UsageProjectionV1) -> Result<(), String> {
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
