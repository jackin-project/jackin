// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::overview::context;
use super::{database, failure, providers, selected};
use crate::{app::App, cli::UsageFilter, error::CommandError};
use jackin_usage::accounts::AccountRegistry;
use jackin_usage::view::{Report, ViewFilter, build, report};
use serde_json::json;

pub(super) fn show(
    app: &mut App,
    filter: &UsageFilter,
    json_output: bool,
) -> Result<(), CommandError> {
    let provider = filter
        .provider
        .as_deref()
        .map(providers::connector)
        .transpose()?;
    let now = app.deps.host.clock.now();
    let db = AccountRegistry::open_read_only(&database(app)?).map_err(failure)?;
    let mut fetched = false;
    let mut matched = false;
    if json_output {
        let mut accounts = Vec::new();
        if let Some(db) = &db {
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
        matched = !accounts.is_empty();
        fetched = accounts.iter().any(|a| !a["usage"].is_null());
        if !matched && (provider.is_some() || filter.account.is_some()) {
            return Err(CommandError::Argument("no matching usage account".into()));
        }
        app.io
            .json(&json!({"schema":1,"generated_at":now.0,"accounts":accounts}))?;
    } else if let Some(db) = &db {
        let view = build(
            db,
            &ViewFilter {
                provider,
                account: filter.account,
            },
            now,
            app.deps.host.clock.utc_offset_secs(),
        )
        .map_err(failure)?;
        matched = view.account_count() > 0;
        fetched = view
            .accounts()
            .any(|(_, account)| account.reading.fetched_at().is_some());
        if !matched && (provider.is_some() || filter.account.is_some()) {
            return Err(CommandError::Argument("no matching usage account".into()));
        }
        let lines = report(&view, &context(app), Report::Details);
        app.io.lines(&lines)?;
    } else if provider.is_some() || filter.account.is_some() {
        return Err(CommandError::Argument("no matching usage account".into()));
    }
    if matched && fetched {
        Ok(())
    } else {
        Err(CommandError::NoUsageSnapshot)
    }
}
