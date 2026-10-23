// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{FocusedUsageView, UsageSource};
use jackin_protocol::usage_broker::UsageAccountCapability;
use std::collections::BTreeMap;
use std::fs;

#[test]
fn public_snapshot_reload_preserves_full_capability_keys_and_legacy_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let account_a = capability("account-A", "codex");
    let account_b = capability("account-B", "codex");
    let other_surface = capability("account-A", "claude");

    let mut a_cli = capability_view("Codex", "same@example.test", account_a.clone(), 12, 200);
    a_cli.source = UsageSource::Cli;
    store_usage_snapshot(&db, &a_cli).expect("store first bound A source");
    let b_api = capability_view("Codex", "same@example.test", account_b.clone(), 84, 220);
    store_usage_snapshot(&db, &b_api).expect("store same-label bound B");

    let mut renamed_a = a_cli.clone();
    renamed_a.account.account_label = "renamed@example.test".to_owned();
    renamed_a.fetched_at_epoch = 260;
    renamed_a.buckets[0].remaining_percent = Some(reported_remaining(25));
    store_usage_snapshot(&db, &renamed_a).expect("rename bound A without changing its key");

    let mut a_api = capability_view("Codex", "renamed@example.test", account_a.clone(), 26, 260);
    a_api.source = UsageSource::ProviderApi;
    store_usage_snapshot(&db, &a_api).expect("store newer preferred source for A");

    let legacy = provider_usage_view(
        "Codex",
        "renamed@example.test",
        Some("legacy plan"),
        "Session",
        31,
        260,
    );
    store_usage_snapshot(&db, &legacy).expect("store explicitly unbound legacy row");

    let claude = capability_view(
        "Claude",
        "renamed@example.test",
        other_surface.clone(),
        42,
        260,
    );
    store_usage_snapshot(&db, &claude).expect("same account id on another surface");

    let rows = stored_account_snapshots(&db).expect("load stored snapshot rows");
    assert_eq!(rows.len(), 5, "rename updates one source/window row");
    assert!(
        rows.iter()
            .filter(|row| row.account_capability == Some(account_a.clone()))
            .all(|row| row.account_key_hash == account_a.opaque_identity_key())
    );
    assert_eq!(
        rows.iter()
            .find(|row| row.account_capability.is_none())
            .unwrap()
            .account_key_hash,
        account_key_hash("Codex", "renamed@example.test")
    );

    let identities = list_account_identities(&db).expect("list capability identities");
    assert_eq!(identities.len(), 4);
    assert!(identities.iter().any(|identity| {
        identity.account_capability == Some(account_a.clone())
            && identity.account_label == "renamed@example.test"
    }));
    assert!(identities.iter().any(|identity| {
        identity.account_capability == Some(account_b.clone())
            && identity.account_label == "same@example.test"
    }));
    assert!(
        identities
            .iter()
            .any(|identity| { identity.account_capability == Some(other_surface.clone()) })
    );
    assert!(
        identities
            .iter()
            .any(|identity| identity.account_capability.is_none())
    );

    let views = load_all_account_usage_views(&db, 300).expect("reconstruct stored views");
    assert_eq!(views.len(), 4);
    let a_view = views
        .iter()
        .find(|stored| stored.account_key_hash == account_a.opaque_identity_key())
        .expect("bound A view");
    assert_eq!(a_view.view.source, UsageSource::ProviderApi);
    assert_eq!(
        a_view.view.account.account_capability,
        Some(account_a.clone())
    );
    assert_eq!(a_view.view.buckets, a_api.buckets);
    let b_view = views
        .iter()
        .find(|stored| stored.account_key_hash == account_b.opaque_identity_key())
        .expect("bound B view");
    assert_eq!(b_view.view.buckets, b_api.buckets);
    let other_surface_view = views
        .iter()
        .find(|stored| stored.account_key_hash == other_surface.opaque_identity_key())
        .expect("same account id on another surface");
    assert_eq!(other_surface_view.view.buckets, claude.buckets);
    let legacy_view = views
        .iter()
        .find(|stored| stored.view.account.account_capability.is_none())
        .expect("unbound legacy view");
    assert_eq!(legacy_view.view.buckets, legacy.buckets);
    let loaded_a = load_account_usage_view(&db, &account_a.opaque_identity_key(), 300)
        .expect("load A by opaque identity")
        .expect("A stored view");
    assert_eq!(loaded_a.account.account_capability, Some(account_a.clone()));
    assert_eq!(loaded_a.buckets, a_api.buckets);

    let production_views = views.iter().map(|stored| &stored.view).collect::<Vec<_>>();
    let tabs = jackin_usage_provider_core::provider_tabs(&production_views);
    assert_eq!(tabs.len(), 4);
    assert!(tabs.iter().any(|tab| {
        tab.id == account_a.opaque_identity_key()
            && tab.account_capability == Some(account_a.clone())
            && tab.account_label == "renamed@example.test"
    }));
    assert!(tabs.iter().any(|tab| {
        tab.id == account_b.opaque_identity_key()
            && tab.account_capability == Some(account_b.clone())
    }));
    assert!(tabs.iter().any(|tab| {
        tab.id == other_surface.opaque_identity_key()
            && tab.account_capability.as_ref() == Some(&other_surface)
    }));
    assert!(tabs.iter().any(|tab| tab.account_capability.is_none()));
}

#[test]
fn failed_snapshot_statement_rolls_back_and_reopened_cached_store_retries() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    store_usage_snapshot(&db, &usage_view()).expect("store initial snapshot");
    assert_eq!(schema_version(&db).unwrap().as_deref(), Some("6"));
    assert_eq!(connection_build_count(&db).unwrap(), 1);

    let before = stored_account_snapshots(&db).expect("load initial snapshot rows");
    assert_eq!(before.len(), 2);
    block_on_store(async {
        let conn = open_test_store(&db).await?;
        conn.execute(
            "ALTER TABLE account_usage_snapshots ADD COLUMN test_statement_guard INTEGER CHECK(window_kind <> 'Reject')",
            (),
        )
        .await
        .map_err(|error| error.to_string())?;
        Ok::<(), String>(())
    })
    .expect("install immediate statement-failure guard");

    let mut rejected = usage_view();
    rejected.fetched_at_epoch += 60;
    rejected.buckets[0].remaining_percent = Some(reported_remaining(21));
    rejected.buckets[1].label = "Reject".to_owned();
    let error = store_usage_snapshot(&db, &rejected).expect_err("later row violates CHECK");
    assert!(
        error.starts_with("upsert telemetry account snapshot failed: "),
        "{error}"
    );
    assert!(
        error.contains("CHECK constraint failed: window_kind <> 'Reject'"),
        "{error}"
    );
    assert_eq!(connection_build_count(&db).unwrap(), 1);

    let after_failure = stored_account_snapshots(&db).expect("read after statement rollback");
    assert_eq!(
        after_failure, before,
        "statement failure rolls back every row"
    );
    assert!(
        after_failure.iter().all(|row| row.window_kind != "Reject"),
        "failed later row must not be inserted"
    );
    assert_eq!(
        connection_build_count(&db).unwrap(),
        2,
        "read rebuilds the cached store removed by the failed write"
    );

    let mut retry = rejected.clone();
    retry.buckets[1].label = "Credits".to_owned();
    store_usage_snapshot(&db, &retry).expect("retry after changing only rejected label");
    let after_retry = stored_account_snapshots(&db).expect("read after successful retry");
    assert_eq!(after_retry.len(), before.len());
    assert!(after_retry.iter().all(|row| row.window_kind != "Reject"));
    let session = after_retry
        .iter()
        .find(|row| row.window_kind == "Session")
        .expect("updated session row");
    assert_eq!(session.quota_bucket.remaining_percent.unwrap().get(), 21.0);
    assert_eq!(session.fetched_at, rejected.fetched_at_epoch);
    assert_eq!(connection_build_count(&db).unwrap(), 2);
}

#[test]
fn invalid_capability_writes_fail_before_open_and_identity_codec_fails_closed() {
    let invalid = [
        ("Codex", capability("", "codex")),
        ("Codex", capability(&"a".repeat(129), "codex")),
        ("Codex", capability("account with space", "codex")),
        ("Codex", capability("account-A", "claude")),
        ("Unknown Provider", capability("account-A", "codex")),
    ];
    for (index, (provider, account_capability)) in invalid.into_iter().enumerate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join(format!("invalid-{index}.db"));
        let view = capability_view(
            provider,
            "same@example.test",
            account_capability,
            10,
            1_800_000_000,
        );
        let error = store_usage_snapshot(&db, &view).expect_err("invalid selector is rejected");
        assert!(
            error.contains("capability") || error.contains("surface") || error.contains("provider"),
            "{error}"
        );
        assert!(
            !db.exists(),
            "identity validation precedes opening the database"
        );
    }

    let valid = capability("account-A", "codex");
    let partial_pair_error = identity::decode_account_capability(
        "Codex",
        &valid.opaque_identity_key(),
        Some(valid.account_id.clone()),
        None,
    )
    .unwrap_err();
    assert_eq!(
        partial_pair_error,
        "snapshot account capability columns must be both NULL or both present"
    );
    let reserved_key_error = identity::decode_account_capability(
        "Codex",
        "capability-v1:legacy-without-pair",
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(
        reserved_key_error,
        "unbound snapshot account key uses the reserved capability namespace"
    );
    let historical_key =
        identity::decode_account_capability("Codex", "sha256:arbitrary-historical-key", None, None)
            .unwrap();
    assert_eq!(historical_key, None);
    assert!(
        identity::validate_account_identity(
            "Codex",
            &valid.opaque_identity_key(),
            Some(&capability("account-B", "codex")),
        )
        .is_err()
    );

    let opencode_dir = tempfile::tempdir().expect("tempdir");
    let opencode_db = opencode_dir.path().join("opencode.db");
    let opencode_capability = capability("opencode-account", "opencode");
    let opencode = capability_view(
        "OpenCode",
        "same@example.test",
        opencode_capability.clone(),
        50,
        1_800_000_000,
    );
    store_usage_snapshot(&opencode_db, &opencode)
        .expect("accept exact OpenCode provider and surface pair");
    let opencode_views = load_all_account_usage_views(&opencode_db, 1_800_000_060)
        .expect("load OpenCode-bound snapshot");
    assert_eq!(
        opencode_views[0].view.account.account_capability,
        Some(opencode_capability)
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("invalid-batch.db");
    let valid_view = capability_view(
        "Codex",
        "same@example.test",
        capability("valid-account", "codex"),
        50,
        1_800_000_000,
    );
    let mut invalid_view = valid_view.clone();
    invalid_view.account.account_capability = Some(capability("invalid space", "codex"));
    invalid_view.buckets.clear();
    assert!(store_usage_snapshots(&db, &[valid_view, invalid_view]).is_err());
    assert!(
        !db.exists(),
        "all input views validate before opening the store"
    );
}

#[test]
fn corrupted_persisted_identity_groups_fail_reads_without_rewriting_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let account = capability("account-A", "codex");
    let view = capability_view("Codex", "same@example.test", account.clone(), 37, 100);
    store_usage_snapshot(&db, &view).expect("store valid bound identity");

    block_on_store(async {
        let conn = open_store(&db).await?;
        conn.execute(
            "INSERT INTO account_usage_snapshots (
                provider, account_key_hash, account_label, source, confidence, window_kind,
                fetched_at, status, view_status, updated_label, status_bar_label,
                quota_bucket_json, capability_account_id, capability_surface_id
             )
             SELECT 'OpenAI', account_key_hash, account_label, 'cli', confidence, window_kind,
                    fetched_at, status, view_status, updated_label, status_bar_label,
                    quota_bucket_json, capability_account_id, capability_surface_id
             FROM account_usage_snapshots WHERE source = 'provider_api'",
            (),
        )
        .await
        .map_err(|error| error.to_string())?;
        Ok::<(), String>(())
    })
    .expect("insert individually valid provider alias under same opaque key");

    let error = load_all_account_usage_views(&db, 200)
        .expect_err("conflicting provider aliases under one key are invalid");
    assert!(
        error.contains("conflicting provider or capability"),
        "{error}"
    );

    block_on_store(async {
        let conn = open_test_store(&db).await?;
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM account_usage_snapshots
                 WHERE account_key_hash = ?1 AND provider IN ('Codex', 'OpenAI')",
                [account.opaque_identity_key().as_str()],
            )
            .await
            .map_err(|error| error.to_string())?;
        let row = rows
            .next()
            .await
            .map_err(|error| error.to_string())?
            .unwrap();
        let count = row.get::<i64>(0).map_err(|error| error.to_string())?;
        assert_eq!(count, 2, "failed identity reads do not discard source rows");
        Ok::<(), String>(())
    })
    .expect("verify both conflicting source rows remain stored");
}

#[test]
fn persisted_identity_pair_key_provider_and_identifier_corruption_fail_closed() {
    let corruptions = [
        (
            "partial_pair",
            "UPDATE account_usage_snapshots SET capability_surface_id = NULL",
            "both NULL or both present",
        ),
        (
            "nontext_pair_value",
            "UPDATE account_usage_snapshots SET capability_account_id = X'37'",
            "decode telemetry capability_account_id",
        ),
        (
            "invalid_identifier",
            "UPDATE account_usage_snapshots SET capability_account_id = 'bad id'",
            "identifier domain",
        ),
        (
            "provider_surface_mismatch",
            "UPDATE account_usage_snapshots SET provider = 'Claude'",
            "does not match provider",
        ),
        (
            "opaque_key_mismatch",
            "UPDATE account_usage_snapshots SET capability_account_id = 'account-B'",
            "does not match account_key_hash",
        ),
        (
            "reserved_unbound_namespace",
            "UPDATE account_usage_snapshots
             SET account_key_hash = 'capability-v1:historical',
                 capability_account_id = NULL, capability_surface_id = NULL",
            "reserved capability namespace",
        ),
    ];

    for (name, sql, expected_error) in corruptions {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join(format!("{name}.db"));
        let account = capability("account-A", "codex");
        let view = capability_view("Codex", "same@example.test", account, 37, 100);
        store_usage_snapshot(&db, &view).expect("store valid source row");
        block_on_store(async {
            let conn = open_store(&db).await?;
            conn.execute(sql, ())
                .await
                .map_err(|error| error.to_string())?;
            if name == "nontext_pair_value" {
                let mut rows = conn
                    .query(
                        "SELECT typeof(capability_account_id)
                         FROM account_usage_snapshots",
                        (),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                let row = rows
                    .next()
                    .await
                    .map_err(|error| error.to_string())?
                    .expect("corrupt identity row remains present");
                let stored_type: String = row.get(0).map_err(|error| error.to_string())?;
                assert_eq!(stored_type, "blob", "fixture must store an actual BLOB");
            }
            Ok::<(), String>(())
        })
        .expect("apply one synthetic persisted identity corruption");

        let error = load_all_account_usage_views(&db, 200)
            .expect_err("corrupt identity must fail before materializing a view");
        assert!(
            error.contains(expected_error),
            "{name} expected {expected_error:?}, got {error:?}"
        );
        block_on_store(async {
            let conn = open_test_store(&db).await?;
            let mut rows = conn
                .query("SELECT COUNT(*) FROM account_usage_snapshots", ())
                .await
                .map_err(|error| error.to_string())?;
            let row = rows
                .next()
                .await
                .map_err(|error| error.to_string())?
                .unwrap();
            let count = row.get::<i64>(0).map_err(|error| error.to_string())?;
            assert_eq!(count, 1, "failed reads preserve the historical source row");
            Ok::<(), String>(())
        })
        .expect("verify failed read did not delete source row");
    }
}

#[test]
fn capability_round_trip_is_proved_by_separate_writer_and_reader_processes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("process-snapshots.db");
    durable_process::run_durable_store_child("roundtrip_write", &db);
    durable_process::run_durable_store_child("roundtrip_read", &db);

    let identities = list_account_identities(&db).expect("parent reload after both children exit");
    assert_eq!(identities.len(), 2);
    assert_eq!(schema_version(&db).unwrap().as_deref(), Some("6"));
}

#[test]
fn recognized_schema5_shapes_preserve_typed_payload_bytes_and_restore_from_backup() {
    for mode in ["v5_fresh", "v5_nullable"] {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("schema5.db");
        let expected_buckets = v5_typed_buckets();
        let expected_json = v5_seed_payloads(&expected_buckets);
        seed_snapshot_in_finished_child(&db, mode);
        let original_sql = read_old_snapshot_projection(&db);
        assert_eq!(
            read_v5_payloads(&db),
            expected_v5_payload_rows(&expected_json)
        );
        let seeded_schema = schema5_fixture_snapshot(&db);
        let quota_json_not_null = seeded_schema
            .columns
            .iter()
            .find(|(name, _, _, _)| name == "quota_bucket_json")
            .expect("schema 5 quota JSON column")
            .2;
        assert_eq!(quota_json_not_null, mode == "v5_fresh");
        assert_eq!(
            seeded_schema
                .metadata
                .get("schema_version")
                .map(String::as_str),
            Some("5")
        );
        assert_eq!(
            seeded_schema
                .metadata
                .get("schema_version_before_quota_v5")
                .map(String::as_str),
            Some("4")
        );

        durable_process::run_durable_store_child("v5_migrate", &db);

        let rows = stored_account_snapshots(&db).expect("load migrated schema 6 rows");
        let expected_rows = expected_v5_stored_rows(&expected_buckets);
        assert_eq!(
            rows, expected_rows,
            "all typed and legacy scalar fields survive"
        );
        assert_eq!(read_old_snapshot_projection(&db), original_sql);
        assert_eq!(
            read_v5_payloads(&db),
            expected_v5_payload_rows(&expected_json)
        );
        assert_null_capability_pairs(&db);
        assert_schema6_metadata(&db, &seeded_schema.metadata);

        let backup_dir = committed_backup_directory(&db);
        let ready = read_manifest(&backup_dir.join("backup_ready.manifest"));
        let migrated = read_manifest(&backup_dir.join("migration_committed.manifest"));
        assert_eq!(
            ready.get("status").map(String::as_str),
            Some("backup_ready")
        );
        assert_eq!(
            ready.get("kind").map(String::as_str),
            Some("quota_snapshot")
        );
        assert_eq!(ready.get("old_version").map(String::as_str), Some("5"));
        assert_eq!(ready.get("new_version").map(String::as_str), Some("6"));
        assert_eq!(
            ready.get("backup_file").map(String::as_str),
            Some("rollback.sqlite")
        );
        assert_eq!(
            migrated.get("status").map(String::as_str),
            Some("migration_committed")
        );
        assert_eq!(migrated.get("new_version").map(String::as_str), Some("6"));

        let rollback_path = backup_dir.join("rollback.sqlite");
        let rollback_bytes = fs::read(&rollback_path)
            .expect("read immutable rollback artifact after migration child exits");
        let restore_db = dir.path().join("restored-copy.db");
        fs::copy(&rollback_path, &restore_db)
            .expect("copy validated immutable rollback artifact to a new restore path");
        let restored = stored_account_snapshots(&restore_db)
            .expect("public loader migrates restored backup copy");
        assert_eq!(restored, rows, "restore equals the migrated row vector");
        assert_eq!(
            restored, expected_rows,
            "restore preserves the complete typed rows"
        );
        assert_eq!(read_old_snapshot_projection(&restore_db), original_sql);
        assert_eq!(
            read_v5_payloads(&restore_db),
            expected_v5_payload_rows(&expected_json)
        );
        assert_null_capability_pairs(&restore_db);
        assert_schema6_metadata(&restore_db, &seeded_schema.metadata);
        assert_eq!(schema_version(&restore_db).unwrap().as_deref(), Some("6"));
        assert_eq!(
            fs::read(&rollback_path).expect("recheck immutable rollback artifact"),
            rollback_bytes,
            "opening the separate restore copy must preserve the rollback bytes"
        );
    }
}

#[test]
fn schema5_refusal_matrix_preserves_source_and_creates_no_migration_receipts() {
    let refusals = [
        (
            "v5_capability_only_no_payload",
            "schema 5 is missing quota_bucket_json",
        ),
        (
            "v5_capability_account_only",
            "schema 5 contains unsupported capability identity columns",
        ),
        (
            "v5_capability_surface_only",
            "schema 5 contains unsupported capability identity columns",
        ),
        (
            "v5_capability_both",
            "schema 5 contains unsupported capability identity columns",
        ),
        (
            "v5_null_payload",
            "snapshot row 1 is missing quota_bucket_json",
        ),
        (
            "v5_malformed_payload",
            "decode quota_bucket_json for snapshot row 1 failed",
        ),
        ("v5_wrong_type", "schema 5 column used_amount has type"),
        ("v5_missing_column", "schema 5 is missing plan_label"),
        (
            "v5_missing_key",
            "schema 5 is missing UNIQUE(provider, account_key_hash, source, window_kind)",
        ),
        (
            "v5_incorrect_key",
            "schema 5 is missing UNIQUE(provider, account_key_hash, source, window_kind)",
        ),
    ];

    for (mode, expected_error) in refusals {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("schema5-invalid.db");
        seed_snapshot_in_finished_child(&db, mode);
        let before = schema5_fixture_snapshot(&db);

        let open_error = block_on_store(async { open_store(&db).await.map(|_| ()) })
            .expect_err("invalid schema 5 shape must fail on open");
        assert!(
            open_error.contains(expected_error),
            "{mode} expected {expected_error:?}, got {open_error:?}"
        );
        let read_error = load_all_account_usage_views(&db, 1_800_000_100)
            .expect_err("invalid schema 5 shape must fail on public load");
        assert!(
            read_error.contains(expected_error),
            "{mode} load expected {expected_error:?}, got {read_error:?}"
        );
        let write_error = store_usage_snapshot(&db, &usage_view())
            .expect_err("invalid schema 5 shape must fail on public write");
        assert!(
            write_error.contains(expected_error),
            "{mode} write expected {expected_error:?}, got {write_error:?}"
        );

        assert_eq!(
            schema5_fixture_snapshot(&db),
            before,
            "{mode} must remain exact"
        );
        assert_no_migration_receipts(&db);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Schema5FixtureSnapshot {
    schema_objects: Vec<(String, String, String, Option<String>)>,
    columns: Vec<(String, String, bool, bool)>,
    metadata: BTreeMap<String, String>,
    rows: Vec<Vec<String>>,
}

fn schema5_fixture_snapshot(path: &std::path::Path) -> Schema5FixtureSnapshot {
    block_on_store(async {
        let conn = open_test_store(path).await?;
        let mut object_rows = conn
            .query(
                "SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name",
                (),
            )
            .await
            .map_err(|error| error.to_string())?;
        let mut schema_objects = Vec::new();
        while let Some(row) = object_rows
            .next()
            .await
            .map_err(|error| error.to_string())?
        {
            schema_objects.push((
                row.get::<String>(0).map_err(|error| error.to_string())?,
                row.get::<String>(1).map_err(|error| error.to_string())?,
                row.get::<String>(2).map_err(|error| error.to_string())?,
                row.get::<Option<String>>(3)
                    .map_err(|error| error.to_string())?,
            ));
        }

        let mut column_rows = conn
            .query("PRAGMA table_info(account_usage_snapshots)", ())
            .await
            .map_err(|error| error.to_string())?;
        let mut columns = Vec::new();
        while let Some(row) = column_rows
            .next()
            .await
            .map_err(|error| error.to_string())?
        {
            columns.push((
                row.get::<String>(1).map_err(|error| error.to_string())?,
                row.get::<String>(2).map_err(|error| error.to_string())?,
                row.get::<i64>(3).map_err(|error| error.to_string())? != 0,
                row.get::<i64>(5).map_err(|error| error.to_string())? != 0,
            ));
        }

        let mut metadata_rows = conn
            .query("SELECT key, value FROM _meta ORDER BY key", ())
            .await
            .map_err(|error| error.to_string())?;
        let mut metadata = BTreeMap::new();
        while let Some(row) = metadata_rows
            .next()
            .await
            .map_err(|error| error.to_string())?
        {
            metadata.insert(
                row.get::<String>(0).map_err(|error| error.to_string())?,
                row.get::<String>(1).map_err(|error| error.to_string())?,
            );
        }

        let expressions = columns
            .iter()
            .map(|(name, _, _, _)| {
                let identifier = format!("\"{}\"", name.replace('"', "\"\""));
                format!("quote({identifier}) || ':' || typeof({identifier})")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let mut data_rows = conn
            .query(
                &format!("SELECT {expressions} FROM account_usage_snapshots ORDER BY rowid"),
                (),
            )
            .await
            .map_err(|error| error.to_string())?;
        let mut rows = Vec::new();
        while let Some(row) = data_rows.next().await.map_err(|error| error.to_string())? {
            let mut values = Vec::with_capacity(columns.len());
            for index in 0..columns.len() {
                values.push(
                    row.get::<String>(index)
                        .map_err(|error| error.to_string())?,
                );
            }
            rows.push(values);
        }

        Ok::<_, String>(Schema5FixtureSnapshot {
            schema_objects,
            columns,
            metadata,
            rows,
        })
    })
    .expect("capture schema 5 state through managed connection")
}

fn assert_no_migration_receipts(db: &std::path::Path) {
    let parent = db
        .parent()
        .expect("schema 5 database has a parent directory");
    for entry in fs::read_dir(parent)
        .expect("scan sibling backup generations")
        .map(|entry| entry.expect("read sibling directory entry"))
    {
        let path = entry.path();
        let is_backup = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".jackin-quota-backup-"));
        if is_backup {
            assert!(
                !path.join("backup_ready.manifest").exists(),
                "preflight refusal must not create a backup_ready receipt"
            );
            assert!(
                !path.join("migration_committed.manifest").exists(),
                "preflight refusal must not create a migration_committed receipt"
            );
        }
    }
}

fn expected_v5_stored_rows(buckets: &[QuotaBucketView]) -> Vec<StoredAccountUsageSnapshot> {
    let session_bucket = buckets
        .iter()
        .find(|bucket| bucket.label == "Session")
        .expect("expected schema 5 Session bucket")
        .clone();
    let unlimited_bucket = buckets
        .iter()
        .find(|bucket| bucket.label == "Unlimited")
        .expect("expected schema 5 Unlimited bucket")
        .clone();
    let spend_bucket = buckets
        .iter()
        .find(|bucket| bucket.label == "Spend")
        .expect("expected schema 5 Spend bucket")
        .clone();
    let mut rows = vec![
        StoredAccountUsageSnapshot {
            provider: "Codex".to_owned(),
            account_key_hash: "legacy-bucket-key".to_owned(),
            account_capability: None,
            account_label: "historic@example.test".to_owned(),
            source: "provider_api".to_owned(),
            confidence: "authoritative".to_owned(),
            window_kind: "Session".to_owned(),
            used_amount: Some(i64::MAX),
            used_unit: Some("tokens".to_owned()),
            limit_amount: Some(i64::MIN),
            limit_unit: Some("tokens".to_owned()),
            resets_at: Some(1_900_000_000),
            fetched_at: 1_800_000_000,
            expires_at: Some(1_800_000_600),
            status: "stale".to_owned(),
            last_error: Some("historic scalar error".to_owned()),
            focused_provider: Some("OpenAI".to_owned()),
            plan_label: Some("Historic Pro".to_owned()),
            quota_bucket: session_bucket,
            used_label: Some("historic used".to_owned()),
            limit_label: Some("historic limit".to_owned()),
            reset_label: Some("historic reset".to_owned()),
            pace_label: Some("historic pace".to_owned()),
            view_status: "stale".to_owned(),
            updated_label: "historic update".to_owned(),
            status_bar_label: "historic status bar".to_owned(),
        },
        StoredAccountUsageSnapshot {
            provider: "Codex".to_owned(),
            account_key_hash: "legacy-bucket-key".to_owned(),
            account_capability: None,
            account_label: "historic@example.test".to_owned(),
            source: "provider_api".to_owned(),
            confidence: "authoritative".to_owned(),
            window_kind: "Unlimited".to_owned(),
            used_amount: None,
            used_unit: None,
            limit_amount: None,
            limit_unit: None,
            resets_at: None,
            fetched_at: 1_800_000_000,
            expires_at: None,
            status: "fresh".to_owned(),
            last_error: None,
            focused_provider: Some("OpenAI".to_owned()),
            plan_label: Some("Historic Pro".to_owned()),
            quota_bucket: unlimited_bucket,
            used_label: None,
            limit_label: None,
            reset_label: None,
            pace_label: None,
            view_status: "fresh".to_owned(),
            updated_label: "Updated just now".to_owned(),
            status_bar_label: "unlimited quota".to_owned(),
        },
    ];
    let mut spend_row = rows[0].clone();
    spend_row.window_kind = "Spend".to_owned();
    spend_row.quota_bucket = spend_bucket;
    rows.push(spend_row);
    rows.sort_by(|left, right| left.window_kind.cmp(&right.window_kind));
    rows
}

fn read_old_snapshot_projection(path: &std::path::Path) -> Vec<Vec<String>> {
    const COLUMNS: &[&str] = &[
        "id",
        "provider",
        "account_key_hash",
        "account_label",
        "source",
        "confidence",
        "window_kind",
        "used_amount",
        "used_unit",
        "limit_amount",
        "limit_unit",
        "resets_at",
        "fetched_at",
        "expires_at",
        "status",
        "last_error",
        "focused_provider",
        "plan_label",
        "remaining_percent",
        "used_label",
        "limit_label",
        "reset_label",
        "pace_label",
        "view_status",
        "updated_label",
        "status_bar_label",
        "quota_bucket_json",
    ];

    block_on_store(async {
        let conn = open_test_store(path).await?;
        let expressions = COLUMNS
            .iter()
            .map(|column| format!("quote(\"{column}\") || ':' || typeof(\"{column}\")"))
            .collect::<Vec<_>>()
            .join(", ");
        let mut rows = conn
            .query(
                &format!("SELECT {expressions} FROM account_usage_snapshots ORDER BY window_kind"),
                (),
            )
            .await
            .map_err(|error| error.to_string())?;
        let mut projection = Vec::new();
        while let Some(row) = rows.next().await.map_err(|error| error.to_string())? {
            let mut values = Vec::with_capacity(COLUMNS.len());
            for index in 0..COLUMNS.len() {
                values.push(
                    row.get::<String>(index)
                        .map_err(|error| error.to_string())?,
                );
            }
            projection.push(values);
        }
        Ok::<_, String>(projection)
    })
    .expect("read old schema 5 SQL projection through managed connection")
}

fn read_v5_payloads(path: &std::path::Path) -> Vec<(String, Option<String>)> {
    block_on_store(async {
        let conn = open_test_store(path).await?;
        let mut rows = conn
            .query(
                "SELECT window_kind, quota_bucket_json
                 FROM account_usage_snapshots ORDER BY window_kind",
                (),
            )
            .await
            .map_err(|error| error.to_string())?;
        let mut payloads = Vec::new();
        while let Some(row) = rows.next().await.map_err(|error| error.to_string())? {
            payloads.push((
                row.get::<String>(0).map_err(|error| error.to_string())?,
                row.get::<Option<String>>(1)
                    .map_err(|error| error.to_string())?,
            ));
        }
        Ok::<_, String>(payloads)
    })
    .expect("read exact schema 5 quota JSON through managed connection")
}

fn expected_v5_payload_rows(payloads: &[String]) -> Vec<(String, Option<String>)> {
    assert_eq!(
        payloads.len(),
        3,
        "schema 5 fixture has three exact payloads"
    );
    vec![
        ("Session".to_owned(), Some(payloads[0].clone())),
        ("Spend".to_owned(), Some(payloads[2].clone())),
        ("Unlimited".to_owned(), Some(payloads[1].clone())),
    ]
}

fn assert_null_capability_pairs(path: &std::path::Path) {
    let pairs = block_on_store(async {
        let conn = open_test_store(path).await?;
        let mut rows = conn
            .query(
                "SELECT window_kind, capability_account_id, capability_surface_id
                 FROM account_usage_snapshots ORDER BY window_kind",
                (),
            )
            .await
            .map_err(|error| error.to_string())?;
        let mut pairs = Vec::new();
        while let Some(row) = rows.next().await.map_err(|error| error.to_string())? {
            pairs.push((
                row.get::<String>(0).map_err(|error| error.to_string())?,
                row.get::<Option<String>>(1)
                    .map_err(|error| error.to_string())?,
                row.get::<Option<String>>(2)
                    .map_err(|error| error.to_string())?,
            ));
        }
        Ok::<_, String>(pairs)
    })
    .expect("read capability pairs through managed connection");
    assert_eq!(
        pairs,
        vec![
            ("Session".to_owned(), None, None),
            ("Spend".to_owned(), None, None),
            ("Unlimited".to_owned(), None, None),
        ]
    );
}

fn assert_schema6_metadata(path: &std::path::Path, prior_metadata: &BTreeMap<String, String>) {
    let metadata = read_metadata(path);
    assert_eq!(
        metadata.get("schema_version").map(String::as_str),
        Some("6")
    );
    for (key, value) in prior_metadata {
        if key != "schema_version" {
            assert_eq!(metadata.get(key), Some(value), "preserve metadata {key}");
        }
    }
    assert_eq!(
        metadata
            .get("schema_version_before_capability_v6")
            .map(String::as_str),
        Some("5")
    );
}

fn capability(account_id: &str, surface_id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: account_id.to_owned(),
        surface_id: surface_id.to_owned(),
    }
}

fn capability_view(
    provider: &str,
    account_label: &str,
    account_capability: UsageAccountCapability,
    remaining: u8,
    fetched_at_epoch: i64,
) -> FocusedUsageView {
    let mut view = provider_usage_view(
        provider,
        account_label,
        Some("Pro"),
        "Session",
        remaining,
        fetched_at_epoch,
    );
    view.account.account_capability = Some(account_capability);
    view
}

fn read_metadata(path: &std::path::Path) -> BTreeMap<String, String> {
    block_on_store(async {
        let conn = open_test_store(path).await?;
        let mut rows = conn
            .query("SELECT key, value FROM _meta ORDER BY key", ())
            .await
            .map_err(|error| error.to_string())?;
        let mut metadata = BTreeMap::new();
        while let Some(row) = rows.next().await.map_err(|error| error.to_string())? {
            let key = row.get::<String>(0).map_err(|error| error.to_string())?;
            let value = row.get::<String>(1).map_err(|error| error.to_string())?;
            metadata.insert(key, value);
        }
        Ok::<_, String>(metadata)
    })
    .expect("read schema metadata")
}

fn committed_backup_directory(db: &std::path::Path) -> std::path::PathBuf {
    let mut matches = fs::read_dir(db.parent().unwrap())
        .expect("scan sibling backup generations")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".jackin-quota-backup-"))
                && path.join("backup_ready.manifest").is_file()
                && path.join("migration_committed.manifest").is_file()
        })
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "one completed migration backup exists");
    matches.remove(0)
}

fn read_manifest(path: &std::path::Path) -> BTreeMap<String, String> {
    fs::read_to_string(path)
        .expect("read backup manifest")
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}
