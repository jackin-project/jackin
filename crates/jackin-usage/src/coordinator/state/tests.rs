// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::os::unix::fs::symlink;

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageProjectionRefreshStateV2, UsageProjectionSchemaV2,
    UsageProjectionV2, UsageRefreshPhase,
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
        count_quota: None,
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
        remaining_money: None,
        severity: UsageSeverity::Normal,
    }];
    view.last_error = None;
    view
}

fn completed(epoch: i64, label: &str) -> AccountStateEnvelope {
    let view = sanitize_usage_view(quota_view(epoch, label), &capability(), None);
    AccountStateEnvelope {
        schema_version: ACCOUNT_STATE_SCHEMA_VERSION,
        capability: capability(),
        accepted_catalog_entry: None,
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
fn validated_envelope_overrides_missing_and_forged_snapshot_identity() {
    let authority = capability();
    let mut envelope = completed(1_000, "same@example.test");
    envelope.terminal_result.as_mut().unwrap().account_identity = None;
    envelope.last_good.as_mut().unwrap().account_identity =
        Some(jackin_protocol::control::UsageAccountIdentity {
            account_id: "forged-account".into(),
            surface_id: "codex".into(),
            source_revision: Some("forged-revision".into()),
        });

    let validated = validate_envelope(envelope, &authority, 1_000).unwrap();
    for view in [validated.terminal_result, validated.last_good] {
        assert_eq!(view.unwrap().account_identity, Some((&authority).into()));
    }
}

#[test]
fn durable_envelope_binds_both_snapshots_to_canonical_authority() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let authority = capability();
    let mut envelope = completed(1_000, "");
    envelope.terminal_result.as_mut().unwrap().account_identity =
        Some(jackin_protocol::control::UsageAccountIdentity {
            account_id: "forged-account".into(),
            surface_id: "codex".into(),
            source_revision: Some("forged-revision".into()),
        });
    envelope.last_good.as_mut().unwrap().account_identity = None;

    store.store(&envelope, 1_000).unwrap();
    let loaded = store.load(&authority, 1_000).unwrap().unwrap();
    for view in [loaded.terminal_result, loaded.last_good] {
        let view = view.unwrap();
        assert_eq!(view.account_identity, Some((&authority).into()));
        assert!(view.account.account_label.is_empty());
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

fn empty_projection() -> UsageProjectionV2 {
    UsageProjectionV2 {
        schema_version: UsageProjectionSchemaV2,
        projection_id: "projection-1".into(),
        generated_at_epoch: 1_000,
        discovery_revision: "catalog-1".into(),
        broker_instance_id: "instance-1".into(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV2::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        unresolved_grants: Vec::new(),
        issues: Vec::new(),
    }
}

#[test]
fn projection_state_is_one_atomic_envelope_and_quarantines_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let envelope = ProjectionStateEnvelope {
        schema_version: PROJECTION_STATE_SCHEMA_VERSION,
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
fn projection_state_envelope_v1_is_quarantined_without_catalog_migration() {
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

#[test]
fn projection_state_envelope_v2_is_quarantined_without_migration() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileProjectionStateStore::under_data_dir(temp.path());
    let path = temp.path().join("usage-broker/projection.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let legacy = serde_json::json!({
        "schema_version": 2,
        "projection": empty_projection(),
        "aliases": [],
        "catalog_revision": "catalog-1",
        "catalog": [],
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

#[test]
fn durable_envelope_stamps_exact_catalog_proof_and_clears_anonymous_forgery() {
    use jackin_protocol::control::{UsageCanonicalAccountIdentity, UsageCanonicalAccountSubject};
    let temp = tempfile::tempdir().unwrap();
    let store = FileAccountStateStore::at(temp.path().join("accounts"));
    let proof = UsageCanonicalAccountIdentity {
        surface_id: capability().surface_id,
        subject: UsageCanonicalAccountSubject::ProviderId("accepted-provider-account".into()),
    };
    let forged = UsageCanonicalAccountIdentity {
        surface_id: "codex".into(),
        subject: UsageCanonicalAccountSubject::ProviderStableHandle("forged@example.test".into()),
    };
    let mut envelope = completed(1_000, "same-label");
    envelope.accepted_catalog_entry = Some(UsageCatalogEntry {
        capability: capability(),
        revision: "revision-1".into(),
        canonical_identity: Some(proof.clone()),
        provenance_count: 1,
    });
    for view in [&mut envelope.terminal_result, &mut envelope.last_good] {
        view.as_mut().unwrap().canonical_identity = Some(forged.clone());
        view.as_mut()
            .unwrap()
            .account_identity
            .as_mut()
            .unwrap()
            .source_revision = Some("forged-revision".into());
    }
    store.store(&envelope, 1_000).unwrap();
    let loaded = store.load(&capability(), 1_000).unwrap().unwrap();
    assert_eq!(
        loaded.accepted_catalog_entry,
        envelope.accepted_catalog_entry
    );
    for view in [&loaded.terminal_result, &loaded.last_good] {
        assert_eq!(
            view.as_ref()
                .unwrap()
                .account_identity
                .as_ref()
                .unwrap()
                .source_revision
                .as_deref(),
            Some("revision-1")
        );
    }
    assert_eq!(
        loaded.terminal_result.unwrap().canonical_identity,
        Some(proof.clone())
    );
    assert_eq!(loaded.last_good.unwrap().canonical_identity, Some(proof));
    envelope
        .accepted_catalog_entry
        .as_mut()
        .unwrap()
        .canonical_identity = None;
    store.store(&envelope, 1_000).unwrap();
    let loaded = store.load(&capability(), 1_000).unwrap().unwrap();
    assert!(loaded.terminal_result.unwrap().canonical_identity.is_none());
    assert!(loaded.last_good.unwrap().canonical_identity.is_none());
}

#[test]
fn envelope_rejects_catalog_evidence_for_another_route() {
    let mut envelope = completed(1_000, "fixture");
    let mut wrong = capability();
    wrong.account_id = "other-account".into();
    envelope.accepted_catalog_entry = Some(UsageCatalogEntry {
        capability: wrong,
        revision: "revision-1".into(),
        canonical_identity: None,
        provenance_count: 0,
    });
    assert_eq!(
        validate_envelope(envelope, &capability(), 1_000),
        Err(StateStoreError::Corrupt)
    );
}

#[test]
fn old_account_cache_is_quarantined_and_rebuilt_with_catalog_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("accounts");
    let store = FileAccountStateStore::at(root.clone());
    let envelope = completed(1_000, "old-cache");
    store.store(&envelope, 1_000).unwrap();
    let path = root.join(state_filename(&capability()));
    let mut old: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    old["schema_version"] = serde_json::json!(1);
    old.as_object_mut()
        .unwrap()
        .remove("accepted_catalog_entry");
    fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(store.load(&capability(), 1_000).unwrap(), None);
    assert!(!path.exists());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    let mut rebuilt = completed(1_001, "new-cache");
    rebuilt.accepted_catalog_entry = Some(UsageCatalogEntry {
        capability: capability(),
        revision: "current".into(),
        canonical_identity: None,
        provenance_count: 0,
    });
    store.store(&rebuilt, 1_001).unwrap();
    assert_eq!(store.load(&capability(), 1_001).unwrap(), Some(rebuilt));
    old["schema_version"] = serde_json::json!(99);
    fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(
        store.load(&capability(), 1_001),
        Err(StateStoreError::Corrupt)
    );
    assert!(path.exists());
}

#[test]
fn envelope_rejects_blank_catalog_proof_subjects() {
    use jackin_protocol::control::{UsageCanonicalAccountIdentity, UsageCanonicalAccountSubject};
    for subject in [
        UsageCanonicalAccountSubject::ProviderId("".into()),
        UsageCanonicalAccountSubject::ProviderStableHandle(" ".into()),
        UsageCanonicalAccountSubject::SourceCapability("\t".into()),
    ] {
        let mut envelope = completed(1_000, "fixture");
        envelope.accepted_catalog_entry = Some(UsageCatalogEntry {
            capability: capability(),
            revision: "revision-a".into(),
            provenance_count: 1,
            canonical_identity: Some(UsageCanonicalAccountIdentity {
                surface_id: capability().surface_id,
                subject,
            }),
        });
        assert_eq!(
            validate_envelope(envelope, &capability(), 1_000),
            Err(StateStoreError::Corrupt)
        );
    }
}

#[test]
fn envelope_rejects_authenticated_catalog_proof_without_provenance() {
    use jackin_protocol::control::{UsageCanonicalAccountIdentity, UsageCanonicalAccountSubject};
    let mut envelope = completed(1_000, "fixture");
    envelope.accepted_catalog_entry = Some(UsageCatalogEntry {
        capability: capability(),
        revision: "revision-a".into(),
        provenance_count: 0,
        canonical_identity: Some(UsageCanonicalAccountIdentity {
            surface_id: capability().surface_id,
            subject: UsageCanonicalAccountSubject::ProviderId("valid-id".into()),
        }),
    });
    assert_eq!(
        validate_envelope(envelope.clone(), &capability(), 1_000),
        Err(StateStoreError::Corrupt)
    );
    envelope
        .accepted_catalog_entry
        .as_mut()
        .unwrap()
        .provenance_count = 1;
    validate_envelope(envelope, &capability(), 1_000).unwrap();
}
