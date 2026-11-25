// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Timestamp;
use time::{Date, OffsetDateTime, UtcOffset};

use super::model::{Currency, Reading, Unit, UsedFraction};
use super::render::{Line, Span, Tone};

pub const BAR_CELLS: usize = 8;

const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const SCALES: [(f64, &str); 4] = [(1e3, "K"), (1e6, "M"), (1e9, "B"), (1e12, "T")];
const COMPACT_FROM: f64 = 100_000.0;
const NEARBY_DAYS: i64 = 7;
const BOUNDARY: f64 = 1e-9;
const SEPARATOR: &str = " · ";
const MIN_HANG: usize = 2;

pub fn percent(used: UsedFraction) -> String {
    let points = used.percent();
    if points <= BOUNDARY {
        "0%".to_owned()
    } else if points < 1.0 - BOUNDARY {
        "<1%".to_owned()
    } else if points > 99.0 + BOUNDARY && points < 100.0 - BOUNDARY {
        ">99%".to_owned()
    } else {
        format!("{}%", used.rounded())
    }
}

pub fn bar_cells(used: UsedFraction) -> usize {
    let value = used.get();
    if value <= 0.0 {
        return 0;
    }
    if value >= 1.0 {
        return BAR_CELLS;
    }
    let nearest = (0..=8u8)
        .find(|cells| value * 8.0 < f64::from(*cells) + 0.5)
        .unwrap_or(8);
    usize::from(nearest).clamp(1, BAR_CELLS - 1)
}

pub fn bar(used: UsedFraction) -> (String, String) {
    let filled = bar_cells(used);
    ("█".repeat(filled), "░".repeat(BAR_CELLS - filled))
}

pub fn track() -> String {
    "╌".repeat(BAR_CELLS)
}

pub fn tone_for(used: UsedFraction, warn_at: UsedFraction, critical_at: UsedFraction) -> Tone {
    if used.at_least(critical_at) {
        Tone::Crit
    } else if used.at_least(warn_at) {
        Tone::Warn
    } else {
        Tone::Ok
    }
}

fn group(digits: &str) -> String {
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

fn tiny(value: f64) -> Option<String> {
    if value == 0.0 || value.abs() >= 0.005 {
        return None;
    }
    let fixed = format!("{:.6}", value.abs());
    let trimmed = fixed.trim_end_matches('0');
    let sign = if value < 0.0 { "-" } else { "" };
    Some(if trimmed == "0." {
        format!("{sign}<0.000001")
    } else {
        format!("{sign}{trimmed}")
    })
}

fn decimal(value: f64, keep_cents: bool) -> String {
    if let Some(text) = tiny(value) {
        return text;
    }
    let fixed = format!("{:.2}", value.abs());
    let (whole, cents) = fixed.split_once('.').unwrap_or((fixed.as_str(), "00"));
    let sign = if value < 0.0 && fixed != "0.00" {
        "-"
    } else {
        ""
    };
    let cents = if keep_cents {
        cents
    } else {
        cents.trim_end_matches('0')
    };
    if cents == "00" || cents.is_empty() {
        format!("{sign}{}", group(whole))
    } else {
        format!("{sign}{}.{cents}", group(whole))
    }
}

fn significant(scaled: f64) -> String {
    let rounded = (scaled * 100.0).round() / 100.0;
    let places = if rounded >= 100.0 {
        0
    } else if rounded >= 10.0 {
        1
    } else {
        2
    };
    let text = format!("{scaled:.places$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

fn compact(value: f64) -> String {
    let magnitude = value.abs();
    let mut index = SCALES
        .iter()
        .rposition(|(scale, _)| magnitude >= *scale)
        .unwrap_or(0);
    let mut text = significant(magnitude / SCALES[index].0);
    while index + 1 < SCALES.len() && text.parse::<f64>().is_ok_and(|n| n >= 1000.0) {
        index += 1;
        text = significant(magnitude / SCALES[index].0);
    }
    let sign = if value < 0.0 { "-" } else { "" };
    format!("{sign}{text}{}", SCALES[index].1)
}

pub fn number(value: f64) -> String {
    if value.abs() < COMPACT_FROM {
        decimal(value, false)
    } else {
        compact(value)
    }
}

pub fn money(value: f64, currency: Currency) -> String {
    format!("{} {}", decimal(value, true), currency.as_str())
}

fn unit_word(unit: Unit) -> Option<&'static str> {
    match unit {
        Unit::Credits => Some("credits"),
        Unit::Tokens => Some("tokens"),
        Unit::Requests => Some("requests"),
        Unit::Searches => Some("searches"),
        Unit::Cents => Some("cents"),
        Unit::Monetary => Some("monetary units"),
        Unit::Money(_) => None,
    }
}

pub fn amount(value: f64, unit: Unit) -> String {
    match unit {
        Unit::Money(currency) => money(value, currency),
        _ => format!("{} {}", number(value), unit_word(unit).unwrap_or_default()),
    }
}

pub fn quantity(used: f64, limit: f64, unit: Unit) -> String {
    let left = (limit - used).max(0.0);
    match unit {
        Unit::Money(currency) => format!(
            "{} of {} used · {} left",
            number(used),
            money(limit, currency),
            money(left, currency)
        ),
        _ => format!(
            "{} of {} {} used · {} left",
            number(used),
            number(limit),
            unit_word(unit).unwrap_or_default(),
            number(left)
        ),
    }
}

pub fn rel(seconds: i64) -> String {
    let seconds = seconds.unsigned_abs();
    if seconds < 60 {
        return "<1m".to_owned();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h {}m", minutes % 60);
    }
    format!("{}d {}h", hours / 24, hours % 24)
}

fn local(at: Timestamp, utc_offset_secs: i32) -> Option<OffsetDateTime> {
    let utc = OffsetDateTime::from_unix_timestamp(at.0).ok()?;
    let offset = UtcOffset::from_whole_seconds(utc_offset_secs).ok()?;
    Some(utc.to_offset(offset))
}

fn day_name(at: OffsetDateTime) -> &'static str {
    DAYS.get(usize::from(at.weekday().number_days_from_monday()))
        .copied()
        .unwrap_or_default()
}

fn month_name(date: Date) -> &'static str {
    MONTHS
        .get(usize::from(u8::from(date.month())).saturating_sub(1))
        .copied()
        .unwrap_or_default()
}

fn hours_minutes(at: OffsetDateTime) -> String {
    format!("{:02}:{:02}", at.hour(), at.minute())
}

pub fn clock(at: Timestamp, now: Timestamp, utc_offset_secs: i32) -> String {
    let (Some(then), Some(today)) = (local(at, utc_offset_secs), local(now, utc_offset_secs))
    else {
        return at.rfc3339();
    };
    let days = (then.date() - today.date()).whole_days();
    let time = hours_minutes(then);
    if days == 0 {
        time
    } else if days.abs() < NEARBY_DAYS {
        format!("{} {time}", day_name(then))
    } else if then.year() == today.year() {
        format!("{} {} {time}", then.day(), month_name(then.date()))
    } else {
        format!("{} {} {}", then.day(), month_name(then.date()), then.year())
    }
}

pub fn date(at: Timestamp, now: Timestamp, utc_offset_secs: i32) -> String {
    let (Some(then), Some(today)) = (local(at, utc_offset_secs), local(now, utc_offset_secs))
    else {
        return at.rfc3339();
    };
    if then.year() == today.year() {
        format!("{} {}", then.day(), month_name(then.date()))
    } else {
        format!("{} {} {}", then.day(), month_name(then.date()), then.year())
    }
}

pub fn full_date(at: Timestamp, now: Timestamp, utc_offset_secs: i32, with_year: bool) -> String {
    let (Some(then), Some(today)) = (local(at, utc_offset_secs), local(now, utc_offset_secs))
    else {
        return at.rfc3339();
    };
    let year = if with_year || then.year() != today.year() {
        format!(" {}", then.year())
    } else {
        String::new()
    };
    format!(
        "{} {} {}{year} {}",
        day_name(then),
        then.day(),
        month_name(then.date()),
        hours_minutes(then)
    )
}

pub fn offset_label(utc_offset_secs: i32) -> String {
    if utc_offset_secs == 0 {
        return "UTC".to_owned();
    }
    let sign = if utc_offset_secs < 0 { '-' } else { '+' };
    let total = utc_offset_secs.unsigned_abs();
    let (hours, minutes) = (total / 3600, total % 3600 / 60);
    if minutes == 0 {
        format!("{sign}{hours:02}")
    } else {
        format!("{sign}{hours:02}:{minutes:02}")
    }
}

pub fn freshness(reading: &Reading, now: Timestamp, narrow: bool) -> Vec<Span> {
    let age = |at: Timestamp| rel(now.0.saturating_sub(at.0));
    match reading {
        Reading::Waiting => vec![Span::dim(if narrow {
            "waiting"
        } else {
            "waiting for first refresh"
        })],
        Reading::Fresh { at } if narrow => vec![Span::dim(format!("fresh {}", age(*at)))],
        Reading::Fresh { at } => vec![Span::dim(format!("fetched {} ago", age(*at)))],
        Reading::Stale { at } => marked("stale", Tone::Warn, Some(age(*at)), narrow),
        Reading::Failed { at, .. } => marked("failed", Tone::Crit, at.map(age), narrow),
    }
}

fn marked(word: &str, tone: Tone, age: Option<String>, narrow: bool) -> Vec<Span> {
    match (age, narrow) {
        (None, _) => vec![Span::new(word, tone)],
        (Some(age), true) => vec![Span::new(word, tone), Span::dim(format!(" {age}"))],
        (Some(age), false) => vec![
            Span::new(word, tone),
            Span::plain(" · "),
            Span::dim(format!("fetched {age} ago")),
        ],
    }
}

pub fn reconnect_command(id: uuid::Uuid) -> String {
    format!("run: jackin account usage reconnect {}", id.hyphenated())
}

pub const STATUS_LINE_FEED: &str = "updated by the Antigravity status-line hook";

pub fn status_line_note(at: Timestamp, now: Timestamp) -> String {
    format!(
        "{STATUS_LINE_FEED} · last update {} ago",
        rel(now.0.saturating_sub(at.0))
    )
}

pub fn truncate(text: &str, columns: usize) -> String {
    if text.chars().count() <= columns {
        return text.to_owned();
    }
    let keep = columns.max(1) - 1;
    let mut cut: String = text.chars().take(keep).collect();
    cut.push('…');
    cut
}

pub fn split_parts(text: &str, tone: Tone) -> Vec<(String, Tone)> {
    text.split(SEPARATOR)
        .map(|part| (part.to_owned(), tone))
        .collect()
}

struct Flow {
    lines: Vec<Line>,
    spans: Vec<Span>,
    used: usize,
    indent: usize,
    columns: usize,
    started: bool,
}

impl Flow {
    fn push(&mut self, text: String, tone: Tone) {
        self.used += text.chars().count();
        match self.spans.last_mut() {
            Some(last) if self.started && last.tone == tone => last.text.push_str(&text),
            _ => self.spans.push(Span::new(text, tone)),
        }
        self.started = true;
    }

    fn turn(&mut self) {
        self.lines.push(Line::of(std::mem::take(&mut self.spans)));
        self.spans = vec![Span::plain(" ".repeat(self.indent))];
        self.used = self.indent;
        self.started = false;
    }

    fn dedent(&mut self, width: usize) {
        let room = self.columns.saturating_sub(width);
        if self.used + width > self.columns && room >= MIN_HANG {
            self.spans = vec![Span::plain(" ".repeat(room))];
            self.used = room;
        }
    }

    fn put(&mut self, text: &str, tone: Tone) {
        if self.used + text.chars().count() <= self.columns {
            self.push(text.to_owned(), tone);
            return;
        }
        if let Some((head, tail)) = text.split_once(": ") {
            let head = format!("{head}:");
            if self.used + head.chars().count() <= self.columns
                && self.indent + tail.chars().count() <= self.columns
            {
                self.push(head, tone);
                self.turn();
                self.push(tail.to_owned(), tone);
                return;
            }
        }
        let mut chunk = " ".repeat(text.len() - text.trim_start().len());
        let mut empty = true;
        for word in text.split_whitespace() {
            let gap = usize::from(!empty);
            let reach = self.used + chunk.chars().count() + gap + word.chars().count();
            if reach <= self.columns {
                chunk.push_str(&" ".repeat(gap));
                chunk.push_str(word);
                empty = false;
                continue;
            }
            if !empty {
                self.push(std::mem::take(&mut chunk), tone);
                self.turn();
            }
            let room = self
                .columns
                .saturating_sub(self.used + chunk.chars().count());
            chunk.push_str(&truncate(word, room));
            empty = false;
        }
        if !chunk.is_empty() {
            self.push(chunk, tone);
        }
    }
}

pub fn wrap(
    prefix: Vec<Span>,
    indent: usize,
    parts: &[(String, Tone)],
    columns: usize,
) -> Vec<Line> {
    let used = prefix.iter().map(Span::width).sum();
    let mut flow = Flow {
        lines: Vec::new(),
        spans: prefix,
        used,
        indent,
        columns,
        started: false,
    };
    for (position, (text, tone)) in parts.iter().enumerate() {
        let width = text.chars().count();
        if position == 0 {
            flow.put(text, *tone);
        } else if flow.used + SEPARATOR.chars().count() + width <= columns {
            flow.push(format!("{SEPARATOR}{text}"), *tone);
        } else {
            flow.turn();
            flow.dedent(width);
            flow.put(text, *tone);
        }
    }
    flow.lines.push(Line::of(flow.spans));
    flow.lines
}
