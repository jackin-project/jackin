// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_usage_coordinator::FileProjectionStateStore;

fn legacy_projection() -> UsageProjectionV1 {
    let mut projection: UsageProjectionV1 = serde_json::from_str(include_str!(
        "../../../jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
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
