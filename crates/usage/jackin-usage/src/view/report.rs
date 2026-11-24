// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::card::{self, reset_phrase};
use super::model::{AccountQuota, Attention, Reading, UsageView, short_id};
use super::render::{Ctx, Depth, Line, Report, Span, Tone};
use super::text;

const TAG_WIDTH: usize = 10;
const DETAIL_INDENT: usize = 14;
const BANNER: &str = "Saved readings only; no provider or credential access.";
const REFRESH_COMMAND: &str = "jackin usage refresh";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttentionText {
    pub tag: &'static str,
    pub tone: Tone,
    pub subject: String,
    pub detail: String,
}

pub fn report(view: &UsageView, ctx: &Ctx, kind: Report) -> Vec<Line> {
    let depth = if kind == Report::Details {
        Depth::Full
    } else {
        Depth::Summary
    };
    let mut lines = vec![title(view, kind)];
    if kind == Report::Snapshot {
        lines.push(Line::of(vec![Span::dim(BANNER)]));
    }
    let mut before = Vec::new();
    if kind != Report::Details {
        before.extend(attention_lines(view));
    }
    before.extend(stale_line(view));
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

pub fn title(view: &UsageView, kind: Report) -> Line {
    let word = match kind {
        Report::Usage => "Usage",
        Report::Snapshot => "Usage snapshot",
        Report::Details => "Usage details",
    };
    Line::of(vec![
        Span::strong(word),
        Span::plain(format!(
            " · {} ({}) · {} · {}",
            text::full_date(view.now, view.now, view.utc_offset_secs, true),
            text::offset_label(view.utc_offset_secs),
            plural(view.account_count(), "account"),
            plural(view.groups.len(), "provider"),
        )),
    ])
}

pub fn stale_line(view: &UsageView) -> Option<Line> {
    let ages = view.stale_ages();
    let (youngest, oldest) = (ages.first()?, ages.last()?);
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
    Some(Line::of(vec![
        Span::new("Stale", Tone::Warn),
        Span::plain(format!("  {who}, fetched {range} ago · {REFRESH_COMMAND}")),
    ]))
}

pub fn attention_lines(view: &UsageView) -> Vec<Line> {
    let entries: Vec<AttentionText> = view
        .attention
        .iter()
        .filter_map(|item| attention_text(view, item))
        .collect();
    if entries.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![Line::of(vec![Span::strong(format!(
        "Needs attention ({})",
        entries.len()
    ))])];
    for entry in entries {
        lines.push(Line::of(vec![
            Span::plain("  "),
            Span::new(format!("{:<TAG_WIDTH$}", entry.tag), entry.tone),
            Span::plain(format!("  {}", entry.subject)),
        ]));
        lines.push(Line::of(vec![Span::plain(format!(
            "{:DETAIL_INDENT$}{}",
            "", entry.detail
        ))]));
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
            let detail = match retry_at {
                Some(_) if *reconnect => {
                    format!("{message} · {}", text::reconnect_command(account.id))
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
            })
        }
        Attention::Duplicate { accounts, remove } => {
            let (group, first) = view.account(*accounts.first()?)?;
            let shorts: Vec<String> = accounts.iter().copied().map(short_id).collect();
            Some(AttentionText {
                tag: "DUPLICATE",
                tone: Tone::Warn,
                subject: subject(group.name, first, &shorts),
                detail: format!("jackin account usage rm {}", remove.hyphenated()),
            })
        }
    }
}
