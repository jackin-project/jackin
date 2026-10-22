// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{CanonicalRemainingPercent, PercentObservationOrigin};

#[test]
fn account_snapshot_rows_are_persisted_and_upserted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");

    store_usage_snapshot(&db, &usage_view()).expect("store first snapshot");
    let mut changed = usage_view();
    changed.buckets[0].remaining_percent = Some(CanonicalRemainingPercent::new(25.125).unwrap());
    changed.fetched_at_epoch += 60;
    store_usage_snapshot(&db, &changed).expect("store updated snapshot");

    let rows = stored_account_snapshots(&db).expect("read snapshots");
    assert_eq!(rows.len(), 2);
    let session = rows
        .iter()
        .find(|row| row.window_kind == "Session")
        .expect("session row");
    assert_eq!(session.provider, "Codex");
    assert!(session.account_key_hash.starts_with("sha256:"));
    assert_eq!(session.source, "cli");
    assert_eq!(session.confidence, "authoritative");
    assert_eq!(session.used_amount, None);
    assert_eq!(session.used_unit, None);
    assert_eq!(session.limit_amount, None);
    assert_eq!(session.status, "fresh");
    assert_eq!(session.fetched_at, 1_781_185_620);
    assert_eq!(
        session.quota_bucket.remaining_percent.unwrap().get(),
        25.125
    );
    assert_eq!(session.used_label.as_deref(), Some("63% used"));
    assert_eq!(session.limit_label.as_deref(), Some("100%"));
    assert_eq!(session.plan_label.as_deref(), Some("Pro 20x"));
}

#[test]
fn canon_durable_reconstruction_pins_one_preferred_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let mut cli = usage_view();
    cli.source = UsageSource::Cli;
    cli.buckets = vec![QuotaBucketView {
        label: "Weekly".to_owned(),
        remaining_percent: Some(reported_remaining(12)),
        ..cli.buckets[0].clone()
    }];
    let mut provider_api = cli.clone();
    provider_api.source = UsageSource::ProviderApi;
    provider_api.buckets[0].remaining_percent = Some(reported_remaining(88));
    store_usage_snapshot(&db, &cli).expect("store CLI");
    store_usage_snapshot(&db, &provider_api).expect("store provider API");

    let views =
        load_all_account_usage_views(&db, provider_api.fetched_at_epoch).expect("materialize once");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].view.source, UsageSource::ProviderApi);
    assert_eq!(views[0].view.buckets.len(), 1);
    assert_eq!(
        views[0].view.buckets[0].remaining_percent.unwrap().get(),
        88.0
    );
}

#[test]
fn repeated_writes_reuse_cached_connection() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");

    store_usage_snapshot(&db, &usage_view()).expect("store first snapshot");
    assert_eq!(
        connection_build_count(&db).expect("first build count"),
        1,
        "first write should build one connection"
    );

    let mut changed = usage_view();
    changed.fetched_at_epoch += 60;
    changed.buckets[0].remaining_percent = Some(CanonicalRemainingPercent::new(21.75).unwrap());
    store_usage_snapshot(&db, &changed).expect("store second snapshot");

    assert_eq!(
        connection_build_count(&db).expect("second build count"),
        1,
        "second write should reuse the cached connection"
    );
    let rows = stored_account_snapshots(&db).expect("read snapshots");
    let session = rows
        .iter()
        .find(|row| row.window_kind == "Session")
        .expect("session row");
    assert_eq!(session.quota_bucket.remaining_percent.unwrap().get(), 21.75);
}

#[test]
fn schema_v4_migration_preserves_old_scalars_and_marks_percent_history_unknown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    seed_snapshot_in_finished_child(&db, "v4_full");

    block_on_store(async {
        let _conn = open_store(&db).await?;
        Ok::<(), String>(())
    })
    .expect("migrate schema v4");

    let rows = stored_account_snapshots(&db).expect("read migrated rows");
    let legacy = rows
        .iter()
        .find(|r| r.account_key_hash == "sha256:legacy")
        .expect("legacy row survived the migration");
    assert_eq!(legacy.view_status, "stale");
    assert_eq!(legacy.updated_label, "historic updated");
    assert_eq!(legacy.status_bar_label, "historic status bar");
    assert_eq!(legacy.plan_label.as_deref(), Some("historic plan"));
    assert_eq!(legacy.quota_bucket.remaining_percent.unwrap().get(), 57.0);
    assert_eq!(
        legacy.quota_bucket.remaining_percent_origin,
        Some(PercentObservationOrigin::HistoricalUnknown)
    );
    assert!(legacy.quota_bucket.used_percent.is_none());
    assert!(legacy.quota_bucket.count_quota.is_none());
    assert!(legacy.quota_bucket.provider_quota_state.is_none());
    assert_eq!(legacy.used_amount, Some(i64::MAX));
    assert_eq!(legacy.used_unit.as_deref(), Some("tokens"));
    assert_eq!(legacy.limit_amount, Some(-i64::MAX));
    assert_eq!(legacy.limit_unit.as_deref(), Some("tokens"));
    assert_eq!(legacy.resets_at, Some(12345));
    assert_eq!(legacy.expires_at, Some(200));
    assert_eq!(legacy.last_error.as_deref(), Some("historic error"));
    assert_eq!(legacy.used_label.as_deref(), Some("historic used"));
    assert_eq!(legacy.limit_label.as_deref(), Some("Unlimited"));
    assert_eq!(legacy.reset_label.as_deref(), Some("historic reset"));
    assert_eq!(legacy.pace_label.as_deref(), Some("historic pace"));
    assert_eq!(
        schema_version(&db).expect("schema version"),
        Some("5".to_owned())
    );
    block_on_store(async {
        let conn = open_store(&db).await?;
        let mut rows = conn
            .query(
                "SELECT remaining_percent FROM account_usage_snapshots WHERE account_key_hash = 'sha256:legacy'",
                (),
            )
            .await
            .map_err(|err| err.to_string())?;
        let row = rows.next().await.map_err(|err| err.to_string())?.unwrap();
        let old_percent: i64 = row.get(0).map_err(|err| err.to_string())?;
        assert_eq!(old_percent, 57, "legacy scalar remains preserved separately");
        Ok::<(), String>(())
    })
    .expect("verify old scalar remains in table");
}

#[test]
fn versioned_snapshot_store_missing_table_fails_open_and_write_without_mutation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    seed_snapshot_in_finished_child(&db, "v4_missing_table");

    let read_error = block_on_store(async { open_store(&db).await.map(|_| ()) })
        .expect_err("versioned missing table must fail closed");
    assert!(
        read_error.contains("missing account_usage_snapshots"),
        "{read_error}"
    );
    let write_error = store_usage_snapshot(&db, &usage_view())
        .expect_err("writer must not recreate versioned missing table");
    assert!(
        write_error.contains("missing account_usage_snapshots"),
        "{write_error}"
    );

    block_on_store(async {
        let conn = open_test_store(&db).await?;
        let mut rows = conn
            .query("SELECT value FROM _meta WHERE key = 'schema_version'", ())
            .await
            .map_err(|err| err.to_string())?;
        let row = rows
            .next()
            .await
            .map_err(|err| err.to_string())?
            .expect("version row remains");
        let version: String = row.get(0).map_err(|err| err.to_string())?;
        assert_eq!(version, "4");
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'account_usage_snapshots'",
                (),
            )
            .await
            .map_err(|err| err.to_string())?;
        let row = rows.next().await.map_err(|err| err.to_string())?.unwrap();
        let count: i64 = row.get(0).map_err(|err| err.to_string())?;
        assert_eq!(count, 0, "failed open and write must not create the table");
        Ok::<(), String>(())
    })
    .expect("verify version-only database remains unchanged");
}

#[test]
fn schema_v4_rejects_i64_max_legacy_percent_without_changing_database() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    seed_snapshot_in_finished_child(&db, "v4_i64_max");

    let error = block_on_store(async { open_store(&db).await.map(|_| ()) })
        .expect_err("i64::MAX cannot be a historical u8 percentage");
    assert!(error.contains("historical u8 percentage domain"), "{error}");

    block_on_store(async {
        let conn = open_test_store(&db).await?;
        let mut rows = conn
            .query("SELECT value FROM _meta WHERE key = 'schema_version'", ())
            .await
            .map_err(|err| err.to_string())?;
        let row = rows.next().await.map_err(|err| err.to_string())?.unwrap();
        let version: String = row.get(0).map_err(|err| err.to_string())?;
        assert_eq!(version, "4");
        let mut rows = conn
            .query(
                "SELECT remaining_percent FROM account_usage_snapshots WHERE account_key_hash = 'legacy-key'",
                (),
            )
            .await
            .map_err(|err| err.to_string())?;
        let row = rows.next().await.map_err(|err| err.to_string())?.unwrap();
        let remaining: i64 = row.get(0).map_err(|err| err.to_string())?;
        assert_eq!(remaining, i64::MAX);
        let columns = conn
            .query("PRAGMA table_info(account_usage_snapshots)", ())
            .await
            .map_err(|err| err.to_string())?;
        let mut columns = columns;
        while let Some(row) = columns.next().await.map_err(|err| err.to_string())? {
            let name: String = row.get(1).map_err(|err| err.to_string())?;
            assert_ne!(name, "quota_bucket_json", "failed migration must roll back DDL");
        }
        Ok::<(), String>(())
    })
    .expect("verify rejected value and schema remain intact");
}

#[test]
fn failed_snapshot_migration_commit_rolls_back_and_can_retry() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    seed_snapshot_in_finished_child(&db, "v4_commit_failure");

    let error = store_usage_snapshot(&db, &usage_view())
        .expect_err("deferred foreign key must make COMMIT fail");
    assert!(
        error.contains("commit usage snapshot schema migration failed"),
        "{error}"
    );
    assert_eq!(
        connection_build_count(&db).expect("failed migration count"),
        1
    );

    block_on_store(async {
        let conn = open_test_store(&db).await?;
        conn.execute(
            "UPDATE _meta SET value = '0' WHERE key = 'test_fail_schema_commit'",
            (),
        )
        .await
        .map_err(|err| err.to_string())?;
        let mut rows = conn
            .query("SELECT value FROM _meta WHERE key = 'schema_version'", ())
            .await
            .map_err(|err| err.to_string())?;
        let row = rows.next().await.map_err(|err| err.to_string())?.unwrap();
        let version: String = row.get(0).map_err(|err| err.to_string())?;
        assert_eq!(version, "4", "COMMIT failure must roll back the version");
        Ok::<(), String>(())
    })
    .expect("verify failed commit rolled back");

    store_usage_snapshot(&db, &usage_view()).expect("retry migration with a fresh handle");
    assert_eq!(
        connection_build_count(&db).expect("retry connection count"),
        2
    );
    assert_eq!(
        schema_version(&db).expect("schema version").as_deref(),
        Some("5")
    );
    let rows = stored_account_snapshots(&db).expect("read retry result");
    assert_eq!(
        rows.len(),
        3,
        "retry retains old row and stores new buckets"
    );
    let legacy = rows
        .iter()
        .find(|row| row.account_key_hash == "legacy-key")
        .expect("legacy row survives the retry");
    assert_eq!(legacy.used_amount, None);
    assert_eq!(legacy.quota_bucket.remaining_percent.unwrap().get(), 64.0);
    assert_eq!(
        legacy.quota_bucket.remaining_percent_origin,
        Some(PercentObservationOrigin::HistoricalUnknown)
    );
}

#[test]
fn failed_snapshot_upsert_commit_rolls_back_and_a_reopened_cached_store_retries() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    store_usage_snapshot(&db, &usage_view()).expect("store initial snapshot");
    assert_eq!(connection_build_count(&db).unwrap(), 1);
    block_on_store(async {
        let conn = open_store(&db).await?;
        conn.execute(
            "INSERT INTO _meta (key, value) VALUES ('test_fail_snapshot_upsert_commit', '1')",
            (),
        )
        .await
        .map_err(|error| error.to_string())?;
        Ok::<(), String>(())
    })
    .expect("enable deterministic upsert commit failure");

    let mut changed = usage_view();
    changed.fetched_at_epoch += 60;
    changed.buckets[0].remaining_percent = Some(CanonicalRemainingPercent::new(21.125).unwrap());
    let error = store_usage_snapshot(&db, &changed).expect_err("deferred FK must fail commit");
    assert!(
        error.contains("commit telemetry snapshot transaction failed"),
        "{error}"
    );

    let rows = stored_account_snapshots(&db).expect("read after rollback");
    let session = rows
        .iter()
        .find(|row| row.window_kind == "Session")
        .expect("session row remains after rollback");
    assert_eq!(session.quota_bucket.remaining_percent.unwrap().get(), 37.0);
    assert_eq!(rows.len(), 2, "failed transaction adds no partial rows");
    assert_eq!(connection_build_count(&db).unwrap(), 2);

    block_on_store(async {
        let conn = open_store(&db).await?;
        conn.execute(
            "UPDATE _meta SET value = '0' WHERE key = 'test_fail_snapshot_upsert_commit'",
            (),
        )
        .await
        .map_err(|error| error.to_string())?;
        Ok::<(), String>(())
    })
    .expect("disable failure marker on reopened cached store");
    store_usage_snapshot(&db, &changed).expect("retry on the reopened cached store");
    assert_eq!(connection_build_count(&db).unwrap(), 2);
    let rows = stored_account_snapshots(&db).expect("read retried snapshot");
    assert_eq!(rows.len(), 2);
    let session = rows
        .iter()
        .find(|row| row.window_kind == "Session")
        .expect("session row after retry");
    assert_eq!(
        session.quota_bucket.remaining_percent.unwrap().get(),
        21.125
    );
}

#[test]
fn schema_migration_error_rolls_back_and_can_retry_after_fixing_source_row() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    seed_snapshot_in_finished_child(&db, "v4_failed_migration");

    let failure = block_on_store(async { open_store(&db).await.map(|_| ()) })
        .expect_err("unknown legacy status must fail closed");
    assert!(failure.contains("unknown legacy status"), "{failure}");
    block_on_store(async {
        let conn = open_test_store(&db).await?;
        let columns = conn
            .query("PRAGMA table_info(account_usage_snapshots)", ())
            .await
            .map_err(|err| err.to_string())?;
        let mut columns = columns;
        let mut has_payload = false;
        while let Some(row) = columns.next().await.map_err(|err| err.to_string())? {
            let name: String = row.get(1).map_err(|err| err.to_string())?;
            has_payload |= name == "quota_bucket_json";
        }
        assert!(!has_payload, "DDL changes rolled back with the migration");
        conn.execute(
            "UPDATE account_usage_snapshots SET status = 'fresh' WHERE account_key_hash = 'retry-key'",
            (),
        )
        .await
        .map_err(|err| err.to_string())?;
        let mut status_rows = conn
            .query(
                "SELECT status FROM account_usage_snapshots WHERE account_key_hash = 'retry-key'",
                (),
            )
            .await
            .map_err(|err| err.to_string())?;
        let status_row = status_rows
            .next()
            .await
            .map_err(|err| err.to_string())?
            .expect("repaired historical row remains present");
        let status: String = status_row.get(0).map_err(|err| err.to_string())?;
        assert_eq!(status, "fresh", "source repair must be durable before retry");
        Ok::<(), String>(())
    })
    .unwrap();

    block_on_store(async { open_store(&db).await.map(|_| ()) })
        .expect("migration retry after source repair");
    let row = stored_account_snapshots(&db).unwrap().remove(0);
    assert_eq!(row.quota_bucket.remaining_percent.unwrap().get(), 64.0);
    assert_eq!(
        row.quota_bucket.remaining_percent_origin,
        Some(PercentObservationOrigin::HistoricalUnknown)
    );
    assert_eq!(schema_version(&db).unwrap().as_deref(), Some("5"));
}

#[test]
fn partial_typed_snapshot_schema_fails_closed_without_rewriting() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    seed_snapshot_in_finished_child(&db, "v4_partial_typed");

    let error = block_on_store(async { open_store(&db).await.map(|_| ()) })
        .expect_err("partial typed schema must fail closed");
    assert!(error.contains("partial typed quota column"), "{error}");
    let store = block_on_store(async { open_test_store(&db).await }).unwrap();
    let (version, count) = block_on_store(async {
        let mut version_rows = store
            .query("SELECT value FROM _meta WHERE key = 'schema_version'", ())
            .await
            .map_err(|error| error.to_string())?;
        let version_row = version_rows
            .next()
            .await
            .map_err(|error| error.to_string())?
            .expect("legacy schema version row");
        let version: String = version_row.get(0).map_err(|error| error.to_string())?;
        let mut rows = store
            .query("SELECT COUNT(*) FROM account_usage_snapshots", ())
            .await
            .map_err(|error| error.to_string())?;
        let row = rows
            .next()
            .await
            .map_err(|error| error.to_string())?
            .expect("legacy snapshot row count");
        let count = row.get::<i64>(0).map_err(|error| error.to_string())?;
        Ok::<(String, i64), String>((version, count))
    })
    .unwrap();
    assert_eq!(version, "4");
    assert_eq!(count, 1, "the legacy row remains present");
    drop(store);
}

#[test]
fn cached_snapshot_connection_rechecks_future_schema_before_writing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    store_usage_snapshot(&db, &usage_view()).expect("store initial data");
    block_on_store(async {
        let conn = open_store(&db).await?;
        conn.execute(
            "UPDATE _meta SET value = '6' WHERE key = 'schema_version'",
            (),
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok::<(), String>(())
    })
    .unwrap();

    let failure = store_usage_snapshot(&db, &usage_view()).expect_err("future schema must fail");
    assert!(failure.contains("newer than supported schema"), "{failure}");
    let version = block_on_store(async {
        let conn = open_test_store(&db).await?;
        let mut rows = conn
            .query("SELECT value FROM _meta WHERE key = 'schema_version'", ())
            .await
            .map_err(|err| err.to_string())?;
        let row = rows.next().await.map_err(|err| err.to_string())?.unwrap();
        let version: String = row.get(0).map_err(|err| err.to_string())?;
        Ok::<String, String>(version)
    });
    assert_eq!(version.unwrap(), "6");
}

#[test]
fn corrupt_typed_snapshot_remains_failing_and_retains_live_bytes_across_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    store_usage_snapshot(&db, &usage_view()).expect("store typed snapshot");
    block_on_store(async {
        let conn = open_store(&db).await?;
        conn.execute(
            "UPDATE account_usage_snapshots
             SET quota_bucket_json = '{broken' WHERE window_kind = 'Session'",
            (),
        )
        .await
        .map_err(|error| error.to_string())?;
        Ok::<(), String>(())
    })
    .expect("corrupt the persisted typed payload");
    block_on_store(async { close_cached_store(&db).await })
        .expect("close cached connection before restart simulation");

    let first = stored_account_snapshots(&db).expect_err("corrupt payload must fail");
    assert!(first.contains("quota_bucket_json"), "{first}");
    let second = stored_account_snapshots(&db).expect_err("reopen must still fail");
    assert!(second.contains("quota_bucket_json"), "{second}");

    let retained = block_on_store(async {
        let conn = open_test_store(&db).await?;
        let mut rows = conn
            .query(
                "SELECT quota_bucket_json FROM account_usage_snapshots
                 WHERE window_kind = 'Session'",
                (),
            )
            .await
            .map_err(|error| error.to_string())?;
        let row = rows
            .next()
            .await
            .map_err(|error| error.to_string())?
            .expect("session snapshot row remains");
        row.get::<String>(0).map_err(|error| error.to_string())
    })
    .unwrap();
    assert_eq!(
        retained, "{broken",
        "failed reads keep the original live bytes"
    );
}

#[test]
fn focused_usage_view_rebuilds_snapshot_from_account_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    let view = focused_usage_view(&db, Some("codex"), Some("Codex"), 1_781_185_590)
        .expect("read focused usage")
        .expect("stored usage view");

    assert_eq!(view.focused_agent.as_deref(), Some("codex"));
    assert_eq!(view.focused_provider.as_deref(), Some("OpenAI"));
    assert_eq!(view.account.provider_label, "Codex");
    assert_eq!(view.account.account_label, "alexey@example.com");
    assert_eq!(view.account.plan_label.as_deref(), Some("Pro 20x"));
    assert_eq!(view.buckets.len(), 2);
    assert_eq!(view.buckets[0].label, "Session");
    assert_eq!(view.buckets[0].remaining_percent.unwrap().get(), 37.0);
    assert_eq!(view.buckets[1].label, "Credits");
    assert!(
        view.buckets
            .iter()
            .all(|bucket| bucket.status_slot.is_none())
    );
    assert_eq!(view.updated_label, "Updated now");
    assert_eq!(view.status_bar_label, "Codex Session: 63% used · 37% left");
}

#[test]
fn focused_usage_view_ticks_relative_updated_label_from_fetch_time() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    let view = focused_usage_view(&db, Some("codex"), Some("Codex"), 1_781_185_680)
        .expect("read focused usage")
        .expect("stored usage view");

    assert_eq!(view.updated_label, "Updated 2m ago");
}

#[test]
fn focused_usage_view_resolves_provider_from_agent_when_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let now = 1_781_185_680;
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Codex",
            "codex@example.com",
            Some("Pro 20x"),
            "Session",
            37,
            now,
        ),
    )
    .expect("store codex snapshot");
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Amp",
            "amp@example.com",
            Some("Amp Free"),
            "Amp Free",
            9,
            now,
        ),
    )
    .expect("store amp snapshot");

    let view = focused_usage_view(&db, Some("amp"), None, now)
        .expect("read focused usage")
        .expect("stored provider usage");

    assert_eq!(view.focused_agent.as_deref(), Some("amp"));
    assert_eq!(view.focused_provider.as_deref(), Some("Amp"));
    assert_eq!(view.account.provider_label, "Amp");
    assert_eq!(view.account.account_label, "amp@example.com");
    assert_eq!(view.buckets[0].label, "Amp Free");
    assert_eq!(view.buckets[0].remaining_percent.unwrap().get(), 9.0);
}

#[test]
fn focused_usage_view_without_resolved_provider_does_not_match_all() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    let view = focused_usage_view(&db, Some("unknown-agent"), None, 1_781_185_680)
        .expect("read focused usage");

    assert!(view.is_none());
}

#[test]
fn focused_usage_view_sorts_provider_buckets_canonically() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let now = 1_781_185_680;
    let mut view = provider_usage_view(
        "GLM / Z.AI",
        "zai@example.com",
        Some("Coding Pro"),
        "5-hour",
        100,
        now,
    );
    let base_bucket = view.buckets[0].clone();
    view.buckets.extend([
        QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "MCP".to_owned(),
            remaining_percent: Some(reported_remaining(100)),
            pace_label: Some("0 / 100 (100 remaining)".to_owned()),
            ..base_bucket.clone()
        },
        QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Tokens".to_owned(),
            remaining_percent: Some(reported_remaining(99)),
            ..base_bucket
        },
    ]);
    store_usage_snapshot(&db, &view).expect("store snapshot");

    let view = focused_usage_view(&db, Some("codex"), Some("Z.AI"), now)
        .expect("read focused usage")
        .expect("stored provider usage");

    assert_eq!(
        view.buckets
            .iter()
            .map(|bucket| bucket.label.as_str())
            .collect::<Vec<_>>(),
        vec!["5-hour", "Tokens", "MCP"]
    );
}
