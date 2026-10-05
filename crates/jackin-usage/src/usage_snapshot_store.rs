// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule-local structured usage telemetry cache.
//!
//! This is a daemon-owned store under `/jackin/state/`: Capsule writes quota
//! snapshots after provider refresh and renderers read through the daemon cache,
//! not by opening this database. The schema mirrors the roadmap V1 account
//! snapshot shape so the later host-daemon store can reuse the same rows.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::Path;
#[cfg(test)]
use std::sync::Mutex;
use std::sync::OnceLock;

use crate::store_backend::{
    self, Connection, DbOperation, ReadOnlyConnection, ReadOnlyRow, Row, connect_local, params,
};
use jackin_protocol::control::{
    AccountUsageSnapshotView, CountQuota, FocusedAccountHeader, FocusedUsageView, Money,
    QuotaBucketView, UsageAccountIdentity, UsageAccountMembershipV1, UsageCanonicalAccountIdentity,
    UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_protocol::usage_broker::UsageProjectionV2;
use jackin_telemetry::ResultTelemetryExt as _;

const SCHEMA_VERSION: &str = "8";

#[cfg(test)]
static CONNECTION_BUILDS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();

/// Distinct account identity known to the durable snapshot store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountIdentitySummary {
    /// Broker routing account/provider identity retained independently of labels.
    pub account_identity: UsageAccountIdentity,
    /// Validated logical evidence; absent in migrated route-only snapshots.
    pub canonical_identity: Option<UsageCanonicalAccountIdentity>,
    /// Provider label as stored (`Anthropic / Claude`, `OpenAI / Codex`, …).
    pub provider: String,
    /// `account_key_hash` (stable multi-account id).
    pub account_key_hash: String,
    /// Operator-visible account label.
    pub account_label: String,
    /// Plan when last stored.
    pub plan_label: Option<String>,
    /// Tightest remaining % among latest windows for this account.
    pub remaining_percent: Option<u8>,
    /// Latest `fetched_at` epoch among rows for this account.
    pub fetched_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAccountUsageSnapshot {
    pub provider: String,
    pub account_key_hash: String,
    pub account_identity: UsageAccountIdentity,
    /// Validated logical evidence; absent in migrated route-only snapshots.
    pub canonical_identity: Option<UsageCanonicalAccountIdentity>,
    pub account_label: String,
    pub source: String,
    pub confidence: String,
    pub window_kind: String,
    pub count_quota: Option<CountQuota>,
    pub used_money: Option<Money>,
    pub limit_money: Option<Money>,
    pub remaining_money: Option<Money>,
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
pub fn store_usage_snapshot(path: &Path, view: &FocusedUsageView) -> Result<(), String> {
    store_usage_snapshots(path, std::slice::from_ref(view))
}

pub fn store_usage_snapshots(path: &Path, views: &[FocusedUsageView]) -> Result<(), String> {
    for view in views {
        validate_snapshot_binding(
            view.account_identity.as_ref(),
            view.canonical_identity.as_ref(),
        )?;
    }
    let path = path.to_path_buf();
    let rows = views
        .iter()
        .flat_map(account_snapshot_rows)
        .collect::<Vec<_>>();
    validate_account_snapshot_rows(&rows)?;
    block_on_store(async move {
        let conn = open_store(&path).await?;
        upsert_account_snapshot_rows(&conn, rows).await
    })
}

/// Host-admitted immutable container and current workspace configuration proof.
/// The caller validates this scope through the runtime and configuration authority.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageMembershipScope {
    pub container_id: String,
    pub workspace_config_proof: String,
}

/// Latest explicit membership state for one authenticated container namespace.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredUsageMembership {
    pub scope: UsageMembershipScope,
    pub membership: UsageAccountMembershipV1,
}

/// Accepted authority survives availability changes and binds its original scope.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedUsageMembershipAuthority {
    workspace_config_proof: String,
    projection: UsageProjectionV2,
    revoked: bool,
}

fn decode_membership_authority(
    json: Option<String>,
) -> Result<Option<AcceptedUsageMembershipAuthority>, String> {
    let authority = json
        .map(|json| {
            serde_json::from_str::<AcceptedUsageMembershipAuthority>(&json)
                .map_err(|_| "invalid scoped membership accepted authority".to_owned())
        })
        .transpose()?;
    if let Some(authority) = &authority {
        UsageAccountMembershipV1::validate_current_projection(&authority.projection)?;
        if authority.workspace_config_proof.trim().is_empty()
            || authority
                .workspace_config_proof
                .chars()
                .any(char::is_control)
        {
            return Err(
                "accepted membership authority lacks workspace configuration proof".to_owned(),
            );
        }
    }
    Ok(authority)
}

fn validate_current_membership_authority(
    scope: &UsageMembershipScope,
    membership: &UsageAccountMembershipV1,
    authority: Option<&AcceptedUsageMembershipAuthority>,
) -> Result<(), String> {
    if let UsageAccountMembershipV1::Current { projection } = membership {
        let authority =
            authority.ok_or_else(|| "current membership lacks accepted authority".to_owned())?;
        if authority.revoked
            || &authority.projection != projection.as_ref()
            || authority.workspace_config_proof != scope.workspace_config_proof
        {
            return Err("current scoped membership disagrees with accepted authority".to_owned());
        }
    }
    Ok(())
}

fn validate_membership_scope(scope: &UsageMembershipScope) -> Result<(), String> {
    if scope.container_id.len() != 64
        || !scope
            .container_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || scope.workspace_config_proof.trim().is_empty()
        || scope.container_id.chars().any(char::is_control)
        || scope.workspace_config_proof.chars().any(char::is_control)
    {
        return Err("invalid admitted usage membership scope".to_owned());
    }
    Ok(())
}

fn validate_membership(membership: &UsageAccountMembershipV1) -> Result<(), String> {
    if let UsageAccountMembershipV1::Current { projection } = membership {
        UsageAccountMembershipV1::validate_current_projection(projection)?;
    }
    Ok(())
}

/// Write a complete scoped publication, preserving quota history separately.
/// Empty Current is authoritative; Unavailable and Revoked remove current
/// authority only for this immutable container. Issuer high-water marks survive.
pub fn store_usage_membership(
    path: &Path,
    scope: &UsageMembershipScope,
    membership: &UsageAccountMembershipV1,
) -> Result<(), String> {
    validate_membership_scope(scope)?;
    validate_membership(membership)?;
    let path = path.to_path_buf();
    let scope = scope.clone();
    let membership = membership.clone();
    block_on_store(async move {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let conn = connect_local(&path_to_turso(&path)?).await?;
        store_backend::operation(
            DbOperation::Update,
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS usage_scoped_memberships (
                container_id TEXT PRIMARY KEY,
                workspace_config_proof TEXT NOT NULL,
                membership_json TEXT NOT NULL,
                accepted_authority_json TEXT
             );
             CREATE TABLE IF NOT EXISTS usage_retired_membership_issuers (
                container_id TEXT NOT NULL,
                issuer_id TEXT NOT NULL,
                PRIMARY KEY(container_id, issuer_id)
             );",
            ),
        )
        .await
        .map_err(|err| format!("initialize scoped usage membership failed: {err}"))?;
        store_backend::operation(DbOperation::Begin, conn.execute("BEGIN", ()))
            .await
            .map_err(|err| err.to_string())?;
        let result = write_usage_membership(&conn, scope, membership).await;
        if let Err(err) = result {
            let _rollback =
                store_backend::operation(DbOperation::Rollback, conn.execute("ROLLBACK", ()))
                    .await
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError);
            return Err(err);
        }
        if let Err(err) =
            store_backend::operation(DbOperation::Commit, conn.execute("COMMIT", ())).await
        {
            let _rollback =
                store_backend::operation(DbOperation::Rollback, conn.execute("ROLLBACK", ()))
                    .await
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError);
            return Err(format!("commit scoped usage membership failed: {err}"));
        }
        Ok(())
    })
}

async fn write_usage_membership(
    conn: &Connection,
    scope: UsageMembershipScope,
    membership: UsageAccountMembershipV1,
) -> Result<(), String> {
    let mut rows = store_backend::operation(DbOperation::Select,
        conn.query("SELECT workspace_config_proof, membership_json, accepted_authority_json FROM usage_scoped_memberships WHERE container_id = ?1", [scope.container_id.clone()]))
        .await.map_err(|err| err.to_string())?;
    let previous = rows.next().await.map_err(|err| err.to_string())?;
    let mut authority = None;
    if let Some(row) = previous {
        let previous_scope = UsageMembershipScope {
            container_id: scope.container_id.clone(),
            workspace_config_proof: row_string(&row, 0, "workspace_config_proof")?,
        };
        validate_membership_scope(&previous_scope)?;
        let previous_state = serde_json::from_str::<UsageAccountMembershipV1>(&row_string(
            &row,
            1,
            "membership_json",
        )?)
        .map_err(|_| "invalid stored scoped membership".to_owned())?;
        validate_membership(&previous_state)?;
        authority =
            decode_membership_authority(row_opt_string(&row, 2, "accepted_authority_json")?)?;
        validate_current_membership_authority(
            &previous_scope,
            &previous_state,
            authority.as_ref(),
        )?;
    }
    drop(rows);
    match &membership {
        UsageAccountMembershipV1::Current { projection } => {
            let mut retired = store_backend::operation(DbOperation::Select,
                conn.query("SELECT 1 FROM usage_retired_membership_issuers WHERE container_id = ?1 AND issuer_id = ?2", params![scope.container_id.clone(), projection.broker_instance_id.clone()]))
                .await.map_err(|err| err.to_string())?;
            if retired
                .next()
                .await
                .map_err(|err| err.to_string())?
                .is_some()
            {
                return Err("retired usage membership issuer cannot regain authority".to_owned());
            }
            drop(retired);
            if let Some(previous_authority) = &authority {
                let previous = &previous_authority.projection;
                if previous.broker_instance_id == projection.broker_instance_id {
                    if projection.broker_generation < previous.broker_generation {
                        return Err("stale usage membership publication rejected".to_owned());
                    }
                    if projection.broker_generation == previous.broker_generation {
                        if projection.as_ref() != previous
                            || previous_authority.workspace_config_proof
                                != scope.workspace_config_proof
                        {
                            return Err(
                                "conflicting usage membership publication at issuer generation"
                                    .to_owned(),
                            );
                        }
                        if previous_authority.revoked {
                            return Err(
                                "revoked usage membership requires a newer accepted publication"
                                    .to_owned(),
                            );
                        }
                    }
                } else {
                    store_backend::operation(DbOperation::Upsert, conn.execute(
                        "INSERT INTO usage_retired_membership_issuers (container_id, issuer_id) VALUES (?1, ?2)",
                        params![scope.container_id.clone(), previous.broker_instance_id.clone()]))
                        .await.map_err(|err| err.to_string())?;
                }
            }
            authority = Some(AcceptedUsageMembershipAuthority {
                workspace_config_proof: scope.workspace_config_proof.clone(),
                projection: projection.as_ref().clone(),
                revoked: false,
            });
        }
        UsageAccountMembershipV1::Revoked => {
            if let Some(authority) = &mut authority {
                authority.revoked = true;
            }
        }
        UsageAccountMembershipV1::Unavailable => {}
    }
    let accepted_json = authority
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|err| err.to_string())?;
    let membership_json = serde_json::to_string(&membership).map_err(|err| err.to_string())?;
    store_backend::operation(DbOperation::Upsert, conn.execute(
        "INSERT INTO usage_scoped_memberships (container_id, workspace_config_proof, membership_json, accepted_authority_json)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(container_id) DO UPDATE SET workspace_config_proof = excluded.workspace_config_proof,
         membership_json = excluded.membership_json, accepted_authority_json = excluded.accepted_authority_json",
        params![scope.container_id, scope.workspace_config_proof, membership_json, accepted_json]))
        .await.map_err(|err| format!("store scoped usage membership failed: {err}"))?;
    Ok(())
}

/// Read explicit latest states; quota snapshot rows never establish membership.
/// Missing files have no recorded scopes. Existing caches without membership
/// authority report refresh required and are never initialized by this reader.
pub fn read_usage_memberships(path: &Path) -> Result<Vec<StoredUsageMembership>, String> {
    let path = path_to_turso(path)?;
    block_on_store(async move {
        let memberships = store_backend::read_local(&path, |conn| {
            let tables = conn.query("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('usage_scoped_memberships', 'usage_retired_membership_issuers')")?;
            let row = tables
                .first()
                .ok_or_else(|| "missing membership schema inspection".to_owned())?;
            if row.i64(0, "membership_tables")? != 2 {
                return Err(
                    "usage membership cache unavailable; authenticated refresh required".to_owned(),
                );
            }
            let rows = conn.query("SELECT container_id, workspace_config_proof, membership_json, accepted_authority_json FROM usage_scoped_memberships ORDER BY container_id")?;
            let mut memberships = Vec::new();
            for row in rows {
                let scope = UsageMembershipScope {
                    container_id: row.string(0, "container_id")?,
                    workspace_config_proof: row.string(1, "workspace_config_proof")?,
                };
                validate_membership_scope(&scope)?;
                let membership = serde_json::from_str::<UsageAccountMembershipV1>(
                    &row.string(2, "membership_json")?,
                )
                .map_err(|_| "invalid stored scoped membership".to_owned())?;
                validate_membership(&membership)?;
                let authority = decode_membership_authority(
                    row.optional_string(3, "accepted_authority_json")?,
                )?;
                validate_current_membership_authority(&scope, &membership, authority.as_ref())?;
                memberships.push(StoredUsageMembership { scope, membership });
            }
            Ok(memberships)
        })?;
        Ok(memberships.unwrap_or_default())
    })
}

/// Persist canonically bound host account snapshots through the shared schema.
pub fn store_account_usage_snapshots(
    path: &Path,
    accounts: &[AccountUsageSnapshotView],
) -> Result<(), String> {
    for account in accounts {
        validate_snapshot_binding(
            account.account_identity.as_ref(),
            account.canonical_identity.as_ref(),
        )?;
    }
    let path = path.to_path_buf();
    let rows = accounts
        .iter()
        .filter_map(|account| {
            let identity = account.account_identity.as_ref()?;
            let (used_amount, used_unit, limit_amount, limit_unit) =
                if let Some(count) = &account.count_quota {
                    let used = count.used.and_then(|value| i64::try_from(value).ok());
                    let limit = count.limit.and_then(|value| i64::try_from(value).ok());
                    (
                        used,
                        used.map(|_| "requests".to_owned()),
                        limit,
                        limit.map(|_| "requests".to_owned()),
                    )
                } else if account.used_money.is_some()
                    || account.limit_money.is_some()
                    || account.remaining_money.is_some()
                {
                    (None, None, None, None)
                } else {
                    (
                        account.used_amount,
                        account.used_unit.clone(),
                        account.limit_amount,
                        account.limit_unit.clone(),
                    )
                };
            Some(StoredAccountUsageSnapshot {
                provider: account.provider.clone(),
                account_key_hash: crate::usage::usage_account_tab_id(identity),
                account_identity: identity.clone(),
                canonical_identity: account.canonical_identity.clone(),
                account_label: account.account_label.clone(),
                source: account.source.clone(),
                confidence: account.confidence.clone(),
                window_kind: account.window_kind.clone(),
                count_quota: account.count_quota.clone(),
                used_money: account.used_money.clone(),
                limit_money: account.limit_money.clone(),
                remaining_money: account.remaining_money.clone(),
                used_amount,
                used_unit: used_unit.clone(),
                limit_amount,
                limit_unit: limit_unit.clone(),
                resets_at: account.resets_at,
                fetched_at: account.fetched_at,
                expires_at: account.expires_at,
                status: account.status.clone(),
                last_error: account.last_error.clone(),
                focused_provider: None,
                plan_label: None,
                remaining_percent: snapshot_remaining_percent(account),
                used_label: account
                    .used_money
                    .as_ref()
                    .map(|money| format!("{money} used"))
                    .or_else(|| {
                        snapshot_amount_label(used_amount, used_unit.as_deref())
                            .map(|label| format!("{label} used"))
                    }),
                limit_label: account
                    .limit_money
                    .as_ref()
                    .map(ToString::to_string)
                    .or_else(|| snapshot_amount_label(limit_amount, limit_unit.as_deref())),
                reset_label: None,
                pace_label: None,
                view_status: account.status.clone(),
                updated_label: String::new(),
                status_bar_label: String::new(),
            })
        })
        .collect::<Vec<_>>();
    validate_account_snapshot_rows(&rows)?;
    block_on_store(async move {
        let conn = open_store(&path).await?;
        upsert_account_snapshot_rows(&conn, rows).await
    })
}

fn snapshot_amount_label(amount: Option<i64>, unit: Option<&str>) -> Option<String> {
    amount.map(|amount| match unit {
        Some("percent") => format!("{amount}%"),
        Some(unit) if !unit.is_empty() => format!("{amount} {unit}"),
        _ => amount.to_string(),
    })
}

fn snapshot_remaining_percent(account: &AccountUsageSnapshotView) -> Option<i64> {
    if let Some(count) = &account.count_quota {
        return count.remaining_percent().map(i64::from);
    }
    if account.used_money.is_some()
        || account.limit_money.is_some()
        || account.remaining_money.is_some()
    {
        return monetary_remaining_percent(
            account.used_money.as_ref(),
            account.limit_money.as_ref(),
            account.remaining_money.as_ref(),
        );
    }
    let unit = account.used_unit.as_deref()?;
    if unit.is_empty() || account.limit_unit.as_deref() != Some(unit) {
        return None;
    }
    let used = i128::from(account.used_amount?);
    let limit = i128::from(account.limit_amount?);
    if used < 0 || limit <= 0 {
        return None;
    }
    // Widen before multiplication; valid i64 quantities may exceed the limit.
    i64::try_from(((limit - used) * 100 / limit).clamp(0, 100)).ok()
}

/// Read an existing canonical cache without creating or migrating it.
/// Obsolete display-derived caches stay untouched until the next write.
pub fn read_account_usage_snapshots(path: &Path) -> Result<Vec<AccountUsageSnapshotView>, String> {
    let rows = load_all_account_snapshot_rows(path)?;
    let mut accounts: Vec<_> = rows
        .into_iter()
        .map(|row| {
            let row = project_account_snapshot_row(row);
            AccountUsageSnapshotView {
                account_identity: Some(row.account_identity),
                canonical_identity: row.canonical_identity,
                count_quota: row.count_quota,
                used_money: row.used_money,
                limit_money: row.limit_money,
                remaining_money: row.remaining_money,
                provider: row.provider,
                account_label: row.account_label,
                source: row.source,
                confidence: row.confidence,
                window_kind: row.window_kind,
                used_amount: row.used_amount,
                used_unit: row.used_unit,
                limit_amount: row.limit_amount,
                limit_unit: row.limit_unit,
                resets_at: row.resets_at,
                fetched_at: row.fetched_at,
                expires_at: row.expires_at,
                status: row.status,
                last_error: row.last_error,
            }
        })
        .collect();
    accounts.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then(a.account_label.cmp(&b.account_label))
            .then(a.account_identity.cmp(&b.account_identity))
            .then(a.source.cmp(&b.source))
            .then(a.window_kind.cmp(&b.window_kind))
    });
    Ok(accounts)
}

fn block_on_store<T, Fut>(future: Fut) -> Result<T, String>
where
    Fut: Future<Output = Result<T, String>>,
{
    store_backend::with_store_custody(|| {
        // One process-wide current-thread runtime, reused across every store call.
        // Callers run inside `spawn_blocking` (no enclosing runtime), so `block_on`
        // never nests; custody serializes complete database operations and drops all
        // connections before releasing the gate. Reuse avoids rebuilding a runtime per snapshot
        // write. Build errors propagate without panicking. INVARIANT: never call the
        // store functions from inside the async runtime — route them through
        // `spawn_blocking`, or `block_on` panics ("Cannot start a runtime from within
        // a runtime").
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = if let Some(runtime) = RUNTIME.get() {
            runtime
        } else {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .map_err(|err| format!("create usage snapshot store runtime failed: {err}"))?;
            RUNTIME.get_or_init(move || runtime)
        };
        runtime.block_on(future)
    })
}

async fn open_store(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create usage snapshot store dir failed: {err}"))?;
    }
    let path = path_to_turso(path)?;
    let conn = connect_local(&path)
        .await
        .map_err(|err| format!("open usage snapshot store failed: {err}"))?;
    record_connection_build(&path);
    // Revalidate the opened database each operation. Paths and schema stamps
    // never retain authority after this operation releases its connection.
    initialize_schema(&conn).await?;
    Ok(conn)
}

fn path_to_turso(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "usage snapshot store path is not utf8".to_owned())
}

#[cfg(test)]
fn record_connection_build(path: &str) {
    if let Ok(mut builds) = CONNECTION_BUILDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        *builds.entry(path.to_owned()).or_default() += 1;
    }
}

#[cfg(not(test))]
fn record_connection_build(_path: &str) {}

#[cfg(test)]
fn connection_build_count(path: &Path) -> Result<usize, String> {
    let path = path_to_turso(path)?;
    Ok(CONNECTION_BUILDS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map(|builds| builds.get(&path).copied().unwrap_or_default())
        .unwrap_or_default())
}

async fn initialize_schema(conn: &Connection) -> Result<(), String> {
    store_backend::operation(
        DbOperation::Update,
        conn.execute("PRAGMA foreign_keys = ON", ()),
    )
    .await
    .map_err(|err| err.to_string())?;
    store_backend::operation(DbOperation::Begin, conn.execute("BEGIN", ()))
        .await
        .map_err(|err| format!("begin usage snapshot schema transaction failed: {err}"))?;
    let result = initialize_schema_contents(conn).await;
    if let Err(err) = result {
        let _rollback =
            store_backend::operation(DbOperation::Rollback, conn.execute("ROLLBACK", ()))
                .await
                .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError);
        return Err(err);
    }
    if let Err(err) =
        store_backend::operation(DbOperation::Commit, conn.execute("COMMIT", ())).await
    {
        let _rollback =
            store_backend::operation(DbOperation::Rollback, conn.execute("ROLLBACK", ()))
                .await
                .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError);
        return Err(format!(
            "commit usage snapshot schema transaction failed: {err}"
        ));
    }
    Ok(())
}

async fn initialize_schema_contents(conn: &Connection) -> Result<(), String> {
    store_backend::operation(
        DbOperation::Update,
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _meta (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        ),
    )
    .await
    .map_err(|err| format!("initialize usage snapshot store metadata failed: {err}"))?;
    let mut versions = store_backend::operation(
        DbOperation::Select,
        conn.query("SELECT value FROM _meta WHERE key = 'schema_version'", ()),
    )
    .await
    .map_err(|err| format!("query usage snapshot store schema version failed: {err}"))?;
    let version = versions
        .next()
        .await
        .map_err(|err| format!("read usage snapshot store schema version failed: {err}"))?
        .map(|row| row_string(&row, 0, "schema_version"))
        .transpose()?;
    drop(versions);
    // V5/v6/v7 retain routing identities. Preserve last-good rows, exact counts,
    // and logical evidence. Missing Money scales stay unknown rather than
    // being inferred from display labels or scalar amounts.
    if matches!(version.as_deref(), Some("5" | "6" | "7" | "8")) {
        let mut columns = store_backend::operation(
            DbOperation::Select,
            conn.query("PRAGMA table_info(account_usage_snapshots)", ()),
        )
        .await
        .map_err(|err| err.to_string())?;
        let mut has_counts = false;
        let mut has_canonical = false;
        let mut has_money = false;
        let mut has_source_revision = false;
        while let Some(row) = columns.next().await.map_err(|err| err.to_string())? {
            let name = row_string(&row, 1, "column_name")?;
            has_counts |= name == "count_quota_json";
            has_canonical |= name == "canonical_identity_json";
            has_money |= name == "monetary_quota_json";
            has_source_revision |= name == "source_revision";
        }
        drop(columns);
        if !has_counts {
            store_backend::operation(
                DbOperation::Update,
                conn.execute(
                    "ALTER TABLE account_usage_snapshots ADD COLUMN count_quota_json TEXT",
                    (),
                ),
            )
            .await
            .map_err(|err| err.to_string())?;
        }
        if !has_canonical {
            store_backend::operation(
                DbOperation::Update,
                conn.execute(
                    "ALTER TABLE account_usage_snapshots ADD COLUMN canonical_identity_json TEXT",
                    (),
                ),
            )
            .await
            .map_err(|err| err.to_string())?;
        }
        if !has_money {
            store_backend::operation(
                DbOperation::Update,
                conn.execute(
                    "ALTER TABLE account_usage_snapshots ADD COLUMN monetary_quota_json TEXT",
                    (),
                ),
            )
            .await
            .map_err(|err| err.to_string())?;
        }
        if !has_source_revision {
            store_backend::operation(
                DbOperation::Update,
                conn.execute(
                    "ALTER TABLE account_usage_snapshots ADD COLUMN source_revision TEXT",
                    (),
                ),
            )
            .await
            .map_err(|err| err.to_string())?;
            store_backend::operation(DbOperation::Upsert, conn.execute(
                "INSERT INTO _meta (key, value) VALUES ('source_revision_migration_status', 'refresh_required')
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value", ()))
                .await.map_err(|err| err.to_string())?;
        }
        if !has_canonical || matches!(version.as_deref(), Some("5" | "6")) {
            store_backend::operation(
                DbOperation::Upsert,
                conn.execute(
                    "INSERT INTO _meta (key, value) VALUES ('canonical_identity_migration_status', 'refresh_required')
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value", (),
                ),
            ).await.map_err(|err| err.to_string())?;
        }
        store_backend::operation(
            DbOperation::Update,
            conn.execute(
                "UPDATE _meta SET value = ?1 WHERE key = 'schema_version'",
                [SCHEMA_VERSION],
            ),
        )
        .await
        .map_err(|err| err.to_string())?;
    } else if version.as_deref() != Some(SCHEMA_VERSION) {
        if !matches!(version.as_deref(), None | Some("1" | "2" | "3" | "4")) {
            return Err("unsupported usage snapshot cache schema; cache retained".to_owned());
        }
        // Display-derived older keys cannot recover broker identities.
        let mut tables = store_backend::operation(DbOperation::Select, conn.query("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'account_usage_snapshots'", ()))
            .await.map_err(|err| err.to_string())?;
        let had_snapshots = tables
            .next()
            .await
            .map_err(|err| err.to_string())?
            .is_some();
        drop(tables);
        store_backend::operation(
            DbOperation::Update,
            conn.execute("DROP TABLE IF EXISTS account_usage_snapshots", ()),
        )
        .await
        .map_err(|err| format!("invalidate obsolete usage snapshot cache failed: {err}"))?;
        if had_snapshots {
            store_backend::operation(
                DbOperation::Upsert,
                conn.execute(
                    "INSERT INTO _meta (key, value) VALUES ('usage_snapshot_migration_status', 'display_derived_cache_invalidated')
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value", (),
                ),
            ).await.map_err(|err| format!("record usage snapshot migration status failed: {err}"))?;
        }
    }
    store_backend::operation(
        DbOperation::Update,
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS account_usage_snapshots (
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
                account_id TEXT NOT NULL,
                surface_id TEXT NOT NULL,
                count_quota_json TEXT,
                canonical_identity_json TEXT,
                monetary_quota_json TEXT,
                source_revision TEXT,
                UNIQUE(surface_id, account_id, source, window_kind)
            );",
        ),
    )
    .await
    .map_err(|err| format!("initialize usage snapshot store schema failed: {err}"))?;
    store_backend::operation(
        DbOperation::Upsert,
        conn.execute(
            "INSERT INTO _meta (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [SCHEMA_VERSION],
        ),
    )
    .await
    .map_err(|err| format!("record usage snapshot store schema version failed: {err}"))?;
    Ok(())
}

async fn upsert_account_snapshot_rows(
    conn: &Connection,
    rows: Vec<StoredAccountUsageSnapshot>,
) -> Result<(), String> {
    validate_account_snapshot_rows(&rows)?;
    let canonical_json = rows
        .iter()
        .map(|row| {
            row.canonical_identity
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|_| "serialize canonical account evidence failed".to_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    let count_json = rows
        .iter()
        .map(|row| {
            row.count_quota
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|err| format!("serialize typed count quota failed: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let monetary_json = rows
        .iter()
        .map(|row| {
            StoredMonetaryQuota::from_amounts(
                row.used_money.clone(),
                row.limit_money.clone(),
                row.remaining_money.clone(),
            )
            .map(|quota| serde_json::to_string(&quota))
            .transpose()
            .map_err(|err| format!("serialize monetary quota failed: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    jackin_diagnostics::incr_db_statement("begin");
    store_backend::operation(DbOperation::Begin, conn.execute("BEGIN", ()))
        .await
        .map_err(|err| format!("begin telemetry snapshot transaction failed: {err}"))?;
    for (((row, count_json), canonical_json), monetary_json) in rows
        .into_iter()
        .zip(count_json)
        .zip(canonical_json)
        .zip(monetary_json)
    {
        jackin_diagnostics::incr_db_statement("upsert_account_usage_snapshot");
        if let Err(err) = store_backend::operation(
            DbOperation::Upsert,
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
                status_bar_label,
                account_id,
                surface_id,
                count_quota_json,
                canonical_identity_json,
                monetary_quota_json,
                source_revision
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31)
            ON CONFLICT(surface_id, account_id, source, window_kind) DO UPDATE SET
                provider = excluded.provider,
                account_key_hash = excluded.account_key_hash,
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
                status_bar_label = excluded.status_bar_label,
                count_quota_json = excluded.count_quota_json,
                canonical_identity_json = excluded.canonical_identity_json,
                monetary_quota_json = excluded.monetary_quota_json,
                source_revision = excluded.source_revision
            WHERE NOT (
                excluded.count_quota_json IS NULL AND excluded.monetary_quota_json IS NULL
                AND excluded.used_amount IS NULL AND excluded.limit_amount IS NULL
                AND (excluded.status IN ('unavailable', 'error', 'needs_login', 'needs_secret', 'unsupported')
                     OR excluded.view_status IN ('unavailable', 'error', 'needs_login', 'needs_secret', 'unsupported'))
                AND (account_usage_snapshots.count_quota_json IS NOT NULL
                     OR account_usage_snapshots.monetary_quota_json IS NOT NULL
                     OR account_usage_snapshots.used_amount IS NOT NULL
                     OR account_usage_snapshots.limit_amount IS NOT NULL
                     OR account_usage_snapshots.remaining_percent IS NOT NULL)
            )
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
                row.account_identity.account_id,
                row.account_identity.surface_id,
                count_json,
                canonical_json,
                monetary_json,
                row.account_identity.source_revision,
            ],
            ),
        )
        .await
        {
            // Roll the whole batch back so a mid-batch failure never leaves a
            // partially-written snapshot set; surface the original row error.
            let _rollback =
                store_backend::operation(DbOperation::Rollback, conn.execute("ROLLBACK", ()))
                    .await
                    .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError);
            return Err(format!("upsert telemetry account snapshot failed: {err}"));
        }
    }
    if let Err(err) =
        store_backend::operation(DbOperation::Commit, conn.execute("COMMIT", ())).await
    {
        // Roll back before the owned connection leaves this operation.
        let _rollback =
            store_backend::operation(DbOperation::Rollback, conn.execute("ROLLBACK", ()))
                .await
                .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::DbError);
        return Err(format!(
            "commit telemetry snapshot transaction failed: {err}"
        ));
    }
    Ok(())
}

fn account_snapshot_rows(view: &FocusedUsageView) -> Vec<StoredAccountUsageSnapshot> {
    let provider = view.account.provider_label.clone();
    let account_label = view.account.account_label.clone();
    let Some(identity) = view.account_identity.as_ref() else {
        return Vec::new();
    };
    let account_key_hash = crate::usage::usage_account_tab_id(identity);
    let source = crate::usage::usage_source_storage_label(view.source).to_owned();
    let confidence = crate::usage::usage_confidence_storage_label(view.confidence).to_owned();
    let fetched_at = view.fetched_at_epoch;
    let last_error = view.last_error.clone();
    view.buckets
        .iter()
        .map(|bucket| {
            let quota = quota_amounts(bucket);
            StoredAccountUsageSnapshot {
                provider: provider.clone(),
                account_key_hash: account_key_hash.clone(),
                account_identity: identity.clone(),
                canonical_identity: view.canonical_identity.clone(),
                account_label: account_label.clone(),
                source: source.clone(),
                confidence: confidence.clone(),
                window_kind: bucket.label.clone(),
                count_quota: bucket.count_quota.clone(),
                used_money: bucket.used_money.clone(),
                limit_money: bucket.limit_money.clone(),
                remaining_money: bucket.remaining_money.clone(),
                used_amount: quota.used_amount,
                used_unit: quota.used_unit,
                limit_amount: quota.limit_amount,
                limit_unit: quota.limit_unit,
                resets_at: bucket.resets_at,
                fetched_at,
                expires_at: None,
                status: crate::usage::usage_status_storage_label(bucket.status).to_owned(),
                last_error: last_error.clone(),
                focused_provider: view.focused_provider.clone(),
                plan_label: view.account.plan_label.clone(),
                remaining_percent: match &bucket.count_quota {
                    Some(count) => count.remaining_percent().map(i64::from),
                    None if bucket.used_money.is_some()
                        || bucket.limit_money.is_some()
                        || bucket.remaining_money.is_some() =>
                    {
                        monetary_remaining_percent(
                            bucket.used_money.as_ref(),
                            bucket.limit_money.as_ref(),
                            bucket.remaining_money.as_ref(),
                        )
                    }
                    None => bucket.remaining_percent.map(i64::from),
                },
                used_label: bucket.used_label.clone(),
                limit_label: bucket.limit_label.clone(),
                reset_label: bucket.reset_label.clone(),
                pace_label: bucket.pace_label.clone(),
                view_status: crate::usage::usage_status_storage_label(view.status).to_owned(),
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
    let (used_amount, used_unit, limit_amount, limit_unit) =
        crate::usage::quota_amounts_for_account_snapshot(bucket);
    QuotaAmounts {
        used_amount,
        used_unit,
        limit_amount,
        limit_unit,
    }
}

#[cfg(test)]
pub fn focused_usage_view(
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
        .map(|row| {
            let row = project_account_snapshot_row(row.clone());
            QuotaBucketView {
                count_quota: row.count_quota.clone(),
                used_money: row.used_money.clone(),
                limit_money: row.limit_money.clone(),
                remaining_money: row.remaining_money.clone(),
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: row.window_kind.clone(),
                used_label: row.used_label.clone(),
                limit_label: row.limit_label.clone(),
                remaining_percent: row
                    .remaining_percent
                    .and_then(|value| u8::try_from(value.clamp(0, 100)).ok()),
                reset_label: row.reset_label.clone(),
                resets_at: row.resets_at,
                // The headline is persisted as `status_bar_label`, so the slot tag is
                // not stored; a store-restored bucket carries none and the live
                // refresh re-tags it.
                status_slot: None,
                pace_label: row.pace_label.clone(),
                status: usage_status_from_label(&row.status),
            }
        })
        .collect::<Vec<_>>();
    buckets.sort_by_key(|bucket| usage_bucket_order(&provider, &bucket.label));
    let precision_status_bar = precision_safe_status_bar_label(&rows, &buckets);
    Ok(Some(FocusedUsageView {
        focused_agent: focused_agent.map(str::to_owned),
        focused_provider: first
            .focused_provider
            .clone()
            .or_else(|| resolved_provider.map(str::to_owned))
            .or_else(|| Some(provider.clone())),
        account_identity: Some(first.account_identity.clone()),
        canonical_identity: first.canonical_identity.clone(),
        account: FocusedAccountHeader {
            provider_label: provider,
            account_label: first.account_label.clone(),
            // username + credential_origin are live-snapshot fields, not
            // persisted in the store yet; a store-restored header gets them
            // on the next refresh.
            username: None,
            plan_label: first.plan_label.clone(),
            credential_origin: None,
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
        status_bar_label: if let Some(label) = precision_status_bar {
            label
        } else if first.status_bar_label.trim().is_empty() {
            lifecycle_status_bar_label(status)
        } else {
            first.status_bar_label.clone()
        },
        tabs,
        last_error: focused_precision_error(first.last_error.clone(), &rows),
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
        &[
            "Session",
            "Weekly",
            "All models",
            "Sonnet",
            "Daily Routines",
        ]
    } else if provider_matches("amp", &provider) {
        &["Amp Free", "Credits", "Individual credits"]
    } else if provider_matches("zai", &provider) || provider_matches("glm", &provider) {
        // F9: short/active window first (operator override of CodexBar's
        // Tokens, MCP, 5-hour order).
        &["5-hour", "Tokens", "MCP"]
    } else if provider_matches("kimi", &provider) {
        // F10: rate (short/active) window on top, then Weekly (operator
        // override of CodexBar's Weekly, Rate Limit order).
        &["Rate Limit", "Weekly"]
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
    // One tab per distinct stored account, keyed by the stable
    // `account_key_hash`; the newest fetch wins per account. Same-provider
    // accounts never collapse, and an empty store stays empty.
    let mut latest: HashMap<&str, &StoredAccountUsageSnapshot> = HashMap::new();
    for row in rows {
        latest
            .entry(row.account_key_hash.as_str())
            .and_modify(|current| {
                if row.fetched_at > current.fetched_at {
                    *current = row;
                }
            })
            .or_insert(row);
    }
    let mut tabs: Vec<jackin_protocol::control::UsageProviderTab> = latest
        .values()
        .map(|row| jackin_protocol::control::UsageProviderTab {
            id: row.account_key_hash.clone(),
            label: crate::usage::account_tab_label_for_parts(
                &row.provider,
                &row.account_label,
                row.focused_provider.as_deref(),
            ),
            status_label: tab_status_label(row, rows),
            account_label: row.account_label.clone(),
            plan_label: row.plan_label.clone(),
            source_label: Some(format!("{} · {}", row.view_status, row.source)),
            active: false,
        })
        .collect();
    tabs.sort_by(|left, right| {
        left.label
            .cmp(&right.label)
            .then(left.account_label.cmp(&right.account_label))
            .then(left.id.cmp(&right.id))
    });
    tabs
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
        .filter(|candidate| !has_imprecise_monetary_scalar(candidate))
        .find_map(|candidate| {
            candidate
                .remaining_percent
                .map(|remaining| format!("{} {remaining}% left", candidate.window_kind))
        })
        .unwrap_or_else(|| {
            if has_imprecise_monetary_scalar(row) {
                "refresh required".to_owned()
            } else {
                row.view_status.clone()
            }
        })
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
    load_all_account_snapshot_rows(path)
}

#[cfg(test)]
pub fn schema_version(path: &Path) -> Result<Option<String>, String> {
    let path = path_to_turso(path)?;
    block_on_store(async move {
        Ok(store_backend::read_local(&path, |conn| {
            let rows = conn.query("SELECT value FROM _meta WHERE key = 'schema_version'")?;
            rows.first()
                .map(|row| row.string(0, "schema_version"))
                .transpose()
        })?
        .flatten())
    })
}

#[cfg(test)]
fn row_i64(row: &Row, idx: usize, name: &str) -> Result<i64, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

fn row_string(row: &Row, idx: usize, name: &str) -> Result<String, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

fn row_opt_string(row: &Row, idx: usize, name: &str) -> Result<Option<String>, String> {
    row.get(idx)
        .map_err(|err| format!("decode telemetry {name} failed: {err}"))
}

/// One durable account reconstructed from a single, source-consistent snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAccountUsageView {
    /// Stable wire key derived from the canonical account and surface ids.
    pub account_key_hash: String,
    /// Reconstructed latest view. All buckets come from one selected source.
    pub view: FocusedUsageView,
}

/// List distinct accounts in the durable store (multi-account Desktop / host).
pub fn list_account_identities(path: &Path) -> Result<Vec<AccountIdentitySummary>, String> {
    // Use the same complete authority and fetch generation as focused views.
    // Historical rows never contribute meters under a newer route/proof.
    let mut out = Vec::new();
    for stored in load_all_account_usage_views(path, 0)? {
        let view = stored.view;
        let account_identity = view
            .account_identity
            .ok_or_else(|| "stored usage view lacks routing identity".to_owned())?;
        out.push(AccountIdentitySummary {
            account_identity,
            canonical_identity: view.canonical_identity,
            provider: view.account.provider_label,
            account_key_hash: stored.account_key_hash,
            account_label: view.account.account_label,
            plan_label: view.account.plan_label,
            remaining_percent: view
                .buckets
                .iter()
                .filter_map(|bucket| bucket.remaining_percent)
                .min(),
            fetched_at: view.fetched_at_epoch,
        });
    }

    out.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then(a.account_label.cmp(&b.account_label))
    });
    Ok(out)
}

/// Reconstruct a focused usage view for one account key from the durable store.
pub fn load_account_usage_view(
    path: &Path,
    account_key_hash: &str,
    now_epoch: i64,
) -> Result<Option<FocusedUsageView>, String> {
    Ok(load_all_account_usage_views(path, now_epoch)?
        .into_iter()
        .find(|stored| stored.account_key_hash == account_key_hash)
        .map(|stored| stored.view))
}

/// Reconstruct every durable account with one database scan.
///
/// Rows first pin the newest fetch generation, then one evidence source. This
/// prevents a same-timestamp provider API/CLI/local-log mix from duplicating
/// buckets or pairing one source's header with another source's limits.
pub fn load_all_account_usage_views(
    path: &Path,
    now_epoch: i64,
) -> Result<Vec<StoredAccountUsageView>, String> {
    let mut grouped: HashMap<String, Vec<StoredAccountUsageSnapshot>> = HashMap::new();
    for row in load_all_account_snapshot_rows(path)? {
        grouped
            .entry(row.account_key_hash.clone())
            .or_default()
            .push(row);
    }
    let mut views = Vec::with_capacity(grouped.len());
    for (account_key_hash, rows) in grouped {
        if let Some(view) = account_usage_view_from_rows(rows, now_epoch)? {
            views.push(StoredAccountUsageView {
                account_key_hash,
                view,
            });
        }
    }
    views.sort_by(|a, b| {
        a.view
            .account
            .provider_label
            .cmp(&b.view.account.provider_label)
            .then(
                a.view
                    .account
                    .account_label
                    .cmp(&b.view.account.account_label),
            )
    });
    Ok(views)
}

fn account_usage_view_from_rows(
    mut matches: Vec<StoredAccountUsageSnapshot>,
    now_epoch: i64,
) -> Result<Option<FocusedUsageView>, String> {
    if let Some(latest) = matches.iter().map(|row| row.fetched_at).max() {
        matches.retain(|row| row.fetched_at == latest);
    }
    let authorities: HashSet<_> = matches
        .iter()
        .map(|row| (&row.account_identity, &row.canonical_identity))
        .collect();
    if authorities.len() > 1 {
        return Err(
            "conflicting usage snapshot authority at latest fetch; explicit refresh required"
                .to_owned(),
        );
    }
    Ok(account_usage_view_from_selected_rows(matches, now_epoch))
}

fn account_usage_view_from_selected_rows(
    mut matches: Vec<StoredAccountUsageSnapshot>,
    now_epoch: i64,
) -> Option<FocusedUsageView> {
    if matches.is_empty() {
        return None;
    }
    let latest = matches.iter().map(|r| r.fetched_at).max().unwrap_or(0);
    matches.retain(|r| r.fetched_at == latest);
    let selected_source = matches
        .iter()
        .max_by_key(|row| durable_source_priority(&row.source))?
        .source
        .clone();
    matches.retain(|row| row.source == selected_source);
    matches.sort_by(|a, b| a.window_kind.cmp(&b.window_kind));
    let first = matches.first()?;
    let provider = first.provider.clone();
    let status = usage_status_from_storage(&first.view_status);
    let source = usage_source_from_storage(&first.source);
    let confidence = usage_confidence_from_storage(&first.confidence);
    let mut buckets: Vec<QuotaBucketView> = matches
        .iter()
        .map(|row| {
            let row = project_account_snapshot_row(row.clone());
            QuotaBucketView {
                count_quota: row.count_quota.clone(),
                used_money: row.used_money.clone(),
                limit_money: row.limit_money.clone(),
                remaining_money: row.remaining_money.clone(),
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: row.window_kind.clone(),
                used_label: row.used_label.clone(),
                limit_label: row.limit_label.clone(),
                remaining_percent: row
                    .remaining_percent
                    .and_then(|value| u8::try_from(value.clamp(0, 100)).ok()),
                reset_label: row.reset_label.clone(),
                resets_at: row.resets_at,
                status_slot: None,
                pace_label: row.pace_label.clone(),
                status: usage_status_from_storage(&row.status),
            }
        })
        .collect();
    // Stable window order: session/weekly first when present.
    buckets.sort_by(|a, b| a.label.cmp(&b.label));
    buckets.dedup_by(|a, b| a.label == b.label);
    let precision_status_bar = precision_safe_status_bar_label(&matches, &buckets);
    Some(FocusedUsageView {
        focused_agent: None,
        focused_provider: first.focused_provider.clone().or(Some(provider.clone())),
        account_identity: Some(first.account_identity.clone()),
        canonical_identity: first.canonical_identity.clone(),
        account: FocusedAccountHeader {
            provider_label: provider,
            account_label: first.account_label.clone(),
            username: None,
            plan_label: first.plan_label.clone(),
            credential_origin: None,
        },
        buckets,
        status,
        source,
        confidence,
        fetched_at_epoch: latest,
        updated_label: if first.updated_label.trim().is_empty() {
            crate::usage::relative_updated_label(latest, now_epoch)
        } else {
            first.updated_label.clone()
        },
        status_bar_label: if let Some(label) = precision_status_bar {
            label
        } else if first.status_bar_label.trim().is_empty() {
            "usage cached".to_owned()
        } else {
            first.status_bar_label.clone()
        },
        tabs: Vec::new(),
        last_error: focused_precision_error(first.last_error.clone(), &matches),
    })
}

fn durable_source_priority(source: &str) -> u8 {
    match source {
        "provider_api" => 4,
        "cli" => 3,
        "local_logs" => 2,
        "cache" => 1,
        _ => 0,
    }
}

fn usage_status_from_storage(label: &str) -> UsageSnapshotStatus {
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

fn usage_source_from_storage(label: &str) -> UsageSource {
    match label {
        "provider_api" => UsageSource::ProviderApi,
        "cli" => UsageSource::Cli,
        "local_logs" => UsageSource::LocalLogs,
        "cache" => UsageSource::Cache,
        _ => UsageSource::None,
    }
}

fn usage_confidence_from_storage(label: &str) -> UsageConfidence {
    match label {
        "authoritative" => UsageConfidence::Authoritative,
        "estimated" => UsageConfidence::Estimated,
        "presence_only" => UsageConfidence::PresenceOnly,
        _ => UsageConfidence::None,
    }
}

fn load_all_account_snapshot_rows(path: &Path) -> Result<Vec<StoredAccountUsageSnapshot>, String> {
    let path = path_to_turso(path)?;
    block_on_store(async move {
        Ok(store_backend::read_local(&path, |conn| {
            if !existing_snapshot_schema(conn)? {
                return Ok(Vec::new());
            }
            load_account_snapshot_rows(conn)
        })?
        .unwrap_or_default())
    })
}

fn existing_snapshot_schema(conn: &ReadOnlyConnection) -> Result<bool, String> {
    let tables = conn.query("SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('_meta', 'account_usage_snapshots')")?;
    let mut has_metadata = false;
    let mut has_snapshots = false;
    for row in tables {
        match row.string(0, "table_name")?.as_str() {
            "_meta" => has_metadata = true,
            "account_usage_snapshots" => has_snapshots = true,
            _ => {}
        }
    }
    if !has_snapshots {
        return Ok(false);
    }
    if !has_metadata {
        return Err(
            "usage snapshot cache schema requires explicit refresh migration; cache retained"
                .to_owned(),
        );
    }
    let versions = conn.query("SELECT value FROM _meta WHERE key = 'schema_version'")?;
    let current = versions
        .first()
        .map(|row| row.string(0, "schema_version"))
        .transpose()?;
    if current.as_deref() != Some(SCHEMA_VERSION) {
        if matches!(
            current.as_deref(),
            None | Some("1" | "2" | "3" | "4" | "5" | "6" | "7")
        ) {
            return Err(
                "usage snapshot cache schema requires explicit refresh migration; cache retained"
                    .to_owned(),
            );
        }
        return Err("unsupported usage snapshot cache schema; cache retained".to_owned());
    }
    let mut columns = HashSet::new();
    for row in conn.query("PRAGMA table_info(account_usage_snapshots)")? {
        columns.insert(row.string(1, "column_name")?);
    }
    if [
        "count_quota_json",
        "canonical_identity_json",
        "monetary_quota_json",
        "source_revision",
    ]
    .iter()
    .any(|name| !columns.contains(*name))
    {
        return Err(
            "usage snapshot cache schema requires explicit refresh migration; cache retained"
                .to_owned(),
        );
    }
    Ok(true)
}

fn load_account_snapshot_rows(
    conn: &ReadOnlyConnection,
) -> Result<Vec<StoredAccountUsageSnapshot>, String> {
    let rows = conn.query(
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
                    status_bar_label,
                    account_id,
                    surface_id,
                    count_quota_json,
                    canonical_identity_json,
                    monetary_quota_json,
                    source_revision
                FROM account_usage_snapshots
                ORDER BY provider, account_key_hash, source, window_kind
                ",
    )?;
    let mut snapshots = Vec::new();
    for row in rows {
        let monetary = row_monetary_quota(&row, 29)?;
        snapshots.push(StoredAccountUsageSnapshot {
            count_quota: row_count_quota(&row, 27)?,
            used_money: monetary.used,
            limit_money: monetary.limit,
            remaining_money: monetary.remaining,
            canonical_identity: row_canonical_identity(&row)?,
            provider: row.string(0, "provider")?,
            account_key_hash: row.string(1, "account_key_hash")?,
            account_label: row.string(2, "account_label")?,
            source: row.string(3, "source")?,
            confidence: row.string(4, "confidence")?,
            window_kind: row.string(5, "window_kind")?,
            used_amount: row.optional_i64(6, "used_amount")?,
            used_unit: row.optional_string(7, "used_unit")?,
            limit_amount: row.optional_i64(8, "limit_amount")?,
            limit_unit: row.optional_string(9, "limit_unit")?,
            resets_at: row.optional_i64(10, "resets_at")?,
            fetched_at: row.i64(11, "fetched_at")?,
            expires_at: row.optional_i64(12, "expires_at")?,
            status: row.string(13, "status")?,
            last_error: row.optional_string(14, "last_error")?,
            focused_provider: row.optional_string(15, "focused_provider")?,
            plan_label: row.optional_string(16, "plan_label")?,
            remaining_percent: row.optional_i64(17, "remaining_percent")?,
            used_label: row.optional_string(18, "used_label")?,
            limit_label: row.optional_string(19, "limit_label")?,
            reset_label: row.optional_string(20, "reset_label")?,
            pace_label: row.optional_string(21, "pace_label")?,
            view_status: row.string(22, "view_status")?,
            updated_label: row.string(23, "updated_label")?,
            status_bar_label: row.string(24, "status_bar_label")?,
            account_identity: UsageAccountIdentity {
                source_revision: row_source_revision(&row)?,
                account_id: row.string(25, "account_id")?,
                surface_id: row.string(26, "surface_id")?,
            },
        });
    }
    validate_account_snapshot_rows(&snapshots)?;
    Ok(snapshots)
}

const HISTORICAL_MONEY_PRECISION_ERROR: &str =
    "cached monetary quota lacks exact decimal scale; provider refresh required";

fn has_imprecise_monetary_scalar(row: &StoredAccountUsageSnapshot) -> bool {
    if row.count_quota.is_some()
        || row.used_money.is_some()
        || row.limit_money.is_some()
        || row.remaining_money.is_some()
    {
        return false;
    }
    [row.used_unit.as_deref(), row.limit_unit.as_deref()]
        .into_iter()
        .flatten()
        .any(|unit| {
            unit == "credits"
                || (unit.len() == 3 && unit.bytes().all(|byte| byte.is_ascii_uppercase()))
        })
}

fn project_account_snapshot_row(mut row: StoredAccountUsageSnapshot) -> StoredAccountUsageSnapshot {
    if row.used_money.is_some() || row.limit_money.is_some() || row.remaining_money.is_some() {
        row.used_amount = None;
        row.used_unit = None;
        row.limit_amount = None;
        row.limit_unit = None;
    }
    if has_imprecise_monetary_scalar(&row) {
        row.used_amount = None;
        row.used_unit = None;
        row.limit_amount = None;
        row.limit_unit = None;
        row.used_label = None;
        row.limit_label = None;
        row.remaining_percent = None;
        row.status = "unavailable".to_owned();
        row.last_error = Some(append_precision_error(row.last_error));
        row.pace_label = Some(HISTORICAL_MONEY_PRECISION_ERROR.to_owned());
    }
    row
}

fn precision_safe_status_bar_label(
    rows: &[StoredAccountUsageSnapshot],
    buckets: &[QuotaBucketView],
) -> Option<String> {
    if !rows.iter().any(has_imprecise_monetary_scalar) {
        return None;
    }
    let mut labels = crate::usage::status_bar_quota_labels(buckets);
    labels.extend(crate::usage::spend_headline_label(buckets));
    Some(if labels.is_empty() {
        "monetary quota refresh required".to_owned()
    } else {
        labels.join(" · ")
    })
}

fn append_precision_error(previous: Option<String>) -> String {
    match previous.filter(|error| !error.is_empty()) {
        Some(error) if error.contains(HISTORICAL_MONEY_PRECISION_ERROR) => error,
        Some(error) => format!("{error}; {HISTORICAL_MONEY_PRECISION_ERROR}"),
        None => HISTORICAL_MONEY_PRECISION_ERROR.to_owned(),
    }
}

fn focused_precision_error(
    previous: Option<String>,
    rows: &[StoredAccountUsageSnapshot],
) -> Option<String> {
    if rows.iter().any(has_imprecise_monetary_scalar) {
        Some(append_precision_error(previous))
    } else {
        previous
    }
}

fn validate_account_snapshot_rows(rows: &[StoredAccountUsageSnapshot]) -> Result<(), String> {
    for row in rows {
        validate_snapshot_binding(Some(&row.account_identity), row.canonical_identity.as_ref())?;
        if row.count_quota.is_some()
            && (row.used_money.is_some()
                || row.limit_money.is_some()
                || row.remaining_money.is_some())
        {
            return Err("snapshot quota combines request counts with monetary amounts".to_owned());
        }
        if row
            .used_money
            .as_ref()
            .is_some_and(|money| money.amount_minor < 0)
            || row
                .limit_money
                .as_ref()
                .is_some_and(|money| money.amount_minor < 0)
        {
            return Err("snapshot monetary usage or cap is negative".to_owned());
        }
        let mut amounts = [
            row.used_money.as_ref(),
            row.limit_money.as_ref(),
            row.remaining_money.as_ref(),
        ]
        .into_iter()
        .flatten();
        if let Some(first) = amounts.next() {
            if amounts.any(|money| money.currency != first.currency) {
                return Err("snapshot monetary quota combines currencies".to_owned());
            }
        }
    }
    Ok(())
}

fn validate_snapshot_binding(
    route: Option<&UsageAccountIdentity>,
    canonical: Option<&UsageCanonicalAccountIdentity>,
) -> Result<(), String> {
    match route {
        Some(route) => {
            if route.account_id.trim().is_empty() || route.surface_id.trim().is_empty() {
                return Err("invalid usage snapshot routing identity".to_owned());
            }
            validate_source_revision(route.source_revision.as_deref())?;
            validate_canonical_identity(canonical, &route.surface_id)
        }
        None if canonical.is_some() => {
            Err("canonical usage snapshot evidence lacks routing binding".to_owned())
        }
        None => Ok(()),
    }
}

fn validate_canonical_identity(
    identity: Option<&UsageCanonicalAccountIdentity>,
    surface_id: &str,
) -> Result<(), String> {
    let Some(identity) = identity else {
        return Ok(());
    };
    if identity.validate().is_err() || identity.surface_id != surface_id {
        return Err("invalid canonical account evidence in usage snapshot".to_owned());
    }
    Ok(())
}

fn row_canonical_identity(
    row: &ReadOnlyRow,
) -> Result<Option<UsageCanonicalAccountIdentity>, String> {
    let identity = row
        .optional_string(28, "canonical_identity_json")?
        .map(|json| {
            serde_json::from_str::<UsageCanonicalAccountIdentity>(&json)
                .map_err(|_| "decode canonical account evidence failed".to_owned())
        })
        .transpose()?;
    Ok(identity)
}

fn validate_source_revision(revision: Option<&str>) -> Result<(), String> {
    if revision.is_some_and(|value| value.trim().is_empty()) {
        return Err("invalid usage snapshot source revision".to_owned());
    }
    Ok(())
}

fn row_source_revision(row: &ReadOnlyRow) -> Result<Option<String>, String> {
    row.optional_string(30, "source_revision")
}

fn row_count_quota(row: &ReadOnlyRow, index: usize) -> Result<Option<CountQuota>, String> {
    row.optional_string(index, "count_quota_json")?
        .map(|json| {
            serde_json::from_str(&json)
                .map_err(|err| format!("decode typed count quota failed: {err}"))
        })
        .transpose()
}

fn monetary_remaining_percent(
    used: Option<&Money>,
    limit: Option<&Money>,
    remaining: Option<&Money>,
) -> Option<i64> {
    let limit = limit?;
    if let Some(remaining) = remaining {
        return remaining.remaining_percent_of(limit).map(i64::from);
    }
    limit
        .checked_sub(used?)?
        .remaining_percent_of(limit)
        .map(i64::from)
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct StoredMonetaryQuota {
    used: Option<Money>,
    limit: Option<Money>,
    remaining: Option<Money>,
}

impl StoredMonetaryQuota {
    fn from_amounts(
        used: Option<Money>,
        limit: Option<Money>,
        remaining: Option<Money>,
    ) -> Option<Self> {
        (used.is_some() || limit.is_some() || remaining.is_some()).then_some(Self {
            used,
            limit,
            remaining,
        })
    }
}

fn row_monetary_quota(row: &ReadOnlyRow, index: usize) -> Result<StoredMonetaryQuota, String> {
    row.optional_string(index, "monetary_quota_json")?
        .map(|json| {
            serde_json::from_str(&json)
                .map_err(|err| format!("decode monetary quota failed: {err}"))
        })
        .transpose()
        .map(|quota| quota.unwrap_or_default())
}

#[cfg(test)]
mod tests;
