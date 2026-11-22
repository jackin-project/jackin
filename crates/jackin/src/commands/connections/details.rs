// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{database, failure, selected};
use crate::{app::App, cli::UsageFilter, error::CommandError};
use jackin_core::plain;
use jackin_usage::accounts::{AccountRegistry, UsageProvider};
use serde_json::{Value, json};

pub(super) fn show(
    app: &mut App,
    filter: &UsageFilter,
    json_output: bool,
) -> Result<(), CommandError> {
    let provider = filter
        .provider
        .as_ref()
        .map(|p| p.parse::<UsageProvider>())
        .transpose()
        .map_err(|e| CommandError::Argument(e.to_string()))?;
    let now = app.deps.host.clock.now();
    let mut accounts = Vec::new();
    if let Some(db) = AccountRegistry::open_read_only(&database(app)?).map_err(failure)? {
        let settings = db.dashboard_settings().map_err(failure)?;
        for (index, row) in db.snapshots().map_err(failure)?.into_iter().enumerate() {
            if selected(&row.0, filter, provider) {
                let fault = db.failure(row.0.id).map_err(failure)?;
                accounts.push(jackin_usage::export::details_account(
                    row,
                    index,
                    &settings,
                    fault.as_ref(),
                    now,
                ));
            }
        }
    }
    if accounts.is_empty() && (provider.is_some() || filter.account.is_some()) {
        return Err(CommandError::Argument("no matching usage account".into()));
    }
    if json_output {
        app.io
            .json(&json!({"schema":1,"generated_at":now.0,"accounts":accounts}))?;
    } else {
        for (index, a) in accounts.iter().enumerate() {
            if index > 0 {
                app.io.line("")?;
            }
            let record = &a["account"];
            let field = |key: &str| plain(record[key].as_str().unwrap_or_default());
            let stale = if a["stale"] == true && !a["usage"].is_null() {
                " · stale"
            } else {
                ""
            };
            app.io.line(&format!(
                "{} · {} · {}{stale}",
                field("provider"),
                field("label"),
                field("id")
            ))?;
            for line in a["details"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                app.io.line(&format!("  {line}"))?;
            }
        }
    }
    if accounts.iter().all(|a| a["usage"].is_null()) {
        return Err(CommandError::NoUsageSnapshot);
    }
    Ok(())
}
