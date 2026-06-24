//! Capsule-local structured usage telemetry cache.
//!
//! This is a daemon-owned store under `/jackin/state/`: Capsule writes quota
//! snapshots after provider refresh and renderers read through the daemon cache,
//! not by opening this database. The schema mirrors the roadmap V1 account
//! snapshot shape so the later host-daemon store can reuse the same rows.

use std::future::Future;
use std::path::Path;

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use sha2::{Digest, Sha256};
use turso::{Connection, Row, params};

const SCHEMA_VERSION: &str = "4";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredAccountUsageSnapshot {
    pub provider: String,
    pub account_key_hash: String,
    pub account_label: String,
    pub source: String,
    pub confidence: String,
    pub window_kind: String,
    pub used_amount: Option<i64>,
    pub used_unit: Option<String>,
    pub limit_amount: Option<i64>,
    pub limit_unit: Option<String>,
    pub resets_at: Option<i64>,
    pub fetched_at: i64,
    pub expires_at: Option<i64>,
    pub status: String,
    pub last_error: Option<String>,
    pub focused_provider: Option<String>,
    pub plan_label: Option<String>,
    pub remaining_percent: Option<i64>,
    pub used_label: Option<String>,
    pub limit_label: Option<String>,
    pub reset_label: Option<String>,
    pub pace_label: Option<String>,
    pub view_status: String,
    pub updated_label: String,
    pub status_bar_label: String,
}

#[cfg(test)]
pub(crate) fn store_usage_snapshot(path: &Path, view: &FocusedUsageView) -> Result<(), String> {
    store_usage_snapshots(path, std::slice::from_ref(view))
}

pub(crate) fn store_usage_snapshots(path: &Path, views: &[FocusedUsageView]) -> Result<(), String> {
    let path = path.to_path_buf();
    let rows = views
        .iter()
        .flat_map(account_snapshot_rows)
        .collect::<Vec<_>>();
    block_on_store(async move {
        let conn = open_store(&path).await?;
        upsert_account_snapshot_rows(&conn, rows).await
    })
}

fn block_on_store<T, Fut>(future: Fut) -> Result<T, String>
where
    Fut: Future<Output = Result<T, String>>,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|err| format!("create telemetry store runtime failed: {err}"))?;
    runtime.block_on(future)
}

async fn open_store(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create telemetry store dir failed: {err}"))?;
    }
    let path = path_to_turso(path)?;
    let db = turso::Builder::new_local(&path)
        .build()
        .await
        .map_err(|err| format!("open telemetry store failed: {err}"))?;
    let conn = db
        .connect()
        .map_err(|err| format!("connect telemetry store failed: {err}"))?;
    initialize_schema(&conn).await?;
    Ok(conn)
}

fn path_to_turso(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "telemetry store path is not utf8".to_owned())
}

async fn initialize_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS _meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS account_usage_snapshots (
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

        ",
    )
    .await
    .map_err(|err| format!("initialize telemetry store schema failed: {err}"))?;
    ensure_account_snapshot_columns(conn).await?;
    conn.execute(
        "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [SCHEMA_VERSION],
    )
    .await
    .map_err(|err| format!("record telemetry store schema version failed: {err}"))?;
    Ok(())
}

async fn ensure_account_snapshot_columns(conn: &Connection) -> Result<(), String> {
    let mut rows = conn
        .query("PRAGMA table_info(account_usage_snapshots)", ())
        .await
        .map_err(|err| format!("inspect telemetry snapshot schema failed: {err}"))?;
    let mut columns = std::collections::HashSet::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|err| format!("read telemetry snapshot schema failed: {err}"))?
    {
        columns.insert(row_string(&row, 1, "column_name")?);
    }
    for (name, ddl) in [
        (
            "focused_provider",
            "ALTER TABLE account_usage_snapshots ADD COLUMN focused_provider TEXT",
        ),
        (
            "plan_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN plan_label TEXT",
        ),
        (
            "remaining_percent",
            "ALTER TABLE account_usage_snapshots ADD COLUMN remaining_percent INTEGER",
        ),
        (
            "used_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN used_label TEXT",
        ),
        (
            "limit_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN limit_label TEXT",
        ),
        (
            "reset_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN reset_label TEXT",
        ),
        (
            "pace_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN pace_label TEXT",
        ),
        (
            "view_status",
            "ALTER TABLE account_usage_snapshots ADD COLUMN view_status TEXT NOT NULL DEFAULT 'unavailable'",
        ),
        (
            "updated_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN updated_label TEXT NOT NULL DEFAULT 'Unavailable'",
        ),
        (
            "status_bar_label",
            "ALTER TABLE account_usage_snapshots ADD COLUMN status_bar_label TEXT NOT NULL DEFAULT 'usage unavailable'",
        ),
    ] {
        if !columns.contains(name) {
            conn.execute(ddl, ())
                .await
                .map_err(|err| format!("upgrade telemetry snapshot schema failed: {err}"))?;
        }
    }
    Ok(())
}

async fn upsert_account_snapshot_rows(
    conn: &Connection,
    rows: Vec<StoredAccountUsageSnapshot>,
) -> Result<(), String> {
    for row in rows {
        conn.execute(
            "
            INSERT INTO account_usage_snapshots (
                provider,
                account_key_hash,
                account_label,
                source,
                confidence,
                window_kind,
                used_amount,
                used_unit,
                limit_amount,
                limit_unit,
                resets_at,
                fetched_at,
                expires_at,
                status,
                last_error,
                focused_provider,
                plan_label,
                remaining_percent,
                used_label,
                limit_label,
                reset_label,
                pace_label,
                view_status,
                updated_label,
                status_bar_label
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
            ON CONFLICT(provider, account_key_hash, source, window_kind) DO UPDATE SET
                account_label = excluded.account_label,
                confidence = excluded.confidence,
                used_amount = excluded.used_amount,
                used_unit = excluded.used_unit,
                limit_amount = excluded.limit_amount,
                limit_unit = excluded.limit_unit,
                resets_at = excluded.resets_at,
                fetched_at = excluded.fetched_at,
                expires_at = excluded.expires_at,
                status = excluded.status,
                last_error = excluded.last_error,
                focused_provider = excluded.focused_provider,
                plan_label = excluded.plan_label,
                remaining_percent = excluded.remaining_percent,
                used_label = excluded.used_label,
                limit_label = excluded.limit_label,
                reset_label = excluded.reset_label,
                pace_label = excluded.pace_label,
                view_status = excluded.view_status,
                updated_label = excluded.updated_label,
                status_bar_label = excluded.status_bar_label
            ",
            params![
                row.provider,
                row.account_key_hash,
                row.account_label,
                row.source,
                row.confidence,
                row.window_kind,
                row.used_amount,
                row.used_unit,
                row.limit_amount,
                row.limit_unit,
                row.resets_at,
                row.fetched_at,
                row.expires_at,
                row.status,
                row.last_error,
                row.focused_provider,
                row.plan_label,
                row.remaining_percent,
                row.used_label,
                row.limit_label,
                row.reset_label,
                row.pace_label,
                row.view_status,
                row.updated_label,
                row.status_bar_label,
            ],
        )
        .await
        .map_err(|err| format!("upsert telemetry account snapshot failed: {err}"))?;
    }
    Ok(())
}

fn account_snapshot_rows(view: &FocusedUsageView) -> Vec<StoredAccountUsageSnapshot> {
    let provider = view.account.provider_label.clone();
    let account_label = view.account.account_label.clone();
    let account_key_hash = account_key_hash(&provider, &account_label);
    let source = source_label(view.source).to_owned();
    let confidence = confidence_label(view.confidence).to_owned();
    let fetched_at = view.fetched_at_epoch;
    let last_error = view.last_error.clone();
    view.buckets
        .iter()
        .map(|bucket| {
            let quota = quota_amounts(bucket);
            StoredAccountUsageSnapshot {
                provider: provider.clone(),
                account_key_hash: account_key_hash.clone(),
                account_label: account_label.clone(),
                source: source.clone(),
                confidence: confidence.clone(),
                window_kind: bucket.label.clone(),
                used_amount: quota.used_amount,
                used_unit: quota.used_unit,
                limit_amount: quota.limit_amount,
                limit_unit: quota.limit_unit,
                resets_at: None,
                fetched_at,
                expires_at: None,
                status: status_label(bucket.status).to_owned(),
                last_error: last_error.clone(),
                focused_provider: view.focused_provider.clone(),
                plan_label: view.account.plan_label.clone(),
                remaining_percent: bucket.remaining_percent.map(i64::from),
                used_label: bucket.used_label.clone(),
                limit_label: bucket.limit_label.clone(),
                reset_label: bucket.reset_label.clone(),
                pace_label: bucket.pace_label.clone(),
                view_status: status_label(view.status).to_owned(),
                updated_label: view.updated_label.clone(),
                status_bar_label: view.status_bar_label.clone(),
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct QuotaAmounts {
    used_amount: Option<i64>,
    used_unit: Option<String>,
    limit_amount: Option<i64>,
    limit_unit: Option<String>,
}

fn quota_amounts(bucket: &QuotaBucketView) -> QuotaAmounts {
    if let Some(remaining) = bucket.remaining_percent {
        return QuotaAmounts {
            used_amount: Some(i64::from(100_u8.saturating_sub(remaining.min(100)))),
            used_unit: Some("percent".to_owned()),
            limit_amount: Some(100),
            limit_unit: Some("percent".to_owned()),
        };
    }
    QuotaAmounts {
        used_amount: None,
        used_unit: None,
        limit_amount: None,
        limit_unit: None,
    }
}

fn account_key_hash(provider: &str, account_label: &str) -> String {
    let digest = Sha256::digest(format!("{provider}\0{account_label}").as_bytes());
    format!("sha256:{}", hex_lower(&digest))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

fn source_label(source: UsageSource) -> &'static str {
    match source {
        UsageSource::ProviderApi => "provider_api",
        UsageSource::Cli => "cli",
        UsageSource::LocalLogs => "local_logs",
        UsageSource::Cache => "cache",
        UsageSource::None => "none",
    }
}

fn confidence_label(confidence: UsageConfidence) -> &'static str {
    match confidence {
        UsageConfidence::Authoritative => "authoritative",
        UsageConfidence::Estimated => "estimated",
        UsageConfidence::PresenceOnly => "presence_only",
        UsageConfidence::None => "none",
    }
}

fn status_label(status: UsageSnapshotStatus) -> &'static str {
    match status {
        UsageSnapshotStatus::Fresh => "fresh",
        UsageSnapshotStatus::Stale => "stale",
        UsageSnapshotStatus::NeedsLogin => "needs_login",
        UsageSnapshotStatus::NeedsSecret => "needs_secret",
        UsageSnapshotStatus::Unsupported => "unsupported",
        UsageSnapshotStatus::Unavailable => "unavailable",
        UsageSnapshotStatus::Error => "error",
    }
}

#[cfg(test)]
pub(crate) fn focused_usage_view(
    path: &Path,
    focused_agent: Option<&str>,
    focused_provider: Option<&str>,
    now_epoch: i64,
) -> Result<Option<FocusedUsageView>, String> {
    let rows = stored_account_snapshots(path)?;
    let tabs = usage_provider_tabs_from_rows(&rows);
    let resolved_provider = focused_provider.or_else(|| {
        focused_agent.and_then(|agent| crate::usage::resolved_usage_provider_label(agent, None))
    });
    let Some((provider, rows)) = select_provider_rows(rows, resolved_provider) else {
        return Ok(None);
    };
    let Some(first) = rows.first() else {
        return Ok(None);
    };
    let status = usage_status_from_label(&first.view_status);
    let source = usage_source_from_label(&first.source);
    let confidence = usage_confidence_from_label(&first.confidence);
    let fetched_at = rows.iter().map(|row| row.fetched_at).max().unwrap_or(0);
    let mut buckets = rows
        .iter()
        .map(|row| QuotaBucketView {
            label: row.window_kind.clone(),
            used_label: row.used_label.clone(),
            limit_label: row.limit_label.clone(),
            remaining_percent: row
                .remaining_percent
                .and_then(|value| u8::try_from(value.clamp(0, 100)).ok()),
            reset_label: row.reset_label.clone(),
            pace_label: row.pace_label.clone(),
            status: usage_status_from_label(&row.status),
        })
        .collect::<Vec<_>>();
    buckets.sort_by_key(|bucket| usage_bucket_order(&provider, &bucket.label));
    Ok(Some(FocusedUsageView {
        focused_agent: focused_agent.map(str::to_owned),
        focused_provider: first
            .focused_provider
            .clone()
            .or_else(|| resolved_provider.map(str::to_owned))
            .or_else(|| Some(provider.clone())),
        account: jackin_protocol::control::FocusedAccountHeader {
            provider_label: provider,
            account_label: first.account_label.clone(),
            plan_label: first.plan_label.clone(),
        },
        buckets,
        status,
        source,
        confidence,
        fetched_at_epoch: fetched_at,
        updated_label: if matches!(
            status,
            UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
        ) || first.updated_label.trim().is_empty()
        {
            crate::usage::relative_updated_label(fetched_at, now_epoch)
        } else {
            first.updated_label.clone()
        },
        status_bar_label: if first.status_bar_label.trim().is_empty() {
            lifecycle_status_bar_label(status)
        } else {
            first.status_bar_label.clone()
        },
        tabs,
        last_error: first.last_error.clone(),
    }))
}

#[cfg(test)]
fn usage_bucket_order(provider: &str, label: &str) -> usize {
    let provider = normalize_provider_label(provider);
    let order: &[&str] = if provider_matches("openai", &provider)
        || provider_matches("codex", &provider)
    {
        &[
            "Session",
            "Weekly",
            "Codex Spark 5-hour",
            "Codex Spark Weekly",
            "Limit Reset Credits",
            "Credits",
        ]
    } else if provider_matches("anthropic", &provider) || provider_matches("claude", &provider) {
        &["Session", "Weekly", "Sonnet", "Daily Routines"]
    } else if provider_matches("amp", &provider) {
        &["Amp Free", "Credits", "Individual credits"]
    } else if provider_matches("zai", &provider) || provider_matches("glm", &provider) {
        &["Tokens", "MCP", "5-hour"]
    } else if provider_matches("kimi", &provider) {
        &["Weekly", "Rate Limit"]
    } else if provider_matches("minimax", &provider) {
        &["General · 5h", "General · Weekly", "Video"]
    } else {
        &[]
    };
    order
        .iter()
        .position(|entry| provider_matches(entry, label))
        .unwrap_or(order.len())
}

#[cfg(test)]
fn select_provider_rows(
    rows: Vec<StoredAccountUsageSnapshot>,
    focused_provider: Option<&str>,
) -> Option<(String, Vec<StoredAccountUsageSnapshot>)> {
    let focused = focused_provider.unwrap_or_default();
    let mut matches = rows
        .into_iter()
        .filter(|row| provider_matches(focused, &row.provider))
        .collect::<Vec<_>>();
    if matches.is_empty() {
        return None;
    }
    let latest = matches.iter().map(|row| row.fetched_at).max()?;
    matches.retain(|row| row.fetched_at == latest);
    let provider = matches.first()?.provider.clone();
    Some((provider, matches))
}

#[cfg(test)]
fn provider_matches(needle: &str, provider: &str) -> bool {
    if needle.trim().is_empty() {
        return false;
    }
    let needle = normalize_provider_label(needle);
    let provider = normalize_provider_label(provider);
    provider.contains(&needle)
        || needle.contains(&provider)
        || (needle.contains("openai") && provider.contains("codex"))
        || (needle.contains("codex") && provider.contains("openai"))
        || (needle.contains("anthropic") && provider.contains("claude"))
        || (needle.contains("claude") && provider.contains("anthropic"))
        || (needle.contains("xai") && provider.contains("grok"))
        || (needle.contains("grok") && provider.contains("xai"))
        || (needle.contains("zai") && provider.contains("glm"))
        || (needle.contains("glm") && provider.contains("zai"))
}

#[cfg(test)]
fn normalize_provider_label(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

#[cfg(test)]
fn usage_provider_tabs_from_rows(
    rows: &[StoredAccountUsageSnapshot],
) -> Vec<jackin_protocol::control::UsageProviderTab> {
    [
        "Codex",
        "Claude",
        "Amp",
        "Grok Build",
        "GLM / Z.AI",
        "Kimi",
        "MiniMax",
    ]
    .into_iter()
    .map(|label| {
        let latest = rows
            .iter()
            .filter(|row| provider_matches(label, &row.provider))
            .max_by_key(|row| row.fetched_at);
        jackin_protocol::control::UsageProviderTab {
            label: label.to_owned(),
            status_label: latest.map_or_else(
                || "not cached".to_owned(),
                |row| tab_status_label(row, rows),
            ),
            account_label: latest.map_or_else(
                || "account unavailable".to_owned(),
                |row| row.account_label.clone(),
            ),
            plan_label: latest.and_then(|row| row.plan_label.clone()),
            source_label: latest.map(|row| format!("{} · {}", row.view_status, row.source)),
            active: false,
        }
    })
    .collect()
}

#[cfg(test)]
fn tab_status_label(
    row: &StoredAccountUsageSnapshot,
    rows: &[StoredAccountUsageSnapshot],
) -> String {
    rows.iter()
        .filter(|candidate| {
            candidate.provider == row.provider
                && candidate.account_key_hash == row.account_key_hash
                && candidate.fetched_at == row.fetched_at
        })
        .find_map(|candidate| {
            candidate
                .remaining_percent
                .map(|remaining| format!("{} {remaining}% left", candidate.window_kind))
        })
        .unwrap_or_else(|| row.view_status.clone())
}

#[cfg(test)]
fn usage_status_from_label(label: &str) -> UsageSnapshotStatus {
    match label {
        "fresh" => UsageSnapshotStatus::Fresh,
        "stale" => UsageSnapshotStatus::Stale,
        "needs_login" => UsageSnapshotStatus::NeedsLogin,
        "needs_secret" => UsageSnapshotStatus::NeedsSecret,
        "unsupported" => UsageSnapshotStatus::Unsupported,
        "error" => UsageSnapshotStatus::Error,
        _ => UsageSnapshotStatus::Unavailable,
    }
}

#[cfg(test)]
fn usage_source_from_label(label: &str) -> UsageSource {
    match label {
        "provider_api" => UsageSource::ProviderApi,
        "cli" => UsageSource::Cli,
        "local_logs" => UsageSource::LocalLogs,
        "cache" => UsageSource::Cache,
        _ => UsageSource::None,
    }
}

#[cfg(test)]
fn usage_confidence_from_label(label: &str) -> UsageConfidence {
    match label {
        "authoritative" => UsageConfidence::Authoritative,
        "estimated" => UsageConfidence::Estimated,
        "presence_only" => UsageConfidence::PresenceOnly,
        _ => UsageConfidence::None,
    }
}

#[cfg(test)]
fn lifecycle_status_bar_label(status: UsageSnapshotStatus) -> String {
    match status {
        UsageSnapshotStatus::Fresh => "usage cached",
        UsageSnapshotStatus::Stale => "stale",
        UsageSnapshotStatus::NeedsLogin => "needs login",
        UsageSnapshotStatus::NeedsSecret => "needs secret",
        UsageSnapshotStatus::Unsupported => "unsupported",
        UsageSnapshotStatus::Unavailable => "usage unavailable",
        UsageSnapshotStatus::Error => "error",
    }
    .to_owned()
}

#[cfg(test)]
fn stored_account_snapshots(path: &Path) -> Result<Vec<StoredAccountUsageSnapshot>, String> {
    let path = path.to_path_buf();
    block_on_store(async move {
        let conn = open_store(&path).await?;
        let mut rows = conn
            .query(
                "
                SELECT
                    provider,
                    account_key_hash,
                    account_label,
                    source,
                    confidence,
                    window_kind,
                    used_amount,
                    used_unit,
                    limit_amount,
                    limit_unit,
                    resets_at,
                    fetched_at,
                    expires_at,
                    status,
                    last_error,
                    focused_provider,
                    plan_label,
                    remaining_percent,
                    used_label,
                    limit_label,
                    reset_label,
                    pace_label,
                    view_status,
                    updated_label,
                    status_bar_label
                FROM account_usage_snapshots
                ORDER BY provider, account_key_hash, source, window_kind
                ",
                (),
            )
            .await
            .map_err(|err| format!("query telemetry snapshots failed: {err}"))?;
        let mut snapshots = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|err| format!("read telemetry snapshot row failed: {err}"))?
        {
            snapshots.push(StoredAccountUsageSnapshot {
                provider: row_string(&row, 0, "provider")?,
                account_key_hash: row_string(&row, 1, "account_key_hash")?,
                account_label: row_string(&row, 2, "account_label")?,
                source: row_string(&row, 3, "source")?,
                confidence: row_string(&row, 4, "confidence")?,
                window_kind: row_string(&row, 5, "window_kind")?,
                used_amount: row_opt_i64(&row, 6, "used_amount")?,
                used_unit: row_opt_string(&row, 7, "used_unit")?,
                limit_amount: row_opt_i64(&row, 8, "limit_amount")?,
                limit_unit: row_opt_string(&row, 9, "limit_unit")?,
                resets_at: row_opt_i64(&row, 10, "resets_at")?,
                fetched_at: row_i64(&row, 11, "fetched_at")?,
                expires_at: row_opt_i64(&row, 12, "expires_at")?,
                status: row_string(&row, 13, "status")?,
                last_error: row_opt_string(&row, 14, "last_error")?,
                focused_provider: row_opt_string(&row, 15, "focused_provider")?,
                plan_label: row_opt_string(&row, 16, "plan_label")?,
                remaining_percent: row_opt_i64(&row, 17, "remaining_percent")?,
                used_label: row_opt_string(&row, 18, "used_label")?,
                limit_label: row_opt_string(&row, 19, "limit_label")?,
                reset_label: row_opt_string(&row, 20, "reset_label")?,
                pace_label: row_opt_string(&row, 21, "pace_label")?,
                view_status: row_string(&row, 22, "view_status")?,
                updated_label: row_string(&row, 23, "updated_label")?,
                status_bar_label: row_string(&row, 24, "status_bar_label")?,
            });
        }
        Ok(snapshots)
    })
}

#[cfg(test)]
pub(crate) fn schema_version(path: &Path) -> Result<Option<String>, String> {
    let path = path.to_path_buf();
    block_on_store(async move {
        let conn = open_store(&path).await?;
        let mut rows = conn
            .query("SELECT value FROM _meta WHERE key = 'schema_version'", ())
            .await
            .map_err(|err| format!("query telemetry schema version failed: {err}"))?;
        rows.next()
            .await
            .map_err(|err| format!("read telemetry schema version failed: {err}"))?
            .map(|row| row_string(&row, 0, "schema_version"))
            .transpose()
    })
}

#[cfg(test)]
fn row_i64(row: &Row, idx: usize, name: &str) -> Result<i64, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

#[cfg(test)]
fn row_opt_i64(row: &Row, idx: usize, name: &str) -> Result<Option<i64>, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

fn row_string(row: &Row, idx: usize, name: &str) -> Result<String, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

#[cfg(test)]
fn row_opt_string(row: &Row, idx: usize, name: &str) -> Result<Option<String>, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

#[cfg(test)]
mod tests {
    use crate::tui::components::dialog::Dialog;

    use jackin_protocol::control::{
        FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageConfidence,
        UsageSnapshotStatus, UsageSource,
    };

    use super::*;

    fn usage_view() -> FocusedUsageView {
        FocusedUsageView {
            focused_agent: Some("codex".to_owned()),
            focused_provider: Some("OpenAI".to_owned()),
            account: FocusedAccountHeader {
                provider_label: "Codex".to_owned(),
                account_label: "alexey@example.com".to_owned(),
                plan_label: Some("Pro 20x".to_owned()),
            },
            buckets: vec![
                QuotaBucketView {
                    label: "Session".to_owned(),
                    used_label: Some("63% used".to_owned()),
                    limit_label: Some("100%".to_owned()),
                    remaining_percent: Some(37),
                    reset_label: Some("Resets in 1h".to_owned()),
                    pace_label: None,
                    status: UsageSnapshotStatus::Fresh,
                },
                QuotaBucketView {
                    label: "Credits".to_owned(),
                    used_label: None,
                    limit_label: None,
                    remaining_percent: None,
                    reset_label: None,
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

    fn provider_usage_view(
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
                plan_label: plan.map(str::to_owned),
            },
            buckets: vec![QuotaBucketView {
                label: bucket.to_owned(),
                used_label: Some(format!("{}% used", 100_u8.saturating_sub(remaining))),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(remaining),
                reset_label: Some("Resets at 15:00 UTC".to_owned()),
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

    #[test]
    fn account_snapshot_rows_are_persisted_and_upserted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");

        store_usage_snapshot(&db, &usage_view()).expect("store first snapshot");
        let mut changed = usage_view();
        changed.buckets[0].remaining_percent = Some(25);
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
        assert_eq!(session.used_amount, Some(75));
        assert_eq!(session.used_unit.as_deref(), Some("percent"));
        assert_eq!(session.limit_amount, Some(100));
        assert_eq!(session.status, "fresh");
        assert_eq!(session.fetched_at, 1_781_185_620);
        assert_eq!(session.remaining_percent, Some(25));
        assert_eq!(session.used_label.as_deref(), Some("63% used"));
        assert_eq!(session.limit_label.as_deref(), Some("100%"));
        assert_eq!(session.plan_label.as_deref(), Some("Pro 20x"));
    }

    #[test]
    fn focused_usage_view_rebuilds_snapshot_from_account_rows() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");
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
        assert_eq!(view.buckets[0].remaining_percent, Some(37));
        assert_eq!(view.buckets[1].label, "Credits");
        assert_eq!(view.updated_label, "Updated just now");
        assert_eq!(view.status_bar_label, "Codex Session: 63% used · 37% left");
    }

    #[test]
    fn focused_usage_view_ticks_relative_updated_label_from_fetch_time() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");
        store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

        let view = focused_usage_view(&db, Some("codex"), Some("Codex"), 1_781_185_680)
            .expect("read focused usage")
            .expect("stored usage view");

        assert_eq!(view.updated_label, "Updated 2m ago");
    }

    #[test]
    fn focused_usage_view_resolves_provider_from_agent_when_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");
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
        assert_eq!(view.buckets[0].remaining_percent, Some(9));
    }

    #[test]
    fn focused_usage_view_without_resolved_provider_does_not_match_all() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");
        store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

        let view = focused_usage_view(&db, Some("unknown-agent"), None, 1_781_185_680)
            .expect("read focused usage");

        assert!(view.is_none());
    }

    #[test]
    fn focused_usage_view_sorts_provider_buckets_canonically() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");
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
                label: "MCP".to_owned(),
                remaining_percent: Some(100),
                pace_label: Some("0 / 100 (100 remaining)".to_owned()),
                ..base_bucket.clone()
            },
            QuotaBucketView {
                label: "Tokens".to_owned(),
                remaining_percent: Some(99),
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
            vec!["Tokens", "MCP", "5-hour"]
        );
    }

    #[test]
    fn all_provider_snapshots_round_trip_from_turso_to_usage_overlay_rows() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");
        let now = 1_781_185_680;
        let providers = [
            (
                "Codex",
                "OpenAI",
                "codex@example.com",
                Some("Pro 20x"),
                "Session",
                37,
            ),
            (
                "Claude",
                "Anthropic",
                "claude@example.com",
                Some("Max"),
                "Weekly",
                42,
            ),
            (
                "Amp",
                "Amp",
                "amp@example.com",
                Some("Amp Free"),
                "Amp Free",
                55,
            ),
            ("Grok Build", "xAI", "local Grok auth", None, "Credits", 61),
            (
                "GLM / Z.AI",
                "Z.AI",
                "zai@example.com",
                Some("GLM Coding"),
                "Tokens",
                72,
            ),
            (
                "Kimi",
                "Kimi",
                "kimi@example.com",
                Some("K2"),
                "5-hour rate limit",
                83,
            ),
            (
                "MiniMax",
                "MiniMax",
                "minimax@example.com",
                Some("MiniMax Pro"),
                "MiniMax Text Coding plan",
                94,
            ),
        ];

        for (provider, _tab_label, account, plan, bucket, remaining) in providers {
            store_usage_snapshot(
                &db,
                &provider_usage_view(provider, account, plan, bucket, remaining, now - 120),
            )
            .expect("store provider snapshot");
        }

        for (provider, tab_label, account, plan, bucket, remaining) in providers {
            let view = focused_usage_view(&db, Some("codex"), Some(tab_label), now)
                .expect("read focused usage")
                .expect("stored provider usage");
            assert_eq!(view.account.provider_label, provider);
            assert_eq!(view.account.account_label, account);
            assert_eq!(view.account.plan_label.as_deref(), plan);
            assert_eq!(view.buckets.len(), 1);
            assert_eq!(view.buckets[0].label, bucket);
            assert_eq!(view.buckets[0].remaining_percent, Some(remaining));
            assert_eq!(view.updated_label, "Updated 2m ago");
            assert_eq!(view.tabs.len(), 7);

            let state = Dialog::new_usage(view).usage_state().expect("usage state");
            let rows = state.rows();
            assert!(
                rows.iter()
                    .any(|row| row.label() == "Header" && row.value() == tab_label),
                "provider header row missing for {provider}: {rows:?}"
            );
            assert!(
                rows.iter()
                    .any(|row| row.label() == bucket && row.value().contains("left")),
                "bucket row missing for {provider}/{bucket}: {rows:?}"
            );
        }
    }

    #[test]
    fn telemetry_store_records_schema_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("usage.db");

        store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

        assert_eq!(
            schema_version(&db).expect("schema version").as_deref(),
            Some("4")
        );
    }
}
