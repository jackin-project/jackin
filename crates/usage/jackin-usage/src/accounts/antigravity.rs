// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AccountRegistry, ConnectionError, vault};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use std::{path::Path, sync::Arc};
use uuid::Uuid;

impl AccountRegistry {
    pub(crate) fn open_statusline(path: &Path) -> Result<Self, ConnectionError> {
        Self::open_metadata(path, Arc::new(vault::SystemVault))
    }

    pub(crate) fn start_enrollment(
        &mut self,
        ticket: Uuid,
        email: &str,
        replace: Option<Uuid>,
        now: i64,
        expires: i64,
    ) -> Result<(), ConnectionError> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM antigravity_enrollments WHERE expires<=?1",
            [now],
        )?;
        let existing = tx
            .query_row(
                "SELECT id FROM accounts
             WHERE provider='antigravity' AND kind='statusline' AND identity=?1",
                [email],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if replace.map(|id| id.to_string()) != existing {
            return Err(if replace.is_some() {
                ConnectionError::DifferentIdentity
            } else {
                ConnectionError::Duplicate
            });
        }
        let inserted = tx.execute(
            "INSERT INTO antigravity_enrollments(ticket,email,expires)
             VALUES (?1,?2,?3) ON CONFLICT(email) DO NOTHING",
            params![ticket.to_string(), email, expires],
        )?;
        if inserted == 0 {
            return Err(ConnectionError::EnrollmentPending);
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn cancel_enrollment(&self, ticket: Uuid) -> Result<(), ConnectionError> {
        self.db.execute(
            "DELETE FROM antigravity_enrollments WHERE ticket=?1",
            [ticket.to_string()],
        )?;
        Ok(())
    }

    pub(crate) fn receive_statusline(
        &mut self,
        email: &str,
        snapshot: &str,
        now: i64,
    ) -> Result<bool, ConnectionError> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let reading = "CASE
            WHEN json_type(?1,'$.plan') IS NULL AND json_type(snapshot,'$.plan')='text'
            THEN json_set(?1,'$.plan',json_extract(snapshot,'$.plan'))
            ELSE ?1 END";
        let enrolled = tx.execute(
            &format!(
                "UPDATE antigravity_enrollments SET snapshot={reading},fetched_at=?2
                      WHERE email=?3 AND expires>?2 AND (fetched_at IS NULL OR fetched_at<=?2)"
            ),
            params![snapshot, now, email],
        )?;
        let linked = tx.execute(
            &format!(
                "UPDATE accounts SET snapshot={reading},fetched_at=?2
                      WHERE provider='antigravity' AND kind='statusline' AND identity=?3
                        AND fetched_at<=?2"
            ),
            params![snapshot, now, email],
        )?;
        tx.commit()?;
        Ok(enrolled + linked > 0)
    }

    pub(crate) fn finish_enrollment(
        &mut self,
        ticket: Uuid,
        replace: Option<Uuid>,
        now: i64,
    ) -> Result<Option<Uuid>, ConnectionError> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (email, fetched, snapshot) = tx
            .query_row(
                "SELECT email,fetched_at,snapshot FROM antigravity_enrollments
             WHERE ticket=?1 AND expires>?2",
                params![ticket.to_string(), now],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or(ConnectionError::Changed)?;
        let (Some(fetched), Some(snapshot)) = (fetched, snapshot) else {
            return Ok(None);
        };
        let id = if let Some(id) = replace {
            id
        } else {
            let id = Uuid::new_v4();
            tx.execute(
                "INSERT INTO accounts(id,provider,label,kind,identity,slot,position,fetched_at,snapshot)
                 SELECT ?1,'antigravity',?2,'statusline',?2,?1,COALESCE(MAX(position),-1)+1,?3,?4
                 FROM accounts",
                params![id.to_string(), email, fetched, snapshot],
            )?;
            id
        };
        tx.execute(
            "DELETE FROM antigravity_enrollments WHERE ticket=?1",
            [ticket.to_string()],
        )?;
        tx.commit()?;
        Ok(Some(id))
    }
}
