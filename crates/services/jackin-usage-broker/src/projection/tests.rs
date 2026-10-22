// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_usage_coordinator::FileProjectionStateStore;

fn projection_json(schema_version: u32, legacy_provider_id: bool) -> serde_json::Value {
    let mut projection: serde_json::Value = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
    ))
    .unwrap();
    if legacy_provider_id {
        projection["providers"][0]["provider_id"] = serde_json::json!("codex");
        projection["providers"][0]["display_name"] = serde_json::json!("codex");
    }
    projection["providers"][0]["accounts"][0]["identity_kind"] =
        serde_json::json!("provider_account_id");
    serde_json::json!({
        "schema_version": schema_version,
        "projection": projection,
        "aliases": [],
        "catalog_revision": "catalog-1",
        "catalog": [],
        "retry_deadline_epoch": null,
        "success_deadline_epoch": null,
        "broker_instance_id": "broker-incarnation-3"
    })
}

#[test]
fn startup_migrates_projection_v2_before_exposing_identity() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().join("data"));
    let store = FileProjectionStateStore::under_data_dir(&config.data_dir);
    let legacy = projection_json(
        ProjectionStateEnvelope::OLDER_MIGRATABLE_SCHEMA_VERSION,
        true,
    );
    let path = config.data_dir.join("usage-broker/projection.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let bytes = serde_json::to_vec(&legacy).unwrap();
    std::fs::write(&path, &bytes).unwrap();

    let loaded = load_projection(&config).unwrap();
    let projection = loaded.projection.lock().unwrap();
    assert_eq!(projection.providers[0].provider_id, "openai");
    assert_eq!(projection.providers[0].display_name, "OpenAI");
    assert_eq!(
        projection.providers[0].accounts[0].identity_kind,
        UsageIdentityKindV1::UnverifiedHandle
    );
    let migrated_window = &projection.providers[0].accounts[0].windows[0];
    assert!(
        migrated_window.remaining_percent.is_none(),
        "migration must not claim selected display geometry was a raw observation"
    );
    assert_eq!(migrated_window.meter_percent.unwrap().get(), 57.0);
    assert_eq!(migrated_window.meter_side, Some(UsageMeterSide::Remaining));
    let persisted = store.load().unwrap().unwrap();
    assert_eq!(
        persisted.schema_version,
        ProjectionStateEnvelope::SCHEMA_VERSION
    );
    assert_eq!(persisted.projection, *projection);
    assert_eq!(
        std::fs::read(
            config
                .data_dir
                .join("usage-broker/.projection.json.pre-quota-v4.backup")
        )
        .unwrap(),
        bytes
    );
}

#[test]
fn unknown_legacy_provider_fails_without_replacing_projection_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().join("data"));
    let mut legacy = projection_json(
        ProjectionStateEnvelope::OLDER_MIGRATABLE_SCHEMA_VERSION,
        true,
    );
    legacy["projection"]["providers"][0]["provider_id"] = serde_json::json!("unknown-provider");
    let path = config.data_dir.join("usage-broker/projection.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let bytes = serde_json::to_vec(&legacy).unwrap();
    std::fs::write(&path, &bytes).unwrap();

    assert!(load_projection(&config).is_err());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn corrupt_current_projection_keeps_live_preimage_and_fails_across_restarts() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().join("data"));
    let path = config.data_dir.join("usage-broker/projection.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let bytes = serde_json::to_vec(&projection_json(
        ProjectionStateEnvelope::SCHEMA_VERSION,
        false,
    ))
    .unwrap();
    std::fs::write(&path, &bytes).unwrap();

    for attempt in 1..=3 {
        assert!(
            load_projection(&config).is_err(),
            "corrupt projection must fail on startup attempt {attempt}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn malformed_projection_syntax_keeps_live_preimage_and_fails_across_restarts() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().join("data"));
    let path = config.data_dir.join("usage-broker/projection.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let bytes = b"{ malformed durable projection";
    std::fs::write(&path, bytes).unwrap();

    for attempt in 1..=3 {
        assert!(
            load_projection(&config).is_err(),
            "malformed projection must fail on startup attempt {attempt}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn unsupported_historical_projection_shape_keeps_live_preimage_and_fails_across_restarts() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().join("data"));
    let path = config.data_dir.join("usage-broker/projection.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let legacy = projection_json(1, false);
    let bytes = serde_json::to_vec(&legacy).unwrap();
    std::fs::write(&path, &bytes).unwrap();

    for attempt in 1..=3 {
        assert!(
            load_projection(&config).is_err(),
            "unsupported historical projection must fail on startup attempt {attempt}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn projection_v3_migration_preserves_raw_values_and_historical_selected_geometry() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().join("data"));
    let mut legacy = projection_json(ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION, false);
    let old_window = &mut legacy["projection"]["providers"][0]["accounts"][0]["windows"][0];
    old_window["remaining_percent"] = serde_json::json!(0);
    old_window["remaining_raw_percent"] = serde_json::json!(-5);
    old_window["limit_label"] = serde_json::json!("Unlimited");
    let path = config.data_dir.join("usage-broker/projection.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    let loaded = load_projection(&config).unwrap();
    let projection = loaded.projection.lock().unwrap();
    let account = &projection.providers[0].accounts[0];
    let window = &account.windows[0];
    assert_eq!(window.remaining_percent.unwrap().get(), -5.0);
    assert_eq!(
        window.remaining_percent_origin,
        Some(PercentObservationOrigin::HistoricalUnknown)
    );
    assert_eq!(window.meter_percent.unwrap().get(), 0.0);
    assert_eq!(window.meter_side, Some(UsageMeterSide::Remaining));
    assert_eq!(
        account.identity_kind,
        UsageIdentityKindV1::ProviderAccountId
    );
    assert!(window.provider_quota_state.is_none());
    assert!(window.money_used_percent.is_none());
}

#[test]
fn projection_v3_migration_keeps_raw_evidence_independent_of_selected_geometry() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().join("data"));
    let mut legacy = projection_json(ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION, false);
    let old_window = &mut legacy["projection"]["providers"][0]["accounts"][0]["windows"][0];
    old_window["remaining_percent"] = serde_json::json!(10);
    old_window["remaining_raw_percent"] = serde_json::json!(57);
    let path = config.data_dir.join("usage-broker/projection.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    let loaded = load_projection(&config).unwrap();
    let projection = loaded.projection.lock().unwrap();
    let window = &projection.providers[0].accounts[0].windows[0];
    assert_eq!(window.remaining_percent.unwrap().get(), 57.0);
    assert_eq!(
        window.remaining_percent_origin,
        Some(PercentObservationOrigin::HistoricalUnknown)
    );
    assert_eq!(window.meter_percent.unwrap().get(), 10.0);
    assert_eq!(window.meter_side, Some(UsageMeterSide::Remaining));
}

#[test]
fn projection_v3_migration_preserves_raw_evidence_without_selected_geometry() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().join("data"));
    let mut legacy = projection_json(ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION, false);
    let old_window = &mut legacy["projection"]["providers"][0]["accounts"][0]["windows"][0];
    old_window["remaining_percent"] = serde_json::Value::Null;
    old_window["remaining_raw_percent"] = serde_json::json!(57);
    let path = config.data_dir.join("usage-broker/projection.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    let loaded = load_projection(&config).unwrap();
    let projection = loaded.projection.lock().unwrap();
    let window = &projection.providers[0].accounts[0].windows[0];
    assert_eq!(window.remaining_percent.unwrap().get(), 57.0);
    assert_eq!(
        window.remaining_percent_origin,
        Some(PercentObservationOrigin::HistoricalUnknown)
    );
    assert!(window.meter_percent.is_none());
    assert!(window.meter_side.is_none());
}
