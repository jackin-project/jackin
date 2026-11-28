// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    AccountRegistry, ConnectionError, CredentialVault, CredentialVersion, changed, replace_slot,
    uuid,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{path::Path, sync::Arc};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct DirectSlot {
    id: Uuid,
    version: CredentialVersion,
}

impl DirectSlot {
    pub(crate) fn require_missing(
        self,
        vault: &dyn CredentialVault,
    ) -> Result<(), ConnectionError> {
        match vault.load(self.version.0) {
            Err(ConnectionError::MissingCredential) => Ok(()),
            Ok(_) => Err(ConnectionError::DuplicateDirect),
            Err(error) => Err(error),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnrollmentTarget {
    New,
    Convert(DirectSlot),
    Reconnect(Uuid),
}

impl AccountRegistry {
    pub(crate) fn open_statusline(
        path: &Path,
        vault: Arc<dyn CredentialVault>,
    ) -> Result<Self, ConnectionError> {
        Self::open_metadata(path, vault)
    }

    pub(crate) fn enrollment_target(
        &self,
        email: &str,
        replace: Option<Uuid>,
    ) -> Result<EnrollmentTarget, ConnectionError> {
        if statusline_account(&self.db, email)? != replace {
            return Err(if replace.is_some() {
                ConnectionError::DifferentIdentity
            } else {
                ConnectionError::Duplicate
            });
        }
        if let Some(id) = replace {
            Ok(EnrollmentTarget::Reconnect(id))
        } else {
            Ok(direct_slot(&self.db, email)?
                .map_or(EnrollmentTarget::New, EnrollmentTarget::Convert))
        }
    }

    pub(crate) fn start_enrollment(
        &mut self,
        ticket: Uuid,
        email: &str,
        target: EnrollmentTarget,
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
        check_target(&tx, email, target)?;
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

    pub(crate) fn enrollment_ready(&self, ticket: Uuid, now: i64) -> Result<bool, ConnectionError> {
        self.db
            .query_row(
                "SELECT fetched_at IS NOT NULL AND snapshot IS NOT NULL
                 FROM antigravity_enrollments WHERE ticket=?1 AND expires>?2",
                params![ticket.to_string(), now],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(ConnectionError::Changed)
    }

    pub(crate) fn finish_enrollment(
        &mut self,
        ticket: Uuid,
        target: EnrollmentTarget,
        now: i64,
    ) -> Result<Uuid, ConnectionError> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (email, fetched, snapshot) = tx
            .query_row(
                "SELECT email,fetched_at,snapshot FROM antigravity_enrollments
             WHERE ticket=?1 AND expires>?2 AND fetched_at IS NOT NULL AND snapshot IS NOT NULL",
                params![ticket.to_string(), now],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or(ConnectionError::Changed)?;
        check_target(&tx, &email, target)?;
        let id = match target {
            EnrollmentTarget::Reconnect(id) => id,
            EnrollmentTarget::Convert(DirectSlot { id, version }) => {
                convert_direct(&tx, id, version, &email, fetched, &snapshot)?;
                id
            }
            EnrollmentTarget::New => {
                let id = Uuid::new_v4();
                tx.execute(
                    "INSERT INTO accounts(id,provider,label,kind,identity,slot,position,fetched_at,snapshot)
                 SELECT ?1,'antigravity',?2,'statusline',?2,?1,COALESCE(MAX(position),-1)+1,?3,?4
                 FROM accounts",
                    params![id.to_string(), email, fetched, snapshot],
                )?;
                id
            }
        };
        tx.execute(
            "DELETE FROM antigravity_enrollments WHERE ticket=?1",
            [ticket.to_string()],
        )?;
        tx.commit()?;
        Ok(id)
    }
}

fn statusline_account(db: &Connection, email: &str) -> Result<Option<Uuid>, ConnectionError> {
    Ok(db
        .query_row(
            "SELECT id FROM accounts
             WHERE provider='antigravity' AND kind='statusline' AND identity=?1",
            [email],
            |row| uuid(row, 0),
        )
        .optional()?)
}

fn direct_slot(db: &Connection, email: &str) -> Result<Option<DirectSlot>, ConnectionError> {
    Ok(db
        .query_row(
            "SELECT id,slot FROM accounts
             WHERE provider='antigravity' AND kind='oauth' AND lower(identity)=lower(?1)",
            [email],
            |row| {
                Ok(DirectSlot {
                    id: uuid(row, 0)?,
                    version: CredentialVersion(uuid(row, 1)?),
                })
            },
        )
        .optional()?)
}

fn check_target(
    db: &Connection,
    email: &str,
    target: EnrollmentTarget,
) -> Result<(), ConnectionError> {
    let statusline = statusline_account(db, email)?;
    let matches = match target {
        EnrollmentTarget::Reconnect(id) => statusline == Some(id),
        EnrollmentTarget::New => statusline.is_none() && direct_slot(db, email)?.is_none(),
        EnrollmentTarget::Convert(slot) => {
            statusline.is_none() && direct_slot(db, email)? == Some(slot)
        }
    };
    if !matches {
        return Err(ConnectionError::Changed);
    }
    Ok(())
}

fn convert_direct(
    tx: &Connection,
    id: Uuid,
    expected: CredentialVersion,
    email: &str,
    fetched: i64,
    snapshot: &str,
) -> Result<(), ConnectionError> {
    replace_slot(tx, id, expected, id)?;
    changed(tx.execute(
        "UPDATE accounts SET kind='statusline', identity=?1, fetched_at=?2, snapshot=?3
         WHERE id=?4 AND kind='oauth'",
        params![email, fetched, snapshot, id.to_string()],
    )?)
}
