// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn atomic_state_round_trip_uses_private_permissions_and_old_or_new_envelopes() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let first = completed(1_000, "first@example.test");
    store.store(&first, 1_000).unwrap();
    assert_eq!(store.load(&capability(), 1_000).unwrap(), Some(first));

    let mut second = completed(1_001, "second@example.test");
    second.generation = 2;
    store.store(&second, 1_001).unwrap();
    assert_eq!(store.load(&capability(), 1_001).unwrap(), Some(second));

    let directory = temp.path().join("accounts");
    let file = directory.join("claude-account-123.json");
    assert_eq!(fs::metadata(&directory).unwrap().mode() & 0o777, 0o700);
    let metadata = fs::metadata(file).unwrap();
    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert_eq!(metadata.uid(), geteuid().as_raw());
}

#[test]
fn account_state_v1_migration_retains_account_and_starts_conservative_attempt_floor() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let path = temp.path().join("accounts/claude-account-123.json");
    store
        .store(&completed(1_000, "existing@example.test"), 1_000)
        .unwrap();

    let mut legacy = serde_json::to_value(completed(1_000, "existing@example.test")).unwrap();
    legacy["schema_version"] = serde_json::json!(LEGACY_ACCOUNT_STATE_SCHEMA_VERSION);
    legacy
        .as_object_mut()
        .unwrap()
        .remove("provider_invoked_at_epoch");
    legacy
        .as_object_mut()
        .unwrap()
        .remove("reload_fence_required");
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    let migrated = store.load(&capability(), 2_000).unwrap().unwrap();
    assert_eq!(migrated.schema_version, ACCOUNT_STATE_SCHEMA_VERSION);
    assert_eq!(migrated.started_at_epoch, Some(1_000));
    assert_eq!(migrated.provider_invoked_at_epoch, None);
    assert!(migrated.reload_fence_required);
    assert_eq!(
        migrated.last_good.unwrap().account.account_label,
        "existing@example.test"
    );
    let durable: AccountStateEnvelope = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(durable.provider_invoked_at_epoch, None);
    assert!(durable.reload_fence_required);
}

#[test]
fn account_state_v1_migration_distinguishes_fresh_queue_from_ambiguous_attempts() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let account = capability();
    let path = temp.path().join("accounts/claude-account-123.json");
    let mut queued = AccountStateEnvelope::idle(account.clone());
    queued.schema_version = LEGACY_ACCOUNT_STATE_SCHEMA_VERSION;
    queued.generation = 1;
    queued.phase = UsageRefreshPhase::Queued;
    queued.started_at_epoch = Some(1_000);
    store.store(&queued, 1_000).unwrap();
    let mut legacy = serde_json::to_value(&queued).unwrap();
    legacy["schema_version"] = serde_json::json!(LEGACY_ACCOUNT_STATE_SCHEMA_VERSION);
    legacy
        .as_object_mut()
        .unwrap()
        .remove("provider_invoked_at_epoch");
    legacy
        .as_object_mut()
        .unwrap()
        .remove("reload_fence_required");
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    let migrated = store.load(&account, 1_000).unwrap().unwrap();
    assert_eq!(migrated.schema_version, ACCOUNT_STATE_SCHEMA_VERSION);
    assert_eq!(migrated.phase, UsageRefreshPhase::Queued);
    assert_eq!(migrated.provider_invoked_at_epoch, None);
    assert!(!migrated.reload_fence_required);

    let durable: AccountStateEnvelope = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(durable.schema_version, ACCOUNT_STATE_SCHEMA_VERSION);
    assert_eq!(durable.provider_invoked_at_epoch, None);
    assert!(!durable.reload_fence_required);

    let mut contradictory_queued = AccountStateEnvelope::idle(account.clone());
    contradictory_queued.generation = 1;
    contradictory_queued.phase = UsageRefreshPhase::Queued;
    contradictory_queued.started_at_epoch = Some(1_000);
    contradictory_queued.retry_deadline_epoch = Some(1_300);
    let mut legacy = serde_json::to_value(&contradictory_queued).unwrap();
    legacy["schema_version"] = serde_json::json!(LEGACY_ACCOUNT_STATE_SCHEMA_VERSION);
    legacy
        .as_object_mut()
        .unwrap()
        .remove("provider_invoked_at_epoch");
    legacy
        .as_object_mut()
        .unwrap()
        .remove("reload_fence_required");
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    let migrated = store.load(&account, 1_000).unwrap().unwrap();
    assert_eq!(migrated.phase, UsageRefreshPhase::Queued);
    assert_eq!(migrated.provider_invoked_at_epoch, None);
    assert!(
        migrated.reload_fence_required,
        "contradictory queued state with a retry deadline is ambiguous"
    );

    let mut unresolved = AccountStateEnvelope::idle(account.clone());
    unresolved.generation = 0;
    unresolved.phase = UsageRefreshPhase::Updating;
    let mut legacy = serde_json::to_value(&unresolved).unwrap();
    legacy["schema_version"] = serde_json::json!(LEGACY_ACCOUNT_STATE_SCHEMA_VERSION);
    legacy
        .as_object_mut()
        .unwrap()
        .remove("provider_invoked_at_epoch");
    legacy
        .as_object_mut()
        .unwrap()
        .remove("reload_fence_required");
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    let migrated = store.load(&account, 1_000).unwrap().unwrap();
    assert_eq!(migrated.phase, UsageRefreshPhase::Updating);
    assert_eq!(migrated.provider_invoked_at_epoch, None);
    assert!(
        migrated.reload_fence_required,
        "an unresolved Updating record is uncertain even when other fields conflict"
    );
}

#[test]
fn account_state_v2_migration_persists_unresolved_attempt_provenance_atomically() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let account = capability();
    let path = temp.path().join("accounts/claude-account-123.json");
    let mut updating = AccountStateEnvelope::idle(account.clone());
    updating.schema_version = PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION;
    updating.generation = 4;
    updating.phase = UsageRefreshPhase::Updating;
    updating.started_at_epoch = Some(1_000);
    store.store(&updating, 1_000).unwrap();

    let mut legacy = serde_json::to_value(&updating).unwrap();
    legacy["schema_version"] = serde_json::json!(PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION);
    legacy
        .as_object_mut()
        .unwrap()
        .remove("reload_fence_required");
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    let migrated = store.load(&account, 2_000).unwrap().unwrap();
    assert_eq!(migrated.schema_version, ACCOUNT_STATE_SCHEMA_VERSION);
    assert_eq!(migrated.phase, UsageRefreshPhase::Updating);
    assert_eq!(migrated.provider_invoked_at_epoch, None);
    assert!(migrated.reload_fence_required);

    let durable: AccountStateEnvelope = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(durable.schema_version, ACCOUNT_STATE_SCHEMA_VERSION);
    assert!(durable.reload_fence_required);
    assert_eq!(durable.provider_invoked_at_epoch, None);
}

#[test]
fn atomic_state_symlink_directory_is_rejected_without_touching_target() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
    let link = temp.path().join("accounts");
    symlink(&target, &link).unwrap();
    let store = FileAccountStateStore::at(&link);

    assert_eq!(
        store.store(&completed(1_000, "safe"), 1_000),
        Err(StateStoreError::Unavailable)
    );
    assert_eq!(fs::metadata(target).unwrap().mode() & 0o777, 0o755);
}

#[test]
fn atomic_state_symlink_file_is_not_followed() {
    let temp = tempfile::tempdir().unwrap();
    let accounts = temp.path().join("accounts");
    fs::create_dir(&accounts).unwrap();
    fs::set_permissions(&accounts, fs::Permissions::from_mode(0o700)).unwrap();
    let victim = temp.path().join("victim");
    fs::write(&victim, "unchanged").unwrap();
    symlink(&victim, accounts.join("claude-account-123.json")).unwrap();
    let store = FileAccountStateStore::at(accounts);

    assert_eq!(
        store.load(&capability(), 1_000),
        Err(StateStoreError::Unavailable)
    );
    assert_eq!(fs::read_to_string(victim).unwrap(), "unchanged");
}

#[test]
fn atomic_state_corrupt_schema_and_future_epoch_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    store.store(&completed(1_000, "safe"), 1_000).unwrap();
    let path = temp.path().join("accounts").join("claude-account-123.json");
    fs::write(&path, b"{not-json").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        store.load(&capability(), 1_000),
        Err(StateStoreError::Corrupt)
    );

    let mut future_schema = completed(1_000, "newer broker");
    future_schema.schema_version = ACCOUNT_STATE_SCHEMA_VERSION + 1;
    let future_bytes = serde_json::to_vec(&future_schema).unwrap();
    fs::write(&path, &future_bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        store.load(&capability(), 1_000),
        Err(StateStoreError::Corrupt)
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        future_bytes,
        "an older broker must leave unknown future-schema bytes untouched"
    );

    let mut future = completed(1_301, "safe");
    future.schema_version = ACCOUNT_STATE_SCHEMA_VERSION;
    let bytes = serde_json::to_vec(&future).unwrap();
    fs::write(&path, bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        store.load(&capability(), 1_000),
        Err(StateStoreError::Corrupt)
    );
}

#[test]
fn atomic_state_sanitizes_control_characters_and_clamps_display_fields() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let hostile = format!("{}\n\t", "x".repeat(MAX_DISPLAY_CHARS + 10));
    let envelope = completed(1_000, &hostile);

    store.store(&envelope, 1_000).unwrap();
    let loaded = store.load(&capability(), 1_000).unwrap().unwrap();
    let label = &loaded.last_good.unwrap().account.account_label;
    assert_eq!(label.chars().count(), MAX_DISPLAY_CHARS);
    assert!(!label.chars().any(char::is_control));
}

#[test]
fn projection_state_is_one_atomic_envelope_and_quarantines_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let envelope = ProjectionStateEnvelope {
        schema_version: ProjectionStateEnvelope::SCHEMA_VERSION,
        projection: empty_projection(),
        aliases: vec![ProjectionAlias {
            capability_id: "capability-1".into(),
            canonical_account_id: "account-1".into(),
        }],
        catalog_revision: "catalog-1".into(),
        catalog: Vec::new(),
        retry_deadline_epoch: Some(1_030),
        success_deadline_epoch: Some(1_300),
        broker_instance_id: "instance-1".into(),
    };
    store.store(&envelope).unwrap();
    assert_eq!(store.load().unwrap(), Some(envelope));
    let path = temp.path().join("usage-broker/projection.json");
    fs::write(&path, b"not-json").unwrap();
    assert_eq!(store.load(), Err(StateStoreError::Corrupt));
    assert!(!path.exists());
    assert!(
        fs::read_dir(path.parent().unwrap())
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
    );
}

#[test]
fn projection_v2_is_visible_only_to_the_broker_migration_loader() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let path = temp.path().join("usage-broker/projection.json");
    let legacy = ProjectionStateEnvelope {
        schema_version: ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION,
        projection: empty_projection(),
        aliases: Vec::new(),
        catalog_revision: "catalog-1".into(),
        catalog: Vec::new(),
        retry_deadline_epoch: Some(1_030),
        success_deadline_epoch: Some(1_300),
        broker_instance_id: "instance-1".into(),
    };
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    assert_eq!(
        store.load_for_broker_migration().unwrap(),
        Some(legacy.clone())
    );
    assert_eq!(store.store(&legacy), Err(StateStoreError::Corrupt));
    assert_eq!(
        store.load(),
        Err(StateStoreError::SchemaMigrationRequired {
            found: u64::from(ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION),
            current: ProjectionStateEnvelope::SCHEMA_VERSION,
        })
    );
    assert!(
        path.exists(),
        "ordinary reads must leave migration input intact"
    );

    let mut migrated = legacy;
    migrated.schema_version = ProjectionStateEnvelope::SCHEMA_VERSION;
    store.store(&migrated).unwrap();
    assert_eq!(store.load().unwrap(), Some(migrated));
}

#[test]
fn projection_store_preserves_valid_future_schema_for_a_newer_broker() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let path = temp.path().join("usage-broker/projection.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let future = serde_json::to_vec(&serde_json::json!({
        "schema_version": 4,
        "future_projection_payload": { "opaque": true }
    }))
    .unwrap();
    fs::write(&path, &future).unwrap();

    assert_eq!(
        store.load(),
        Err(StateStoreError::SchemaMigrationRequired {
            found: 4,
            current: ProjectionStateEnvelope::SCHEMA_VERSION,
        })
    );
    assert_eq!(fs::read(&path).unwrap(), future);
}

#[test]
fn projection_store_still_quarantines_corrupt_v2_contents() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let path = temp.path().join("usage-broker/projection.json");
    let mut legacy = serde_json::to_value(ProjectionStateEnvelope {
        schema_version: ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION,
        projection: empty_projection(),
        aliases: Vec::new(),
        catalog_revision: "catalog-1".into(),
        catalog: Vec::new(),
        retry_deadline_epoch: None,
        success_deadline_epoch: None,
        broker_instance_id: "instance-1".into(),
    })
    .unwrap();
    legacy["projection"]["broker_generation"] = serde_json::json!(-1);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    assert_eq!(
        store.load_for_broker_migration(),
        Err(StateStoreError::Corrupt)
    );
    assert!(!path.exists());
}

#[test]
fn projection_state_v1_is_quarantined_without_catalog_migration() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let path = temp.path().join("usage-broker/projection.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let legacy = serde_json::json!({
        "schema_version": 1,
        "projection": empty_projection(),
        "aliases": [],
        "catalog_revision": "catalog-1",
        "retry_deadline_epoch": null,
        "success_deadline_epoch": null,
        "broker_instance_id": "instance-1"
    });
    fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

    assert_eq!(store.load(), Err(StateStoreError::Corrupt));
    assert!(!path.exists());
    assert!(
        fs::read_dir(path.parent().unwrap())
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().contains("corrupt"))
    );
}
