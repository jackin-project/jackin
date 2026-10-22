// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{
    CanonicalUsedPercent, CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
    PercentObservationOrigin, QuotaBucketView, UsageSeverity,
};
use std::ops::Deref;

const SEED_CHILD_MODE: &str = "JACKIN_USAGE_CACHE_SEED_MODE";
const SEED_CHILD_PATH: &str = "JACKIN_USAGE_CACHE_SEED_PATH";

struct TestStore {
    _operation: StoreOperationGuard,
    store: LocalStore,
}

impl Deref for TestStore {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.store.connection()
    }
}

async fn open_test_store(path: &Path) -> Result<TestStore> {
    let operation = acquire_store_operation(path)
        .await
        .context("acquire test host cache operation")?;
    let store = open_existing_store(path, &operation).await?;
    Ok(TestStore {
        _operation: operation,
        store,
    })
}

async fn schema_version(path: &Path) -> Result<Option<String>> {
    let store = open_test_store(path).await?;
    stored_schema_version(&store).await
}

fn account(window: &str, used: i64) -> AccountUsageSnapshotView {
    AccountUsageSnapshotView {
        provider: "codex".to_owned(),
        account_label: "alexey@example.com".to_owned(),
        source: "codex-rpc".to_owned(),
        confidence: "authoritative".to_owned(),
        window_kind: window.to_owned(),
        used_amount: None,
        used_unit: None,
        limit_amount: None,
        limit_unit: None,
        resets_at: Some(1_781_200_000),
        fetched_at: 1_781_190_000,
        expires_at: Some(1_781_190_300),
        status: "fresh".to_owned(),
        last_error: None,
        quota_bucket: Some(QuotaBucketView {
            label: window.to_owned(),
            used_label: Some(format!("{used}% used")),
            limit_label: Some("100%".to_owned()),
            used_percent: Some(CanonicalUsedPercent::new(used as f64).unwrap()),
            used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            remaining_percent: None,
            remaining_percent_origin: None,
            count_quota: None,
            provider_quota_state: None,
            reset_label: None,
            resets_at: Some(1_781_200_000),
            status_slot: None,
            pace_label: None,
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::default(),
        }),
    }
}

fn seed_host_cache_in_finished_child(path: &Path, mode: &str) {
    let path = path.to_str().expect("test cache path is UTF-8");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("cli::usage::store::tests::seed_host_cache_child")
        .arg("--nocapture")
        .env(SEED_CHILD_MODE, mode)
        .env(SEED_CHILD_PATH, path)
        .status()
        .expect("start finished-child cache seeder");
    assert!(status.success(), "finished-child cache seed {mode} failed");
}

async fn create_partial_typed_host_cache_schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                     INSERT INTO _meta (key, value) VALUES ('schema_version', '1');
                     CREATE TABLE account_usage_snapshots (
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
                         quota_bucket_json TEXT,
                         UNIQUE(provider, account_key_hash, source, window_kind)
                     );",
    )
    .await
    .expect("create partial typed host cache schema");
}

#[test]
fn seed_host_cache_child() {
    let Ok(mode) = std::env::var(SEED_CHILD_MODE) else {
        return;
    };
    let path = std::env::var(SEED_CHILD_PATH).expect("cache seed child path");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("cache seed child runtime");
    runtime.block_on(async {
        let conn = jackin_usage::store_backend::connect_local(&path)
            .await
            .expect("open legacy cache seed in isolated child");
        match mode.as_str() {
            "v1" | "commit_failure" | "v1_scalar_refresh" => {
                let historical_key_hash = account_key_hash("codex", "historic@example.test");
                conn.execute_batch(
                    "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                     CREATE TABLE account_usage_snapshots (
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
                         UNIQUE(provider, account_key_hash, source, window_kind)
                     );
                     INSERT INTO _meta (key, value) VALUES ('schema_version', '1');",
                )
                .await
                .expect("create old host cache schema");
                conn.execute(
                    "INSERT INTO account_usage_snapshots
                     (provider, account_key_hash, account_label, source, confidence,
                      window_kind, used_amount, used_unit, limit_amount, limit_unit,
                      resets_at, fetched_at, expires_at, status, last_error)
                     VALUES ('codex', ?1, 'historic@example.test', 'codex-rpc',
                             'authoritative', 'session', ?2, 'historic unit', ?3,
                             'historic limit unit', ?4, ?5, ?6, 'fresh', 'historic failure')",
                    params![
                        historical_key_hash,
                        i64::MIN,
                        i64::MAX,
                        i64::MIN,
                        i64::MAX,
                        i64::MIN
                    ],
                )
                .await
                .expect("insert exact old host cache row");
                if mode == "v1_scalar_refresh" {
                    conn.execute(
                        "INSERT INTO account_usage_snapshots
                         (provider, account_key_hash, account_label, source, confidence,
                          window_kind, used_amount, used_unit, limit_amount, limit_unit,
                          resets_at, fetched_at, expires_at, status, last_error)
                         VALUES ('codex', 'unrelated-key', 'unrelated@example.test', 'codex-rpc',
                                 'authoritative', 'weekly', 0, 'unrelated amount unit',
                                 ?1, 'unrelated limit unit', 789, 456, ?2, 'stale',
                                 'unrelated historical error')",
                        params![i64::MIN, i64::MAX],
                    )
                    .await
                    .expect("insert unrelated exact old host cache row");
                }
                if mode == "commit_failure" {
                    conn.execute(
                        "INSERT INTO _meta (key, value) VALUES ('test_fail_schema_commit', '1')",
                        (),
                    )
                    .await
                    .expect("add old host cache commit failure marker");
                }
            }
            "missing_table_v1" | "missing_table_v2" => {
                let version = if mode.ends_with('1') { "1" } else { "2" };
                conn.execute(
                    "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
                    (),
                )
                .await
                .expect("create missing-table metadata");
                conn.execute(
                    "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)",
                    [version],
                )
                .await
                .expect("write missing-table version");
            }
            "failed_migration" => {
                conn.execute_batch(
                    "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
                     INSERT INTO _meta (key, value) VALUES ('schema_version', '1');
                     CREATE TABLE account_usage_snapshots (
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
                         UNIQUE(provider, account_key_hash, source, window_kind)
                     );
                     INSERT INTO account_usage_snapshots
                       (provider, account_key_hash, account_label, source, confidence,
                        window_kind, used_amount, used_unit, limit_amount, limit_unit,
                        resets_at, fetched_at, expires_at, status)
                     VALUES ('codex', 'retry-key', 'retry@example.test', 'codex-rpc',
                             'authoritative', 'session', 9223372036854775807, 'historic',
                             -9223372036854775807, 'historic limit', 123, 456, 789, 'fresh');",
                )
                .await
                .expect("create retry source with a missing required column");
            }
            "partial_typed_column" => create_partial_typed_host_cache_schema(&conn).await,
            _ => panic!("unknown host cache seed mode {mode}"),
        }
    });
}

#[tokio::test]
async fn host_account_cache_upserts_rows() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let rows = [account("session", 37), account("weekly", 10)];

    let path = upsert_accounts(&paths, &rows).await.unwrap();
    assert_eq!(path, paths.data_dir.join("daemon").join("accounts.db"));
    assert_eq!(count_account_rows(path.clone()).await.unwrap(), 2);

    upsert_accounts(&paths, &[account("session", 38)])
        .await
        .unwrap();
    assert_eq!(count_account_rows(path).await.unwrap(), 2);
}

#[tokio::test]
async fn host_account_cache_reads_seeded_rows_without_provider_poll() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let seeded = [account("session", 37), account("weekly", 10)];

    let path = upsert_accounts(&paths, &seeded).await.unwrap();
    let (read_path, rows) = read_accounts(&paths).await.unwrap();

    assert_eq!(read_path, path);
    assert_eq!(rows, seeded);
}

#[tokio::test]
async fn host_account_cache_round_trips_exact_count_money_and_fractional_percentages() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let mut count = account("requests", 0);
    count.quota_bucket = Some(QuotaBucketView {
        label: "requests".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: Some(CountQuota::new(
            Some(u64::MAX),
            Some(u64::MAX),
            Some(0),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
            CountQuotaProvenance::ProviderReported,
        )),
        provider_quota_state: Some(jackin_protocol::control::ProviderQuotaState::Exhausted),
        reset_label: Some("reset exactly".to_owned()),
        resets_at: Some(i64::MAX),
        status_slot: None,
        pace_label: Some("pace exactly".to_owned()),
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
    });
    count.used_amount = None;
    count.used_unit = None;
    count.limit_amount = None;
    count.limit_unit = None;

    let mut money = account("money", 43);
    let bucket = money.quota_bucket.as_mut().unwrap();
    let adjacent_used = f64::from_bits(43.375_f64.to_bits() + 1);
    let adjacent_remaining = f64::from_bits(57.125_f64.to_bits() + 1);
    bucket.used_percent = Some(CanonicalUsedPercent::new(adjacent_used).unwrap());
    bucket.remaining_percent =
        Some(jackin_protocol::control::CanonicalRemainingPercent::new(adjacent_remaining).unwrap());
    bucket.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    bucket.used_money = Some(jackin_protocol::control::Money::new(i64::MIN, "USD", 4));
    bucket.limit_money = Some(jackin_protocol::control::Money::new(i64::MAX, "USD", 7));
    money.used_amount = Some(i64::MIN);
    money.used_unit = Some("USD minor units".to_owned());
    money.limit_amount = Some(i64::MAX);
    money.limit_unit = Some("USD minor units".to_owned());

    let mut unlimited = account("unlimited", 0);
    let unlimited_bucket = unlimited.quota_bucket.as_mut().unwrap();
    unlimited_bucket.used_percent = None;
    unlimited_bucket.used_percent_origin = None;
    unlimited_bucket.remaining_percent = None;
    unlimited_bucket.remaining_percent_origin = None;
    unlimited_bucket.used_money = None;
    unlimited_bucket.limit_money = None;
    unlimited_bucket.provider_quota_state =
        Some(jackin_protocol::control::ProviderQuotaState::Unlimited);
    unlimited.used_amount = None;
    unlimited.used_unit = None;
    unlimited.limit_amount = None;
    unlimited.limit_unit = None;
    unlimited.resets_at = None;

    let expected = [money, count, unlimited];
    upsert_accounts(&paths, &expected).await.unwrap();
    let (_, loaded) = read_accounts(&paths).await.unwrap();
    assert_eq!(loaded, expected);
    assert_eq!(
        loaded[1]
            .quota_bucket
            .as_ref()
            .unwrap()
            .count_quota
            .unwrap()
            .used,
        Some(u64::MAX)
    );
    assert_eq!(
        loaded[1]
            .quota_bucket
            .as_ref()
            .unwrap()
            .provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Exhausted)
    );
    assert_eq!(
        loaded[0]
            .quota_bucket
            .as_ref()
            .unwrap()
            .used_percent
            .unwrap()
            .get()
            .to_bits(),
        adjacent_used.to_bits()
    );
    assert_eq!(
        loaded[0]
            .quota_bucket
            .as_ref()
            .unwrap()
            .remaining_percent
            .unwrap()
            .get()
            .to_bits(),
        adjacent_remaining.to_bits()
    );
    assert_eq!(
        loaded[0]
            .quota_bucket
            .as_ref()
            .unwrap()
            .used_money
            .as_ref()
            .unwrap()
            .amount_minor,
        i64::MIN
    );
    assert_eq!(
        loaded[2]
            .quota_bucket
            .as_ref()
            .unwrap()
            .provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Unlimited)
    );
}

#[tokio::test]
async fn host_account_cache_v1_migration_preserves_legacy_scalars_and_null_bucket() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    seed_host_cache_in_finished_child(&path, "v1");

    let (read_path, rows) = read_accounts(&paths).await.unwrap();
    assert_eq!(read_path, path);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].used_amount, Some(i64::MIN));
    assert_eq!(rows[0].used_unit.as_deref(), Some("historic unit"));
    assert_eq!(rows[0].limit_amount, Some(i64::MAX));
    assert_eq!(rows[0].limit_unit.as_deref(), Some("historic limit unit"));
    assert_eq!(rows[0].resets_at, Some(i64::MIN));
    assert_eq!(rows[0].fetched_at, i64::MAX);
    assert_eq!(rows[0].expires_at, Some(i64::MIN));
    assert_eq!(rows[0].last_error.as_deref(), Some("historic failure"));
    assert!(rows[0].quota_bucket.is_none());
    assert_eq!(schema_version(&path).await.unwrap().as_deref(), Some("2"));
}

#[tokio::test]
async fn typed_refresh_preserves_migrated_extreme_scalar_pairs_after_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    seed_host_cache_in_finished_child(&path, "v1_scalar_refresh");

    let (_, migrated) = read_accounts(&paths).await.unwrap();
    assert_eq!(migrated.len(), 2);
    let historical = migrated
        .iter()
        .find(|row| row.account_label == "historic@example.test")
        .unwrap();
    assert_eq!(historical.used_amount, Some(i64::MIN));
    assert_eq!(historical.used_unit.as_deref(), Some("historic unit"));
    assert_eq!(historical.limit_amount, Some(i64::MAX));
    assert_eq!(
        historical.limit_unit.as_deref(),
        Some("historic limit unit")
    );
    assert!(historical.quota_bucket.is_none());

    let mut refreshed = account("session", 37);
    refreshed.account_label = "historic@example.test".to_owned();
    let refreshed_bucket = refreshed.quota_bucket.as_mut().unwrap();
    refreshed_bucket.used_percent = Some(CanonicalUsedPercent::new(100.0).unwrap());
    refreshed_bucket.count_quota = Some(CountQuota::new(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    refreshed_bucket.provider_quota_state =
        Some(jackin_protocol::control::ProviderQuotaState::Exhausted);
    upsert_accounts(&paths, &[refreshed]).await.unwrap();

    let (reopened_path, rows) = read_accounts(&paths).await.unwrap();
    assert_eq!(reopened_path, path);
    assert_eq!(rows.len(), 2, "refresh must update the same historical row");
    let row = rows
        .iter()
        .find(|row| row.account_label == "historic@example.test")
        .unwrap();
    assert_eq!(row.account_label, "historic@example.test");
    assert_eq!(row.used_amount, Some(i64::MIN));
    assert_eq!(row.used_unit.as_deref(), Some("historic unit"));
    assert_eq!(row.limit_amount, Some(i64::MAX));
    assert_eq!(row.limit_unit.as_deref(), Some("historic limit unit"));
    let bucket = row.quota_bucket.as_ref().unwrap();
    assert_eq!(
        bucket.used_percent.unwrap().get().to_bits(),
        100.0_f64.to_bits()
    );
    assert_eq!(
        bucket.used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert!(bucket.remaining_percent.is_none());
    assert!(bucket.remaining_percent_origin.is_none());
    assert!(bucket.used_money.is_none());
    assert!(bucket.limit_money.is_none());
    let count = bucket.count_quota.unwrap();
    assert_eq!(count.used, Some(u64::MAX));
    assert_eq!(count.limit, Some(u64::MAX));
    assert_eq!(count.remaining, Some(0));
    assert_eq!(
        bucket.provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Exhausted)
    );

    let unrelated = rows
        .iter()
        .find(|row| row.account_label == "unrelated@example.test")
        .unwrap();
    assert_eq!(unrelated.used_amount, Some(0));
    assert_eq!(
        unrelated.used_unit.as_deref(),
        Some("unrelated amount unit")
    );
    assert_eq!(unrelated.limit_amount, Some(i64::MIN));
    assert_eq!(
        unrelated.limit_unit.as_deref(),
        Some("unrelated limit unit")
    );
    assert_eq!(unrelated.resets_at, Some(789));
    assert_eq!(unrelated.fetched_at, 456);
    assert_eq!(unrelated.expires_at, Some(i64::MAX));
    assert_eq!(
        unrelated.last_error.as_deref(),
        Some("unrelated historical error")
    );
    assert!(unrelated.quota_bucket.is_none());

    let mut explicit = account("session", 0);
    explicit.account_label = "historic@example.test".to_owned();
    explicit.used_amount = Some(0);
    explicit.used_unit = Some("explicit amount unit".to_owned());
    explicit.limit_amount = Some(i64::MIN);
    explicit.limit_unit = Some("explicit limit unit".to_owned());
    let explicit_bucket = explicit.quota_bucket.as_mut().unwrap();
    explicit_bucket.used_percent = None;
    explicit_bucket.used_percent_origin = None;
    explicit_bucket.used_money = Some(jackin_protocol::control::Money::new(i64::MIN, "USD", 4));
    explicit_bucket.limit_money = Some(jackin_protocol::control::Money::new(i64::MAX, "USD", 7));
    upsert_accounts(&paths, &[explicit]).await.unwrap();

    let (_, explicitly_updated) = read_accounts(&paths).await.unwrap();
    let explicit_row = explicitly_updated
        .iter()
        .find(|row| row.account_label == "historic@example.test")
        .unwrap();
    assert_eq!(explicit_row.used_amount, Some(0));
    assert_eq!(
        explicit_row.used_unit.as_deref(),
        Some("explicit amount unit")
    );
    assert_eq!(explicit_row.limit_amount, Some(i64::MIN));
    assert_eq!(
        explicit_row.limit_unit.as_deref(),
        Some("explicit limit unit")
    );
    let explicit_bucket = explicit_row.quota_bucket.as_ref().unwrap();
    assert_eq!(
        explicit_bucket.used_money.as_ref().unwrap().amount_minor,
        i64::MIN
    );
    assert_eq!(
        explicit_bucket.limit_money.as_ref().unwrap().amount_minor,
        i64::MAX
    );

    let mut invalid_percent = account("session", 99);
    invalid_percent.account_label = "historic@example.test".to_owned();
    invalid_percent
        .quota_bucket
        .as_mut()
        .unwrap()
        .used_percent_origin = None;
    let mut invalid_missing_bucket = account("invalid", 10);
    invalid_missing_bucket.quota_bucket = None;
    let before_invalid_batch = explicitly_updated;
    let error = upsert_accounts(&paths, &[invalid_percent, invalid_missing_bucket])
        .await
        .unwrap_err();
    assert!(error.to_string().contains("percent and observation origin"));
    let (_, after_invalid_batch) = read_accounts(&paths).await.unwrap();
    assert_eq!(after_invalid_batch, before_invalid_batch);
}

#[tokio::test]
async fn host_account_cache_future_version_fails_closed_without_rewriting_version() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = upsert_accounts(&paths, &[account("session", 37)])
        .await
        .unwrap();
    let conn = open_test_store(&path).await.unwrap();
    conn.execute(
        "UPDATE _meta SET value = '3' WHERE key = 'schema_version'",
        (),
    )
    .await
    .unwrap();
    let version = stored_schema_version(&conn).await.unwrap();
    assert_eq!(version.as_deref(), Some("3"));
    drop(conn);

    let error = read_accounts(&paths).await.unwrap_err();
    assert!(error.to_string().contains("newer than supported schema"));
    let conn = open_test_store(&path).await.unwrap();
    let version = stored_schema_version(&conn).await.unwrap();
    assert_eq!(version.as_deref(), Some("3"));
    drop(conn);
}

#[tokio::test]
async fn host_account_cache_versioned_file_missing_rows_table_is_not_silent_empty() {
    for version in ["1", "2"] {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let path = host_account_cache_path(&paths);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        seed_host_cache_in_finished_child(
            &path,
            if version == "1" {
                "missing_table_v1"
            } else {
                "missing_table_v2"
            },
        );

        let read_error = read_accounts(&paths).await.unwrap_err();
        assert!(
            read_error
                .to_string()
                .contains("missing account_usage_snapshots"),
            "schema {version}: {read_error:#}"
        );
        let write_error = upsert_accounts(&paths, &[account("session", 37)])
            .await
            .unwrap_err();
        assert!(
            write_error
                .to_string()
                .contains("missing account_usage_snapshots"),
            "schema {version}: {write_error:#}"
        );
        let conn = open_test_store(&path).await.unwrap();
        assert_eq!(
            stored_schema_version(&conn).await.unwrap().as_deref(),
            Some(version)
        );
        assert!(
            !table_exists(&conn, "account_usage_snapshots")
                .await
                .unwrap()
        );
        drop(conn);
    }
}

#[tokio::test]
async fn host_account_cache_malformed_typed_payload_is_a_named_error() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = upsert_accounts(&paths, &[account("session", 37)])
        .await
        .unwrap();
    let conn = open_test_store(&path).await.unwrap();
    conn.execute(
        "UPDATE account_usage_snapshots SET quota_bucket_json = '{broken'",
        (),
    )
    .await
    .unwrap();
    drop(conn);

    let error = read_accounts(&paths).await.unwrap_err();
    let message = error.to_string();
    assert!(message.contains("quota_bucket_json"), "{message}");
    assert!(message.contains("session"), "{message}");
    let restarted_error = read_accounts(&paths).await.unwrap_err().to_string();
    assert!(
        restarted_error.contains("quota_bucket_json"),
        "corruption remains failing on a fresh store open: {restarted_error}"
    );
    let conn = open_test_store(&path).await.unwrap();
    let mut rows = conn
        .query(
            "SELECT quota_bucket_json FROM account_usage_snapshots WHERE window_kind = 'session'",
            (),
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let retained: String = row.get(0).unwrap();
    assert_eq!(
        retained, "{broken",
        "failed reads retain the damaged live bytes"
    );
}

#[tokio::test]
async fn host_account_cache_partial_typed_schema_fails_closed_without_rewriting() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    seed_host_cache_in_finished_child(&path, "partial_typed_column");

    let read_error = read_accounts(&paths).await.unwrap_err();
    assert!(
        read_error
            .to_string()
            .contains("partial typed quota column"),
        "{read_error:#}"
    );
    let write_error = upsert_accounts(&paths, &[account("session", 37)])
        .await
        .unwrap_err();
    assert!(
        write_error
            .to_string()
            .contains("partial typed quota column"),
        "{write_error:#}"
    );
    let conn = open_test_store(&path).await.unwrap();
    assert_eq!(
        stored_schema_version(&conn).await.unwrap().as_deref(),
        Some("1")
    );
    assert!(
        account_snapshot_columns(&conn)
            .await
            .unwrap()
            .contains("quota_bucket_json")
    );
}

#[tokio::test]
async fn host_account_cache_commit_failure_rolls_back_and_migration_can_retry() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    seed_host_cache_in_finished_child(&path, "commit_failure");

    let failure = read_accounts(&paths).await.unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("commit host usage cache migration failed"),
        "{failure:#}"
    );
    let conn = open_test_store(&path).await.unwrap();
    assert_eq!(
        stored_schema_version(&conn).await.unwrap().as_deref(),
        Some("1")
    );
    assert!(
        !account_snapshot_columns(&conn)
            .await
            .unwrap()
            .contains("quota_bucket_json")
    );
    conn.execute(
        "UPDATE _meta SET value = '0' WHERE key = 'test_fail_schema_commit'",
        (),
    )
    .await
    .unwrap();
    drop(conn);

    let (_, rows) = read_accounts(&paths).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].account_label, "historic@example.test");
    assert_eq!(rows[0].used_amount, Some(i64::MIN));
    assert_eq!(rows[0].used_unit.as_deref(), Some("historic unit"));
    assert_eq!(rows[0].limit_amount, Some(i64::MAX));
    assert_eq!(rows[0].limit_unit.as_deref(), Some("historic limit unit"));
    assert_eq!(rows[0].resets_at, Some(i64::MIN));
    assert_eq!(rows[0].fetched_at, i64::MAX);
    assert_eq!(rows[0].expires_at, Some(i64::MIN));
    assert_eq!(rows[0].last_error.as_deref(), Some("historic failure"));
    assert!(rows[0].quota_bucket.is_none());
    assert_eq!(schema_version(&path).await.unwrap().as_deref(), Some("2"));
}

#[tokio::test]
async fn host_account_cache_failed_migration_rolls_back_and_retries_after_repair() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    seed_host_cache_in_finished_child(&path, "failed_migration");

    let failure = read_accounts(&paths).await.unwrap_err();
    assert!(failure.to_string().contains("last_error"), "{failure:#}");
    let conn = open_test_store(&path).await.unwrap();
    assert_eq!(
        stored_schema_version(&conn).await.unwrap().as_deref(),
        Some("1")
    );
    assert!(
        !account_snapshot_columns(&conn)
            .await
            .unwrap()
            .contains("quota_bucket_json")
    );
    conn.execute(
        "ALTER TABLE account_usage_snapshots ADD COLUMN last_error TEXT",
        (),
    )
    .await
    .unwrap();
    drop(conn);

    let (_, rows) = read_accounts(&paths).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].used_amount, Some(i64::MAX));
    assert_eq!(rows[0].used_unit.as_deref(), Some("historic"));
    assert_eq!(rows[0].limit_amount, Some(-i64::MAX));
    assert!(rows[0].quota_bucket.is_none());
    assert_eq!(schema_version(&path).await.unwrap().as_deref(), Some("2"));
}

#[tokio::test]
async fn host_account_cache_missing_file_reads_empty_without_creating_db() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = paths.data_dir.join("daemon").join("accounts.db");

    let (read_path, rows) = read_accounts(&paths).await.unwrap();

    assert_eq!(read_path, path);
    assert!(rows.is_empty());
    assert!(!read_path.exists());
}

#[test]
fn account_hash_is_stable_and_namespaced() {
    assert_eq!(
        account_key_hash("codex", "alexey@example.com"),
        account_key_hash("codex", "alexey@example.com")
    );
    assert_ne!(
        account_key_hash("codex", "alexey@example.com"),
        account_key_hash("claude", "alexey@example.com")
    );
}

#[test]
fn host_account_cache_exports_owned_operations_without_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("host-cache-secret-path");
    let paths = JackinPaths::for_tests(&root);
    let mut sensitive_account = account("host-cache-secret-window", 37);
    sensitive_account.account_label = "host-cache-secret@example.com".to_owned();

    let _interest_cache_anchor = tracing::Dispatch::new(tracing_subscriber::registry());
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    tracing::subscriber::with_default(subscriber, || {
        runtime.block_on(async {
            upsert_accounts(&paths, &[sensitive_account]).await.unwrap();
            read_accounts(&paths).await.unwrap();
        });
        export.force_flush();
    });

    export.force_flush();
    assert!(
        !export.finished_spans().is_empty(),
        "cache operations should produce spans"
    );
    assert_eq!(export.error_span_count(), 0);
    for expected in ["connect", "update", "upsert", "select"] {
        assert!(export.contains_span_text(expected), "missing {expected}");
    }
    for prohibited in [
        "host-cache-secret-path",
        "host-cache-secret-window",
        "host-cache-secret@example.com",
        "account_usage_snapshots",
        "CREATE TABLE",
    ] {
        assert!(!export.contains_span_text(prohibited));
    }
}

async fn count_account_rows(path: PathBuf) -> Result<i64> {
    let store = open_test_store(&path).await?;
    operation(DbOperation::Select, async {
        let mut rows = store
            .query("SELECT COUNT(*) FROM account_usage_snapshots", ())
            .await
            .context("count host usage account rows")?;
        let row = rows
            .next()
            .await
            .context("read host usage account count")?
            .ok_or_else(|| anyhow::anyhow!("missing host usage account count row"))?;
        row_i64(&row, 0)
    })
    .await
}
