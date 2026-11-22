// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AccountPreferences, AccountRegistry, ConnectionError};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetricState {
    Visible,
    Hidden,
    Pinned,
}

#[derive(Clone, Debug)]
pub struct MetricEdit {
    pub account: Uuid,
    pub before: MetricState,
    pub after: MetricState,
}

impl AccountRegistry {
    pub fn edit_metric(&mut self, key: &str, edits: &[MetricEdit]) -> Result<(), ConnectionError> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut accounts = BTreeSet::new();
        for edit in edits {
            if !accounts.insert(edit.account) {
                return Err(ConnectionError::Invalid);
            }
            let id = edit.account.to_string();
            let data: String = tx
                .query_row("SELECT preferences FROM accounts WHERE id=?1", [&id], |r| {
                    r.get(0)
                })
                .optional()?
                .ok_or(ConnectionError::Changed)?;
            let mut prefs: AccountPreferences =
                serde_json::from_str(&data).map_err(|_| ConnectionError::Invalid)?;
            let prior = if prefs.hidden.contains(key) {
                MetricState::Hidden
            } else if prefs.pinned.contains(key) {
                MetricState::Pinned
            } else {
                MetricState::Visible
            };
            if prior != edit.before {
                return Err(ConnectionError::Changed);
            }
            prefs.hidden.remove(key);
            prefs.pinned.remove(key);
            match edit.after {
                MetricState::Pinned => {
                    prefs.pinned.insert(key.into());
                }
                MetricState::Hidden => {
                    prefs.hidden.insert(key.into());
                }
                MetricState::Visible => {}
            }
            let data = serde_json::to_string(&prefs).map_err(|_| ConnectionError::Invalid)?;
            tx.execute(
                "UPDATE accounts SET preferences=?1 WHERE id=?2",
                params![data, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}
