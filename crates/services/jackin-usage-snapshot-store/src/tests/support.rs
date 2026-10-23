// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{
    CanonicalRemainingPercent, CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
    Money, PercentObservationOrigin, QuotaBucketView, UsageSeverity, UsageSnapshotStatus,
};
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
    if mode.starts_with("v5_") {
        let marker = format!("JACKIN_SNAPSHOT_SEED_COMPLETE:{mode}");
        durable_process::run_bounded_test_child(
            "tests::support::seed_snapshot_child",
            &[(SEED_CHILD_MODE, mode), (SEED_CHILD_PATH, path)],
            &marker,
        );
        return;
    }
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
    let seeded_v5 = mode.starts_with("v5_");
    let completion_mode = mode.clone();
    block_on_store(async move {
        let conn = connect_local(&path).await?;
        if mode == "v5_fresh" || mode == "v5_nullable" {
            seed_v5_snapshot(&conn, mode == "v5_nullable").await?;
            return Ok::<(), String>(());
        }
        if mode.starts_with("v5_") {
            seed_v5_refusal_snapshot(&conn, &mode).await?;
            return Ok::<(), String>(());
        }
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
    if seeded_v5 {
        println!("JACKIN_SNAPSHOT_SEED_COMPLETE:{completion_mode}");
    }
}

async fn seed_v5_snapshot(
    conn: &jackin_usage_store_backend::Connection,
    nullable_quota_json: bool,
) -> Result<(), String> {
    let quota_column = if nullable_quota_json {
        "quota_bucket_json TEXT"
    } else {
        "quota_bucket_json TEXT NOT NULL"
    };
    conn.execute_batch(&format!(
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
            {quota_column},
            UNIQUE(provider, account_key_hash, source, window_kind)
        );
        CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        INSERT INTO _meta (key, value) VALUES ('schema_version', '5');
        INSERT INTO _meta (key, value)
        VALUES ('schema_version_before_quota_v5', '4');"
    ))
    .await
    .map_err(|error| error.to_string())?;

    let buckets = v5_typed_buckets();
    if buckets.len() != 3
        || buckets[0].label != "Session"
        || buckets[1].label != "Unlimited"
        || buckets[2].label != "Spend"
    {
        return Err(
            "schema 5 fixture must contain Session, Unlimited, and Spend buckets".to_owned(),
        );
    }
    let payloads = v5_seed_payloads(&buckets);
    if payloads.len() != buckets.len() {
        return Err("schema 5 fixture payload count does not match bucket count".to_owned());
    }
    for (index, (bucket, encoded)) in buckets.iter().zip(payloads).enumerate() {
        match (index, bucket.label.as_str()) {
            (0, "Session") => {
                conn.execute(
                    "INSERT INTO account_usage_snapshots (
                    provider, account_key_hash, account_label, source, confidence, window_kind,
                    used_amount, used_unit, limit_amount, limit_unit, resets_at, fetched_at,
                    expires_at, status, last_error, focused_provider, plan_label,
                    remaining_percent, used_label, limit_label, reset_label, pace_label,
                    view_status, updated_label, status_bar_label, quota_bucket_json
                ) VALUES (
                    'Codex', 'legacy-bucket-key', 'historic@example.test', 'provider_api',
                    'authoritative', 'Session', ?1, 'tokens', ?2, 'tokens', ?3, ?4,
                    ?5, 'stale', 'historic scalar error', 'OpenAI', 'Historic Pro',
                    57, 'historic used', 'historic limit', 'historic reset', 'historic pace',
                    'stale', 'historic update', 'historic status bar', ?6
                )",
                    jackin_usage_store_backend::params![
                        i64::MAX,
                        i64::MIN,
                        1_900_000_000_i64,
                        1_800_000_000_i64,
                        1_800_000_600_i64,
                        encoded,
                    ],
                )
                .await
                .map_err(|error| error.to_string())?;
            }
            (1, "Unlimited") => {
                conn.execute(
                    "INSERT INTO account_usage_snapshots (
                    provider, account_key_hash, account_label, source, confidence, window_kind,
                    used_amount, used_unit, limit_amount, limit_unit, resets_at, fetched_at,
                    expires_at, status, last_error, focused_provider, plan_label,
                    remaining_percent, used_label, limit_label, reset_label, pace_label,
                    view_status, updated_label, status_bar_label, quota_bucket_json
                ) VALUES (
                    'Codex', 'legacy-bucket-key', 'historic@example.test', 'provider_api',
                    'authoritative', 'Unlimited', NULL, NULL, NULL, NULL, NULL, ?1,
                    NULL, 'fresh', NULL, 'OpenAI', 'Historic Pro', NULL, NULL, NULL,
                    NULL, NULL, 'fresh', 'Updated just now', 'unlimited quota', ?2
                )",
                    jackin_usage_store_backend::params![1_800_000_000_i64, encoded],
                )
                .await
                .map_err(|error| error.to_string())?;
            }
            (2, "Spend") => {
                conn.execute(
                    "INSERT INTO account_usage_snapshots (
                        provider, account_key_hash, account_label, source, confidence, window_kind,
                        used_amount, used_unit, limit_amount, limit_unit, resets_at, fetched_at,
                        expires_at, status, last_error, focused_provider, plan_label,
                        remaining_percent, used_label, limit_label, reset_label, pace_label,
                        view_status, updated_label, status_bar_label, quota_bucket_json
                    ) VALUES (
                        'Codex', 'legacy-bucket-key', 'historic@example.test', 'provider_api',
                        'authoritative', 'Spend', ?1, 'tokens', ?2, 'tokens', ?3, ?4,
                        ?5, 'stale', 'historic scalar error', 'OpenAI', 'Historic Pro',
                        57, 'historic used', 'historic limit', 'historic reset', 'historic pace',
                        'stale', 'historic update', 'historic status bar', ?6
                    )",
                    jackin_usage_store_backend::params![
                        i64::MAX,
                        i64::MIN,
                        1_900_000_000_i64,
                        1_800_000_000_i64,
                        1_800_000_600_i64,
                        encoded,
                    ],
                )
                .await
                .map_err(|error| error.to_string())?;
            }
            _ => {
                return Err(format!(
                    "unexpected schema 5 fixture row {index} for bucket {:?}",
                    bucket.label
                ));
            }
        }
    }
    Ok(())
}

async fn seed_v5_refusal_snapshot(
    conn: &jackin_usage_store_backend::Connection,
    mode: &str,
) -> Result<(), String> {
    let shape = V5RefusalShape::from_mode(mode)
        .ok_or_else(|| format!("unknown schema 5 refusal seed mode {mode}"))?;
    seed_v5_refusal_schema(conn, shape).await?;
    seed_v5_refusal_row(conn, shape).await
}

#[derive(Clone, Copy)]
struct V5RefusalShape {
    capability_columns: V5RefusalCapabilityColumns,
    quota_payload: V5RefusalPayload,
    wrong_used_type: bool,
    missing_plan: bool,
    unique_key: V5RefusalUniqueKey,
}

#[derive(Clone, Copy)]
enum V5RefusalCapabilityColumns {
    None,
    AccountOnly,
    SurfaceOnly,
    Both,
}

#[derive(Clone, Copy)]
enum V5RefusalPayload {
    Omitted,
    Valid,
    Null,
    Malformed,
}

#[derive(Clone, Copy)]
enum V5RefusalUniqueKey {
    Correct,
    Missing,
    Incorrect,
}

impl V5RefusalShape {
    fn from_mode(mode: &str) -> Option<Self> {
        let empty = Self {
            capability_columns: V5RefusalCapabilityColumns::None,
            quota_payload: V5RefusalPayload::Valid,
            wrong_used_type: false,
            missing_plan: false,
            unique_key: V5RefusalUniqueKey::Correct,
        };
        Some(match mode {
            "v5_capability_only_no_payload" => Self {
                capability_columns: V5RefusalCapabilityColumns::Both,
                quota_payload: V5RefusalPayload::Omitted,
                ..empty
            },
            "v5_capability_account_only" => Self {
                capability_columns: V5RefusalCapabilityColumns::AccountOnly,
                ..empty
            },
            "v5_capability_surface_only" => Self {
                capability_columns: V5RefusalCapabilityColumns::SurfaceOnly,
                ..empty
            },
            "v5_capability_both" => Self {
                capability_columns: V5RefusalCapabilityColumns::Both,
                ..empty
            },
            "v5_null_payload" => Self {
                quota_payload: V5RefusalPayload::Null,
                ..empty
            },
            "v5_malformed_payload" => Self {
                quota_payload: V5RefusalPayload::Malformed,
                ..empty
            },
            "v5_wrong_type" => Self {
                wrong_used_type: true,
                ..empty
            },
            "v5_missing_column" => Self {
                missing_plan: true,
                ..empty
            },
            "v5_missing_key" => Self {
                unique_key: V5RefusalUniqueKey::Missing,
                ..empty
            },
            "v5_incorrect_key" => Self {
                unique_key: V5RefusalUniqueKey::Incorrect,
                ..empty
            },
            _ => return None,
        })
    }
}

async fn seed_v5_refusal_schema(
    conn: &jackin_usage_store_backend::Connection,
    shape: V5RefusalShape,
) -> Result<(), String> {
    let mut definitions = vec![
        "id INTEGER PRIMARY KEY".to_owned(),
        "provider TEXT NOT NULL".to_owned(),
        "account_key_hash TEXT NOT NULL".to_owned(),
        "account_label TEXT NOT NULL".to_owned(),
        "source TEXT NOT NULL".to_owned(),
        "confidence TEXT NOT NULL".to_owned(),
        "window_kind TEXT NOT NULL".to_owned(),
        if shape.wrong_used_type {
            "used_amount TEXT".to_owned()
        } else {
            "used_amount INTEGER".to_owned()
        },
        "used_unit TEXT".to_owned(),
        "limit_amount INTEGER".to_owned(),
        "limit_unit TEXT".to_owned(),
        "resets_at INTEGER".to_owned(),
        "fetched_at INTEGER NOT NULL".to_owned(),
        "expires_at INTEGER".to_owned(),
        "status TEXT NOT NULL".to_owned(),
        "last_error TEXT".to_owned(),
        "focused_provider TEXT".to_owned(),
    ];
    if !shape.missing_plan {
        definitions.push("plan_label TEXT".to_owned());
    }
    definitions.extend([
        "remaining_percent INTEGER".to_owned(),
        "used_label TEXT".to_owned(),
        "limit_label TEXT".to_owned(),
        "reset_label TEXT".to_owned(),
        "pace_label TEXT".to_owned(),
        "view_status TEXT NOT NULL DEFAULT 'unavailable'".to_owned(),
        "updated_label TEXT NOT NULL DEFAULT 'Unavailable'".to_owned(),
        "status_bar_label TEXT NOT NULL DEFAULT 'usage unavailable'".to_owned(),
    ]);
    if !matches!(shape.quota_payload, V5RefusalPayload::Omitted) {
        definitions.push("quota_bucket_json TEXT".to_owned());
    }
    if matches!(
        shape.capability_columns,
        V5RefusalCapabilityColumns::AccountOnly | V5RefusalCapabilityColumns::Both
    ) {
        definitions.push("capability_account_id TEXT".to_owned());
    }
    if matches!(
        shape.capability_columns,
        V5RefusalCapabilityColumns::SurfaceOnly | V5RefusalCapabilityColumns::Both
    ) {
        definitions.push("capability_surface_id TEXT".to_owned());
    }
    match shape.unique_key {
        V5RefusalUniqueKey::Missing => {}
        V5RefusalUniqueKey::Incorrect => {
            definitions.push("UNIQUE(provider, account_key_hash, source)".to_owned());
        }
        V5RefusalUniqueKey::Correct => {
            definitions.push("UNIQUE(provider, account_key_hash, source, window_kind)".to_owned());
        }
    }
    conn.execute_batch(&format!(
        "CREATE TABLE account_usage_snapshots ({});
         CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO _meta (key, value) VALUES ('schema_version', '5');
         INSERT INTO _meta (key, value)
         VALUES ('schema_version_before_quota_v5', '4');",
        definitions.join(",\n            ")
    ))
    .await
    .map_err(|error| error.to_string())?;

    Ok(())
}

async fn seed_v5_refusal_row(
    conn: &jackin_usage_store_backend::Connection,
    shape: V5RefusalShape,
) -> Result<(), String> {
    let mut columns = vec![
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
    ];
    let mut values = vec![
        "'Codex'".to_owned(),
        "'legacy-bucket-key'".to_owned(),
        "'historic@example.test'".to_owned(),
        "'provider_api'".to_owned(),
        "'authoritative'".to_owned(),
        "'Session'".to_owned(),
        if shape.wrong_used_type {
            "'9223372036854775807'".to_owned()
        } else {
            "9223372036854775807".to_owned()
        },
        "'tokens'".to_owned(),
        "-9223372036854775807".to_owned(),
        "'tokens'".to_owned(),
        "1900000000".to_owned(),
        "1800000000".to_owned(),
        "1800000600".to_owned(),
        "'stale'".to_owned(),
        "'historic scalar error'".to_owned(),
        "'OpenAI'".to_owned(),
    ];
    if !shape.missing_plan {
        columns.push("plan_label");
        values.push("'Historic Pro'".to_owned());
    }
    columns.extend([
        "remaining_percent",
        "used_label",
        "limit_label",
        "reset_label",
        "pace_label",
        "view_status",
        "updated_label",
        "status_bar_label",
    ]);
    values.extend([
        "57".to_owned(),
        "'historic used'".to_owned(),
        "'historic limit'".to_owned(),
        "'historic reset'".to_owned(),
        "'historic pace'".to_owned(),
        "'stale'".to_owned(),
        "'historic update'".to_owned(),
        "'historic status bar'".to_owned(),
    ]);
    if !matches!(shape.quota_payload, V5RefusalPayload::Omitted) {
        columns.push("quota_bucket_json");
        let encoded = match shape.quota_payload {
            V5RefusalPayload::Omitted => unreachable!(),
            V5RefusalPayload::Valid => sql_literal(&v5_seed_payloads(&v5_typed_buckets())[0]),
            V5RefusalPayload::Null => "NULL".to_owned(),
            V5RefusalPayload::Malformed => "'{broken'".to_owned(),
        };
        values.push(encoded);
    }
    if matches!(
        shape.capability_columns,
        V5RefusalCapabilityColumns::AccountOnly | V5RefusalCapabilityColumns::Both
    ) {
        columns.push("capability_account_id");
        values.push("'historic-account'".to_owned());
    }
    if matches!(
        shape.capability_columns,
        V5RefusalCapabilityColumns::SurfaceOnly | V5RefusalCapabilityColumns::Both
    ) {
        columns.push("capability_surface_id");
        values.push("'codex'".to_owned());
    }
    conn.execute(
        &format!(
            "INSERT INTO account_usage_snapshots ({}) VALUES ({})",
            columns.join(", "),
            values.join(", ")
        ),
        (),
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(super) fn v5_seed_payloads(buckets: &[QuotaBucketView]) -> Vec<String> {
    buckets
        .iter()
        .enumerate()
        .map(|(index, bucket)| {
            let encoded =
                serde_json::to_string(bucket).expect("encode deterministic schema 5 quota fixture");
            if index == 0 {
                format!("\n  {encoded}\t")
            } else {
                encoded
            }
        })
        .collect()
}

pub(super) fn v5_typed_buckets() -> Vec<QuotaBucketView> {
    let adjacent_used = f64::from_bits(43.375_f64.to_bits() + 1);
    let adjacent_remaining = f64::from_bits(57.125_f64.to_bits() + 1);
    let session = QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
        label: "Session".to_owned(),
        used_label: Some("wide count and money".to_owned()),
        limit_label: Some("u64 maximum".to_owned()),
        used_percent: Some(
            jackin_protocol::control::CanonicalUsedPercent::new(adjacent_used).unwrap(),
        ),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        remaining_percent: Some(CanonicalRemainingPercent::new(adjacent_remaining).unwrap()),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        count_quota: Some(CountQuota::new(
            Some(u64::MAX),
            Some(u64::MAX),
            Some(0),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
            CountQuotaProvenance::ProviderReported,
        )),
        provider_quota_state: Some(jackin_protocol::control::ProviderQuotaState::Exhausted),
        reset_label: Some("historic reset".to_owned()),
        resets_at: Some(1_900_000_000),
        status_slot: None,
        pace_label: Some("historic pace".to_owned()),
        status: UsageSnapshotStatus::Stale,
    };
    let mut spend = session.clone();
    spend.label = "Spend".to_owned();
    spend.used_money = Some(Money::new(i64::MIN, "USD", 4));
    spend.limit_money = Some(Money::new(i64::MAX, "USD", 7));
    spend.count_quota = None;
    spend.provider_quota_state = None;
    spend.used_percent = None;
    spend.used_percent_origin = None;
    spend.remaining_percent = None;
    spend.remaining_percent_origin = None;
    let unlimited = QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
        label: "Unlimited".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: None,
        provider_quota_state: Some(jackin_protocol::control::ProviderQuotaState::Unlimited),
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
    };
    let buckets = vec![session, unlimited, spend];
    for bucket in &buckets {
        bucket
            .validate()
            .expect("positive schema 5 bucket fixture is valid");
    }
    buckets
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
                severity: UsageSeverity::default(),
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
                severity: UsageSeverity::default(),
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
            severity: UsageSeverity::default(),
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
