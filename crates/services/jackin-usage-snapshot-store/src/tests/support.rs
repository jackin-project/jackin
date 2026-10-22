// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{CanonicalRemainingPercent, PercentObservationOrigin};
use jackin_usage_store_backend::connect_local;
use std::ops::Deref;

const SEED_CHILD_MODE: &str = "JACKIN_SNAPSHOT_SEED_MODE";
const SEED_CHILD_PATH: &str = "JACKIN_SNAPSHOT_SEED_PATH";

pub(super) struct TestStore {
    _operation: jackin_usage_store_backend::StoreOperationGuard,
    store: jackin_usage_store_backend::LocalStore,
}

impl Deref for TestStore {
    type Target = jackin_usage_store_backend::Connection;

    fn deref(&self) -> &Self::Target {
        self.store.connection()
    }
}

pub(super) async fn open_test_store(path: &std::path::Path) -> Result<TestStore, String> {
    use jackin_usage_store_backend::{LocalStore, ManagedStoreKind, acquire_store_operation};

    let operation = acquire_store_operation(path)
        .await
        .map_err(|error| format!("acquire snapshot test store operation failed: {error}"))?;
    let store = LocalStore::open_managed(path, ManagedStoreKind::QuotaSnapshot, &operation)
        .await
        .map_err(|error| format!("open snapshot test store failed: {error}"))?;
    Ok(TestStore {
        _operation: operation,
        store,
    })
}

pub(super) fn seed_snapshot_in_finished_child(path: &std::path::Path, mode: &str) {
    let path = path.to_str().expect("snapshot seed path is UTF-8");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("tests::support::seed_snapshot_child")
        .arg("--nocapture")
        .env(SEED_CHILD_MODE, mode)
        .env(SEED_CHILD_PATH, path)
        .status()
        .expect("start finished-child snapshot seeder");
    assert!(
        status.success(),
        "finished-child snapshot seed {mode} failed"
    );
}

#[test]
fn seed_snapshot_child() {
    let Ok(mode) = std::env::var(SEED_CHILD_MODE) else {
        return;
    };
    let path = std::env::var(SEED_CHILD_PATH).expect("snapshot seed child path");
    block_on_store(async move {
        let conn = connect_local(&path).await?;
        if mode == "v4_missing_table" {
            conn.execute_batch(
                "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 INSERT INTO _meta (key, value) VALUES ('schema_version', '4');",
            )
            .await
            .map_err(|error| error.to_string())?;
            return Ok::<(), String>(());
        }

        conn.execute_batch(
            "CREATE TABLE account_usage_snapshots (
                id INTEGER PRIMARY KEY,
                provider TEXT NOT NULL,
                account_key_hash TEXT NOT NULL,
                account_label TEXT NOT NULL,
                source TEXT NOT NULL,
                confidence TEXT NOT NULL,
                window_kind TEXT NOT NULL,
                used_amount INTEGER,
                used_unit TEXT,
                limit_amount INTEGER,
                limit_unit TEXT,
                resets_at INTEGER,
                fetched_at INTEGER NOT NULL,
                expires_at INTEGER,
                status TEXT NOT NULL,
                last_error TEXT,
                focused_provider TEXT,
                plan_label TEXT,
                remaining_percent INTEGER,
                used_label TEXT,
                limit_label TEXT,
                reset_label TEXT,
                pace_label TEXT,
                view_status TEXT NOT NULL DEFAULT 'unavailable',
                updated_label TEXT NOT NULL DEFAULT 'Unavailable',
                status_bar_label TEXT NOT NULL DEFAULT 'usage unavailable',
                UNIQUE(provider, account_key_hash, source, window_kind)
            );
            CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO _meta (key, value) VALUES ('schema_version', '4');",
        )
        .await
        .map_err(|error| error.to_string())?;

        match mode.as_str() {
            "v4_full" => {
                conn.execute(
                    "INSERT INTO account_usage_snapshots
                     (provider, account_key_hash, account_label, source, confidence, window_kind,
                      used_amount, used_unit, limit_amount, limit_unit, resets_at, fetched_at,
                      expires_at, status, last_error, focused_provider, plan_label,
                      remaining_percent, used_label, limit_label, reset_label, pace_label,
                      view_status, updated_label, status_bar_label)
                     VALUES ('Codex', 'sha256:legacy', 'a@b', 'cli', 'authoritative', 'Session',
                      9223372036854775807, 'tokens', -9223372036854775807, 'tokens', 12345,
                      100, 200, 'stale', 'historic error', 'OpenAI', 'historic plan', 57,
                      'historic used', 'Unlimited', 'historic reset', 'historic pace',
                      'stale', 'historic updated', 'historic status bar')",
                    (),
                )
                .await
                .map_err(|error| error.to_string())?;
            }
            "v4_percent"
            | "v4_i64_max"
            | "v4_failed_migration"
            | "v4_commit_failure"
            | "v4_partial_typed" => {
                if mode == "v4_partial_typed" {
                    conn.execute(
                        "ALTER TABLE account_usage_snapshots ADD COLUMN quota_bucket_json TEXT",
                        (),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                }
                let (status, percent) = match mode.as_str() {
                    "v4_i64_max" => ("fresh", i64::MAX),
                    "v4_failed_migration" => ("unknown-future-status", 64),
                    _ => ("fresh", 64),
                };
                let account_key = if mode == "v4_failed_migration" {
                    "retry-key"
                } else {
                    "legacy-key"
                };
                conn.execute(
                    "INSERT INTO account_usage_snapshots
                     (provider, account_key_hash, account_label, source, confidence,
                      window_kind, resets_at, fetched_at, status, remaining_percent)
                     VALUES ('Codex', ?1, 'legacy@example.test', 'cli',
                             'authoritative', 'Session', 123, 100, ?2, ?3)",
                    jackin_usage_store_backend::params![account_key, status, percent],
                )
                .await
                .map_err(|error| error.to_string())?;
                if mode == "v4_commit_failure" {
                    conn.execute(
                        "INSERT INTO _meta (key, value) VALUES ('test_fail_schema_commit', '1')",
                        (),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                }
            }
            _ => panic!("unknown snapshot seed mode {mode}"),
        }
        Ok::<(), String>(())
    })
    .expect("seed snapshot store in finished child");
}

pub(super) fn reported_remaining(value: u8) -> CanonicalRemainingPercent {
    CanonicalRemainingPercent::new(f64::from(value)).unwrap()
}

pub(super) fn usage_view() -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("OpenAI".to_owned()),
        account: FocusedAccountHeader {
            provider_label: "Codex".to_owned(),
            account_label: "alexey@example.com".to_owned(),
            username: None,
            plan_label: Some("Pro 20x".to_owned()),
            credential_origin: None,
            account_capability: None,
        },
        buckets: vec![
            QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Session".to_owned(),
                used_label: Some("63% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                used_percent: None,
                used_percent_origin: None,
                remaining_percent: Some(reported_remaining(37)),
                remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
                count_quota: None,
                provider_quota_state: None,
                reset_label: Some("Resets in 1h".to_owned()),
                resets_at: None,
                status_slot: None,
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
            },
            QuotaBucketView {
                used_money: None,
                limit_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Credits".to_owned(),
                used_percent: None,
                used_percent_origin: None,
                used_label: None,
                limit_label: None,
                remaining_percent: None,
                remaining_percent_origin: None,
                count_quota: None,
                provider_quota_state: None,
                reset_label: None,
                resets_at: None,
                status_slot: None,
                pace_label: Some("ACP billing unavailable".to_owned()),
                status: UsageSnapshotStatus::Unsupported,
            },
        ],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::Cli,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_781_185_560,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
        tabs: Vec::new(),
        last_error: None,
    }
}

pub(super) fn provider_usage_view(
    provider: &str,
    account: &str,
    plan: Option<&str>,
    bucket: &str,
    remaining: u8,
    fetched_at_epoch: i64,
) -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some(provider.to_owned()),
        account: FocusedAccountHeader {
            provider_label: provider.to_owned(),
            account_label: account.to_owned(),
            username: None,
            plan_label: plan.map(str::to_owned),
            credential_origin: None,
            account_capability: None,
        },
        buckets: vec![QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: bucket.to_owned(),
            used_percent: None,
            used_percent_origin: None,
            used_label: Some(format!("{}% used", 100_u8.saturating_sub(remaining))),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(reported_remaining(remaining)),
            remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            count_quota: None,
            provider_quota_state: None,
            reset_label: Some("Resets at 15:00 UTC".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("On pace".to_owned()),
            status: UsageSnapshotStatus::Fresh,
        }],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: format!("{bucket} {remaining}%"),
        tabs: Vec::new(),
        last_error: None,
    }
}
