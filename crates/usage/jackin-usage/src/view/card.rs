// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Timestamp;

use super::detail;
use super::model::{
    AccountQuota, Feed, Meter, Note, Reading, ResetStatus, TodayTokens, UsageView, Used,
};
use super::render::{Ctx, Depth, Line, Span, Tone};
use super::text;

const MAIN_INDENT: usize = 4;
const MAIN_NAME: usize = 16;
const SIDE_INDENT: usize = 6;
const SIDE_NAME: usize = 14;
const SUB_INDENT: usize = 21;
const MIN_GAP: usize = 2;
const FAILURE_INDENT: usize = 6;
const NO_PLAN: &str = "plan not reported";

pub fn card(account: &AccountQuota, view: &UsageView, ctx: &Ctx, depth: Depth) -> Vec<Line> {
    let ctx = *ctx;
    let mut lines = vec![header(account, view, ctx)];
    if let Some(main) = &account.main {
        lines.extend(meter_block(
            main,
            (MAIN_INDENT, MAIN_NAME),
            account,
            view,
            ctx,
            depth,
        ));
    }
    for meter in &account.secondary {
        lines.extend(meter_block(
            meter,
            (SIDE_INDENT, SIDE_NAME),
            account,
            view,
            ctx,
            depth,
        ));
    }
    for note in &account.notes {
        lines.extend(note_block(note, view, ctx));
        if depth == Depth::Full {
            lines.extend(detail::note_lines(account, note, view, &ctx));
        }
    }
    lines.extend(failure_lines(account, view, ctx));
    lines.extend(feed_lines(account, view, ctx));
    lines.extend(duplicate_lines(account));
    if depth == Depth::Full {
        lines.extend(detail::footer(account, view, &ctx));
    }
    lines
}

pub fn heading(name: &str, accounts: usize, ctx: &Ctx) -> Line {
    let count = if accounts == 1 {
        "1 account".to_owned()
    } else {
        format!("{accounts} accounts")
    };
    let gap = ctx
        .columns()
        .saturating_sub(name.chars().count() + count.len())
        .max(MIN_GAP);
    Line::of(vec![
        Span::strong(name),
        Span::plain(" ".repeat(gap)),
        Span::dim(count),
    ])
}

fn header(account: &AccountQuota, view: &UsageView, ctx: Ctx) -> Line {
    let right = text::freshness(&account.reading, view.now, ctx.narrow());
    let right_width: usize = right.iter().map(Span::width).sum();
    let short = account.is_duplicate().then(|| account.short());
    let short_width = short.as_ref().map_or(0, |short| 2 + short.chars().count());
    let room = ctx.columns().saturating_sub(right_width + MIN_GAP);
    let mut label = account.label.clone();
    let mut plan = account.plan.clone().unwrap_or_else(|| NO_PLAN.to_owned());
    let measure = |label: &str, plan: &str| {
        2 + label.chars().count() + short_width + 3 + plan.chars().count()
    };
    let excess = measure(&label, &plan).saturating_sub(room);
    if excess > 0 {
        plan = text::truncate(&plan, plan.chars().count().saturating_sub(excess).max(1));
    }
    let excess = measure(&label, &plan).saturating_sub(room);
    if excess > 0 {
        label = text::truncate(&label, label.chars().count().saturating_sub(excess).max(1));
    }
    let plan_span = if account.plan.is_some() {
        Span::plain(plan)
    } else {
        Span::dim(plan)
    };
    let mut spans = vec![Span::plain("  "), Span::strong(label)];
    if let Some(short) = short {
        spans.push(Span::plain(" "));
        spans.push(Span::dim(format!("#{short}")));
    }
    spans.push(Span::plain(" · "));
    spans.push(plan_span);
    let left_width: usize = spans.iter().map(Span::width).sum();
    spans.push(Span::plain(
        " ".repeat(
            ctx.columns()
                .saturating_sub(left_width + right_width)
                .max(MIN_GAP),
        ),
    ));
    spans.extend(right);
    Line::of(spans)
}

fn meter_block(
    meter: &Meter,
    (indent, name_width): (usize, usize),
    account: &AccountQuota,
    view: &UsageView,
    ctx: Ctx,
    depth: Depth,
) -> Vec<Line> {
    let mut lines = vec![gauge(meter, indent, name_width, view, ctx)];
    if let Used::Amount { used, limit, unit } = &meter.used {
        lines.extend(text::wrap(
            vec![Span::plain(" ".repeat(SUB_INDENT))],
            SUB_INDENT,
            &text::split_parts(&text::quantity(*used, limit.get(), *unit), Tone::Dim),
            ctx.columns(),
        ));
    }
    if depth == Depth::Full {
        lines.extend(detail::meter_lines(account, meter, view, &ctx));
    }
    lines
}

fn gauge(meter: &Meter, indent: usize, name_width: usize, view: &UsageView, ctx: Ctx) -> Line {
    let lead = Span::plain(format!(
        "{:indent$}{:<name_width$} ",
        "",
        text::truncate(&meter.name, name_width)
    ));
    let fraction = meter.fraction();
    let mut head = vec![lead];
    if meter.rolled_over {
        head.push(Span::dim(text::track()));
        head.push(Span::plain(" "));
        head.push(Span::dim("   -"));
        head.push(Span::plain(" ".repeat(7)));
    } else {
        let tone = text::tone_for(fraction, view.warn_at, view.critical_at);
        let (filled, free) = text::bar(fraction);
        head.push(Span::new(filled, tone));
        head.push(Span::dim(free));
        head.push(Span::plain(" "));
        head.push(Span::new(
            format!("{:>4}", text::percent(fraction)),
            if tone == Tone::Ok { Tone::Plain } else { tone },
        ));
        head.push(Span::plain(" used  "));
    }
    let variants: &[(bool, bool)] = match (meter.rolled_over, ctx.narrow()) {
        (true, true) => &[(false, true), (false, false)],
        (true, false) => &[(true, true), (false, true), (false, false)],
        (false, true) => &[(false, true)],
        (false, false) => &[(true, true), (false, true)],
    };
    let mut line = Line::default();
    for (clock, was) in variants {
        let mut spans = head.clone();
        spans.extend(reset_spans(meter, view, *clock, *was));
        line = Line::of(spans);
        if line.width() <= ctx.columns() {
            break;
        }
    }
    line
}

pub fn reset_phrase(resets_at: Option<Timestamp>, view: &UsageView, with_clock: bool) -> String {
    let Some(at) = resets_at else {
        return "reset not reported".to_owned();
    };
    let distance = text::rel(at.0 - view.now.0);
    let phrase = if at > view.now {
        format!("resets in {distance}")
    } else {
        format!("reset {distance} ago")
    };
    if with_clock {
        format!(
            "{phrase} · {}",
            text::clock(at, view.now, view.utc_offset_secs)
        )
    } else {
        phrase
    }
}

fn reset_spans(meter: &Meter, view: &UsageView, with_clock: bool, with_was: bool) -> Vec<Span> {
    let phrase = reset_phrase(meter.resets_at, view, with_clock);
    if meter.rolled_over {
        let mut spans = vec![Span::dim(phrase)];
        if with_was {
            spans.push(Span::plain(" "));
            spans.push(Span::dim(format!(
                "· was {}",
                text::percent(meter.fraction())
            )));
        }
        spans
    } else if meter.resets_at.is_none() {
        vec![Span::dim(phrase)]
    } else {
        vec![Span::plain(phrase)]
    }
}

fn note_block(note: &Note, view: &UsageView, ctx: Ctx) -> Vec<Line> {
    let (name, parts) = note_parts(note, view);
    let name = text::truncate(&name, SIDE_NAME);
    let prefix = vec![Span::plain(format!(
        "{:SIDE_INDENT$}{name:<SIDE_NAME$} ",
        ""
    ))];
    let parts: Vec<(String, Tone)> = parts.into_iter().map(|part| (part, Tone::Plain)).collect();
    text::wrap(prefix, SUB_INDENT, &parts, ctx.columns())
}

pub fn note_parts(note: &Note, view: &UsageView) -> (String, Vec<String>) {
    match note {
        Note::Balance {
            label,
            amount,
            unit,
            approx,
            ..
        } => {
            let mut parts = vec![text::amount(*amount, *unit)];
            parts.extend(
                approx
                    .iter()
                    .map(|(value, currency)| format!("≈ {}", text::money(*value, *currency))),
            );
            (label.clone(), parts)
        }
        Note::Resets(credits) => {
            let with = |status| credits.iter().filter(move |credit| credit.status == status);
            let total = |status| with(status).map(|credit| credit.remaining).sum::<u64>();
            let queued = total(ResetStatus::Queued);
            let unavailable = total(ResetStatus::Unavailable);
            let mut count = format!("{} ready", total(ResetStatus::Ready));
            if queued > 0 {
                count.push_str(&format!(", {queued} queued"));
            }
            if unavailable > 0 {
                count.push_str(&format!(", {unavailable} unavailable"));
            }
            let mut parts = vec![count];
            parts.extend(
                with(ResetStatus::Ready)
                    .map(|credit| credit.expires_at)
                    .min()
                    .map(|at| {
                        format!(
                            "next expires {}",
                            text::date(at, view.now, view.utc_offset_secs)
                        )
                    }),
            );
            ("Limit resets".to_owned(), parts)
        }
        Note::Tokens {
            today,
            yesterday,
            last30,
        } => {
            let total = |tokens: u64| text::number(tokens as f64);
            let mut parts: Vec<String> = match today {
                TodayTokens::Hidden => Vec::new(),
                TodayTokens::Unavailable => vec!["today n/a".to_owned()],
                TodayTokens::Count(tokens) => vec![format!("today {}", total(*tokens))],
            };
            parts.extend(
                yesterday
                    .iter()
                    .map(|activity| format!("yesterday {}", total(activity.tokens))),
            );
            parts.extend(
                last30
                    .iter()
                    .map(|activity| format!("30 days {}", total(activity.tokens))),
            );
            ("Tokens".to_owned(), parts)
        }
        Note::Spend {
            today,
            week,
            month,
            currency,
            ..
        } => {
            let period = |name: &str, spend: &Option<f64>| {
                spend.map(|value| format!("{name} {}", text::money(value, *currency)))
            };
            (
                "Spend".to_owned(),
                [
                    period("today", today),
                    period("week", week),
                    period("month", month),
                ]
                .into_iter()
                .flatten()
                .collect(),
            )
        }
        Note::Daily {
            label,
            records,
            lines,
            ..
        } => {
            let mut parts: Vec<String> = records
                .iter()
                .map(|count| {
                    if *count == 1 {
                        "1 record".to_owned()
                    } else {
                        format!("{count} records")
                    }
                })
                .collect();
            parts.extend(lines.first().cloned());
            (label.clone(), parts)
        }
    }
}

fn failure_lines(account: &AccountQuota, view: &UsageView, ctx: Ctx) -> Vec<Line> {
    let Reading::Failed {
        message,
        retry_at,
        reconnect,
        ..
    } = &account.reading
    else {
        return Vec::new();
    };
    let mut parts = Vec::new();
    match retry_at {
        Some(at) => {
            parts.push((
                format!("{}! refresh failed: {message}", " ".repeat(MAIN_INDENT)),
                Tone::Crit,
            ));
            if *reconnect {
                parts.push((
                    format!(
                        "run: {}",
                        text::reconnect_command(account.id, text::IdForm::Hyphenated)
                    ),
                    Tone::Plain,
                ));
            } else {
                let retry = if *at <= view.now {
                    format!(
                        "retry due since {}",
                        text::clock(*at, view.now, view.utc_offset_secs)
                    )
                } else {
                    format!("next retry in {}", text::rel(at.0 - view.now.0))
                };
                parts.push((retry, Tone::Dim));
            }
        }
        None => parts.push((
            format!("{}! {message}", " ".repeat(MAIN_INDENT)),
            Tone::Crit,
        )),
    }
    text::wrap(Vec::new(), FAILURE_INDENT, &parts, ctx.columns())
}

fn feed_lines(account: &AccountQuota, view: &UsageView, ctx: Ctx) -> Vec<Line> {
    match (account.feed, &account.reading) {
        (Feed::StatusLine, Reading::Stale { at }) => text::wrap(
            vec![Span::plain(" ".repeat(MAIN_INDENT))],
            MAIN_INDENT,
            &text::split_parts(&text::status_line_note(*at, view.now), Tone::Dim),
            ctx.columns(),
        ),
        _ => Vec::new(),
    }
}

fn duplicate_lines(account: &AccountQuota) -> Vec<Line> {
    account
        .duplicates
        .iter()
        .map(|other| {
            Line::of(vec![
                Span::new(
                    format!(
                        "    ! also connected as #{}",
                        super::model::short_id(*other)
                    ),
                    Tone::Warn,
                ),
                Span::dim(" (same provider and label)"),
            ])
        })
        .collect()
}
