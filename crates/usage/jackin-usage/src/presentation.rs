// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::accounts::{ConnectedAccount, ConnectionError, StoredSnapshot};
use crate::dashboard::{DashboardRow, DashboardSettings};
use crate::onboarding::OwnedLogin;
use crate::view::{
    self, AccountQuota, Attention, Ctx, Depth, Line, Meter, Note, Reading, UsageView, Used,
    ViewFilter, card, detail as extras, short_id, text,
};
use jackin_core::plain;
use serde_json::Value;

pub const RESETS: &str = "resets";

const CARD_WIDTH: u16 = 80;
const VALUE_COLUMNS: usize = 32;
const NO_PLAN: &str = "plan not reported";

#[derive(Debug)]
pub struct AccountDetails {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub summary: Vec<String>,
    pub details: Vec<String>,
    pub resets: Vec<ResetItem>,
    pub hub: Option<String>,
    pub region: Option<String>,
    pub device_login: bool,
}

#[derive(Debug)]
pub struct ResetItem {
    pub index: usize,
    pub identity: String,
    pub description: String,
    pub usable: bool,
}

pub(crate) fn detail(
    (account, fetched, payload): StoredSnapshot,
    now: jackin_core::Timestamp,
    settings: &DashboardSettings,
) -> AccountDetails {
    let mut inventory = Vec::new();
    let mut summary = Vec::new();
    let mut lines = Vec::new();
    if let Some(epoch) = fetched {
        lines.push(format!(
            "Updated {}",
            jackin_core::Timestamp(epoch).rfc3339()
        ));
    }
    let region = payload
        .as_ref()
        .and_then(|data| data.get("region"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let device_login = payload
        .as_ref()
        .and_then(|data| data.get("authMethod"))
        .and_then(Value::as_str)
        == Some("device");
    if let Some(data) = payload {
        if let Some(rows) = data["resets"].as_array() {
            for (index, row) in rows.iter().enumerate() {
                let eligible = crate::resets::ResetSelection::from_snapshot(
                    account.id,
                    account.provider,
                    &data,
                    index,
                    now,
                );
                let title = eligible.as_ref().map_or_else(
                    |error| {
                        let kind = row["kind"].as_str().unwrap_or("Reset");
                        let expiry = row["expiresAt"]
                            .as_i64()
                            .map(jackin_core::Timestamp)
                            .map_or_else(|| "unknown".into(), jackin_core::Timestamp::rfc3339);
                        format!("{kind} · expiry {expiry} · {error}")
                    },
                    crate::resets::ResetSelection::description,
                );
                inventory.push(ResetItem {
                    index,
                    identity: row.to_string(),
                    description: plain(title),
                    usable: eligible.is_ok(),
                });
            }
        }
        match account.provider {
            crate::accounts::UsageProvider::Kimi
            | crate::accounts::UsageProvider::MiniMax
            | crate::accounts::UsageProvider::Mimo => {
                if let Some(region) = region.as_deref() {
                    let name = match region {
                        "cn" => "China",
                        "global" => "Global",
                        "sgp" => "Singapore",
                        "ams" => "Amsterdam",
                        other => other,
                    };
                    lines.push(format!("Region · {name}"));
                }
                for (field, label) in [
                    ("auto_renew", "Automatic renewal"),
                    ("claw_enabled", "MiMo Claw"),
                    ("is_available", "Balance available"),
                    ("balanceAlertsEnabled", "Balance alerts"),
                ] {
                    if let Some(enabled) = data[field].as_bool() {
                        lines.push(format!("{label} · {}", if enabled { "On" } else { "Off" }));
                    }
                }
            }
            crate::accounts::UsageProvider::OpenRouter => {
                if let Some(kind) = data["key_type"].as_str() {
                    lines.push(format!("Key type · {kind}"));
                    if kind == "standard" {
                        lines.push(
                            "Workspace balance and activity history require a management key."
                                .into(),
                        );
                    }
                }
            }
            _ => {}
        }
        if let Some(plan) = data.get("plan").and_then(Value::as_str) {
            lines.push(format!("Plan · {plan}"));
            summary.push(caption("Plan", plan));
        }
        let measurements =
            crate::dashboard::account_detail_lines(account.provider, &data, settings, now.0);
        let overview = crate::dashboard::account_summary_lines(&account, &data, settings, now.0);
        if let Some(notice) = data["notice"].as_str() {
            lines.push(notice.to_owned());
            summary.push(plain(notice));
        } else if measurements.is_empty() {
            lines.push("No limits reported.".into());
            summary.push("No limits reported.".into());
        } else if overview.is_empty() {
            summary.push("No visible usage metrics.".into());
        }
        summary.extend(overview);
        lines.extend(measurements);
    } else {
        lines.push("Waiting for first usage refresh.".into());
        summary.push("Waiting for first usage refresh.".into());
    }
    AccountDetails {
        resets: inventory,
        hub: None,
        region,
        device_login,
        id: account.id.to_string(),
        label: plain(account.label),
        provider: account.provider.to_string(),
        summary,
        details: lines.into_iter().map(plain).collect(),
    }
}

pub fn entries(service: &OwnedLogin) -> Result<Vec<AccountDetails>, ConnectionError> {
    let Some(db) = service.open_read_only_registry()? else {
        return Ok(Vec::new());
    };
    let settings = db.dashboard_settings()?;
    let now = service.now();
    let view = view::build(&db, &ViewFilter::default(), now, service.utc_offset_secs())?;
    let mut stored = db.snapshots()?;
    let mut entries = Vec::new();
    for (group, quota) in view.accounts() {
        let Some(position) = stored
            .iter()
            .position(|(account, _, _)| account.id == quota.id)
        else {
            continue;
        };
        let mut entry = detail(stored.remove(position), now, &settings);
        entry.hub = db.hub_link(quota.id)?.map(|mirror| mirror.hub.to_string());
        entry.provider = group.provider.to_string();
        entry.label = usage_label(group.name, quota);
        entry.summary = summary(&view, quota);
        entry.details = plain_lines(&card(quota, &view, &Ctx::new(CARD_WIDTH), Depth::Full));
        entries.push(entry);
    }
    Ok(entries)
}

pub fn entry_label(account: &AccountQuota) -> String {
    if account.is_duplicate() {
        format!("{} #{}", account.label, account.short())
    } else {
        account.label.clone()
    }
}

pub fn short_provider(name: &str) -> &str {
    name.split_once(" (").map_or(name, |(short, _)| short)
}

fn names_provider(label: &str, provider: &str) -> bool {
    label
        .to_lowercase()
        .starts_with(&short_provider(provider).to_lowercase())
}

pub fn usage_label(provider: &str, account: &AccountQuota) -> String {
    let provider = short_provider(provider);
    if account.is_duplicate() {
        format!("{provider} #{} · {}", account.short(), account.label)
    } else if names_provider(&account.label, provider) {
        account.label.clone()
    } else {
        format!("{provider} · {}", account.label)
    }
}

pub fn bare_label(name: String, account: &ConnectedAccount) -> String {
    let provider = crate::catalog::providers()
        .find(|product| product.id == account.provider.as_str())
        .map_or("", |product| short_provider(product.name));
    let tagged = format!("{provider} #{} · ", short_id(account.id));
    let plain = format!("{provider} · ");
    let own_plain = (!names_provider(&account.label, provider)).then_some(plain.as_str());
    [Some(tagged.as_str()), own_plain]
        .into_iter()
        .flatten()
        .find_map(|prefix| name.strip_prefix(prefix))
        .map_or_else(|| name.clone(), str::to_owned)
}

fn plain_lines(lines: &[Line]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.text().trim_end().to_owned())
        .collect()
}

fn freshness_text(account: &AccountQuota, view: &UsageView) -> String {
    text::freshness(&account.reading, view.now, false)
        .into_iter()
        .map(|span| span.text)
        .collect()
}

fn main_line(view: &UsageView, meter: &Meter) -> String {
    let shown = text::percent(meter.fraction());
    let reset = card::reset_phrase(meter.resets_at, view, true);
    if meter.rolled_over {
        format!("{} {reset} · was {shown}", meter.name)
    } else {
        format!("{} {shown} used · {reset}", meter.name)
    }
}

pub fn summary(view: &UsageView, account: &AccountQuota) -> Vec<String> {
    let plan = account.plan.as_deref().unwrap_or(NO_PLAN);
    let mut lines = vec![format!("{plan} · {}", freshness_text(account, view))];
    lines.extend(account.main.iter().map(|meter| main_line(view, meter)));
    if let Reading::Failed {
        message,
        retry_at,
        reconnect,
        ..
    } = &account.reading
    {
        lines.push(if retry_at.is_some() {
            format!("! refresh failed: {message}")
        } else {
            format!("! {message}")
        });
        if *reconnect {
            lines.push(text::reconnect_command(account.id, text::IdForm::Simple));
        }
    }
    if let (view::Feed::StatusLine, Reading::Stale { at }) = (account.feed, &account.reading) {
        lines.push(text::status_line_note(*at, view.now));
    }
    lines.extend(
        account
            .duplicates
            .iter()
            .map(|other| format!("! also connected as #{}", short_id(*other))),
    );
    lines
}

pub fn board_summary(view: &UsageView) -> String {
    let total = view.account_count();
    let stale = view.stale_ages().len();
    let count = if total == 1 {
        "1 account".to_owned()
    } else {
        format!("{total} accounts")
    };
    match stale {
        0 => count,
        stale if stale == total => format!("{count} · all stale"),
        stale => format!("{count} · {stale} of {total} stale"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Normal,
    Warning,
    Critical,
    Muted,
}

#[derive(Clone, Debug)]
pub struct ReadingText {
    pub value: String,
    pub exact: String,
    pub reset: Option<String>,
    pub inspector: Vec<String>,
    pub details: Vec<String>,
    pub level: Level,
    pub bar: Option<u8>,
}

fn window_label(seconds: u32) -> String {
    match seconds {
        s if s % 86_400 == 0 => format!("{}d", s / 86_400),
        s if s % 3_600 == 0 => format!("{}h", s / 3_600),
        s if s % 60 == 0 => format!("{}m", s / 60),
        s => text::rel(i64::from(s)),
    }
}

fn inspector(view: &UsageView, account: &AccountQuota, window: Option<u32>) -> Vec<String> {
    let mut lines: Vec<String> = account
        .plan
        .iter()
        .map(|plan| caption("Plan", plan))
        .collect();
    lines.push(freshness_text(account, view));
    lines.extend(window.map(|seconds| format!("{} window", window_label(seconds))));
    lines
}

fn marks(account: &AccountQuota) -> Option<String> {
    let state = match account.reading {
        Reading::Stale { .. } => Some("stale"),
        Reading::Failed { .. } => Some("failed"),
        Reading::Waiting | Reading::Fresh { .. } => None,
    };
    let duplicate = account.is_duplicate().then_some("dup");
    let marks: Vec<&str> = state.into_iter().chain(duplicate).collect();
    (!marks.is_empty()).then(|| marks.join(" · "))
}

fn marked(account: &AccountQuota, value: &str) -> String {
    let Some(marks) = marks(account) else {
        return text::truncate(value, VALUE_COLUMNS);
    };
    let spaced = format!("{marks} · {value}");
    let joined = if text::width(&spaced) <= VALUE_COLUMNS {
        spaced
    } else {
        format!("{marks} {value}")
    };
    text::truncate(&joined, VALUE_COLUMNS)
}

pub fn meter_reading(view: &UsageView, account: &AccountQuota, meter: &Meter) -> ReadingText {
    let fraction = meter.fraction();
    let shown = text::percent(fraction);
    let reset = |at: jackin_core::Timestamp| text::rel(at.0 - view.now.0);
    let value = match meter.resets_at {
        Some(at) if meter.rolled_over => format!("reset {} ago · was {shown}", reset(at)),
        Some(at) if at > view.now => format!("{shown} used · {}", reset(at)),
        _ => format!("{shown} used"),
    };
    let mut exact = vec![if meter.rolled_over {
        format!("was {shown} at fetch")
    } else {
        format!("{shown} used")
    }];
    if let Used::Amount { used, limit, unit } = &meter.used {
        exact.push(text::quantity(*used, limit.get(), *unit));
    }
    if !meter.rolled_over && shown.starts_with(['<', '>']) {
        exact.push(format!("exact {:.2}%", fraction.percent()));
    }
    let level = if meter.rolled_over {
        Level::Muted
    } else if fraction.at_least(view.critical_at) {
        Level::Critical
    } else if fraction.at_least(view.warn_at) {
        Level::Warning
    } else {
        Level::Normal
    };
    ReadingText {
        value: marked(account, &value),
        exact: exact.join(" · "),
        reset: Some(card::reset_phrase(meter.resets_at, view, true)),
        inspector: inspector(view, account, meter.window_secs),
        details: plain_lines(&extras::meter_lines(
            account,
            meter,
            view,
            &Ctx::new(CARD_WIDTH),
        )),
        level,
        bar: (!meter.rolled_over).then(|| fraction.rounded()),
    }
}

pub fn note_key(note: &Note) -> &str {
    match note {
        Note::RemainingQuota { key, .. } | Note::Balance { key, .. } | Note::Daily { key, .. } => {
            key
        }
        Note::Resets(_) => "rateLimitResets",
        Note::Tokens { .. } => "history",
        Note::Spend { .. } => "spend",
    }
}

pub fn covered_keys(account: &AccountQuota) -> Vec<&str> {
    let mut keys: Vec<&str> = account.meters().map(|meter| meter.key.as_str()).collect();
    for note in &account.notes {
        match note {
            Note::RemainingQuota { .. } => {}
            Note::Balance { .. } | Note::Resets(_) | Note::Daily { .. } => {
                keys.push(note_key(note));
            }
            Note::Tokens { .. } => keys.extend(["today", "yesterday", "last30", "prior30"]),
            Note::Spend { .. } => keys.extend(["today", "week", "month"]),
        }
    }
    keys
}

pub fn attention_notices(view: &UsageView) -> Vec<String> {
    view::report::attention_entries(view)
        .map(|item| format!("{} · {} · {}", item.tag, item.subject, item.detail))
        .collect()
}

pub fn attention_summary(view: &UsageView) -> Option<String> {
    let count = view::report::attention_entries(view).count();
    (count > 0).then(|| view::report::attention_heading(count))
}

#[derive(Clone, Debug)]
pub struct AttentionRow {
    pub key: String,
    pub provider: crate::accounts::UsageProvider,
    pub accounts: Vec<uuid::Uuid>,
    pub title: String,
    pub label: String,
    pub reading: ReadingText,
}

pub fn attention_rows(view: &UsageView) -> Vec<AttentionRow> {
    view.attention
        .iter()
        .filter_map(|item| attention_row(view, item))
        .collect()
}

fn attention_row(view: &UsageView, item: &Attention) -> Option<AttentionRow> {
    let text = view::report::attention_text(view, item)?;
    let (kind, accounts, level) = match item {
        Attention::Failing { account } => ("failing", vec![*account], Level::Critical),
        Attention::NearLimit { accounts, .. } => ("near-limit", accounts.clone(), Level::Critical),
        Attention::Duplicate { accounts, .. } => ("duplicate", accounts.clone(), Level::Warning),
    };
    let first = *accounts.first()?;
    let (group, quota) = view.account(first)?;
    let subject = if accounts.len() == 1 {
        entry_label(quota)
    } else {
        quota.label.clone()
    };
    let near = match item {
        Attention::NearLimit { meter, .. } => quota.meters().find(|found| &found.name == meter),
        Attention::Failing { .. } | Attention::Duplicate { .. } => None,
    };
    let mut reading = match near {
        Some(meter) => meter_reading(view, quota, meter),
        None => ReadingText {
            value: String::new(),
            exact: String::new(),
            reset: None,
            inspector: inspector(view, quota, None),
            details: Vec::new(),
            level,
            bar: None,
        },
    };
    reading.level = level;
    reading.exact = text.detail.clone();
    reading.details = match item {
        Attention::Failing { .. } => summary(view, quota),
        Attention::NearLimit { .. } => vec![text.detail.clone()],
        Attention::Duplicate { accounts, .. } => {
            let mut lines = vec![format!(
                "Connected {} times: {}",
                accounts.len(),
                accounts
                    .iter()
                    .copied()
                    .map(|id| format!("#{}", short_id(id)))
                    .collect::<Vec<_>>()
                    .join(" ")
            )];
            lines.push(text.detail.clone());
            lines
        }
    };
    if near.is_none() {
        reading.value = match item {
            Attention::Duplicate { accounts, .. } => {
                format!("duplicate · {} connections", accounts.len())
            }
            Attention::Failing { .. } | Attention::NearLimit { .. } => freshness_text(quota, view),
        };
    }
    let key = match item {
        Attention::NearLimit { meter, .. } => format!("{kind}:{}:{meter}", first.hyphenated()),
        Attention::Failing { .. } | Attention::Duplicate { .. } => {
            format!("{kind}:{}", first.hyphenated())
        }
    };
    Some(AttentionRow {
        key,
        provider: group.provider,
        accounts,
        title: format!("{} · {subject}", text.tag),
        label: match (item, near) {
            (_, Some(meter)) => meter.name.clone(),
            (Attention::Duplicate { .. }, None) => "Connections".to_owned(),
            (_, None) => "Refresh".to_owned(),
        },
        reading,
    })
}

pub fn note_reading(
    view: &UsageView,
    account: &AccountQuota,
    note: &Note,
) -> (String, ReadingText) {
    let (name, parts) = card::note_parts(note, view);
    let full = parts.join(" · ");
    let reading = ReadingText {
        value: text::truncate(&full, VALUE_COLUMNS),
        exact: full,
        reset: None,
        inspector: inspector(view, account, None),
        details: plain_lines(&extras::note_lines(
            account,
            note,
            view,
            &Ctx::new(CARD_WIDTH),
        )),
        level: Level::Normal,
        bar: None,
    };
    (name, reading)
}

macro_rules! flags {
    ($($field:ident => $label:literal),+ $(,)?) => {
        pub fn flags(settings:&DashboardSettings)->Vec<(&'static str,&'static str,bool)> {
            vec![$((stringify!($field),$label,settings.$field)),+]
        }
        pub fn set_flag(settings:&mut DashboardSettings,name:&str,value:bool){
            match name {$(stringify!($field)=>settings.$field=value,)+_=>{}}
        }
    };
}
flags! {
    default_all=>"Open All Usage by default", exact_resets=>"Show exact reset times", pace_marks=>"Show pace marks",
    burning_fast=>"Flag Burning Fast",use_it_or_lose_it=>"Flag Use It or Lose It",conceal_names=>"Conceal account names",
    unify_claude=>"Combine Claude accounts",unify_chatgpt=>"Combine ChatGPT accounts",
    notify_warning=>"Alert at warning",notify_critical=>"Alert at critical",notify_burning=>"Alert on Burning Fast",notify_unused=>"Alert on unused quota",
}

pub fn metric_value(row: &DashboardRow, remaining: bool) -> String {
    metric_caption(row, remaining, &row.compact_value(remaining))
}

pub fn exact_metric_value(row: &DashboardRow, remaining: bool) -> String {
    metric_caption(row, remaining, &row.value)
}

fn metric_caption(row: &DashboardRow, remaining: bool, value: &str) -> String {
    if row.display_fraction(remaining).is_some() {
        format!(
            "{} {}",
            plain(value),
            if remaining { "remaining" } else { "used" }
        )
    } else {
        plain(value)
    }
}

pub fn reset_caption(reset: &str, exact: bool) -> String {
    match reset {
        "Imminent" => "reset due".into(),
        "Unknown" => "reset time unknown".into(),
        _ if exact => format!("resets at {}", plain(reset)),
        _ => format!("resets in {}", plain(reset)),
    }
}

pub fn metric_details(row: &DashboardRow, remaining: bool) -> Vec<String> {
    let mut details = row.supplemental_quantities(remaining);
    details.extend(
        row.details
            .iter()
            .filter(|item| **item != row.value)
            .map(plain),
    );
    if let Some(plan) = &row.plan {
        details.push(format!("Plan · {}", plain(plan)));
    }
    if let Some(pace) = row.pace_mark {
        details.push(format!("Pace · {:.0}%", pace * 100.0));
    }
    if row.stale {
        details.push("Last known usage · stale".into());
    }
    details
}

pub fn metric_inspector(row: &DashboardRow) -> Vec<String> {
    let mut details = Vec::new();
    if let Some(limit) = row.inspector_limit() {
        details.push(caption("Limit", limit));
    }
    if let Some(plan) = &row.plan {
        details.push(caption("Plan", plan));
    }
    if row.stale {
        details.push("Last known usage · stale".into());
    }
    details
}
pub const ALL_CLEAR: &str = "All clear";

pub fn caption(first: impl AsRef<str>, second: impl AsRef<str>) -> String {
    join_captions(&[first.as_ref(), second.as_ref()])
}

pub fn join_captions(parts: &[impl std::borrow::Borrow<str>]) -> String {
    plain(parts.join(" · "))
}

pub fn waiting(label: &str) -> String {
    caption(label, "Waiting for usage")
}
