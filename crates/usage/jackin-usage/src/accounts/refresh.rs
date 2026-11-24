// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AccountRegistry, ConnectionError, CredentialVersion};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Authentication,
    Network,
    Service,
    RateLimited,
    Timeout,
    Limitation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshFailure {
    pub kind: FailureKind,
    pub message: String,
    pub retry_at: i64,
    pub failures: u32,
}

impl RefreshFailure {
    pub fn holds(&self, now: i64, manual: bool) -> bool {
        self.retry_at > now && (!manual || self.kind == FailureKind::RateLimited)
    }
}

impl AccountRegistry {
    pub fn failure(&self, id: Uuid) -> Result<Option<RefreshFailure>, ConnectionError> {
        let row = self
            .db
            .query_row(
                "SELECT refresh_failures.kind,failures,retry_at,message FROM refresh_failures
            JOIN accounts USING(id) WHERE id=?1 AND accounts.slot=refresh_failures.slot",
                [id.to_string()],
                |r| Ok((r.get::<_, String>(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;
        row.map(|(kind, failures, retry_at, message)| {
            Ok(RefreshFailure {
                kind: serde_json::from_str(&kind).map_err(|_| ConnectionError::Changed)?,
                failures,
                retry_at,
                message,
            })
        })
        .transpose()
    }

    pub fn record_failure(
        &mut self,
        id: Uuid,
        version: CredentialVersion,
        now: i64,
        kind: FailureKind,
        retry_after: Option<u64>,
        message: &str,
    ) -> Result<(), ConnectionError> {
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        write_failure(&tx, id, version, now, kind, retry_after, message)?;
        tx.commit()?;
        Ok(())
    }
}

pub(super) fn write_failure(
    tx: &rusqlite::Connection,
    id: Uuid,
    version: CredentialVersion,
    now: i64,
    kind: FailureKind,
    retry_after: Option<u64>,
    message: &str,
) -> Result<(), ConnectionError> {
    let prior: u32 = tx
        .query_row(
            "SELECT failures FROM refresh_failures WHERE id=?1 AND slot=?2",
            params![id.to_string(), version.0.to_string()],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(0);
    let failures = prior.saturating_add(u32::from(kind != FailureKind::Network));
    let default_wait = 300_i64
        .saturating_mul(1 << failures.saturating_sub(1).min(4))
        .min(3600);
    let wait = match kind {
        FailureKind::Authentication | FailureKind::Limitation => 3600,
        FailureKind::Network => 300,
        FailureKind::RateLimited => retry_after.map_or(default_wait, |seconds| {
            i64::try_from(seconds).unwrap_or(3600).clamp(0, 3600)
        }),
        _ => default_wait,
    };
    let kind = serde_json::to_string(&kind).map_err(|_| ConnectionError::Changed)?;
    if tx.execute(
        "INSERT OR REPLACE INTO refresh_failures SELECT id,slot,?3,?4,?5,?6 FROM accounts
            WHERE id=?1 AND slot=?2",
        params![
            id.to_string(),
            version.0.to_string(),
            kind,
            failures,
            now.saturating_add(wait),
            message
        ],
    )? == 0
    {
        return Err(ConnectionError::Changed);
    }
    Ok(())
}
