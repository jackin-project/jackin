// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::TerminalText;
use jackin_usage::accounts::{AccountRegistry, UsageProvider};
use jackin_usage::dashboard::DashboardMemory;
use serde_json::json;
use uuid::Uuid;

use super::{database, failure, providers, selected};
use crate::app::App;
use crate::cli::UsageFilter;
use crate::error::CommandError;

pub(in crate::commands) fn overview(app: &mut App, json: bool) -> Result<(), CommandError> {
    let db = AccountRegistry::open_read_only(&database(app)?).map_err(failure)?;
    if json {
        let snapshots = db
            .as_ref()
            .map(AccountRegistry::snapshots)
            .transpose()
            .map_err(failure)?
            .unwrap_or_default();
        let now = app.deps.host.clock.now().0;
        let catalog = providers::catalog(snapshots.iter().map(|(account, _, _)| account.provider));
        let accounts: Vec<_> = snapshots
            .iter()
            .map(|(account, at, payload)| {
                jackin_usage::export::snapshot_account(account, *at, payload.as_ref(), now)
            })
            .collect();
        app.io.json(
            &json!({"schema":1,"generated_at":now,"accounts":accounts,"providers":catalog}),
        )?;
        return if snapshots.iter().any(|(_, _, payload)| payload.is_some()) {
            Ok(())
        } else {
            Err(CommandError::NoUsageSnapshot)
        };
    }
    let accounts = db
        .as_ref()
        .map(AccountRegistry::list)
        .transpose()
        .map_err(failure)?
        .unwrap_or_default();
    let catalog = providers::catalog(accounts.iter().map(|a| a.provider));
    let Some(db) = db else {
        for provider in catalog {
            app.io.line(&provider.to_string())?;
        }
        app.io.note("To connect your first provider, open Accounts in the console or run jackin account usage connect <provider>.");
        return Err(CommandError::NoUsageSnapshot);
    };
    let board = jackin_usage::dashboard::Dashboard::load(
        &db,
        &mut DashboardMemory::default(),
        app.deps.host.clock.now().0,
    )
    .map_err(failure)?;
    let mut group = None;
    for row in &board.all {
        let next = (row.provider, row.account, row.label.as_str());
        if group != Some(next) {
            let heading = format!("{} · {}", row.provider, row.label);
            app.io
                .line(&TerminalText::sanitize(heading.as_bytes()).to_string())?;
            group = Some(next);
        }
        let mut columns = vec![
            jackin_core::plain(&row.metric_label),
            jackin_usage::presentation::metric_value(row, board.settings.remaining),
        ];
        if let Some(reset) = &row.reset {
            columns.push(jackin_usage::presentation::reset_caption(
                reset,
                board.settings.exact_resets,
            ));
        }
        if row.stale {
            columns.push("stale".into());
        }
        let line = format!("  {}", columns.join(" · "));
        app.io
            .line(&TerminalText::sanitize(line.as_bytes()).to_string())?;
    }
    if board.all.is_empty() && board.waiting.is_empty() {
        app.io.line(&board.summary.to_string())?;
    }
    for (index, account) in accounts.iter().enumerate() {
        let notice = board
            .notices
            .iter()
            .find(|notice| notice.account == account.id);
        if board
            .all
            .iter()
            .any(|row| row.account == Some(account.id) || row.members.contains(&account.id))
        {
            if let Some(notice) = notice {
                app.io.note(
                    &TerminalText::sanitize(
                        format!("{}: {}", notice.label, notice.message).as_bytes(),
                    )
                    .to_string(),
                );
            }
            continue;
        }
        let label = if board.settings.conceal_names {
            jackin_usage::dashboard::concealed_label(index)
        } else {
            account.label.clone()
        };
        let status = if let Some(notice) = notice {
            notice.message.as_str()
        } else if board.waiting.contains(&account.id) {
            "Waiting for a first usage reading"
        } else {
            "No displayed readings"
        };
        let line = format!("{} · {label} · {status}", account.provider);
        app.io
            .line(&TerminalText::sanitize(line.as_bytes()).to_string())?;
    }
    for provider in catalog.iter().filter(|p| p.account_count == 0) {
        app.io.line(&provider.to_string())?;
    }
    if board.has_snapshot {
        Ok(())
    } else {
        Err(CommandError::NoUsageSnapshot)
    }
}

pub(super) fn snapshot(
    app: &mut App,
    filter: &UsageFilter,
    _json: bool,
) -> Result<(), CommandError> {
    let provider = filter
        .provider
        .as_deref()
        .map(providers::connector)
        .transpose()?;
    let db = AccountRegistry::open_read_only(&database(app)?).map_err(failure)?;
    emit(app, db.as_ref(), filter, provider, &[], None)
}

pub(super) fn emit(
    app: &mut App,
    db: Option<&AccountRegistry>,
    filter: &UsageFilter,
    provider: Option<UsageProvider>,
    refresh_errors: &[(Uuid, String)],
    first_error: Option<CommandError>,
) -> Result<(), CommandError> {
    let mut rows = Vec::new();
    if let Some(db) = db {
        let now = app.deps.host.clock.now().0;
        for (account, fetched_at, payload) in db.snapshots().map_err(failure)? {
            if selected(&account, filter, provider) {
                let mut row = jackin_usage::export::snapshot_account(
                    &account,
                    fetched_at,
                    payload.as_ref(),
                    now,
                );
                if let Some(failure) = db.failure(account.id).map_err(failure)? {
                    row["stale"] = true.into();
                    row["failure"] = json!({
                        "message": failure.message,
                        "retry_at": failure.retry_at
                    });
                } else if let Some((_, message)) =
                    refresh_errors.iter().find(|(id, _)| *id == account.id)
                {
                    row["failure"] = json!({"message": message});
                }
                rows.push(row);
            }
        }
    }
    if rows.is_empty() && (provider.is_some() || filter.account.is_some()) {
        return Err(CommandError::Argument("no matching usage account".into()));
    }
    app.io
        .json(&json!({"schema":1,"generated_at":app.deps.host.clock.now().0,"accounts":rows}))?;
    if let Some(error) = first_error {
        return Err(error);
    }
    if rows.iter().all(|r| r["usage"].is_null()) {
        return Err(CommandError::NoUsageSnapshot);
    }
    Ok(())
}
