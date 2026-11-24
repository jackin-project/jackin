// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Timestamp;

use super::model::{
    AccountQuota, Activity, Currency, FRESH_SECS, Fact, Meter, Note, ResetCredit, Unit, UsageView,
    Used, UsedFraction,
};
use super::render::{Ctx, Line, Span, Tone};
use super::text;

const SUB_INDENT: usize = 21;
const NESTED_INDENT: usize = 23;
const FOOT_INDENT: usize = 4;
const EXACT_AMOUNT_FROM: f64 = 100_000.0;
const STATUS_WIDTH: usize = 11;
const MIN_KIND_WIDTH: usize = 6;
const TOP_MODELS: usize = 3;
const DETAILS_JSON: (&str, &str) = ("per-day counts:", "jackin usage details --json");
const REDEEM: &str = "redeem a reset: jackin usage redeem <id> <n>";
const UNKNOWN_FETCH: &str = "not fetched yet";

pub fn meter_lines(
    _account: &AccountQuota,
    meter: &Meter,
    view: &UsageView,
    ctx: &Ctx,
) -> Vec<Line> {
    let ctx = *ctx;
    let mut parts = Vec::new();
    if meter.rolled_over {
        parts.push(format!("was {} at fetch", text::percent(meter.fraction())));
    } else {
        parts.push(format!("{} left", left(meter.fraction())));
    }
    parts.extend(window_phrase(meter, view));
    if !meter.rolled_over {
        parts.extend(exact_percent(meter.fraction()));
    }
    let mut lines = sub(&parts, SUB_INDENT, ctx);
    if let Used::Amount { used, limit, unit } = &meter.used
        && limit.get() >= EXACT_AMOUNT_FROM
    {
        lines.extend(fitted(exact_quantity(*used, limit.get(), *unit), ctx));
    }
    lines
}

pub fn note_lines(_account: &AccountQuota, note: &Note, view: &UsageView, ctx: &Ctx) -> Vec<Line> {
    let ctx = *ctx;
    match note {
        Note::Balance {
            amount,
            unit,
            approx,
        } => balance_lines(*amount, *unit, *approx, ctx),
        Note::Resets(credits) => reset_lines(credits, view, ctx),
        Note::Tokens {
            yesterday, last30, ..
        } => token_lines(yesterday.as_ref(), last30.as_ref(), ctx),
        Note::Spend {
            today,
            week,
            month,
            byok,
            currency,
        } => spend_lines([*today, *week, *month], *byok, *currency, ctx),
    }
}

pub fn footer(account: &AccountQuota, view: &UsageView, ctx: &Ctx) -> Vec<Line> {
    let ctx = *ctx;
    let mut lines = vec![foot(format!("id {}", account.id.hyphenated()))];
    lines.push(foot(fetched_text(account, view)));
    let redeemable = account.notes.iter().any(
        |note| matches!(note, Note::Resets(credits) if credits.iter().any(|credit| credit.ready)),
    );
    if redeemable {
        lines.push(foot(REDEEM.to_owned()));
    }
    lines.extend(fact_lines(&account.facts, view, ctx));
    lines
}

fn sub(parts: &[String], indent: usize, ctx: Ctx) -> Vec<Line> {
    let parts: Vec<(String, Tone)> = parts.iter().map(|part| (part.clone(), Tone::Dim)).collect();
    text::wrap(
        vec![Span::plain(" ".repeat(indent))],
        indent,
        &parts,
        ctx.columns(),
    )
}

fn foot(text: String) -> Line {
    Line::of(vec![Span::plain(" ".repeat(FOOT_INDENT)), Span::dim(text)])
}

fn left(used: UsedFraction) -> String {
    match text::percent(used).as_str() {
        "0%" => "100%".to_owned(),
        "<1%" => ">99%".to_owned(),
        ">99%" => "<1%".to_owned(),
        "100%" => "0%".to_owned(),
        _ => format!("{}%", 100 - used.rounded()),
    }
}

fn exact_percent(used: UsedFraction) -> Option<String> {
    if !matches!(text::percent(used).as_str(), "<1%" | ">99%") {
        return None;
    }
    let points = used.percent();
    Some(if points < 0.005 {
        "exact <0.01%".to_owned()
    } else if points > 99.995 {
        "exact >99.99%".to_owned()
    } else {
        format!("exact {points:.2}%")
    })
}

fn window_label(seconds: u32) -> Option<String> {
    const HOUR: u32 = 3600;
    const DAY: u32 = 24 * HOUR;
    if seconds >= 2 * DAY && seconds.is_multiple_of(DAY) {
        Some(format!("{}d", seconds / DAY))
    } else if seconds.is_multiple_of(HOUR) {
        Some(format!("{}h", seconds / HOUR))
    } else {
        None
    }
}

fn window_phrase(meter: &Meter, view: &UsageView) -> Option<String> {
    let label = window_label(meter.window_secs?)?;
    Some(match meter.resets_at {
        Some(at) => {
            let start = Timestamp(at.0.saturating_sub(i64::from(meter.window_secs?)));
            format!(
                "{label} window from {}",
                text::full_date(start, view.now, view.utc_offset_secs, false)
            )
        }
        None => format!("{label} window"),
    })
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

fn exact_number(value: f64) -> String {
    let fixed = format!("{:.2}", value.abs());
    let (whole, cents) = fixed.split_once('.').unwrap_or((fixed.as_str(), "00"));
    let sign = if value < 0.0 && fixed != "0.00" {
        "-"
    } else {
        ""
    };
    let cents = cents.trim_end_matches('0');
    if cents.is_empty() {
        format!("{sign}{}", group(whole))
    } else {
        format!("{sign}{}.{cents}", group(whole))
    }
}

fn exact_count(value: u64) -> String {
    group(&value.to_string())
}

fn unit_word(unit: Unit) -> &'static str {
    match unit {
        Unit::Credits => "credits",
        Unit::Tokens => "tokens",
        Unit::Requests => "requests",
        Unit::Searches => "searches",
        Unit::Money(_) => "",
    }
}

fn exact_amount(value: f64, unit: Unit) -> String {
    match unit {
        Unit::Money(currency) => text::money(value, currency),
        _ => format!("{} {}", exact_number(value), unit_word(unit)),
    }
}

fn exact_quantity(used: f64, limit: f64, unit: Unit) -> (String, String) {
    match unit {
        Unit::Money(currency) => (
            format!("exact {}", text::money(used, currency)),
            format!("of {} used", text::money(limit, currency)),
        ),
        _ => (
            format!("exact {}", exact_number(used)),
            format!("of {} {} used", exact_number(limit), unit_word(unit)),
        ),
    }
}

fn fitted((first, second): (impl AsRef<str>, impl AsRef<str>), ctx: Ctx) -> Vec<Line> {
    let (first, second) = (first.as_ref(), second.as_ref());
    let whole = format!("{first} {second}");
    if SUB_INDENT + whole.chars().count() <= ctx.columns() {
        sub(&[whole], SUB_INDENT, ctx)
    } else {
        let mut lines = sub(&[first.to_owned()], SUB_INDENT, ctx);
        lines.extend(sub(&[second.to_owned()], SUB_INDENT, ctx));
        lines
    }
}

fn balance_lines(amount: f64, unit: Unit, approx: Option<(f64, Currency)>, ctx: Ctx) -> Vec<Line> {
    let exact = exact_amount(amount, unit);
    if approx.is_none() && exact == text::amount(amount, unit) {
        return Vec::new();
    }
    let mut parts = vec![format!("exact {exact}")];
    parts.extend(approx.map(|(value, currency)| text::money(value, currency)));
    sub(&parts, SUB_INDENT, ctx)
}

fn reset_lines(credits: &[ResetCredit], view: &UsageView, ctx: Ctx) -> Vec<Line> {
    let index_width = credits.len().to_string().len();
    let kind_width = credits
        .iter()
        .map(|credit| credit.kind.chars().count())
        .max()
        .unwrap_or(0)
        .max(MIN_KIND_WIDTH);
    credits
        .iter()
        .enumerate()
        .flat_map(|(position, credit)| {
            let status = if credit.ready { "ready" } else { "unavailable" };
            let expiry = format!(
                "expires {}",
                text::full_date(credit.expires_at, view.now, view.utc_offset_secs, true)
            );
            let aligned = format!(
                "{:<index_width$}  {:<kind_width$} {status:<STATUS_WIDTH$} {expiry}",
                position + 1,
                credit.kind,
            );
            if SUB_INDENT + aligned.chars().count() <= ctx.columns() {
                sub(&[aligned], SUB_INDENT, ctx)
            } else {
                let head = format!("{} {} {status}", position + 1, credit.kind);
                sub(&[head, expiry], SUB_INDENT, ctx)
            }
        })
        .collect()
}

fn plural(count: u64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{} {noun}s", exact_count(count))
    }
}

fn breakdown(activity: &Activity) -> Vec<String> {
    vec![
        format!("{} cached in", text::number(activity.cached_in as f64)),
        format!("{} in", text::number(activity.input as f64)),
        format!("{} out", text::number(activity.output as f64)),
    ]
}

fn token_lines(yesterday: Option<&Activity>, last30: Option<&Activity>, ctx: Ctx) -> Vec<Line> {
    let mut lines = Vec::new();
    if let Some(activity) = yesterday {
        lines.extend(sub(
            &[
                format!("yesterday: {}", plural(activity.tokens, "token")),
                plural(activity.turns, "turn"),
            ],
            SUB_INDENT,
            ctx,
        ));
        lines.extend(sub(&breakdown(activity), NESTED_INDENT, ctx));
    }
    if let Some(activity) = last30 {
        let mut parts = vec![
            format!("30 days: {}", plural(activity.tokens, "token")),
            plural(activity.turns, "turn"),
        ];
        let (seen, expected) = activity.observed;
        if expected > 0 {
            parts.push(format!("{seen}/{expected} days"));
        }
        lines.extend(sub(&parts, SUB_INDENT, ctx));
        lines.extend(sub(&breakdown(activity), NESTED_INDENT, ctx));
        if !activity.models.is_empty() {
            let mut parts = vec!["models, 30 days (turns)".to_owned()];
            parts.extend(
                activity
                    .models
                    .iter()
                    .take(TOP_MODELS)
                    .map(|(name, turns)| format!("{name} {}", exact_count(*turns))),
            );
            let rest = activity.models.len().saturating_sub(TOP_MODELS);
            if rest > 0 {
                parts.push(format!("+{rest} more"));
            }
            lines.extend(sub(&parts, SUB_INDENT, ctx));
        }
    }
    lines.extend(fitted(DETAILS_JSON, ctx));
    lines
}

fn spend_lines(periods: [f64; 3], byok: f64, currency: Currency, ctx: Ctx) -> Vec<Line> {
    let line = if byok <= 0.0 && periods.iter().all(|spend| *spend <= 0.0) {
        format!("BYOK spend {} in every period", text::money(0.0, currency))
    } else {
        format!("BYOK spend {} this month", text::money(byok, currency))
    };
    sub(&[line], SUB_INDENT, ctx)
}

fn fetched_text(account: &AccountQuota, view: &UsageView) -> String {
    let Some(at) = account.reading.fetched_at() else {
        return UNKNOWN_FETCH.to_owned();
    };
    format!(
        "fetched {} · fresh until {}",
        text::full_date(at, view.now, view.utc_offset_secs, false),
        text::clock(
            Timestamp(at.0.saturating_add(FRESH_SECS)),
            view.now,
            view.utc_offset_secs
        )
    )
}

fn region_name(code: &str) -> String {
    match code {
        "cn" => "China".to_owned(),
        "global" => "Global".to_owned(),
        "sgp" => "Singapore".to_owned(),
        "ams" => "Amsterdam".to_owned(),
        other => other.to_owned(),
    }
}

fn switch(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

fn fact_lines(facts: &[Fact], view: &UsageView, ctx: Ctx) -> Vec<Line> {
    let mut inline = Vec::new();
    let mut apart = Vec::new();
    for fact in facts {
        match fact {
            Fact::Region(code) => inline.push(format!("region {}", region_name(code))),
            Fact::AutoRenew(on) => inline.push(format!("auto-renew {}", switch(*on))),
            Fact::MimoClaw(on) => inline.push(format!("MiMo Claw {}", switch(*on))),
            Fact::ApiKeyAuth => inline.push("auth API key".to_owned()),
            Fact::DeviceAuth => inline.push("auth device sign-in".to_owned()),
            Fact::GoogleProject(project) => apart.push((1, format!("Google project {project}"))),
            Fact::KeyType(kind) if kind == "standard" => apart.push((
                2,
                "standard key; workspace balance needs a management key".to_owned(),
            )),
            Fact::KeyType(kind) => apart.push((2, format!("{kind} key"))),
            Fact::PeriodStart(at) => apart.push((
                3,
                format!(
                    "period started {}",
                    text::full_date(*at, view.now, view.utc_offset_secs, false)
                ),
            )),
        }
    }
    apart.sort_by_key(|(order, _)| *order);
    let mut lines = Vec::new();
    if !inline.is_empty() {
        lines.extend(foot_wrapped(&inline, ctx));
    }
    lines.extend(apart.into_iter().map(|(_, text)| foot(text)));
    lines
}

fn foot_wrapped(parts: &[String], ctx: Ctx) -> Vec<Line> {
    let parts: Vec<(String, Tone)> = parts.iter().map(|part| (part.clone(), Tone::Dim)).collect();
    text::wrap(
        vec![Span::plain(" ".repeat(FOOT_INDENT))],
        FOOT_INDENT,
        &parts,
        ctx.columns(),
    )
}
