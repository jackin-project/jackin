// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use super::{AccountRegistry, ConnectionError, CredentialVersion, changed};

impl AccountRegistry {
    pub(crate) fn prior_claim(
        &self,
        id: Uuid,
        target: &str,
        pending_scope: Option<&str>,
        expiry_scope: Option<&str>,
    ) -> Result<Option<(String, Uuid, bool)>, ConnectionError> {
        let row: Option<(String, String, bool)> = self
            .db
            .query_row(
                "SELECT target,request,done FROM reset_claims WHERE account=?1 AND (target=?2 OR (done=0 AND substr(target,1,length(?3))=?3) OR (substr(target,1,6)='codex/' AND substr(target,-length(?4))=?4)) ORDER BY done LIMIT 1",
                params![id.to_string(), target, pending_scope, expiry_scope],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        row.map(|(target, s, done)| {
            Ok((
                target,
                Uuid::parse_str(&s).map_err(|_| ConnectionError::Invalid)?,
                done,
            ))
        })
        .transpose()
    }

    pub(crate) fn record_claim(
        &self,
        id: Uuid,
        version: CredentialVersion,
        target: &str,
        request: Uuid,
    ) -> Result<(), ConnectionError> {
        changed(self.db.execute("INSERT INTO reset_claims(account,target,request) SELECT id,?3,?4 FROM accounts WHERE id=?1 AND slot=?2",params![id.to_string(),version.0.to_string(),target,request.to_string()])?)
    }

    pub(crate) fn settle_claim(
        &self,
        id: Uuid,
        target: &str,
        spent: bool,
    ) -> Result<(), ConnectionError> {
        let sql = if spent {
            "UPDATE reset_claims SET done=1 WHERE account=?1 AND target=?2"
        } else {
            "DELETE FROM reset_claims WHERE account=?1 AND target=?2"
        };
        self.db.execute(sql, params![id.to_string(), target])?;
        Ok(())
    }
}
