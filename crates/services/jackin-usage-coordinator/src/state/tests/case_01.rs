// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

use jackin_protocol::control::{
    CanonicalRemainingPercent, CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
    Money, PercentObservationOrigin,
};

#[test]
fn atomic_state_round_trip_uses_private_permissions_and_old_or_new_envelopes() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let first = completed(1_000, "first@example.test");
    assert!(
        first
            .terminal_result
            .as_ref()
            .unwrap()
            .account
            .account_capability
            .is_none()
    );
    assert!(
        first
            .last_good
            .as_ref()
            .unwrap()
            .account
            .account_capability
            .is_none()
    );
    store.store(&first, 1_000).unwrap();
    let mut expected_first = first.clone();
    let first_capability = expected_first.capability.clone();
    for result in [
        &mut expected_first.terminal_result,
        &mut expected_first.last_good,
    ]
    .into_iter()
    .flatten()
    {
        result.account.account_capability = Some(first_capability.clone());
    }
    assert_eq!(
        store.load(&capability(), 1_000).unwrap(),
        Some(expected_first)
    );

    let mut second = completed(1_001, "second@example.test");
    second.generation = 2;
    assert!(
        second
            .terminal_result
            .as_ref()
            .unwrap()
            .account
            .account_capability
            .is_none()
    );
    assert!(
        second
            .last_good
            .as_ref()
            .unwrap()
            .account
            .account_capability
            .is_none()
    );
    store.store(&second, 1_001).unwrap();
    let mut expected_second = second.clone();
    let second_capability = expected_second.capability.clone();
    for result in [
        &mut expected_second.terminal_result,
        &mut expected_second.last_good,
    ]
    .into_iter()
    .flatten()
    {
        result.account.account_capability = Some(second_capability.clone());
    }
    assert_eq!(
        store.load(&capability(), 1_001).unwrap(),
        Some(expected_second)
    );

    let directory = temp.path().join("accounts");
    let file = directory.join("claude-account-123.json");
    assert_eq!(fs::metadata(&directory).unwrap().mode() & 0o777, 0o700);
    let metadata = fs::metadata(file).unwrap();
    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert_eq!(metadata.uid(), geteuid().as_raw());
}

#[test]
fn account_state_round_trips_exact_percentages_in_both_result_slots() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let adjacent = f64::from_bits(1.0_f64.to_bits() + 1);
    let mut state = completed(1_000, "precision@example.test");
    let terminal = state.terminal_result.as_mut().unwrap();
    terminal.buckets[0].remaining_percent = Some(CanonicalRemainingPercent::new(adjacent).unwrap());
    terminal.buckets[0].remaining_percent_origin = Some(PercentObservationOrigin::Derived);
    let last_good = state.last_good.as_mut().unwrap();
    last_good.buckets[0].remaining_percent =
        Some(CanonicalRemainingPercent::new(57.125_000_000_000_01).unwrap());
    last_good.buckets[0].remaining_percent_origin =
        Some(PercentObservationOrigin::ProviderReported);

    store.store(&state, 1_000).unwrap();
    let loaded = store.load(&capability(), 1_000).unwrap().unwrap();
    let terminal = &loaded.terminal_result.unwrap().buckets[0];
    assert_eq!(terminal.remaining_percent.unwrap().get(), adjacent);
    assert_eq!(
        terminal.remaining_percent_origin,
        Some(PercentObservationOrigin::Derived)
    );
    let last_good = &loaded.last_good.unwrap().buckets[0];
    assert_eq!(
        last_good.remaining_percent.unwrap().get(),
        57.125_000_000_000_01
    );
    assert_eq!(
        last_good.remaining_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
}

#[derive(Clone, Copy)]
enum InvalidQuotaPayload {
    MissingOrigin,
    MissingValue,
    CountAndMoney,
}

fn corrupt_quota_bucket(
    value: &mut serde_json::Value,
    result_name: &str,
    corruption: InvalidQuotaPayload,
) {
    let bucket = &mut value[result_name]["buckets"][0];
    match corruption {
        InvalidQuotaPayload::MissingOrigin => {
            bucket
                .as_object_mut()
                .unwrap()
                .remove("remaining_percent_origin");
        }
        InvalidQuotaPayload::MissingValue => {
            bucket.as_object_mut().unwrap().remove("remaining_percent");
        }
        InvalidQuotaPayload::CountAndMoney => {
            bucket["count_quota"] = serde_json::to_value(CountQuota::new(
                Some(1),
                Some(2),
                Some(1),
                CountQuotaUnit::Requests,
                CountQuotaPeriod::UtcDaily,
                CountQuotaProvenance::ProviderReported,
            ))
            .unwrap();
            bucket["used_money"] = serde_json::to_value(Money::new(1, "USD", 2)).unwrap();
        }
    }
}

#[test]
fn account_state_rejects_invalid_quota_authority_on_load_and_store_without_rewriting() {
    let cases = [
        ("terminal_result", InvalidQuotaPayload::MissingOrigin),
        ("terminal_result", InvalidQuotaPayload::MissingValue),
        ("terminal_result", InvalidQuotaPayload::CountAndMoney),
        ("last_good", InvalidQuotaPayload::MissingOrigin),
        ("last_good", InvalidQuotaPayload::MissingValue),
        ("last_good", InvalidQuotaPayload::CountAndMoney),
    ];

    for (result_name, corruption) in cases {
        let temp = tempfile::tempdir().unwrap();
        let store = FileAccountStateStore::at(temp.path().join("accounts"));
        let valid = completed(1_000, "preserved@example.test");
        store.store(&valid, 1_000).unwrap();
        let path = temp.path().join("accounts/claude-account-123.json");
        let valid_bytes = fs::read(&path).unwrap();

        let mut invalid_value = serde_json::to_value(&valid).unwrap();
        corrupt_quota_bucket(&mut invalid_value, result_name, corruption);
        let invalid: AccountStateEnvelope = serde_json::from_value(invalid_value.clone()).unwrap();
        assert_eq!(store.store(&invalid, 1_000), Err(StateStoreError::Corrupt));
        assert_eq!(fs::read(&path).unwrap(), valid_bytes);

        let invalid_bytes = serde_json::to_vec(&invalid_value).unwrap();
        fs::write(&path, &invalid_bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            store.load(&capability(), 1_000),
            Err(StateStoreError::Corrupt)
        );
        assert_eq!(fs::read(&path).unwrap(), invalid_bytes);

        let restarted_store = FileAccountStateStore::at(temp.path().join("accounts"));
        assert_eq!(
            restarted_store.load(&capability(), 1_000),
            Err(StateStoreError::Corrupt),
            "corrupt authority must fail closed after reopening the store"
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            invalid_bytes,
            "failed loads must retain the live corrupt preimage"
        );
        assert!(
            path.exists(),
            "corruption must not become a missing-state success"
        );
    }
}

#[test]
fn account_state_rejects_conflicting_embedded_selectors_and_upgrades_legacy_none() {
    let expected = capability();
    let conflicting = UsageAccountCapability {
        account_id: "different-account".to_owned(),
        surface_id: expected.surface_id.clone(),
    };
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let path = temp.path().join("accounts/claude-account-123.json");
    let mut legacy = serde_json::to_value(completed(1_000, "legacy@example.test")).unwrap();
    legacy["terminal_result"]["account"]
        .as_object_mut()
        .unwrap()
        .remove("account_capability");
    legacy["last_good"]["account"]
        .as_object_mut()
        .unwrap()
        .remove("account_capability");
    let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(&path, &legacy_bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    let loaded = store.load(&expected, 1_000).unwrap().unwrap();
    assert_eq!(
        loaded
            .terminal_result
            .as_ref()
            .and_then(|view| view.account.account_capability.as_ref()),
        Some(&expected)
    );
    assert_eq!(
        loaded
            .last_good
            .as_ref()
            .and_then(|view| view.account.account_capability.as_ref()),
        Some(&expected)
    );
    assert_eq!(fs::read(&path).unwrap(), legacy_bytes);

    for result_name in ["terminal_result", "last_good"] {
        let mut conflicting_value =
            serde_json::to_value(completed(1_000, "conflict@example.test")).unwrap();
        conflicting_value[result_name]["account"]["account_capability"] =
            serde_json::to_value(&conflicting).unwrap();
        let bytes = serde_json::to_vec(&conflicting_value).unwrap();
        fs::write(&path, &bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        assert_eq!(
            store.load(&expected, 1_000),
            Err(StateStoreError::Corrupt),
            "a conflicting selector in {result_name} must fail closed"
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn account_state_v1_migration_retains_account_and_starts_conservative_attempt_floor() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let path = temp.path().join("accounts/claude-account-123.json");
    store
        .store(&completed(1_000, "existing@example.test"), 1_000)
        .unwrap();

    let mut legacy = legacy_account_state(
        &completed(1_000, "existing@example.test"),
        LEGACY_ACCOUNT_STATE_SCHEMA_VERSION,
    );
    legacy
        .as_object_mut()
        .unwrap()
        .remove("provider_invoked_at_epoch");
    legacy
        .as_object_mut()
        .unwrap()
        .remove("reload_fence_required");
    let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
    fs::write(&path, &legacy_bytes).unwrap();
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
    let backup = temp
        .path()
        .join("accounts/.claude-account-123.json.pre-quota-v4.backup");
    assert_eq!(fs::read(backup).unwrap(), legacy_bytes);
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
    let mut legacy = legacy_account_state(&queued, LEGACY_ACCOUNT_STATE_SCHEMA_VERSION);
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
    let mut legacy =
        legacy_account_state(&contradictory_queued, LEGACY_ACCOUNT_STATE_SCHEMA_VERSION);
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
    let mut legacy = legacy_account_state(&unresolved, LEGACY_ACCOUNT_STATE_SCHEMA_VERSION);
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
    updating.schema_version = OLDER_ACCOUNT_STATE_SCHEMA_VERSION;
    updating.generation = 4;
    updating.phase = UsageRefreshPhase::Updating;
    updating.started_at_epoch = Some(1_000);
    store.store(&updating, 1_000).unwrap();

    let mut legacy = legacy_account_state(&updating, OLDER_ACCOUNT_STATE_SCHEMA_VERSION);
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
fn account_state_v3_migration_preserves_all_view_fields_and_backups_exact_preimage() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let path = temp.path().join("accounts/claude-account-123.json");
    let mut previous = completed(1_000, "precision@example.test");
    for result in [&mut previous.terminal_result, &mut previous.last_good]
        .into_iter()
        .flatten()
    {
        result.buckets[0].limit_label = Some("Unlimited".to_owned());
        result.buckets[0].provider_quota_state = None;
    }
    let legacy = legacy_account_state(&previous, PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION);
    let bytes = serde_json::to_vec(&legacy).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let existing_backup = temp
        .path()
        .join("accounts/.claude-account-123.json.pre-quota-v4.backup");
    let preserved_backup = b"previous durable backup";
    fs::write(&existing_backup, preserved_backup).unwrap();
    fs::set_permissions(&existing_backup, fs::Permissions::from_mode(0o600)).unwrap();

    let migrated = store.load(&capability(), 2_000).unwrap().unwrap();
    assert_eq!(migrated.schema_version, ACCOUNT_STATE_SCHEMA_VERSION);
    assert_eq!(migrated.generation, previous.generation);
    assert_eq!(migrated.retry_deadline_epoch, previous.retry_deadline_epoch);
    for result in [&migrated.terminal_result, &migrated.last_good]
        .into_iter()
        .flatten()
    {
        let bucket = &result.buckets[0];
        assert_eq!(bucket.remaining_percent.unwrap().get(), 72.0);
        assert_eq!(
            bucket.remaining_percent_origin,
            Some(PercentObservationOrigin::HistoricalUnknown)
        );
        assert!(bucket.used_percent.is_none());
        assert!(bucket.count_quota.is_none());
        assert_eq!(bucket.limit_label.as_deref(), Some("Unlimited"));
        assert!(bucket.provider_quota_state.is_none());
    }
    let mut expected = previous.clone();
    expected.schema_version = ACCOUNT_STATE_SCHEMA_VERSION;
    let expected_capability = expected.capability.clone();
    for result in [&mut expected.terminal_result, &mut expected.last_good]
        .into_iter()
        .flatten()
    {
        result.account.account_capability = Some(expected_capability.clone());
        for bucket in &mut result.buckets {
            bucket.used_percent = None;
            bucket.used_percent_origin = None;
            bucket.remaining_percent_origin = bucket
                .remaining_percent
                .map(|_| PercentObservationOrigin::HistoricalUnknown);
            bucket.count_quota = None;
        }
    }
    assert_eq!(migrated, expected);
    assert_eq!(fs::read(&existing_backup).unwrap(), preserved_backup);
    let migration_backup = fs::read_dir(existing_backup.parent().unwrap())
        .unwrap()
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".claude-account-123.json.pre-quota-v4.")
                && entry.file_name().to_string_lossy()
                    != ".claude-account-123.json.pre-quota-v4.backup"
        })
        .expect("migration writes a separate backup when the stable name already exists");
    assert_eq!(fs::read(migration_backup.path()).unwrap(), bytes);
}

#[test]
fn account_state_v3_migration_rejects_i64_max_percent_without_rewriting() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let path = temp.path().join("accounts/claude-account-123.json");
    let mut legacy = legacy_account_state(
        &completed(1_000, "precision@example.test"),
        PREVIOUS_ACCOUNT_STATE_SCHEMA_VERSION,
    );
    for result_name in ["terminal_result", "last_good"] {
        legacy[result_name]["buckets"][0]["remaining_percent"] = serde_json::json!(i64::MAX);
    }
    let bytes = serde_json::to_vec(&legacy).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    assert_eq!(
        store.load(&capability(), 1_000),
        Err(StateStoreError::Corrupt)
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);

    let restarted_store = FileAccountStateStore::at(temp.path().join("accounts"));
    assert_eq!(
        restarted_store.load(&capability(), 1_000),
        Err(StateStoreError::Corrupt),
        "an out-of-domain legacy integer remains corrupt on a fresh store open"
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
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
fn projection_state_is_one_atomic_envelope_and_keeps_corruption_fail_closed() {
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
    let corrupt = b"not-json";
    fs::write(&path, corrupt).unwrap();
    assert_eq!(store.load(), Err(StateStoreError::Corrupt));
    assert_eq!(fs::read(&path).unwrap(), corrupt);
    assert_eq!(store.load(), Err(StateStoreError::Corrupt));
    assert_eq!(fs::read(&path).unwrap(), corrupt);
}

#[test]
fn projection_v3_is_raw_only_for_broker_migration_and_keeps_exact_backup() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let path = temp.path().join("usage-broker/projection.json");
    let current = ProjectionStateEnvelope {
        schema_version: ProjectionStateEnvelope::SCHEMA_VERSION,
        projection: empty_projection(),
        aliases: Vec::new(),
        catalog_revision: "catalog-1".into(),
        catalog: Vec::new(),
        retry_deadline_epoch: Some(1_030),
        success_deadline_epoch: Some(1_300),
        broker_instance_id: "instance-1".into(),
    };
    let mut legacy = serde_json::to_value(&current).unwrap();
    legacy["schema_version"] =
        serde_json::json!(ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION);
    legacy["projection"]["schema_version"] = serde_json::json!(1);
    let bytes = serde_json::to_vec(&legacy).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, &bytes).unwrap();

    assert_eq!(store.load_for_broker_migration().unwrap(), Some(legacy));
    let mut wrong_schema = current.clone();
    wrong_schema.schema_version = ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION;
    assert_eq!(store.store(&wrong_schema), Err(StateStoreError::Corrupt));
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

    store.store_migrated(&current).unwrap();
    assert_eq!(store.load().unwrap(), Some(current));
    assert_eq!(
        fs::read(
            temp.path()
                .join("usage-broker/.projection.json.pre-quota-v4.backup")
        )
        .unwrap(),
        bytes
    );
}

#[test]
fn projection_store_preserves_valid_future_schema_for_a_newer_broker() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let path = temp.path().join("usage-broker/projection.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let future = serde_json::to_vec(&serde_json::json!({
        "schema_version": 5,
        "future_projection_payload": { "opaque": true }
    }))
    .unwrap();
    fs::write(&path, &future).unwrap();

    assert_eq!(
        store.load(),
        Err(StateStoreError::SchemaMigrationRequired {
            found: 5,
            current: ProjectionStateEnvelope::SCHEMA_VERSION,
        })
    );
    assert_eq!(fs::read(&path).unwrap(), future);
}

#[test]
fn projection_store_keeps_corrupt_current_contents_without_sanitizing_them() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let path = temp.path().join("usage-broker/projection.json");
    let mut legacy = serde_json::to_value(ProjectionStateEnvelope {
        schema_version: ProjectionStateEnvelope::SCHEMA_VERSION,
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
    let bytes = serde_json::to_vec(&legacy).unwrap();
    fs::write(&path, &bytes).unwrap();

    assert_eq!(
        store.load_for_broker_migration(),
        Err(StateStoreError::Corrupt)
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(
        store.load_for_broker_migration(),
        Err(StateStoreError::Corrupt)
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn projection_state_v1_is_retained_without_catalog_migration() {
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
    let bytes = serde_json::to_vec(&legacy).unwrap();
    fs::write(&path, &bytes).unwrap();

    assert_eq!(store.load(), Err(StateStoreError::Corrupt));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(store.load(), Err(StateStoreError::Corrupt));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}
