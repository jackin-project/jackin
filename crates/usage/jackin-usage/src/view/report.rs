// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::card::{self, reset_phrase};
use super::model::{AccountQuota, Attention, Reading, UsageView, short_id};
use super::render::{Ctx, Depth, Line, Report, Span, Tone};
use super::text::{self, Part};

const TAG_WIDTH: usize = 10;
const DETAIL_INDENT: usize = 14;
const BANNER: &str = "Saved readings only; no provider or credential access.";
const STALE_TAG: &str = "Stale";
const TITLE_INDENT: usize = 2;
const REFRESH_COMMAND: &str = "jackin usage refresh";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttentionText {
    pub tag: &'static str,
    pub tone: Tone,
    pub subject: String,
    pub detail: String,
    pub command: Option<String>,
}

pub fn report(view: &UsageView, ctx: &Ctx, kind: Report) -> Vec<Line> {
    let depth = if kind == Report::Details {
        Depth::Full
    } else {
        Depth::Summary
    };
    let mut lines = title(view, kind, ctx);
    if kind == Report::Snapshot {
        lines.push(Line::of(vec![Span::dim(BANNER)]));
    }
    let mut before = Vec::new();
    if kind != Report::Details {
        before.extend(attention_lines(view, ctx));
    }
    before.extend(stale_lines(view, ctx));
    if !before.is_empty() {
        lines.push(Line::blank());
        lines.extend(before);
    }
    for group in &view.groups {
        lines.push(Line::blank());
        lines.push(card::heading(group.name, group.accounts.len(), ctx));
        for (position, account) in group.accounts.iter().enumerate() {
            if position > 0 {
                lines.push(Line::blank());
            }
            lines.extend(card::card(account, view, ctx, depth));
        }
    }
    lines
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

pub fn title(view: &UsageView, kind: Report, ctx: &Ctx) -> Vec<Line> {
    let word = match kind {
        Report::Usage => "Usage",
        Report::Snapshot => "Usage snapshot",
        Report::Details => "Usage details",
    };
    let offset = text::offset_label(view.utc_offset_secs);
    let stamps = [
        text::full_date(view.now, view.now, view.utc_offset_secs, true),
        format!(
            "{} {}",
            text::date(view.now, view.now, view.utc_offset_secs),
            text::clock(view.now, view.now, view.utc_offset_secs)
        ),
    ];
    let mut lines = Vec::new();
    for stamp in stamps {
        let parts = [
            Part::new(word, Tone::Strong),
            Part::new(format!("{stamp} ({offset})"), Tone::Plain),
            Part::new(plural(view.account_count(), "account"), Tone::Plain),
            Part::new(plural(view.groups.len(), "provider"), Tone::Plain),
        ];
        lines = text::wrap(Vec::new(), TITLE_INDENT, &parts, ctx.columns());
        if lines.len() == 1 {
            break;
        }
    }
    lines
}

pub fn stale_lines(view: &UsageView, ctx: &Ctx) -> Vec<Line> {
    let ages = view.stale_ages();
    let (Some(youngest), Some(oldest)) = (ages.first(), ages.last()) else {
        return Vec::new();
    };
    let total = view.account_count();
    let who = if ages.len() < total {
        format!("{} of {total} readings", ages.len())
    } else if total == 1 {
        "the reading".to_owned()
    } else {
        format!("all {total} readings")
    };
    let (near, far) = (text::rel(*youngest), text::rel(*oldest));
    let range = if near == far {
        near
    } else {
        format!("{near} to {far}")
    };
    let parts = [
        Part::new(format!("{who}, fetched {range} ago"), Tone::Plain),
        Part::new(REFRESH_COMMAND, Tone::Plain).atom(REFRESH_COMMAND),
    ];
    text::wrap(
        vec![Span::new(STALE_TAG, Tone::Warn), Span::plain("  ")],
        text::width(STALE_TAG) + 2,
        &parts,
        ctx.columns(),
    )
}

pub fn attention_entries(view: &UsageView) -> impl Iterator<Item = AttentionText> + '_ {
    view.attention
        .iter()
        .filter_map(|item| attention_text(view, item))
}

pub fn attention_heading(count: usize) -> String {
    format!("Needs attention ({count})")
}

pub fn attention_lines(view: &UsageView, ctx: &Ctx) -> Vec<Line> {
    let entries: Vec<AttentionText> = attention_entries(view).collect();
    if entries.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![Line::of(vec![Span::strong(attention_heading(
        entries.len(),
    ))])];
    for entry in entries {
        lines.extend(text::wrap(
            vec![
                Span::plain("  "),
                Span::new(text::pad(entry.tag, TAG_WIDTH), entry.tone),
                Span::plain("  "),
            ],
            DETAIL_INDENT,
            &text::split_parts(&entry.subject, Tone::Plain),
            ctx.columns(),
        ));
        lines.extend(text::wrap(
            vec![Span::plain(" ".repeat(DETAIL_INDENT))],
            DETAIL_INDENT,
            &text::split_atomic(&entry.detail, Tone::Plain, entry.command.as_slice()),
            ctx.columns(),
        ));
    }
    lines
}

fn subject(provider: &str, account: &AccountQuota, shorts: &[String]) -> String {
    let mut subject = format!("{provider} · {}", account.label);
    for short in shorts {
        subject.push_str(" #");
        subject.push_str(short);
    }
    subject
}

pub fn attention_text(view: &UsageView, item: &Attention) -> Option<AttentionText> {
    match item {
        Attention::Failing { account } => {
            let (group, account) = view.account(*account)?;
            let Reading::Failed {
                message,
                retry_at,
                reconnect,
                ..
            } = &account.reading
            else {
                return None;
            };
            let shorts: Vec<String> = account
                .is_duplicate()
                .then(|| account.short())
                .into_iter()
                .collect();
            let mut command = None;
            let detail = match retry_at {
                Some(_) if *reconnect => {
                    let run = text::reconnect_command(account.id, text::IdForm::Hyphenated);
                    let detail = format!("{message} · run: {run}");
                    command = Some(run);
                    detail
                }
                Some(at) if *at <= view.now => format!(
                    "{message} · retry due since {}",
                    text::clock(*at, view.now, view.utc_offset_secs)
                ),
                Some(at) => format!("{message} · next retry in {}", text::rel(at.0 - view.now.0)),
                None => message.clone(),
            };
            Some(AttentionText {
                tag: "FAILING",
                tone: Tone::Crit,
                subject: subject(group.name, account, &shorts),
                detail,
                command,
            })
        }
        Attention::NearLimit { accounts, meter } => {
            let (group, first) = view.account(*accounts.first()?)?;
            let found = first.meters().find(|candidate| &candidate.name == meter)?;
            let shorts: Vec<String> = if first.is_duplicate() {
                accounts.iter().copied().map(short_id).collect()
            } else {
                Vec::new()
            };
            Some(AttentionText {
                tag: "NEAR LIMIT",
                tone: Tone::Crit,
                subject: subject(group.name, first, &shorts),
                detail: format!(
                    "{} {} used · {}",
                    found.name,
                    text::percent(found.fraction()),
                    reset_phrase(found.resets_at, view, true)
                ),
                command: None,
            })
        }
        Attention::Duplicate { accounts, remove } => {
            let (group, first) = view.account(*accounts.first()?)?;
            let shorts: Vec<String> = accounts.iter().copied().map(short_id).collect();
            let command = text::remove_command(*remove, text::IdForm::Hyphenated);
            Some(AttentionText {
                tag: "DUPLICATE",
                tone: Tone::Warn,
                subject: subject(group.name, first, &shorts),
                detail: command.clone(),
                command: Some(command),
            })
        }
    }
}
