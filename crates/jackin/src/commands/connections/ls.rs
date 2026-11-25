// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage::accounts::AccountRegistry;
use jackin_usage::view::text;
use jackin_usage::view::{
    AccountQuota, Attention, Ctx, Feed, Line, Reading, Span, Tone, UsageView, ViewFilter, build,
};
use serde_json::{Value, json};

use super::overview::context;
use super::{database, failure};
use crate::app::App;
use crate::cli::UsageLsArgs;
use crate::error::CommandError;

const HEADERS: [&str; 5] = ["PROVIDER", "ACCOUNT", "PLAN", "STATUS", "FLAGS"];
const WIDE_HEADERS: [&str; 3] = ["FETCHED", "ID", "LAST ERROR"];
const GAP: usize = 2;
const NONE: &str = "-";
const FLAG_INDENT: usize = 2;
const NOTE_LEAD: &str = "  ";
const NOTE_INDENT: usize = 4;

pub(super) fn show(app: &mut App, args: &UsageLsArgs) -> Result<(), CommandError> {
    let now = app.deps.host.clock.now();
    let offset = app.deps.host.clock.utc_offset_secs();
    let registry = AccountRegistry::open_read_only(&database(app)?).map_err(failure)?;
    let Some(db) = registry else {
        return if args.json {
            app.io.json(&Vec::<Value>::new())
        } else {
            app.io.lines(&none_connected())
        };
    };
    let view = build(&db, &ViewFilter::default(), now, offset).map_err(failure)?;
    if args.json {
        let rows = db.list().map_err(failure)?;
        let rows: Vec<Value> = rows
            .iter()
            .map(|row| {
                let mut value = serde_json::to_value(row).map_err(failure)?;
                if let (Some(object), Some((group, quota))) =
                    (value.as_object_mut(), view.account(row.id))
                {
                    object.insert("provider_name".into(), json!(group.name));
                    object.insert("plan".into(), json!(quota.plan));
                    object.insert(
                        "fetched_at".into(),
                        json!(quota.reading.fetched_at().map(|at| at.0)),
                    );
                    object.insert("status".into(), json!(status_word(&quota.reading)));
                    object.insert("last_error".into(), json!(last_error(&quota.reading)));
                    object.insert("duplicate_of".into(), json!(quota.duplicates));
                }
                Ok(value)
            })
            .collect::<Result<_, CommandError>>()?;
        return app.io.json(&rows);
    }
    app.io.lines(&table(&view, args.wide, context(app)))
}

fn none_connected() -> Vec<Line> {
    vec![
        Line::of(vec![Span::plain("No usage accounts connected.")]),
        Line::of(vec![Span::dim(
            "Connect one: jackin account usage connect <provider>",
        )]),
    ]
}

fn status_word(reading: &Reading) -> &'static str {
    match reading {
        Reading::Waiting => "waiting",
        Reading::Fresh { .. } => "fresh",
        Reading::Stale { .. } => "stale",
        Reading::Failed { .. } => "failing",
    }
}

fn last_error(reading: &Reading) -> Option<&str> {
    match reading {
        Reading::Failed { message, .. } => Some(message),
        _ => None,
    }
}

fn status_cell(reading: &Reading, now: jackin_core::Timestamp) -> Vec<Span> {
    let tone = match reading {
        Reading::Waiting => Tone::Dim,
        Reading::Fresh { .. } => Tone::Plain,
        Reading::Stale { .. } => Tone::Warn,
        Reading::Failed { .. } => Tone::Crit,
    };
    let mut cell = vec![Span::new(status_word(reading), tone)];
    if let Some(age) = reading.age(now) {
        cell.push(Span::plain(format!(" {}", text::rel(age))));
    }
    cell
}

fn cells(provider: &str, quota: &AccountQuota, view: &UsageView, wide: bool) -> Vec<Vec<Span>> {
    let mut row = vec![
        vec![Span::plain(provider)],
        vec![Span::strong(quota.label.as_str())],
        vec![Span::plain(quota.plan.as_deref().unwrap_or(NONE))],
        status_cell(&quota.reading, view.now),
        flags(quota),
    ];
    if wide {
        row.push(vec![Span::plain(quota.reading.fetched_at().map_or_else(
            || NONE.to_owned(),
            |at| text::full_date(at, view.now, view.utc_offset_secs, false),
        ))]);
        row.push(vec![Span::plain(quota.id.hyphenated().to_string())]);
        row.push(vec![Span::plain(
            last_error(&quota.reading).unwrap_or(NONE),
        )]);
    }
    row
}

fn flags(quota: &AccountQuota) -> Vec<Span> {
    let mut flags = Vec::new();
    if quota.is_duplicate() {
        flags.push(Span::new("duplicate", Tone::Warn));
    }
    if quota.feed == Feed::StatusLine {
        flags.push(Span::dim("status-line"));
    }
    let mut cell = Vec::new();
    for flag in flags {
        if !cell.is_empty() {
            cell.push(Span::plain(", "));
        }
        cell.push(flag);
    }
    cell
}

fn needs_sign_in(quota: &AccountQuota) -> bool {
    matches!(
        quota.reading,
        Reading::Failed {
            reconnect: true,
            ..
        }
    )
}

fn width(cell: &[Span]) -> usize {
    cell.iter().map(Span::width).sum()
}

fn grid(header: Vec<Vec<Span>>, body: Vec<Vec<Vec<Span>>>) -> Vec<Line> {
    let rows: Vec<Vec<Vec<Span>>> = std::iter::once(header).chain(body).collect();
    let columns = rows.first().map_or(0, Vec::len);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| width(cell))
                .max()
                .unwrap_or(0)
        })
        .collect();
    rows.into_iter()
        .map(|row| {
            let last = row.iter().rposition(|cell| !cell.is_empty()).unwrap_or(0);
            let mut spans = Vec::new();
            for (column, cell) in row.into_iter().enumerate().take(last + 1) {
                let pad = widths.get(column).copied().unwrap_or(0) - width(&cell) + GAP;
                spans.extend(cell);
                if column < last {
                    spans.push(Span::plain(" ".repeat(pad)));
                }
            }
            Line::of(spans)
        })
        .collect()
}

const RECONNECT_HINT: &str = "jackin account usage reconnect <id>";
const LS_WIDE: &str = "jackin account usage ls --wide";
const RM_HINT: &str = "jackin account usage rm <id>";

fn flagged(text: &str, atoms: &[String], tone: Tone, ctx: Ctx) -> Vec<Line> {
    text::wrap(
        Vec::new(),
        FLAG_INDENT,
        &text::split_atomic(text, tone, atoms),
        ctx.columns(),
    )
}

fn indented(text: &str, atoms: &[String], tone: Tone, ctx: Ctx) -> Vec<Line> {
    text::wrap(
        vec![Span::plain(NOTE_LEAD)],
        NOTE_INDENT,
        &text::split_atomic(text, tone, atoms),
        ctx.columns(),
    )
}

fn summary(view: &UsageView, ctx: Ctx) -> Vec<Line> {
    let accounts = view.account_count();
    let mut parts = vec![
        text::plural(accounts, "account"),
        text::plural(view.groups.len(), "provider"),
    ];
    let failing = view
        .accounts()
        .filter(|(_, quota)| matches!(quota.reading, Reading::Failed { .. }))
        .count();
    if failing > 0 {
        parts.push(format!("{failing} failing"));
    }
    let duplicates = view
        .attention
        .iter()
        .filter(|entry| matches!(entry, Attention::Duplicate { .. }))
        .count();
    if duplicates > 0 {
        parts.push(format!("{duplicates} duplicate"));
    }
    let stale = view.stale_ages().len();
    if stale == accounts {
        parts.push("all stale".to_owned());
    } else if stale > 0 {
        parts.push(format!("{stale} of {accounts} stale"));
    }
    text::wrap(
        Vec::new(),
        0,
        &parts
            .into_iter()
            .map(|part| text::Part::new(part, Tone::Dim))
            .collect::<Vec<_>>(),
        ctx.columns(),
    )
}

fn footnotes(view: &UsageView, wide: bool, ctx: Ctx) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut duplicated = false;
    for entry in &view.attention {
        let Attention::Duplicate { accounts, remove } = entry else {
            continue;
        };
        let Some((group, quota)) = accounts.first().and_then(|id| view.account(*id)) else {
            continue;
        };
        duplicated = true;
        let times = if accounts.len() == 2 {
            "twice".to_owned()
        } else {
            format!("{} times", accounts.len())
        };
        lines.extend(flagged(
            &format!("! {}: {} is connected {times}.", group.name, quota.label),
            &[],
            Tone::Warn,
            ctx,
        ));
        if wide {
            let which = if view
                .account(*remove)
                .is_some_and(|(_, found)| matches!(found.reading, Reading::Failed { .. }))
            {
                "Remove the failing one"
            } else {
                "Remove one"
            };
            let command = text::remove_command(*remove, text::IdForm::Hyphenated);
            lines.extend(indented(
                &format!("{which}: {command}"),
                &[command],
                Tone::Plain,
                ctx,
            ));
        }
    }
    let signing_in: Vec<&AccountQuota> = view
        .accounts()
        .map(|(_, quota)| quota)
        .filter(|quota| needs_sign_in(quota))
        .collect();
    if wide {
        for quota in &signing_in {
            let command = text::reconnect_command(quota.id, text::IdForm::Hyphenated);
            lines.extend(flagged(
                &format!("! {} needs sign-in: {command}", quota.label),
                &[command],
                Tone::Warn,
                ctx,
            ));
        }
    } else if !signing_in.is_empty() {
        lines.extend(flagged(
            &format!(
                "! {} to sign in again: {RECONNECT_HINT} (IDs: {LS_WIDE})",
                text::plural(signing_in.len(), "account")
            ),
            &[RECONNECT_HINT.to_owned(), LS_WIDE.to_owned()],
            Tone::Warn,
            ctx,
        ));
    }
    let hooked = view.accounts().any(|(_, quota)| {
        quota.feed == Feed::StatusLine && matches!(quota.reading, Reading::Stale { .. })
    });
    if hooked {
        lines.extend(indented(
            "status-line accounts update when the Antigravity status-line hook runs; refresh cannot read them",
            &[],
            Tone::Dim,
            ctx,
        ));
    }
    if duplicated && !wide {
        lines.extend(indented(
            &format!("IDs: {LS_WIDE} · remove one: {RM_HINT}"),
            &[LS_WIDE.to_owned(), RM_HINT.to_owned()],
            Tone::Plain,
            ctx,
        ));
    }
    lines
}

pub(super) fn table(view: &UsageView, wide: bool, ctx: Ctx) -> Vec<Line> {
    if view.account_count() == 0 {
        return none_connected();
    }
    let header = HEADERS
        .iter()
        .chain(WIDE_HEADERS.iter().filter(|_| wide))
        .map(|name| vec![Span::dim(*name)])
        .collect();
    let body = view
        .accounts()
        .map(|(group, quota)| cells(group.name, quota, view, wide))
        .collect();
    let mut lines = grid(header, body);
    lines.push(Line::blank());
    lines.extend(summary(view, ctx));
    lines.extend(footnotes(view, wide, ctx));
    lines
}
