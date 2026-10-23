// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use jackin_protocol::control::{
    CanonicalRemainingPercent, CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit,
    Money, PercentObservationOrigin, ProviderQuotaState, UsageSnapshotStatus,
};
use jackin_protocol::usage_broker::UsageAccountCapability;
use jackin_usage_provider_core::UsageCache;

use super::super::identity;

fn uppercase_fixture_hex(bytes: &[u8]) -> String {
    let mut expected_hex = String::new();
    for byte in bytes {
        write!(&mut expected_hex, "{byte:02X}").expect("write exact fixture hex");
    }
    expected_hex
}

const SCHEMA2_REFUSALS: &[(&str, &str)] = &[
    (
        "schema2_missing_json",
        "schema 2 is missing quota_bucket_json",
    ),
    (
        "schema2_account_column",
        "unexpected capability identity columns",
    ),
    (
        "schema2_surface_column",
        "unexpected capability identity columns",
    ),
    (
        "schema2_both_capability_columns",
        "unexpected capability identity columns",
    ),
    ("schema2_wrong_type", "incompatible type for provider"),
    ("schema2_mixed_affinity", "incompatible type for provider"),
    (
        "schema2_nullable_provider",
        "incompatible nullability for provider",
    ),
    (
        "schema2_id_not_primary_key",
        "rowid-generating INTEGER PRIMARY KEY",
    ),
    (
        "schema2_id_desc_primary_key",
        "rowid-generating INTEGER PRIMARY KEY",
    ),
    (
        "schema2_id_int_primary_key",
        "rowid-generating INTEGER PRIMARY KEY",
    ),
    (
        "schema2_id_composite_primary_key",
        "rowid-generating INTEGER PRIMARY KEY",
    ),
    (
        "schema2_id_without_rowid",
        "rowid-generating INTEGER PRIMARY KEY",
    ),
    (
        "schema2_required_quota",
        "incompatible nullability for quota_bucket_json",
    ),
    ("schema2_missing_required", "schema 2 is missing last_error"),
    (
        "schema2_wrong_unique",
        "missing the required unique account key",
    ),
    (
        "schema2_missing_unique",
        "missing the required unique account key",
    ),
    ("schema2_malformed", "decode quota_bucket_json"),
    ("schema2_reserved_unbound", "reserved capability namespace"),
    (
        "schema2_blob_provider",
        "provider and account key must be TEXT",
    ),
    ("schema2_blob_key", "provider and account key must be TEXT"),
    ("schema2_conflicting_provider_group", "conflicting provider"),
];
const SCHEMA3_REFUSALS: &[(&str, &str)] = &[
    ("schema3_missing_rows", "missing account_usage_snapshots"),
    (
        "schema3_no_capability_columns",
        "schema 3 is missing capability_account_id",
    ),
    (
        "schema3_account_column",
        "schema 3 is missing capability_surface_id",
    ),
    (
        "schema3_surface_column",
        "schema 3 is missing capability_account_id",
    ),
    ("schema3_wrong_type", "incompatible type for provider"),
    ("schema3_mixed_affinity", "incompatible type for provider"),
    (
        "schema3_nullable_provider",
        "incompatible nullability for provider",
    ),
    (
        "schema3_id_not_primary_key",
        "rowid-generating INTEGER PRIMARY KEY",
    ),
    (
        "schema3_id_desc_primary_key",
        "rowid-generating INTEGER PRIMARY KEY",
    ),
    (
        "schema3_id_int_primary_key",
        "rowid-generating INTEGER PRIMARY KEY",
    ),
    (
        "schema3_id_composite_primary_key",
        "rowid-generating INTEGER PRIMARY KEY",
    ),
    (
        "schema3_id_without_rowid",
        "rowid-generating INTEGER PRIMARY KEY",
    ),
    ("schema3_missing_required", "schema 3 is missing last_error"),
    (
        "schema3_required_quota",
        "incompatible nullability for quota_bucket_json",
    ),
    (
        "schema3_required_capability",
        "capability columns must be nullable",
    ),
    (
        "schema3_wrong_unique",
        "missing the required unique account key",
    ),
    (
        "schema3_missing_unique",
        "missing the required unique account key",
    ),
    ("schema3_half_pair", "both NULL or both present"),
    (
        "schema3_blob_account_id",
        "capability cells must be NULL or TEXT",
    ),
    ("schema3_invalid_id", "account_id"),
    ("schema3_wrong_provider", "does not match provider"),
    ("schema3_wrong_key", "does not match account_key_hash"),
    ("schema3_conflicting_group", "conflicting provider"),
    ("schema3_reserved_unbound", "reserved capability namespace"),
    ("schema3_future", "newer than supported schema 3"),
    (
        "schema3_nonnumeric",
        "unsupported host usage cache schema version",
    ),
];

const CHILD_MODE: &str = "JACKIN_CLI_USAGE_CHILD_MODE";
const CHILD_ROOT: &str = "JACKIN_CLI_USAGE_CHILD_ROOT";
const CHILD_TIMEOUT: Duration = Duration::from_secs(45);
const CHILD_TEST: &str = "cli::usage::store::tests::case_01::cli_usage_child_entry";

#[derive(Clone, Copy, PartialEq, Eq)]
enum TextColumnShape {
    Missing,
    Nullable,
    Required,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CapabilityColumnShape {
    None,
    AccountOnly,
    SurfaceOnly,
    PairNullable,
    PairRequired,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProviderColumnShape {
    TextRequired,
    TextNullable,
    BlobRequired,
    IntegerTextRequired,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IdColumnShape {
    IntegerPrimaryKey,
    IntegerWithoutPrimaryKey,
    IntegerPrimaryKeyDesc,
    IntPrimaryKey,
    CompositePrimaryKey,
    IntegerPrimaryKeyWithoutRowid,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UniqueKeyShape {
    Complete,
    Partial,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExtraConstraint {
    None,
    BlockedWindow,
    DeferredKeyReference,
}

#[derive(Clone, Copy)]
struct FixtureShape {
    id: IdColumnShape,
    quota: TextColumnShape,
    capability_columns: CapabilityColumnShape,
    provider: ProviderColumnShape,
    last_error: TextColumnShape,
    unique_key: UniqueKeyShape,
    extra_constraint: ExtraConstraint,
}

struct FixtureRow<'a> {
    provider: &'a str,
    key: &'a str,
    label: &'a str,
    window: &'a str,
    quota_json: Option<&'a str>,
    capability_account_id: Option<&'a str>,
    capability_surface_id: Option<&'a str>,
}

fn fixture_row<'a>(
    provider: &'a str,
    key: &'a str,
    label: &'a str,
    window: &'a str,
    quota_json: Option<&'a str>,
    capability_account_id: Option<&'a str>,
    capability_surface_id: Option<&'a str>,
) -> FixtureRow<'a> {
    FixtureRow {
        provider,
        key,
        label,
        window,
        quota_json,
        capability_account_id,
        capability_surface_id,
    }
}

const LEGACY_V2_SHAPE: FixtureShape = FixtureShape {
    id: IdColumnShape::IntegerPrimaryKey,
    quota: TextColumnShape::Nullable,
    capability_columns: CapabilityColumnShape::None,
    provider: ProviderColumnShape::TextRequired,
    last_error: TextColumnShape::Nullable,
    unique_key: UniqueKeyShape::Complete,
    extra_constraint: ExtraConstraint::None,
};

const CURRENT_V3_SHAPE: FixtureShape = FixtureShape {
    capability_columns: CapabilityColumnShape::PairNullable,
    ..LEGACY_V2_SHAPE
};

fn capability(account_id: &str, surface_id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: account_id.to_owned(),
        surface_id: surface_id.to_owned(),
    }
}

fn quota_bucket(window: &str) -> QuotaBucketView {
    QuotaBucketView {
        label: window.to_owned(),
        used_label: Some("exact used observation".to_owned()),
        limit_label: Some("exact limit observation".to_owned()),
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: None,
        remaining_percent_origin: None,
        count_quota: None,
        provider_quota_state: None,
        reset_label: Some("exact reset label".to_owned()),
        resets_at: Some(i64::MIN),
        status_slot: None,
        pace_label: Some("exact pace label".to_owned()),
        status: UsageSnapshotStatus::Stale,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Warn,
    }
}

fn money_bucket() -> QuotaBucketView {
    let mut bucket = quota_bucket("Session");
    bucket.used_percent = Some(CanonicalUsedPercent::new(43.375).unwrap());
    bucket.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    bucket.remaining_percent = Some(CanonicalRemainingPercent::new(56.625).unwrap());
    bucket.remaining_percent_origin = Some(PercentObservationOrigin::Derived);
    bucket.used_money = Some(Money::new(i64::MIN, "USD", 4));
    bucket.limit_money = Some(Money::new(i64::MAX, "USD", 7));
    bucket
}

fn count_bucket() -> QuotaBucketView {
    let mut bucket = quota_bucket("Session");
    bucket.count_quota = Some(CountQuota::new(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::Unknown,
        CountQuotaProvenance::ProviderReported,
    ));
    bucket.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    bucket
}

fn requests_bucket() -> QuotaBucketView {
    let mut bucket = quota_bucket("Requests");
    bucket.used_percent = Some(CanonicalUsedPercent::new(43.375).unwrap());
    bucket.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    bucket.remaining_percent = Some(CanonicalRemainingPercent::new(57.125).unwrap());
    bucket.remaining_percent_origin = Some(PercentObservationOrigin::Derived);
    bucket.count_quota = Some(CountQuota::new(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    bucket.provider_quota_state = Some(ProviderQuotaState::Exhausted);
    bucket
}

fn spend_bucket() -> QuotaBucketView {
    let mut bucket = money_bucket();
    bucket.label = "Spend".to_owned();
    bucket.used_percent =
        Some(CanonicalUsedPercent::new(f64::from_bits(43.375_f64.to_bits() + 1)).unwrap());
    bucket.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    bucket.remaining_percent =
        Some(CanonicalRemainingPercent::new(f64::from_bits(57.125_f64.to_bits() + 1)).unwrap());
    bucket.remaining_percent_origin = Some(PercentObservationOrigin::Derived);
    bucket
}

fn unlimited_bucket() -> QuotaBucketView {
    let mut bucket = quota_bucket("Unlimited");
    bucket.provider_quota_state = Some(ProviderQuotaState::Unlimited);
    bucket
}

fn cached_view(
    agent: &str,
    provider: &str,
    label: &str,
    bucket: QuotaBucketView,
) -> jackin_protocol::control::FocusedUsageView {
    let mut view = jackin_protocol::control::FocusedUsageView::unavailable(agent, 1_781_190_000);
    view.focused_agent = Some(agent.to_owned());
    view.focused_provider = Some(provider.to_owned());
    view.account.provider_label = provider.to_owned();
    view.account.account_label = label.to_owned();
    view.buckets = vec![bucket];
    view.status = UsageSnapshotStatus::Stale;
    view.source = jackin_protocol::control::UsageSource::ProviderApi;
    view.confidence = jackin_protocol::control::UsageConfidence::Authoritative;
    view.fetched_at_epoch = 1_781_190_000;
    view.last_error = Some("retained source error".to_owned());
    view
}

fn flattened_bound_rows() -> Vec<AccountUsageSnapshotView> {
    let personal = capability("account-personal", "codex");
    let work = capability("account-work", "codex");
    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_capability_for_test(
        "codex",
        Some("OpenAI"),
        &personal,
        cached_view("codex", "OpenAI", "shared@example.test", money_bucket()),
    );
    cache.insert_snapshot_for_capability_for_test(
        "codex",
        Some("OpenAI"),
        &work,
        cached_view("codex", "OpenAI", "shared@example.test", count_bucket()),
    );
    let other_surface = capability("account-personal", "opencode");
    cache.insert_snapshot_for_capability_for_test(
        "opencode",
        Some("OpenCode"),
        &other_surface,
        cached_view(
            "opencode",
            "OpenCode",
            "shared@example.test",
            unlimited_bucket(),
        ),
    );
    cache.account_snapshot_views()
}

fn legacy_account(label: &str, window: &str) -> AccountUsageSnapshotView {
    let mut row = account(window, 31);
    row.provider = "OpenAI".to_owned();
    row.account_label = label.to_owned();
    row.source = "provider_api".to_owned();
    row.confidence = "authoritative".to_owned();
    row.used_amount = Some(i64::MIN);
    row.used_unit = Some("historic used unit".to_owned());
    row.limit_amount = Some(i64::MAX);
    row.limit_unit = Some("historic limit unit".to_owned());
    row.resets_at = Some(i64::MIN);
    row.fetched_at = i64::MAX;
    row.expires_at = Some(i64::MIN);
    row.status = "stale".to_owned();
    row.last_error = Some("historic scalar error".to_owned());
    row.quota_bucket = Some(count_bucket());
    row.account_capability = None;
    row
}

fn expected_process_rows() -> Vec<AccountUsageSnapshotView> {
    flattened_bound_rows()
}

fn run_child(mode: &str, root: &Path) {
    let logs = tempfile::tempdir().expect("create bounded CLI child log directory");
    let stdout_path = logs.path().join("stdout.log");
    let stderr_path = logs.path().join("stderr.log");
    let stdout = File::create(&stdout_path).expect("create CLI child stdout log");
    let stderr = File::create(&stderr_path).expect("create CLI child stderr log");
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .arg("--exact")
        .arg(CHILD_TEST)
        .arg("--nocapture")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .env(CHILD_MODE, mode)
        .env(CHILD_ROOT, root);
    let mut child = command.spawn().expect("start bounded CLI child");
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < CHILD_TIMEOUT => {
                std::thread::park_timeout(Duration::from_millis(20));
            }
            Ok(None) => {
                drop(child.kill());
                drop(child.wait());
                panic!(
                    "CLI child {mode} exceeded {CHILD_TIMEOUT:?}; stdout={} stderr={}",
                    read_child_log(&stdout_path),
                    read_child_log(&stderr_path)
                );
            }
            Err(error) => {
                drop(child.kill());
                drop(child.wait());
                panic!("poll CLI child {mode} failed: {error}");
            }
        }
    };
    let stdout_text = read_child_log(&stdout_path);
    let stderr_text = read_child_log(&stderr_path);
    assert!(
        status.success(),
        "CLI child {mode} failed with {status}; stdout={stdout_text} stderr={stderr_text}"
    );
    let marker = format!("JACKIN_CLI_USAGE_CHILD_COMPLETE:{mode}");
    assert!(
        stdout_text.contains(&marker),
        "CLI child {mode} omitted completion marker; stdout={stdout_text} stderr={stderr_text}"
    );
}

fn read_child_log(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| format!("<unable to read child log: {error}>"))
}

#[test]
fn cli_usage_child_entry() {
    let Ok(mode) = std::env::var(CHILD_MODE) else {
        return;
    };
    let root =
        std::path::PathBuf::from(std::env::var(CHILD_ROOT).expect("CLI usage child data root"));
    let paths = JackinPaths::for_tests(&root);
    let path = host_account_cache_path(&paths);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("CLI usage child runtime");
    runtime.block_on(async {
        if let Some(fixture) = mode.strip_prefix("seed_") {
            seed_fixture(&path, fixture).await;
        } else {
            match mode.as_str() {
                "migrate_schema2_typed" => {
                    let (_, rows) = read_accounts(&paths).await.expect("migrate schema 2");
                    assert_eq!(rows, schema2_expected_rows("schema2_typed"));
                    assert_typed_migration_values(&rows);
                    assert_eq!(schema_version(&path).await.unwrap().as_deref(), Some("3"));
                }
                "migrate_schema2_null" => {
                    let (_, rows) = read_accounts(&paths)
                        .await
                        .expect("migrate schema 2 NULL row");
                    assert_eq!(rows, schema2_expected_rows("schema2_null"));
                    assert_eq!(schema_version(&path).await.unwrap().as_deref(), Some("3"));
                }
                "migrate_prequota" => {
                    let (_, rows) = read_accounts(&paths)
                        .await
                        .expect("migrate pre-quota schema");
                    assert_eq!(rows, [prequota_expected()]);
                    assert_eq!(schema_version(&path).await.unwrap().as_deref(), Some("3"));
                }
                "roundtrip_write" => {
                    let rows = expected_process_rows();
                    upsert_accounts(&paths, &rows)
                        .await
                        .expect("write rows through managed CLI store");
                }
                "roundtrip_read" => {
                    let (_, rows) = read_accounts(&paths)
                        .await
                        .expect("read CLI store in child");
                    assert_complete_capability_rows(&rows, &expected_process_rows());
                }
                _ => panic!("unknown CLI usage child mode {mode}"),
            }
        }
    });
    println!("JACKIN_CLI_USAGE_CHILD_COMPLETE:{mode}");
}

fn assert_complete_capability_rows(
    actual: &[AccountUsageSnapshotView],
    expected: &[AccountUsageSnapshotView],
) {
    assert_eq!(actual.len(), expected.len());
    for expected_row in expected {
        let actual_row = actual
            .iter()
            .find(|row| row.account_capability == expected_row.account_capability)
            .expect("reloaded complete capability row");
        assert_eq!(actual_row, expected_row);
    }
}

fn fixture_table_sql(shape: FixtureShape) -> String {
    let id_column = match shape.id {
        IdColumnShape::IntegerPrimaryKey => "id INTEGER PRIMARY KEY",
        IdColumnShape::IntegerWithoutPrimaryKey => "id INTEGER",
        IdColumnShape::IntegerPrimaryKeyDesc => "id INTEGER PRIMARY KEY DESC",
        IdColumnShape::IntPrimaryKey => "id INT PRIMARY KEY",
        IdColumnShape::CompositePrimaryKey => "id INTEGER",
        IdColumnShape::IntegerPrimaryKeyWithoutRowid => "id INTEGER PRIMARY KEY",
    };
    let provider_column = match shape.provider {
        ProviderColumnShape::TextRequired => "provider TEXT NOT NULL",
        ProviderColumnShape::TextNullable => "provider TEXT",
        ProviderColumnShape::BlobRequired => "provider BLOB NOT NULL",
        ProviderColumnShape::IntegerTextRequired => "provider INTTEXT NOT NULL",
    };
    let mut columns = vec![
        id_column.to_owned(),
        provider_column.to_owned(),
        "account_key_hash TEXT NOT NULL".to_owned(),
        "account_label TEXT NOT NULL".to_owned(),
        "source TEXT NOT NULL".to_owned(),
        "confidence TEXT NOT NULL".to_owned(),
        "window_kind TEXT NOT NULL".to_owned(),
        "used_amount INTEGER".to_owned(),
        "used_unit TEXT".to_owned(),
        "limit_amount INTEGER".to_owned(),
        "limit_unit TEXT".to_owned(),
        "resets_at INTEGER".to_owned(),
        "fetched_at INTEGER NOT NULL".to_owned(),
        "expires_at INTEGER".to_owned(),
        "status TEXT NOT NULL".to_owned(),
    ];
    if shape.id == IdColumnShape::CompositePrimaryKey {
        columns.push("composite_key TEXT NOT NULL".to_owned());
    }
    match shape.last_error {
        TextColumnShape::Missing => {}
        TextColumnShape::Nullable => columns.push("last_error TEXT".to_owned()),
        TextColumnShape::Required => columns.push("last_error TEXT NOT NULL".to_owned()),
    }
    match shape.quota {
        TextColumnShape::Missing => {}
        TextColumnShape::Nullable => columns.push("quota_bucket_json TEXT".to_owned()),
        TextColumnShape::Required => {
            columns.push("quota_bucket_json TEXT NOT NULL".to_owned());
        }
    }
    match shape.capability_columns {
        CapabilityColumnShape::None => {}
        CapabilityColumnShape::AccountOnly => {
            columns.push("capability_account_id TEXT".to_owned());
        }
        CapabilityColumnShape::SurfaceOnly => {
            columns.push("capability_surface_id TEXT".to_owned());
        }
        CapabilityColumnShape::PairNullable => {
            columns.push("capability_account_id TEXT".to_owned());
            columns.push("capability_surface_id TEXT".to_owned());
        }
        CapabilityColumnShape::PairRequired => {
            columns.push("capability_account_id TEXT NOT NULL".to_owned());
            columns.push("capability_surface_id TEXT NOT NULL".to_owned());
        }
    }
    if shape.id == IdColumnShape::CompositePrimaryKey {
        columns.push("PRIMARY KEY(id, composite_key)".to_owned());
    }
    let unique = match shape.unique_key {
        UniqueKeyShape::Complete => "UNIQUE(provider, account_key_hash, source, window_kind)",
        UniqueKeyShape::Partial => "UNIQUE(provider, account_key_hash)",
        UniqueKeyShape::None => "",
    };
    if !unique.is_empty() {
        columns.push(unique.to_owned());
    }
    match shape.extra_constraint {
        ExtraConstraint::None => {}
        ExtraConstraint::BlockedWindow => {
            columns.push("CHECK(window_kind <> 'blocked')".to_owned());
        }
        ExtraConstraint::DeferredKeyReference => columns.push(
            "FOREIGN KEY(account_key_hash) REFERENCES commit_parent(account_key_hash) DEFERRABLE INITIALLY DEFERRED"
                .to_owned(),
        ),
    }
    let without_rowid = if shape.id == IdColumnShape::IntegerPrimaryKeyWithoutRowid {
        " WITHOUT ROWID"
    } else {
        ""
    };
    format!(
        "CREATE TABLE account_usage_snapshots ({}){without_rowid};",
        columns.join(",\n")
    )
}

async fn insert_fixture_row(conn: &Connection, shape: FixtureShape, fixture: FixtureRow<'_>) {
    let mut columns = Vec::new();
    let mut values = Vec::new();
    if matches!(
        shape.id,
        IdColumnShape::CompositePrimaryKey | IdColumnShape::IntegerPrimaryKeyWithoutRowid
    ) {
        columns.push("id");
        values.push("1".to_owned());
    }
    if shape.id == IdColumnShape::CompositePrimaryKey {
        columns.push("composite_key");
        values.push(sql_text("fixture"));
    }
    columns.extend([
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
    ]);
    values.extend([
        sql_text(fixture.provider),
        sql_text(fixture.key),
        sql_text(fixture.label),
        sql_text("provider_api"),
        sql_text("authoritative"),
        sql_text(fixture.window),
        i64::MIN.to_string(),
        sql_text("historic used unit"),
        i64::MAX.to_string(),
        sql_text("historic limit unit"),
        i64::MIN.to_string(),
        i64::MAX.to_string(),
        i64::MIN.to_string(),
        sql_text("stale"),
    ]);
    if !matches!(shape.last_error, TextColumnShape::Missing) {
        columns.push("last_error");
        values.push(sql_text("historic failure"));
    }
    if !matches!(shape.quota, TextColumnShape::Missing) {
        columns.push("quota_bucket_json");
        values.push(sql_option_text(fixture.quota_json));
    }
    match shape.capability_columns {
        CapabilityColumnShape::None => {}
        CapabilityColumnShape::AccountOnly => {
            columns.push("capability_account_id");
            values.push(sql_option_text(fixture.capability_account_id));
        }
        CapabilityColumnShape::SurfaceOnly => {
            columns.push("capability_surface_id");
            values.push(sql_option_text(fixture.capability_surface_id));
        }
        CapabilityColumnShape::PairNullable | CapabilityColumnShape::PairRequired => {
            columns.push("capability_account_id");
            values.push(sql_option_text(fixture.capability_account_id));
            columns.push("capability_surface_id");
            values.push(sql_option_text(fixture.capability_surface_id));
        }
    }
    let query = format!(
        "INSERT INTO account_usage_snapshots ({}) VALUES ({})",
        columns.join(", "),
        values.join(", ")
    );
    conn.execute_batch(&query)
        .await
        .expect("insert CLI schema fixture row");
}

fn sql_text(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sql_option_text(value: Option<&str>) -> String {
    value.map_or_else(|| "NULL".to_owned(), sql_text)
}

fn fixture_shape(mode: &str) -> FixtureShape {
    if mode.starts_with("schema2_") || matches!(mode, "schema0_legacy" | "unversioned_legacy") {
        legacy_fixture_shape(mode)
    } else {
        current_fixture_shape(mode)
    }
}

fn legacy_fixture_shape(mode: &str) -> FixtureShape {
    match mode {
        "schema2_typed"
        | "schema2_null"
        | "schema2_malformed"
        | "schema2_reserved_unbound"
        | "schema2_blob_provider"
        | "schema2_blob_key"
        | "schema2_conflicting_provider_group" => LEGACY_V2_SHAPE,
        "schema0_legacy" | "unversioned_legacy" => FixtureShape {
            quota: TextColumnShape::Missing,
            ..LEGACY_V2_SHAPE
        },
        "schema2_missing_json" => FixtureShape {
            quota: TextColumnShape::Missing,
            ..LEGACY_V2_SHAPE
        },
        "schema2_account_column" => FixtureShape {
            capability_columns: CapabilityColumnShape::AccountOnly,
            ..LEGACY_V2_SHAPE
        },
        "schema2_surface_column" => FixtureShape {
            capability_columns: CapabilityColumnShape::SurfaceOnly,
            ..LEGACY_V2_SHAPE
        },
        "schema2_both_capability_columns" => FixtureShape {
            capability_columns: CapabilityColumnShape::PairNullable,
            ..LEGACY_V2_SHAPE
        },
        "schema2_wrong_type" => FixtureShape {
            provider: ProviderColumnShape::BlobRequired,
            ..LEGACY_V2_SHAPE
        },
        "schema2_mixed_affinity" => FixtureShape {
            provider: ProviderColumnShape::IntegerTextRequired,
            ..LEGACY_V2_SHAPE
        },
        "schema2_nullable_provider" => FixtureShape {
            provider: ProviderColumnShape::TextNullable,
            ..LEGACY_V2_SHAPE
        },
        "schema2_id_not_primary_key" => FixtureShape {
            id: IdColumnShape::IntegerWithoutPrimaryKey,
            ..LEGACY_V2_SHAPE
        },
        "schema2_id_desc_primary_key" => FixtureShape {
            id: IdColumnShape::IntegerPrimaryKeyDesc,
            ..LEGACY_V2_SHAPE
        },
        "schema2_id_int_primary_key" => FixtureShape {
            id: IdColumnShape::IntPrimaryKey,
            ..LEGACY_V2_SHAPE
        },
        "schema2_id_composite_primary_key" => FixtureShape {
            id: IdColumnShape::CompositePrimaryKey,
            ..LEGACY_V2_SHAPE
        },
        "schema2_id_without_rowid" => FixtureShape {
            id: IdColumnShape::IntegerPrimaryKeyWithoutRowid,
            unique_key: UniqueKeyShape::None,
            ..LEGACY_V2_SHAPE
        },
        "schema2_required_quota" => FixtureShape {
            quota: TextColumnShape::Required,
            ..LEGACY_V2_SHAPE
        },
        "schema2_missing_required" => FixtureShape {
            last_error: TextColumnShape::Missing,
            ..LEGACY_V2_SHAPE
        },
        "schema2_wrong_unique" => FixtureShape {
            unique_key: UniqueKeyShape::Partial,
            ..LEGACY_V2_SHAPE
        },
        "schema2_missing_unique" => FixtureShape {
            unique_key: UniqueKeyShape::None,
            ..LEGACY_V2_SHAPE
        },
        _ => panic!("unknown CLI fixture mode {mode}"),
    }
}

fn current_fixture_shape(mode: &str) -> FixtureShape {
    match mode {
        "schema3_no_capability_columns" => FixtureShape {
            capability_columns: CapabilityColumnShape::None,
            ..CURRENT_V3_SHAPE
        },
        "schema3_account_column" => FixtureShape {
            capability_columns: CapabilityColumnShape::AccountOnly,
            ..CURRENT_V3_SHAPE
        },
        "schema3_surface_column" => FixtureShape {
            capability_columns: CapabilityColumnShape::SurfaceOnly,
            ..CURRENT_V3_SHAPE
        },
        "schema3_wrong_type" => FixtureShape {
            provider: ProviderColumnShape::BlobRequired,
            ..CURRENT_V3_SHAPE
        },
        "schema3_mixed_affinity" => FixtureShape {
            provider: ProviderColumnShape::IntegerTextRequired,
            ..CURRENT_V3_SHAPE
        },
        "schema3_nullable_provider" => FixtureShape {
            provider: ProviderColumnShape::TextNullable,
            ..CURRENT_V3_SHAPE
        },
        "schema3_id_not_primary_key" => FixtureShape {
            id: IdColumnShape::IntegerWithoutPrimaryKey,
            ..CURRENT_V3_SHAPE
        },
        "schema3_id_desc_primary_key" => FixtureShape {
            id: IdColumnShape::IntegerPrimaryKeyDesc,
            ..CURRENT_V3_SHAPE
        },
        "schema3_id_int_primary_key" => FixtureShape {
            id: IdColumnShape::IntPrimaryKey,
            ..CURRENT_V3_SHAPE
        },
        "schema3_id_composite_primary_key" => FixtureShape {
            id: IdColumnShape::CompositePrimaryKey,
            ..CURRENT_V3_SHAPE
        },
        "schema3_id_without_rowid" => FixtureShape {
            id: IdColumnShape::IntegerPrimaryKeyWithoutRowid,
            unique_key: UniqueKeyShape::None,
            ..CURRENT_V3_SHAPE
        },
        "schema3_missing_required" => FixtureShape {
            last_error: TextColumnShape::Missing,
            ..CURRENT_V3_SHAPE
        },
        "schema3_required_quota" => FixtureShape {
            quota: TextColumnShape::Required,
            ..CURRENT_V3_SHAPE
        },
        "schema3_required_capability" => FixtureShape {
            capability_columns: CapabilityColumnShape::PairRequired,
            ..CURRENT_V3_SHAPE
        },
        "schema3_wrong_unique" => FixtureShape {
            unique_key: UniqueKeyShape::Partial,
            ..CURRENT_V3_SHAPE
        },
        "schema3_missing_unique" => FixtureShape {
            unique_key: UniqueKeyShape::None,
            ..CURRENT_V3_SHAPE
        },
        "schema3_statement_failure" => FixtureShape {
            extra_constraint: ExtraConstraint::BlockedWindow,
            ..CURRENT_V3_SHAPE
        },
        "schema3_commit_failure" => FixtureShape {
            extra_constraint: ExtraConstraint::DeferredKeyReference,
            ..CURRENT_V3_SHAPE
        },
        "schema3_half_pair"
        | "schema3_blob_account_id"
        | "schema3_invalid_id"
        | "schema3_wrong_provider"
        | "schema3_wrong_key"
        | "schema3_conflicting_group"
        | "schema3_reserved_unbound"
        | "schema3_future"
        | "schema3_nonnumeric" => CURRENT_V3_SHAPE,
        _ => panic!("unknown CLI fixture mode {mode}"),
    }
}

async fn seed_fixture(path: &Path, mode: &str) {
    fs::create_dir_all(path.parent().expect("fixture database parent"))
        .expect("create fixture database parent");
    let path_text = path.to_str().expect("fixture path is UTF-8");
    let conn = if matches!(
        mode,
        "schema2_id_without_rowid" | "schema3_id_without_rowid"
    ) {
        jackin_usage_store_backend::connect_without_rowid_fixture(path_text)
            .await
            .expect("open isolated WITHOUT ROWID CLI fixture database")
    } else {
        jackin_usage::store_backend::connect_local(path_text)
            .await
            .expect("open isolated CLI fixture database")
    };
    if mode == "schema3_missing_rows" {
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta (key, value) VALUES ('schema_version', '3');",
        )
        .await
        .expect("seed schema 3 missing table");
        return;
    }

    let shape = fixture_shape(mode);
    if shape.extra_constraint == ExtraConstraint::DeferredKeyReference {
        conn.execute_batch(
            "CREATE TABLE commit_parent (account_key_hash TEXT PRIMARY KEY);
             CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .await
        .expect("create deferred-constraint fixture parent and metadata");
    } else {
        conn.execute_batch("CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .await
            .expect("create CLI fixture metadata");
    }
    let version = match mode {
        "schema3_future" => "4",
        "schema3_nonnumeric" => "future",
        "schema0_legacy" => "0",
        "unversioned_legacy" => "",
        mode if mode.starts_with("schema2_") => "2",
        _ => "3",
    };
    if !version.is_empty() {
        conn.execute(
            "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)",
            [version],
        )
        .await
        .expect("seed CLI fixture schema version");
    }
    if mode == "schema2_null" {
        conn.execute(
            "INSERT INTO _meta (key, value) VALUES ('schema_version_before_quota_v2', '1')",
            (),
        )
        .await
        .expect("seed upgraded-v1 schema 2 metadata");
    }
    conn.execute_batch(&fixture_table_sql(shape))
        .await
        .expect("create CLI fixture rows table");

    seed_fixture_rows(&conn, mode, shape).await;
}

async fn seed_fixture_rows(conn: &Connection, mode: &str, shape: FixtureShape) {
    let session_json = noncanonical_json(&quota_bucket("Session"));
    if mode.starts_with("schema2_") {
        if mode == "schema2_typed" {
            for (key, row) in [
                "legacy/arbitrary-requests-key",
                "legacy/arbitrary-spend-key",
                "legacy/arbitrary-unlimited-key",
            ]
            .into_iter()
            .zip(schema2_expected_rows("schema2_typed"))
            {
                let bucket = row.quota_bucket.as_ref().expect("typed legacy bucket");
                let quota = noncanonical_json(bucket);
                insert_fixture_row(
                    conn,
                    shape,
                    fixture_row(
                        &row.provider,
                        key,
                        &row.account_label,
                        &row.window_kind,
                        Some(&quota),
                        None,
                        None,
                    ),
                )
                .await;
            }
            return;
        }
        if mode == "schema2_conflicting_provider_group" {
            for provider in ["Codex", "OpenAI"] {
                insert_fixture_row(
                    conn,
                    shape,
                    fixture_row(
                        provider,
                        "legacy/shared-provider-key",
                        "historic@example.test",
                        "Session",
                        Some(&session_json),
                        None,
                        None,
                    ),
                )
                .await;
            }
            return;
        }
        if matches!(mode, "schema2_blob_provider" | "schema2_blob_key") {
            insert_blob_legacy_identity_row(conn, mode, &session_json).await;
            return;
        }
        let key = if mode == "schema2_reserved_unbound" {
            "capability-v1:legacy-unbound"
        } else {
            "legacy/arbitrary-key"
        };
        let quota = match mode {
            "schema2_null" | "schema2_missing_json" => None,
            "schema2_malformed" => Some("{broken"),
            _ => Some(session_json.as_str()),
        };
        insert_fixture_row(
            conn,
            shape,
            fixture_row(
                "Codex",
                key,
                "historic@example.test",
                "Session",
                quota,
                None,
                None,
            ),
        )
        .await;
        return;
    }

    if matches!(mode, "schema0_legacy" | "unversioned_legacy") {
        insert_fixture_row(
            conn,
            shape,
            fixture_row(
                "Codex",
                "legacy/arbitrary-key",
                "historic@example.test",
                "Session",
                None,
                None,
                None,
            ),
        )
        .await;
        return;
    }

    seed_current_v3_rows(conn, mode, shape, &session_json).await;
}

fn noncanonical_json(bucket: &QuotaBucketView) -> String {
    format!("\n  {} \t", serde_json::to_string(bucket).unwrap())
}

fn schema2_expected_rows(mode: &str) -> Vec<AccountUsageSnapshotView> {
    if mode == "schema2_typed" {
        return [requests_bucket(), spend_bucket(), unlimited_bucket()]
            .into_iter()
            .map(|bucket| {
                let mut row = legacy_account("historic@example.test", &bucket.label);
                row.provider = "Codex".to_owned();
                row.last_error = Some("historic failure".to_owned());
                row.quota_bucket = Some(bucket);
                row
            })
            .collect();
    }

    let mut row = legacy_account("historic@example.test", "Session");
    row.provider = "Codex".to_owned();
    row.quota_bucket = None;
    row.last_error = Some("historic failure".to_owned());
    vec![row]
}

async fn seed_valid_current_v3_row(
    conn: &Connection,
    shape: FixtureShape,
    noncanonical_json: &str,
) {
    let cap = capability("account-A", "codex");
    let key = cap.opaque_identity_key();
    insert_fixture_row(
        conn,
        shape,
        fixture_row(
            "Codex",
            &key,
            "historic@example.test",
            "Session",
            Some(noncanonical_json),
            Some(&cap.account_id),
            Some(&cap.surface_id),
        ),
    )
    .await;
}

async fn seed_statement_failure_rows(conn: &Connection, shape: FixtureShape) {
    for row in [
        legacy_account("rollback-session@example.test", "Session"),
        legacy_account("other-weekly@example.test", "Weekly"),
    ] {
        insert_fixture_row(
            conn,
            shape,
            fixture_row(
                &row.provider,
                &account_key_hash(&row.provider, &row.account_label),
                &row.account_label,
                &row.window_kind,
                Some(&serde_json::to_string(row.quota_bucket.as_ref().unwrap()).unwrap()),
                None,
                None,
            ),
        )
        .await;
    }
}

async fn seed_current_v3_rows(
    conn: &Connection,
    mode: &str,
    shape: FixtureShape,
    noncanonical_json: &str,
) {
    match mode {
        "schema3_no_capability_columns"
        | "schema3_account_column"
        | "schema3_surface_column"
        | "schema3_wrong_type"
        | "schema3_wrong_unique"
        | "schema3_missing_unique"
        | "schema3_mixed_affinity"
        | "schema3_nullable_provider"
        | "schema3_id_not_primary_key"
        | "schema3_id_desc_primary_key"
        | "schema3_id_int_primary_key"
        | "schema3_id_composite_primary_key"
        | "schema3_id_without_rowid"
        | "schema3_missing_required"
        | "schema3_required_quota"
        | "schema3_required_capability"
        | "schema3_future"
        | "schema3_nonnumeric" => {
            seed_valid_current_v3_row(conn, shape, noncanonical_json).await;
        }
        "schema3_half_pair" => {
            let cap = capability("account-A", "codex");
            let key = cap.opaque_identity_key();
            insert_fixture_row(
                conn,
                shape,
                fixture_row(
                    "Codex",
                    &key,
                    "historic@example.test",
                    "Session",
                    Some(noncanonical_json),
                    Some(&cap.account_id),
                    None,
                ),
            )
            .await;
        }
        "schema3_blob_account_id" => insert_blob_identity_row(conn).await,
        "schema3_invalid_id" => {
            let cap = capability("account id", "codex");
            let key = cap.opaque_identity_key();
            insert_fixture_row(
                conn,
                shape,
                fixture_row(
                    "Codex",
                    &key,
                    "historic@example.test",
                    "Session",
                    Some(noncanonical_json),
                    Some(&cap.account_id),
                    Some(&cap.surface_id),
                ),
            )
            .await;
        }
        "schema3_wrong_provider" => {
            let cap = capability("account-A", "codex");
            let key = cap.opaque_identity_key();
            insert_fixture_row(
                conn,
                shape,
                fixture_row(
                    "Claude",
                    &key,
                    "historic@example.test",
                    "Session",
                    Some(noncanonical_json),
                    Some(&cap.account_id),
                    Some(&cap.surface_id),
                ),
            )
            .await;
        }
        "schema3_wrong_key" => {
            let cap = capability("account-A", "codex");
            insert_fixture_row(
                conn,
                shape,
                fixture_row(
                    "Codex",
                    "ordinary-but-wrong-key",
                    "historic@example.test",
                    "Session",
                    Some(noncanonical_json),
                    Some(&cap.account_id),
                    Some(&cap.surface_id),
                ),
            )
            .await;
        }
        "schema3_conflicting_group" => {
            let cap = capability("account-A", "codex");
            let key = cap.opaque_identity_key();
            for provider in ["Codex", "OpenAI"] {
                insert_fixture_row(
                    conn,
                    shape,
                    fixture_row(
                        provider,
                        &key,
                        "historic@example.test",
                        "Session",
                        Some(noncanonical_json),
                        Some(&cap.account_id),
                        Some(&cap.surface_id),
                    ),
                )
                .await;
            }
        }
        "schema3_reserved_unbound" => {
            insert_fixture_row(
                conn,
                shape,
                fixture_row(
                    "Codex",
                    "capability-v1:legacy-unbound",
                    "historic@example.test",
                    "Session",
                    Some(noncanonical_json),
                    None,
                    None,
                ),
            )
            .await;
        }
        "schema3_statement_failure" => {
            seed_statement_failure_rows(conn, shape).await;
        }
        "schema3_commit_failure" => {}
        _ => panic!("unknown row fixture mode {mode}"),
    }
}

async fn insert_blob_legacy_identity_row(conn: &Connection, mode: &str, quota_json: &str) {
    let provider = if mode == "schema2_blob_provider" {
        "X'436F646578'"
    } else {
        "'Codex'"
    };
    let key = if mode == "schema2_blob_key" {
        "X'6C65676163792F6172626974726172792D6B6579'"
    } else {
        "'legacy/arbitrary-key'"
    };
    let sql = format!(
        "INSERT INTO account_usage_snapshots (
            provider, account_key_hash, account_label, source, confidence, window_kind,
            used_amount, used_unit, limit_amount, limit_unit, resets_at, fetched_at,
            expires_at, status, last_error, quota_bucket_json
        ) VALUES ({provider}, {key}, 'historic@example.test', 'provider_api', 'authoritative', 'Session',
                  -9223372036854775808, 'historic used unit', 9223372036854775807,
                  'historic limit unit', -9223372036854775808, 9223372036854775807,
                  -9223372036854775808, 'stale', 'historic failure', {})",
        sql_text(quota_json)
    );
    conn.execute_batch(&sql)
        .await
        .expect("insert actual schema 2 BLOB provider/key cell");
}

async fn insert_blob_identity_row(conn: &Connection) {
    let key = capability("account-A", "codex").opaque_identity_key();
    let sql = format!(
        "INSERT INTO account_usage_snapshots (
            provider, account_key_hash, account_label, source, confidence, window_kind,
            used_amount, used_unit, limit_amount, limit_unit, resets_at, fetched_at,
            expires_at, status, last_error, quota_bucket_json,
            capability_account_id, capability_surface_id
        ) VALUES ('Codex', {}, 'historic@example.test', 'provider_api', 'authoritative', 'Session',
                  -9223372036854775808, 'historic used unit', 9223372036854775807,
                  'historic limit unit', -9223372036854775808, 9223372036854775807,
                  -9223372036854775808, 'stale', 'historic failure', NULL,
                  X'6163636f756e742d41', 'codex')",
        sql_text(&key)
    );
    conn.execute_batch(&sql)
        .await
        .expect("insert actual BLOB capability cell");
}

async fn legacy_identity_types(path: &Path) -> (String, String) {
    let store = open_test_store(path)
        .await
        .expect("open legacy identity proof");
    let mut rows = store
        .query(
            "SELECT typeof(provider), typeof(account_key_hash) FROM account_usage_snapshots ORDER BY id LIMIT 1",
            (),
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().expect("legacy identity row");
    (row_string(&row, 0).unwrap(), row_string(&row, 1).unwrap())
}

async fn fixture_snapshot(path: &Path) -> Vec<String> {
    let store = open_test_store(path).await.expect("open fixture managed");
    let conn = &store;
    let mut snapshot = Vec::new();
    let mut schema_objects = conn
        .query(
            "SELECT type, name, tbl_name, sql FROM sqlite_master ORDER BY type, name",
            (),
        )
        .await
        .unwrap();
    while let Some(row) = schema_objects.next().await.unwrap() {
        snapshot.push(format!(
            "schema:{}:{}:{}:{:?}",
            row_string(&row, 0).unwrap(),
            row_string(&row, 1).unwrap(),
            row_string(&row, 2).unwrap(),
            row_opt_string(&row, 3).unwrap()
        ));
    }
    drop(schema_objects);
    if table_exists(conn, "_meta").await.unwrap() {
        let mut rows = conn
            .query(
                "SELECT key, typeof(value), quote(value), hex(CAST(value AS BLOB)) FROM _meta ORDER BY key",
                (),
            )
            .await
            .unwrap();
        while let Some(row) = rows.next().await.unwrap() {
            snapshot.push(format!(
                "meta:{}:{}:{}:{}",
                row_string(&row, 0).unwrap(),
                row_string(&row, 1).unwrap(),
                row_string(&row, 2).unwrap(),
                row_string(&row, 3).unwrap()
            ));
        }
    }
    if table_exists(conn, "account_usage_snapshots").await.unwrap() {
        let mut schema = conn
            .query(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'account_usage_snapshots'",
                (),
            )
            .await
            .unwrap();
        if let Some(row) = schema.next().await.unwrap() {
            snapshot.push(format!("table_sql:{:?}", row_opt_string(&row, 0).unwrap()));
        }
        drop(schema);
        let mut table_info = conn
            .query("PRAGMA table_info(account_usage_snapshots)", ())
            .await
            .unwrap();
        let mut columns = Vec::new();
        while let Some(row) = table_info.next().await.unwrap() {
            let name = row_string(&row, 1).unwrap();
            snapshot.push(format!(
                "column:{name}:{}:{}:{}",
                row_string(&row, 2).unwrap(),
                row.get::<i64>(3).unwrap(),
                row.get::<i64>(5).unwrap()
            ));
            columns.push(name);
        }
        drop(table_info);
        if !columns.is_empty() {
            let expressions = columns
                .iter()
                .flat_map(|name| {
                    let quoted = format!("\"{}\"", name.replace('"', "\"\""));
                    [
                        format!("typeof({quoted})"),
                        format!("quote({quoted})"),
                        format!("hex(CAST({quoted} AS BLOB))"),
                    ]
                })
                .collect::<Vec<_>>()
                .join(", ");
            let query = format!("SELECT {expressions} FROM account_usage_snapshots ORDER BY id");
            let mut rows = conn.query(&query, ()).await.unwrap();
            while let Some(row) = rows.next().await.unwrap() {
                let fields = (0..columns.len() * 3)
                    .map(|index| row_string(&row, index).unwrap())
                    .collect::<Vec<_>>();
                snapshot.push(format!("row:{fields:?}"));
            }
        }
    }
    snapshot
}

async fn old_projection(path: &Path) -> Vec<Vec<String>> {
    const COLUMNS: [&str; 17] = [
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
        "quota_bucket_json",
    ];
    let store = open_test_store(path).await.expect("open old projection");
    let expressions = COLUMNS
        .iter()
        .map(|column| {
            let quoted = format!("\"{column}\"");
            format!("typeof({quoted}), quote({quoted}), hex(CAST({quoted} AS BLOB))")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!("SELECT {expressions} FROM account_usage_snapshots ORDER BY id");
    let mut rows = store.query(&query, ()).await.unwrap();
    let mut projection = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        projection.push(
            (0..COLUMNS.len() * 3)
                .map(|index| row_string(&row, index).unwrap())
                .collect(),
        );
    }
    projection
}

async fn prequota_projection(path: &Path) -> Vec<Vec<String>> {
    const COLUMNS: [&str; 16] = [
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
    ];
    let store = open_test_store(path)
        .await
        .expect("open pre-quota projection");
    let expressions = COLUMNS
        .iter()
        .map(|column| {
            let quoted = format!("\"{column}\"");
            format!("typeof({quoted}), quote({quoted}), hex(CAST({quoted} AS BLOB))")
        })
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!("SELECT {expressions} FROM account_usage_snapshots ORDER BY id");
    let mut rows = store.query(&query, ()).await.unwrap();
    let mut projection = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        projection.push(
            (0..COLUMNS.len() * 3)
                .map(|index| row_string(&row, index).unwrap())
                .collect(),
        );
    }
    projection
}

async fn metadata(path: &Path) -> std::collections::BTreeMap<String, String> {
    let store = open_test_store(path).await.expect("open metadata");
    let mut rows = store
        .query("SELECT key, value FROM _meta ORDER BY key", ())
        .await
        .unwrap();
    let mut metadata = std::collections::BTreeMap::new();
    while let Some(row) = rows.next().await.unwrap() {
        metadata.insert(row_string(&row, 0).unwrap(), row_string(&row, 1).unwrap());
    }
    metadata
}

async fn capability_pair_types(path: &Path) -> Vec<(String, String)> {
    let store = open_test_store(path).await.expect("open capability pair");
    let mut rows = store
        .query(
            "SELECT typeof(capability_account_id), typeof(capability_surface_id) FROM account_usage_snapshots ORDER BY id",
            (),
        )
        .await
        .unwrap();
    let mut types = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        types.push((row_string(&row, 0).unwrap(), row_string(&row, 1).unwrap()));
    }
    types
}

async fn blob_capability_type(path: &Path) -> String {
    let store = open_test_store(path).await.expect("open BLOB proof");
    let mut rows = store
        .query(
            "SELECT typeof(capability_account_id) FROM account_usage_snapshots ORDER BY id LIMIT 1",
            (),
        )
        .await
        .unwrap();
    row_string(&rows.next().await.unwrap().unwrap(), 0).unwrap()
}

fn migration_receipt_exists(path: &Path) -> bool {
    fs::read_dir(path.parent().expect("fixture database parent"))
        .expect("read fixture sibling backup directories")
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .any(|directory| {
            directory
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".jackin-quota-backup-"))
                && (directory.join("backup_ready.manifest").is_file()
                    || directory.join("migration_committed.manifest").is_file())
        })
}

fn committed_backup_directory(path: &Path) -> PathBuf {
    let mut matches = fs::read_dir(path.parent().expect("fixture database parent"))
        .expect("read fixture sibling backup directories")
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|directory| {
            directory
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".jackin-quota-backup-"))
                && directory.join("backup_ready.manifest").is_file()
                && directory.join("migration_committed.manifest").is_file()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "one committed CLI migration backup exists"
    );
    matches.remove(0)
}

fn read_manifest(path: &Path) -> std::collections::BTreeMap<String, String> {
    fs::read_to_string(path)
        .expect("read migration manifest")
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

#[test]
fn cli_identity_codec_validates_exact_tuple_keys_and_reserved_unbound_keys() {
    let valid = capability("account-A_1", "codex");
    let key = identity::account_key_for_view("Codex", "renamed label", Some(&valid)).unwrap();
    assert_eq!(key, valid.opaque_identity_key());
    identity::decode_account_capability(
        "OpenAI",
        &key,
        Some(valid.account_id.clone()),
        Some(valid.surface_id.clone()),
    )
    .unwrap();
    assert_eq!(
        identity::account_key_for_view("Codex", "historic label", None).unwrap(),
        account_key_hash("Codex", "historic label")
    );
    let max_id = "x".repeat(128);
    identity::account_key_for_view("Codex", "label", Some(&capability(&max_id, "codex"))).unwrap();

    let too_long = "x".repeat(129);
    for account_id in ["", "has space", "unicode-é", too_long.as_str()] {
        let invalid = capability(account_id, "codex");
        assert!(
            identity::account_key_for_view("Codex", "label", Some(&invalid))
                .unwrap_err()
                .contains("account_id"),
            "invalid account id {account_id:?}"
        );
    }
    for surface_id in ["", "bad/surface", too_long.as_str()] {
        let invalid = capability("account-A", surface_id);
        assert!(
            identity::account_key_for_view("Codex", "label", Some(&invalid))
                .unwrap_err()
                .contains("surface_id"),
            "invalid surface id {surface_id:?}"
        );
    }
    assert!(
        identity::account_key_for_view("Claude", "label", Some(&valid))
            .unwrap_err()
            .contains("does not match provider")
    );
    assert!(
        identity::account_key_for_view("codex", "label", Some(&valid))
            .unwrap_err()
            .contains("does not match provider")
    );
    assert!(
        identity::account_key_for_view("unknown-provider", "label", Some(&valid))
            .unwrap_err()
            .contains("does not match provider")
    );
    let opencode = capability("account-A", "opencode");
    identity::account_key_for_view("OpenCode", "label", Some(&opencode)).unwrap();
    assert!(
        identity::decode_account_capability(
            "Codex",
            "ordinary-but-wrong-key",
            Some(valid.account_id.clone()),
            Some(valid.surface_id.clone()),
        )
        .unwrap_err()
        .contains("does not match account_key_hash")
    );
    assert!(
        identity::decode_account_capability("Codex", &key, Some(valid.account_id.clone()), None)
            .unwrap_err()
            .contains("both NULL or both present")
    );
    assert!(
        identity::decode_account_capability("Codex", "capability-v1:unbound", None, None,)
            .unwrap_err()
            .contains("reserved capability namespace")
    );

    let alias = ("Codex".to_owned(), key.clone(), Some(valid.clone()));
    let other_alias = ("OpenAI".to_owned(), key, Some(valid));
    assert!(
        identity::validate_identity_groups([
            (alias.0.as_str(), alias.1.as_str(), alias.2.as_ref()),
            (
                other_alias.0.as_str(),
                other_alias.1.as_str(),
                other_alias.2.as_ref()
            ),
        ])
        .unwrap_err()
        .contains("conflicting provider")
    );
}

#[tokio::test]
async fn invalid_later_capability_is_rejected_before_opening_the_cli_store() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    let mut missing_bucket = legacy_account("shared@example.test", "Session");
    missing_bucket.quota_bucket = None;
    let mut invalid_capability = missing_bucket.clone();
    invalid_capability.account_capability = Some(capability("bad id", "codex"));

    let error = upsert_accounts(&paths, &[missing_bucket, invalid_capability])
        .await
        .unwrap_err();
    assert!(error.to_string().contains("account_id"), "{error:#}");
    assert!(!path.exists(), "preflight must precede file creation");
}

#[tokio::test]
async fn flattened_capabilities_round_trip_through_the_public_cli_store() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    let mut expected = flattened_bound_rows();
    expected.push(legacy_account("shared@example.test", "Session"));
    upsert_accounts(&paths, &expected).await.unwrap();

    let (_, loaded) = read_accounts(&paths).await.unwrap();
    assert_eq!(loaded.len(), 4);
    for expected_row in &expected {
        let loaded_row = loaded
            .iter()
            .find(|row| row.account_capability == expected_row.account_capability)
            .expect("public CLI row for capability or legacy identity");
        assert_eq!(loaded_row, expected_row);
    }
    let personal = capability("account-personal", "codex");
    let work = capability("account-work", "codex");
    let other_surface = capability("account-personal", "opencode");
    assert_eq!(other_surface.account_id, personal.account_id);
    assert_ne!(
        other_surface.opaque_identity_key(),
        personal.opaque_identity_key()
    );
    let identity_rows = stored_identity_cells(&path).await;
    assert!(identity_rows.iter().any(|(key, account_id, surface_id)| {
        key == &personal.opaque_identity_key()
            && account_id.as_deref() == Some("account-personal")
            && surface_id.as_deref() == Some("codex")
    }));
    assert!(identity_rows.iter().any(|(key, account_id, surface_id)| {
        key == &work.opaque_identity_key()
            && account_id.as_deref() == Some("account-work")
            && surface_id.as_deref() == Some("codex")
    }));
    assert!(identity_rows.iter().any(|(key, account_id, surface_id)| {
        key == &other_surface.opaque_identity_key()
            && account_id.as_deref() == Some("account-personal")
            && surface_id.as_deref() == Some("opencode")
    }));
    assert!(identity_rows.iter().any(|(key, account_id, surface_id)| {
        key == &account_key_hash("OpenAI", "shared@example.test")
            && account_id.is_none()
            && surface_id.is_none()
    }));

    let mut renamed_refresh = UsageCache::default();
    renamed_refresh.insert_snapshot_for_capability_for_test(
        "codex",
        Some("OpenAI"),
        &personal,
        cached_view("codex", "OpenAI", "renamed@example.test", count_bucket()),
    );
    let refreshed = renamed_refresh.account_snapshot_views();
    upsert_accounts(&paths, &refreshed).await.unwrap();
    let (_, after_rename) = read_accounts(&paths).await.unwrap();
    assert_eq!(after_rename.len(), 4);
    let personal_after = after_rename
        .iter()
        .find(|row| row.account_capability.as_ref() == Some(&personal))
        .expect("same opaque identity after display-label rename");
    assert_eq!(personal_after.account_label, "renamed@example.test");
    assert_eq!(personal_after.used_amount, Some(i64::MIN));
    assert_eq!(personal_after.used_unit.as_deref(), Some("USD"));
    assert_eq!(personal_after.limit_amount, Some(i64::MAX));
    assert_eq!(personal_after.limit_unit.as_deref(), Some("USD"));
    assert_eq!(
        personal_after
            .quota_bucket
            .as_ref()
            .and_then(|bucket| bucket.count_quota.as_ref())
            .map(|quota| quota.used),
        Some(Some(u64::MAX))
    );
    assert!(
        after_rename
            .iter()
            .any(|row| row.account_capability.as_ref() == Some(&work))
    );
    let alternate_surface = after_rename
        .iter()
        .find(|row| row.account_capability.as_ref() == Some(&other_surface))
        .expect("OpenCode surface for the same account id");
    assert_eq!(alternate_surface.provider, "OpenCode");
    assert_eq!(alternate_surface.account_label, "shared@example.test");
    assert!(
        after_rename
            .iter()
            .any(|row| row.account_capability.is_none())
    );
}

async fn stored_identity_cells(path: &Path) -> Vec<(String, Option<String>, Option<String>)> {
    let store = open_test_store(path).await.expect("open stored identities");
    let mut rows = store
        .query(
            "SELECT account_key_hash, capability_account_id, capability_surface_id FROM account_usage_snapshots ORDER BY account_key_hash",
            (),
        )
        .await
        .unwrap();
    let mut identities = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        identities.push((
            row_string(&row, 0).unwrap(),
            row_opt_string(&row, 1).unwrap(),
            row_opt_string(&row, 2).unwrap(),
        ));
    }
    identities
}

#[tokio::test]
async fn schema2_typed_and_upgraded_null_rows_migrate_and_restore_exactly() {
    for mode in ["schema2_typed", "schema2_null"] {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let path = host_account_cache_path(&paths);
        run_child(&format!("seed_{mode}"), temp.path());
        let original_projection = old_projection(&path).await;
        let original_metadata = metadata(&path).await;
        let expected_rows = schema2_expected_rows(mode);
        assert_eq!(original_projection.len(), expected_rows.len());
        for (projection, expected) in original_projection.iter().zip(&expected_rows) {
            let expected_json = expected.quota_bucket.as_ref().map(noncanonical_json);
            match expected_json {
                Some(json) => {
                    assert_eq!(projection[16 * 3], "text");
                    let expected_hex = uppercase_fixture_hex(json.as_bytes());
                    assert_eq!(projection[16 * 3 + 2], expected_hex);
                }
                None => assert_eq!(projection[16 * 3], "null"),
            }
        }

        run_child(&format!("migrate_{mode}"), temp.path());
        let (_, migrated) = read_accounts(&paths).await.unwrap();
        assert_eq!(migrated, expected_rows);
        if mode == "schema2_typed" {
            assert_typed_migration_values(&migrated);
        }
        assert_eq!(old_projection(&path).await, original_projection);
        assert_eq!(
            capability_pair_types(&path).await,
            vec![("null".to_owned(), "null".to_owned()); expected_rows.len()]
        );
        let after_metadata = metadata(&path).await;
        assert_eq!(
            after_metadata.get("schema_version").map(String::as_str),
            Some("3")
        );
        assert_eq!(
            after_metadata
                .get("schema_version_before_capability_v3")
                .map(String::as_str),
            Some("2")
        );
        assert_eq!(
            after_metadata.get("schema_version_before_quota_v2"),
            original_metadata.get("schema_version_before_quota_v2")
        );

        let backup_dir = committed_backup_directory(&path);
        let ready = read_manifest(&backup_dir.join("backup_ready.manifest"));
        let migrated_receipt = read_manifest(&backup_dir.join("migration_committed.manifest"));
        assert_eq!(
            ready.get("status").map(String::as_str),
            Some("backup_ready")
        );
        assert_eq!(ready.get("kind").map(String::as_str), Some("cli_usage"));
        assert_eq!(ready.get("old_version").map(String::as_str), Some("2"));
        assert_eq!(ready.get("new_version").map(String::as_str), Some("3"));
        assert_eq!(
            ready.get("backup_file").map(String::as_str),
            Some("rollback.sqlite")
        );
        assert_eq!(
            migrated_receipt.get("status").map(String::as_str),
            Some("migration_committed")
        );
        assert_eq!(
            migrated_receipt.get("new_version").map(String::as_str),
            Some("3")
        );

        let rollback_path = backup_dir.join("rollback.sqlite");
        let rollback_bytes = fs::read(&rollback_path).expect("read immutable rollback artifact");
        let restore_root = tempfile::tempdir().unwrap();
        let restore_paths = JackinPaths::for_tests(restore_root.path());
        let restore_path = host_account_cache_path(&restore_paths);
        fs::create_dir_all(restore_path.parent().unwrap()).unwrap();
        fs::copy(&rollback_path, &restore_path)
            .expect("copy immutable rollback artifact to a new restore path");
        let (_, restored) = read_accounts(&restore_paths).await.unwrap();
        assert_eq!(restored, migrated);
        assert_eq!(restored, expected_rows);
        if mode == "schema2_typed" {
            assert_typed_migration_values(&restored);
        }
        assert_eq!(old_projection(&restore_path).await, original_projection);
        assert_eq!(
            capability_pair_types(&restore_path).await,
            vec![("null".to_owned(), "null".to_owned()); expected_rows.len()]
        );
        assert_eq!(metadata(&restore_path).await, after_metadata);
        assert_eq!(
            fs::read(&rollback_path).expect("recheck immutable rollback artifact"),
            rollback_bytes
        );
    }
}

fn assert_typed_migration_values(rows: &[AccountUsageSnapshotView]) {
    assert_eq!(rows.len(), 3);
    let requests = rows
        .iter()
        .find(|row| row.window_kind == "Requests")
        .and_then(|row| row.quota_bucket.as_ref())
        .expect("restored Requests bucket");
    let request_count = requests.count_quota.as_ref().expect("Requests count quota");
    assert_eq!(requests.used_money, None);
    assert_eq!(requests.limit_money, None);
    assert_eq!(request_count.used, Some(u64::MAX));
    assert_eq!(request_count.limit, Some(u64::MAX));
    assert_eq!(request_count.remaining, Some(0));
    assert_eq!(request_count.unit, CountQuotaUnit::Requests);
    assert_eq!(request_count.period, CountQuotaPeriod::UtcDaily);
    assert_eq!(
        request_count.provenance,
        CountQuotaProvenance::ProviderReported
    );
    assert_eq!(
        requests.provider_quota_state,
        Some(ProviderQuotaState::Exhausted)
    );
    assert_eq!(
        requests
            .used_percent
            .map(CanonicalUsedPercent::get)
            .map(f64::to_bits),
        Some(43.375_f64.to_bits())
    );
    assert_eq!(
        requests
            .remaining_percent
            .map(CanonicalRemainingPercent::get)
            .map(f64::to_bits),
        Some(57.125_f64.to_bits())
    );
    assert_eq!(
        requests.used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        requests.remaining_percent_origin,
        Some(PercentObservationOrigin::Derived)
    );

    let spend = rows
        .iter()
        .find(|row| row.window_kind == "Spend")
        .and_then(|row| row.quota_bucket.as_ref())
        .expect("restored Spend bucket");
    assert_eq!(spend.count_quota, None);
    assert_eq!(spend.used_money, Some(Money::new(i64::MIN, "USD", 4)));
    assert_eq!(spend.limit_money, Some(Money::new(i64::MAX, "USD", 7)));
    assert_eq!(
        spend
            .used_percent
            .map(CanonicalUsedPercent::get)
            .map(f64::to_bits),
        Some(f64::from_bits(43.375_f64.to_bits() + 1).to_bits())
    );
    assert_eq!(
        spend
            .remaining_percent
            .map(CanonicalRemainingPercent::get)
            .map(f64::to_bits),
        Some(f64::from_bits(57.125_f64.to_bits() + 1).to_bits())
    );
    assert_eq!(
        spend.used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        spend.remaining_percent_origin,
        Some(PercentObservationOrigin::Derived)
    );

    let unlimited = rows
        .iter()
        .find(|row| row.window_kind == "Unlimited")
        .and_then(|row| row.quota_bucket.as_ref())
        .expect("restored Unlimited bucket");
    assert_eq!(
        unlimited.provider_quota_state,
        Some(ProviderQuotaState::Unlimited)
    );
    assert_eq!(unlimited.count_quota, None);
    assert_eq!(unlimited.used_money, None);
    assert_eq!(unlimited.limit_money, None);
}

fn prequota_expected() -> AccountUsageSnapshotView {
    let mut row = legacy_account("historic@example.test", "Session");
    row.provider = "Codex".to_owned();
    row.quota_bucket = None;
    row.last_error = Some("historic failure".to_owned());
    row
}

#[tokio::test]
async fn unversioned_and_schema0_prequota_rows_migrate_directly_to_schema3() {
    for (mode, previous) in [
        ("schema0_legacy", "0"),
        ("unversioned_legacy", "unversioned"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let path = host_account_cache_path(&paths);
        run_child(&format!("seed_{mode}"), temp.path());
        let original_projection = prequota_projection(&path).await;

        run_child("migrate_prequota", temp.path());
        let (_, rows) = read_accounts(&paths).await.unwrap();
        assert_eq!(rows, [prequota_expected()]);
        assert_eq!(prequota_projection(&path).await, original_projection);
        assert_eq!(schema_version(&path).await.unwrap().as_deref(), Some("3"));
        assert_eq!(
            metadata(&path)
                .await
                .get("schema_version_before_quota_v2")
                .map(String::as_str),
            Some(previous)
        );
        assert_eq!(
            metadata(&path)
                .await
                .get("schema_version_before_capability_v3")
                .map(String::as_str),
            Some(previous)
        );
        assert_eq!(
            capability_pair_types(&path).await,
            [("null".to_owned(), "null".to_owned())]
        );
        assert!(
            committed_backup_directory(&path)
                .join("migration_committed.manifest")
                .is_file()
        );
    }
}

#[tokio::test]
async fn cli_schema_refusal_matrix_preserves_rows_and_has_no_migration_receipt() {
    for &(mode, expected_error) in SCHEMA2_REFUSALS.iter().chain(SCHEMA3_REFUSALS) {
        let temp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());
        let path = host_account_cache_path(&paths);
        run_child(&format!("seed_{mode}"), temp.path());
        if mode == "schema3_blob_account_id" {
            assert_eq!(blob_capability_type(&path).await, "blob");
        }
        if mode == "schema2_blob_provider" {
            assert_eq!(
                legacy_identity_types(&path).await,
                ("blob".to_owned(), "text".to_owned())
            );
        }
        if mode == "schema2_blob_key" {
            assert_eq!(
                legacy_identity_types(&path).await,
                ("text".to_owned(), "blob".to_owned())
            );
        }
        if matches!(
            mode,
            "schema2_id_without_rowid" | "schema3_id_without_rowid"
        ) {
            let store = open_test_store(&path)
                .await
                .expect("open seeded WITHOUT ROWID fixture schema");
            let mut schema = store
                .query(
                    "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'account_usage_snapshots'",
                    (),
                )
                .await
                .unwrap();
            let row = schema
                .next()
                .await
                .unwrap()
                .expect("seeded WITHOUT ROWID fixture table");
            let table_sql = row_string(&row, 0).unwrap();
            assert!(
                table_sql.contains("WITHOUT ROWID"),
                "{mode} seeded table is not WITHOUT ROWID: {table_sql}"
            );
            let mut table_metadata = store
                .query("PRAGMA main.table_list(account_usage_snapshots)", ())
                .await
                .expect("inspect seeded WITHOUT ROWID table metadata");
            let metadata = table_metadata
                .next()
                .await
                .expect("read seeded WITHOUT ROWID table metadata")
                .expect("seeded WITHOUT ROWID table metadata row");
            assert_eq!(
                metadata
                    .get::<String>(0)
                    .expect("read table metadata schema"),
                "main"
            );
            assert_eq!(
                metadata.get::<String>(1).expect("read table metadata name"),
                "account_usage_snapshots"
            );
            assert_eq!(
                metadata.get::<String>(2).expect("read table metadata type"),
                "table"
            );
            assert_eq!(
                metadata
                    .get::<i64>(4)
                    .expect("read table without-rowid flag"),
                1,
                "{mode} table_list metadata must confirm WITHOUT ROWID"
            );
            assert!(
                table_metadata
                    .next()
                    .await
                    .expect("read duplicate table metadata")
                    .is_none(),
                "{mode} returned duplicate table metadata rows"
            );
        }
        let before = fixture_snapshot(&path).await;
        let read_error = read_accounts(&paths).await.unwrap_err();
        assert!(
            read_error.to_string().contains(expected_error),
            "{mode} read error: {read_error:#}"
        );
        let write_error = upsert_accounts(&paths, &[account("Session", 37)])
            .await
            .unwrap_err();
        assert!(
            write_error.to_string().contains(expected_error),
            "{mode} write error: {write_error:#}"
        );
        assert_eq!(
            fixture_snapshot(&path).await,
            before,
            "{mode} source changed"
        );
        assert!(
            !migration_receipt_exists(&path),
            "{mode} refusal must not publish a migration receipt"
        );
    }
}

#[tokio::test]
async fn corrupt_current3_empty_write_refuses_without_changing_the_source() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    run_child("seed_schema3_reserved_unbound", temp.path());
    let before = fixture_snapshot(&path).await;

    let error = upsert_accounts(&paths, &[]).await.unwrap_err();
    assert!(
        error.to_string().contains("reserved capability namespace"),
        "{error:#}"
    );
    assert_eq!(fixture_snapshot(&path).await, before);
    assert!(!migration_receipt_exists(&path));
}

#[tokio::test]
async fn invalid_later_selector_on_existing_cli_store_preserves_the_complete_source() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    let existing = legacy_account("existing@example.test", "Session");
    upsert_accounts(&paths, std::slice::from_ref(&existing))
        .await
        .unwrap();
    let before = fixture_snapshot(&path).await;

    let valid_candidate = legacy_account("would-insert@example.test", "Session");
    let mut invalid_later = legacy_account("invalid-provider@example.test", "Session");
    invalid_later.provider = "unknown-provider".to_owned();
    invalid_later.account_capability = Some(capability("account-A", "codex"));
    let error = upsert_accounts(&paths, &[valid_candidate, invalid_later])
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("does not match provider"),
        "{error:#}"
    );
    assert_eq!(fixture_snapshot(&path).await, before);
}

#[tokio::test]
async fn invalid_later_bucket_on_existing_cli_store_preserves_the_complete_source() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    let existing = legacy_account("existing@example.test", "Session");
    upsert_accounts(&paths, std::slice::from_ref(&existing))
        .await
        .unwrap();
    let before = fixture_snapshot(&path).await;

    let valid_candidate = legacy_account("would-insert@example.test", "Session");
    let mut invalid_later = legacy_account("missing-bucket@example.test", "Session");
    invalid_later.quota_bucket = None;
    let error = upsert_accounts(&paths, &[valid_candidate, invalid_later])
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("missing its typed quota bucket"),
        "{error:#}"
    );
    assert_eq!(fixture_snapshot(&path).await, before);
}

#[tokio::test]
async fn finished_writer_then_reader_processes_preserve_flattened_capabilities() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);

    run_child("roundtrip_write", temp.path());
    run_child("roundtrip_read", temp.path());

    let (_, rows) = read_accounts(&paths).await.unwrap();
    assert_complete_capability_rows(&rows, &expected_process_rows());
    assert_eq!(schema_version(&path).await.unwrap().as_deref(), Some("3"));
    assert!(!migration_receipt_exists(&path));
}

#[tokio::test]
async fn later_statement_failure_rolls_back_the_complete_cli_upsert_batch() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    run_child("seed_schema3_statement_failure", temp.path());
    let before = fixture_snapshot(&path).await;

    let mut update = legacy_account("rollback-session@example.test", "Session");
    update.used_amount = None;
    update.used_unit = None;
    update.limit_amount = None;
    update.limit_unit = None;
    update.quota_bucket = Some(count_bucket());
    let blocked = legacy_account("blocked@example.test", "blocked");
    let error = upsert_accounts(&paths, &[update.clone(), blocked])
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("upsert host usage account snapshot"),
        "{error:#}"
    );
    assert!(
        error.chain().any(|cause| cause
            .to_string()
            .contains("CHECK constraint failed: window_kind <> 'blocked'")),
        "{error:#}"
    );
    assert_eq!(fixture_snapshot(&path).await, before);

    upsert_accounts(&paths, &[update]).await.unwrap();
    let (_, rows) = read_accounts(&paths).await.unwrap();
    assert_eq!(rows.len(), 2);
    let refreshed = rows
        .iter()
        .find(|row| row.account_label == "rollback-session@example.test")
        .expect("updated session row");
    assert_eq!(refreshed.used_amount, Some(i64::MIN));
    assert_eq!(refreshed.used_unit.as_deref(), Some("historic used unit"));
    assert_eq!(refreshed.limit_amount, Some(i64::MAX));
    assert_eq!(refreshed.limit_unit.as_deref(), Some("historic limit unit"));
    assert_eq!(
        refreshed
            .quota_bucket
            .as_ref()
            .and_then(|bucket| bucket.count_quota.as_ref())
            .map(|quota| quota.used),
        Some(Some(u64::MAX))
    );
}

#[tokio::test]
async fn deferred_cli_upsert_commit_failure_rolls_back_and_retries() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let path = host_account_cache_path(&paths);
    run_child("seed_schema3_commit_failure", temp.path());
    let row = legacy_account("commit@example.test", "Session");

    let failure = upsert_accounts(&paths, std::slice::from_ref(&row))
        .await
        .unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("commit host usage account snapshot upsert failed"),
        "{failure:#}"
    );
    assert!(read_accounts(&paths).await.unwrap().1.is_empty());
    assert!(!migration_receipt_exists(&path));

    let store = open_test_store(&path).await.unwrap();
    store
        .execute(
            "INSERT INTO commit_parent (account_key_hash) VALUES (?1)",
            [account_key_hash(&row.provider, &row.account_label)],
        )
        .await
        .unwrap();
    drop(store);
    upsert_accounts(&paths, std::slice::from_ref(&row))
        .await
        .unwrap();
    let (_, rows) = read_accounts(&paths).await.unwrap();
    assert_eq!(rows, [row]);
}
