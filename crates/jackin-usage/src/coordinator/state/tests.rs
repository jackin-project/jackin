// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1,
    UsageProjectionV1, UsageRefreshPhase,
};

use super::*;

fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "account-123".into(),
        surface_id: "claude".into(),
    }
}

fn quota_view(epoch: i64, label: &str) -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("fixture", epoch);
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.account.provider_label = "Claude".into();
    view.account.account_label = label.into();
    view.buckets = vec![QuotaBucketView {
        label: "Session".into(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(72),
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];
    view.last_error = None;
    view
}

fn completed(epoch: i64, label: &str) -> AccountStateEnvelope {
    let view = quota_view(epoch, label);
    AccountStateEnvelope {
        schema_version: ACCOUNT_STATE_SCHEMA_VERSION,
        capability: capability(),
        generation: 1,
        phase: UsageRefreshPhase::Completed,
        terminal_result: Some(view.clone()),
        last_good: Some(view),
        terminal_error: None,
        started_at_epoch: Some(epoch),
        completed_at_epoch: Some(epoch),
        rate_limit_deadline_epoch: None,
        retry_deadline_epoch: None,
        success_deadline_epoch: Some(epoch + 300),
        consecutive_failures: 0,
    }
}

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

fn empty_projection() -> UsageProjectionV1 {
    UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "projection-1".into(),
        generated_at_epoch: 1_000,
        discovery_revision: "catalog-1".into(),
        broker_instance_id: "instance-1".into(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        issues: Vec::new(),
    }
}

#[test]
fn projection_state_is_one_atomic_envelope_and_quarantines_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let envelope = ProjectionStateEnvelope {
        schema_version: 1,
        projection: empty_projection(),
        aliases: vec![ProjectionAlias {
            capability_id: "capability-1".into(),
            canonical_account_id: "account-1".into(),
        }],
        catalog_revision: "catalog-1".into(),
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
