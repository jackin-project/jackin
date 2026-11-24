// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage::accounts::{AccountRegistry, UsageProvider};
use jackin_usage::view::{
    self, Ctx, Line, Reading, Report, Span, Tone, UsageView, ViewFilter, render,
};
use serde_json::json;
use uuid::Uuid;

use super::{RefreshOutcome, database, failure, providers, selected};
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
    show(
        app,
        &UsageFilter {
            provider: None,
            account: None,
        },
        Report::Usage,
    )
}

fn context(app: &App) -> Ctx {
    let width = app.terminal.as_ref().map_or(render::WIDE, |terminal| {
        terminal.size().cols.get().min(render::WIDE)
    });
    Ctx::new(width)
}

fn print(app: &mut App, view: &UsageView, kind: Report) -> Result<(), CommandError> {
    let lines = view::report(view, &context(app), kind);
    app.io.lines(&lines)
}

fn introduce(app: &mut App) -> Result<(), CommandError> {
    for provider in providers::catalog(std::iter::empty()) {
        app.io.line(&provider.to_string())?;
    }
    app.io.note("To connect your first provider, open Accounts in the console or run jackin account usage connect <provider>.");
    Err(CommandError::NoUsageSnapshot)
}

fn show(app: &mut App, filter: &UsageFilter, kind: Report) -> Result<(), CommandError> {
    let provider = filter
        .provider
        .as_deref()
        .map(providers::connector)
        .transpose()?;
    let db = AccountRegistry::open_read_only(&database(app)?).map_err(failure)?;
    let filtered = provider.is_some() || filter.account.is_some();
    let Some(db) = db else {
        return if filtered {
            Err(CommandError::Argument("no matching usage account".into()))
        } else {
            introduce(app)
        };
    };
    let view = view::build(
        &db,
        &ViewFilter {
            provider,
            account: filter.account,
        },
        app.deps.host.clock.now(),
        app.deps.host.clock.utc_offset_secs(),
    )
    .map_err(failure)?;
    if view.account_count() == 0 {
        return if filtered {
            Err(CommandError::Argument("no matching usage account".into()))
        } else {
            introduce(app)
        };
    }
    print(app, &view, kind)?;
    let read = db
        .snapshots()
        .map_err(failure)?
        .iter()
        .any(|(account, _, payload)| payload.is_some() && selected(account, filter, provider));
    if read {
        Ok(())
    } else {
        Err(CommandError::NoUsageSnapshot)
    }
}

pub(super) fn snapshot(
    app: &mut App,
    filter: &UsageFilter,
    json: bool,
) -> Result<(), CommandError> {
    if !json {
        return show(app, filter, Report::Snapshot);
    }
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

pub(super) fn refreshed(
    app: &mut App,
    db: &AccountRegistry,
    filter: &UsageFilter,
    provider: Option<UsageProvider>,
    outcome: &RefreshOutcome,
) -> Result<(), CommandError> {
    let view = view::build(
        db,
        &ViewFilter {
            provider,
            account: filter.account,
        },
        app.deps.host.clock.now(),
        app.deps.host.clock.utc_offset_secs(),
    )
    .map_err(failure)?;
    if view.account_count() == 0 {
        return if provider.is_some() || filter.account.is_some() {
            Err(CommandError::Argument("no matching usage account".into()))
        } else {
            introduce(app)
        };
    }
    let failed = view
        .accounts()
        .filter(|(_, quota)| outcome.failed(quota.id))
        .count();
    let skipped = view
        .accounts()
        .filter(|(_, quota)| outcome.skipped(quota.id))
        .count();
    let total = view.account_count();
    let mut headline = format!("Refreshed {} of {total} accounts", total - failed - skipped);
    if skipped > 0 {
        headline.push_str(&format!(" · {skipped} skipped"));
    }
    if failed > 0 {
        headline.push_str(&format!(" · {failed} failed"));
    }
    let mut lines = vec![Line::of(vec![Span::strong(headline)])];
    for (group, quota) in view.accounts() {
        let name = format!("{} · {}", group.name, quota.label);
        if outcome.skipped(quota.id) {
            lines.push(Line::of(vec![
                Span::new("  skipped ", Tone::Warn),
                Span::plain(format!("{name}: {}", view::text::STATUS_LINE_FEED)),
            ]));
            continue;
        }
        match outcome.message(quota.id) {
            None => lines.push(Line::of(vec![
                Span::new("  ok      ", Tone::Ok),
                Span::plain(name),
            ])),
            Some(message) => {
                lines.push(Line::of(vec![
                    Span::new("  failed  ", Tone::Crit),
                    Span::plain(format!("{name}: {message}")),
                ]));
                if matches!(
                    quota.reading,
                    Reading::Failed {
                        reconnect: true,
                        ..
                    }
                ) {
                    lines.push(Line::of(vec![Span::dim(format!(
                        "          {}",
                        view::text::reconnect_command(quota.id)
                    ))]));
                }
            }
        }
    }
    lines.push(Line::blank());
    lines.push(Line::of(vec![Span::dim(
        "View usage: jackin usage · jackin usage details · jackin usage refresh --json",
    )]));
    app.io.lines(&lines)
}
