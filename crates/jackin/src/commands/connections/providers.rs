// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt;

use jackin_usage::accounts::{AccountRegistry, UsageProvider};
use jackin_usage::catalog::{UsageProduct, providers};
use serde::Serialize;

use super::{database, failure};
use crate::{app::App, error::CommandError};

#[derive(Serialize)]
pub(super) struct ProviderRow {
    id: &'static str,
    name: &'static str,
    status: &'static str,
    pub(super) account_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    unavailable_reason: Option<&'static str>,
}

impl fmt::Display for ProviderRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} · {} · ", self.name, self.id)?;
        if let Some(reason) = self.unavailable_reason {
            write!(f, "{reason}")
        } else {
            match self.account_count {
                0 => f.write_str("Not connected"),
                1 => f.write_str("1 connected account"),
                n => write!(f, "{n} connected accounts"),
            }
        }
    }
}

fn product(id: &str) -> Result<UsageProduct, CommandError> {
    providers().find(|p| p.id == id).ok_or_else(|| {
        CommandError::Argument(
            "unknown usage provider; run jackin usage providers for available IDs".into(),
        )
    })
}

pub(super) fn connector(id: &str) -> Result<UsageProvider, CommandError> {
    let p = product(id)?;
    p.connector.ok_or_else(|| {
        CommandError::Argument(format!(
            "{}: {}",
            p.name,
            p.unavailable_reason.unwrap_or("Usage tracking unavailable")
        ))
    })
}

pub(super) fn catalog(accounts: impl Iterator<Item = UsageProvider> + Clone) -> Vec<ProviderRow> {
    providers()
        .map(|p| {
            let account_count = accounts
                .clone()
                .filter(|provider| Some(*provider) == p.connector)
                .count();
            ProviderRow {
                id: p.id,
                name: p.name,
                status: if p.connector.is_none() {
                    "unavailable"
                } else if account_count == 0 {
                    "not_connected"
                } else {
                    "connected"
                },
                account_count,
                unavailable_reason: p.unavailable_reason,
            }
        })
        .collect()
}

pub(crate) fn show(app: &mut App, id: Option<&str>, json: bool) -> Result<(), CommandError> {
    let selected = id.map(product).transpose()?;
    let accounts = AccountRegistry::open_read_only(&database(app)?)
        .map_err(failure)?
        .map(|db| db.list())
        .transpose()
        .map_err(failure)?
        .unwrap_or_default();
    let mut rows = catalog(accounts.iter().map(|a| a.provider));
    if let Some(p) = selected {
        rows.retain(|r| r.id == p.id);
    }
    if json {
        app.io
            .json(&serde_json::json!({"schema":1,"providers":rows}))
    } else {
        for row in rows {
            app.io.line(&row.to_string())?;
        }
        Ok(())
    }
}
