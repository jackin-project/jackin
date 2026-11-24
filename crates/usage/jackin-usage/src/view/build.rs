// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_core::{Timestamp, plain};
use serde_json::{Value, json};
use uuid::Uuid;

use super::model::{
    AccountQuota, Activity, Attention, Currency, FRESH_SECS, Fact, Feed, Meter, Note,
    PositiveAmount, ProviderGroup, Reading, ResetCredit, Role, Unit, UsageView, Used, UsedFraction,
};
use crate::accounts::{
    AccountRegistry, ConnectedAccount, ConnectionError, FailureKind, RefreshFailure, UsageProvider,
};
use crate::catalog;
use crate::dashboard::{DashboardSettings, concealed_label};

const CONCEALED_FAILURE: &str = "Current usage unavailable.";

const TABLE: &[(&str, &str, Role)] = &[
    ("session", "Session", Role::Overall),
    ("weekly", "Weekly", Role::Overall),
    ("monthly", "Monthly", Role::Overall),
    ("monthlyTotal", "Monthly total", Role::Overall),
    ("monthlyCode", "Monthly code", Role::Overall),
    ("tokenPlan", "Token Plan", Role::Overall),
    ("freeRequests", "Free requests", Role::Overall),
    ("general/interval", "General 5h", Role::Overall),
    ("general/weekly", "General weekly", Role::Overall),
    ("fable", "Fable weekly", Role::Scoped),
    ("sonnet", "Sonnet weekly", Role::Scoped),
    ("opus", "Opus weekly", Role::Scoped),
    ("spark", "Spark", Role::Scoped),
    ("sparkWeekly", "Spark weekly", Role::Scoped),
    ("3p-5h", "Claude 5h", Role::Scoped),
    ("3p-weekly", "Claude weekly", Role::Scoped),
    ("gemini-5h", "Gemini 5h", Role::Scoped),
    ("gemini-weekly", "Gemini weekly", Role::Scoped),
    ("gemini-argon-wtu-weekly", "Argon weekly", Role::Scoped),
    ("extraUsage", "Extra usage", Role::Addon),
    ("webSearches", "Web searches", Role::Addon),
    ("booster", "Booster", Role::Addon),
    ("keyLimit", "Key limit", Role::Addon),
    ("budget", "Budget", Role::Addon),
    ("payAsYouGo", "Extra usage cap", Role::Addon),
];

#[derive(Clone, Debug, Default)]
pub struct ViewFilter {
    pub provider: Option<UsageProvider>,
    pub account: Option<Uuid>,
}

struct Source {
    index: usize,
    account: ConnectedAccount,
    fetched_at: Option<i64>,
    payload: Option<Value>,
    failure: Option<RefreshFailure>,
    feed: Feed,
}

pub fn build(
    db: &AccountRegistry,
    filter: &ViewFilter,
    now: Timestamp,
    utc_offset_secs: i32,
) -> Result<UsageView, ConnectionError> {
    let settings = db.dashboard_settings()?;
    let mut sources = Vec::new();
    for (index, (account, fetched_at, payload)) in db.snapshots()?.into_iter().enumerate() {
        let wanted = filter
            .provider
            .is_none_or(|provider| provider == account.provider)
            && filter.account.is_none_or(|id| id == account.id);
        if wanted {
            let failure = db.failure(account.id)?;
            let feed = if db.is_statusline(account.id)? {
                Feed::StatusLine
            } else {
                Feed::Provider
            };
            sources.push(Source {
                index,
                account,
                fetched_at,
                payload,
                failure,
                feed,
            });
        }
    }
    Ok(assemble(&settings, &sources, now, utc_offset_secs))
}

fn threshold(remaining_percent: u8) -> UsedFraction {
    UsedFraction::new(f64::from(100u8.saturating_sub(remaining_percent)) / 100.0)
}

fn assemble(
    settings: &DashboardSettings,
    sources: &[Source],
    now: Timestamp,
    utc_offset_secs: i32,
) -> UsageView {
    let mut providers = UsageProvider::ALL.to_vec();
    providers.sort_by_key(|provider| {
        settings
            .provider_order
            .iter()
            .position(|listed| listed == provider)
            .unwrap_or(usize::MAX)
    });
    let mut groups = Vec::new();
    for provider in providers {
        let members: Vec<&Source> = sources
            .iter()
            .filter(|source| source.account.provider == provider)
            .collect();
        if members.is_empty() {
            continue;
        }
        groups.push(ProviderGroup {
            provider,
            name: catalog::providers()
                .find(|product| product.connector == Some(provider))
                .map_or_else(|| provider.metric_group(), |product| product.name),
            accounts: members
                .iter()
                .map(|source| quota(source, &members, settings, now))
                .collect(),
        });
    }
    let critical_at = threshold(settings.critical);
    UsageView {
        now,
        utc_offset_secs,
        warn_at: threshold(settings.warning),
        critical_at,
        attention: attention(&groups, critical_at, now),
        groups,
    }
}

fn quota(
    source: &Source,
    members: &[&Source],
    settings: &DashboardSettings,
    now: Timestamp,
) -> AccountQuota {
    let body = source.payload.as_ref();
    let hidden = &source.account.preferences.hidden;
    let mut meters = body
        .map(|body| meters(body, source.fetched_at, now, hidden))
        .unwrap_or_default();
    meters.sort_by(|left, right| {
        (left.role, left.window_secs.unwrap_or(u32::MAX), &left.name).cmp(&(
            right.role,
            right.window_secs.unwrap_or(u32::MAX),
            &right.name,
        ))
    });
    let main = pick_main(&meters, now).map(|position| meters.remove(position));
    AccountQuota {
        id: source.account.id,
        label: if settings.conceal_names {
            concealed_label(source.index)
        } else {
            plain(&source.account.label)
        },
        plan: body
            .and_then(|body| body["plan"].as_str())
            .filter(|plan| !plan.trim().is_empty())
            .map(plain),
        reading: reading(source, settings.conceal_names, now),
        duplicates: members
            .iter()
            .filter(|other| {
                other.account.id != source.account.id && other.account.label == source.account.label
            })
            .map(|other| other.account.id)
            .collect(),
        feed: source.feed,
        main,
        secondary: meters,
        notes: body
            .map(|body| notes(body, source.account.provider, now, hidden))
            .unwrap_or_default(),
        facts: body.map(facts).unwrap_or_default(),
    }
}

fn reading(source: &Source, conceal: bool, now: Timestamp) -> Reading {
    let at = source.fetched_at.map(Timestamp);
    let message = |text: &str| {
        if conceal {
            CONCEALED_FAILURE.to_owned()
        } else {
            plain(text)
        }
    };
    if let Some(failure) = &source.failure {
        return Reading::Failed {
            at,
            message: message(&failure.message),
            retry_at: Some(Timestamp(failure.retry_at)),
            reconnect: failure.kind == FailureKind::Authentication,
        };
    }
    if let Some(notice) = source
        .payload
        .as_ref()
        .and_then(|body| body["notice"].as_str())
    {
        return Reading::Failed {
            at,
            message: message(notice),
            retry_at: None,
            reconnect: false,
        };
    }
    match at {
        None => Reading::Waiting,
        Some(at) if now.0.saturating_sub(at.0) >= FRESH_SECS => Reading::Stale { at },
        Some(at) => Reading::Fresh { at },
    }
}

fn pick_main(meters: &[Meter], now: Timestamp) -> Option<usize> {
    for role in [Role::Overall, Role::Scoped] {
        let live = best(
            meters,
            |meter| meter.role == role && meter.live(now),
            |meter| (meter.fraction().rounded(), meter.window_secs.unwrap_or(0)),
        );
        if live.is_some() {
            return live;
        }
    }
    best(
        meters,
        |meter| meter.role != Role::Addon,
        |meter| (0, meter.window_secs.unwrap_or(0)),
    )
}

fn best<K: Ord>(
    meters: &[Meter],
    keep: impl Fn(&Meter) -> bool,
    key: impl Fn(&Meter) -> K,
) -> Option<usize> {
    let mut found: Option<(usize, K)> = None;
    for (position, meter) in meters.iter().enumerate().filter(|(_, meter)| keep(meter)) {
        let candidate = key(meter);
        if found.as_ref().is_none_or(|(_, leader)| candidate > *leader) {
            found = Some((position, candidate));
        }
    }
    found.map(|(position, _)| position)
}

fn meters(
    body: &Value,
    fetched_at: Option<i64>,
    now: Timestamp,
    hidden: &BTreeSet<String>,
) -> Vec<Meter> {
    body["resources"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| !hidden.contains(key.as_str()))
        .filter_map(|(key, resource)| meter(key, resource, fetched_at, now))
        .collect()
}

fn meter(key: &str, resource: &Value, fetched_at: Option<i64>, now: Timestamp) -> Option<Meter> {
    if !matches!(resource["kind"].as_str()?, "consumption" | "quota") {
        return None;
    }
    let used = used_of(resource)?;
    let resets_at = resource["resets_at"]
        .as_i64()
        .or_else(|| resource["resetsAt"].as_i64())
        .map(Timestamp);
    let (name, role) = identify(key, resource);
    let rolled_over = matches!(
        (fetched_at, resets_at),
        (Some(fetched), Some(at)) if fetched < at.0 && at <= now
    );
    Some(Meter {
        key: plain(key),
        name,
        role,
        window_secs: window(resource, resets_at),
        resets_at,
        used,
        rolled_over,
    })
}

fn identify(key: &str, resource: &Value) -> (String, Role) {
    if let Some((_, name, role)) = TABLE.iter().find(|(known, _, _)| *known == key) {
        return ((*name).to_owned(), *role);
    }
    let role = if key.starts_with("general/") {
        Role::Overall
    } else if key.starts_with("spark") || key.starts_with("3p-") || key.starts_with("gemini-") {
        Role::Scoped
    } else {
        Role::Addon
    };
    let name = resource["label"]
        .as_str()
        .filter(|label| !label.trim().is_empty())
        .map_or_else(|| plain(key), plain);
    (name, role)
}

fn window(resource: &Value, resets_at: Option<Timestamp>) -> Option<u32> {
    let seconds = resource["window_seconds"]
        .as_i64()
        .or_else(|| resource["windowSeconds"].as_i64())
        .or_else(|| resets_at?.0.checked_sub(resource["starts_at"].as_i64()?))?;
    u32::try_from(seconds).ok().filter(|seconds| *seconds > 0)
}

fn scale_of(resource: &Value) -> f64 {
    if resource["unit"] == "millionths_of_cent" {
        return 1e6;
    }
    let exponent = resource["exponent"]
        .as_u64()
        .filter(|n| *n <= 9)
        .unwrap_or(0);
    10f64.powi(i32::try_from(exponent).unwrap_or(0))
}

fn unit_of(resource: &Value) -> Option<Unit> {
    if let Some(currency) = resource["currency"].as_str().and_then(Currency::new) {
        return Some(Unit::Money(currency));
    }
    match resource["unit"].as_str()? {
        "credits" => Some(Unit::Credits),
        "tokens" => Some(Unit::Tokens),
        "requests" | "REQUESTS" => Some(Unit::Requests),
        "searches" => Some(Unit::Searches),
        "millionths_of_cent" => Some(Unit::Cents),
        "monetary units" => Some(Unit::Monetary),
        "day" => None,
        other => Currency::new(other).map(Unit::Money),
    }
}

fn used_of(resource: &Value) -> Option<Used> {
    if matches!(
        resource["status"].as_str(),
        Some("unlimited" | "not_in_plan")
    ) {
        return None;
    }
    if let Some(left) = resource["remaining_fraction"].as_f64() {
        return Some(Used::Fraction(UsedFraction::new(1.0 - left)));
    }
    if let Some(left) = resource["remaining_percent"].as_f64() {
        return Some(Used::Fraction(UsedFraction::new(1.0 - left / 100.0)));
    }
    let scale = scale_of(resource);
    let limit = resource["limit"]
        .as_f64()
        .map(|limit| limit / scale)
        .and_then(PositiveAmount::new);
    let used = resource["used"]
        .as_f64()
        .map(|used| used / scale)
        .or_else(|| Some(limit?.get() - resource["remaining"].as_f64()? / scale));
    match (used, limit) {
        (Some(used), Some(limit)) => Some(match unit_of(resource) {
            Some(unit) if resource["unit"] != "percent" => Used::Amount {
                used: used.max(0.0),
                limit,
                unit,
            },
            _ => Used::Fraction(UsedFraction::new(used / limit.get())),
        }),
        _ => resource["utilization"]
            .as_f64()
            .map(|used| Used::Fraction(UsedFraction::new(used))),
    }
}

fn notes(
    body: &Value,
    provider: UsageProvider,
    now: Timestamp,
    hidden: &BTreeSet<String>,
) -> Vec<Note> {
    let mut notes = balances(body, hidden);
    notes.extend(cap(body, hidden));
    notes.extend(daily(body, hidden));
    if !hidden.contains("rateLimitResets") {
        notes.extend(resets(body, provider, now));
    }
    notes.extend(tokens(body));
    notes.extend(spend(body));
    notes
}

fn cap(body: &Value, hidden: &BTreeSet<String>) -> Option<Note> {
    if hidden.contains("payAsYouGo") {
        return None;
    }
    let limit = body["extraUsageCap"]["limit"].as_f64()?;
    Some(Note::Balance {
        key: "payAsYouGo".to_owned(),
        label: "Extra cap".to_owned(),
        amount: limit,
        unit: Unit::Money(Currency::USD),
        approx: None,
    })
}

fn daily(body: &Value, hidden: &BTreeSet<String>) -> Vec<Note> {
    body["resources"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, resource)| !hidden.contains(key.as_str()) && resource["kind"] == "history")
        .map(|(key, resource)| Note::Daily {
            key: plain(key),
            label: match key.as_str() {
                "daily/consumption" => "Daily usage".to_owned(),
                other => resource["label"]
                    .as_str()
                    .map_or_else(|| plain(other), plain),
            },
            records: resource["amount"].as_u64(),
            lines: resource["details"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(plain)
                .collect(),
        })
        .collect()
}

fn balances(body: &Value, hidden: &BTreeSet<String>) -> Vec<Note> {
    let mut found: Vec<(&String, Note)> = body["resources"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| !hidden.contains(key.as_str()))
        .filter_map(|(key, resource)| Some((key, balance(key, resource)?)))
        .collect();
    found.sort_by_key(|(key, _)| (key.as_str() != "credits", wallet_rank(key)));
    found.into_iter().map(|(_, note)| note).collect()
}

fn wallet_rank(key: &str) -> u8 {
    match key.rsplit_once('/').map(|(_, field)| field) {
        Some("totalAvailable") => 0,
        Some("cash") => 1,
        Some("gift") => 2,
        Some("frozen") => 3,
        Some("creditLimit") => 4,
        Some("creditAvailable") => 5,
        _ => 0,
    }
}

fn balance_label(key: &str, resource: &Value, unit: Unit) -> String {
    if unit == Unit::Credits {
        return "Credits".to_owned();
    }
    if let Some(label) = resource["label"]
        .as_str()
        .filter(|label| !label.trim().is_empty())
    {
        return plain(label);
    }
    let named = match key.rsplit_once('/').map_or(key, |(_, field)| field) {
        "booster" => "Booster",
        "monthlyChargeLimit" => "Charge limit",
        "monthlyChargeUsed" => "Charged",
        "totalAvailable" => "Available",
        "cash" => "Cash",
        "gift" => "Gift",
        "frozen" => "Frozen",
        "creditLimit" => "Credit limit",
        "creditAvailable" => "Credit left",
        _ => "Balance",
    };
    named.to_owned()
}

fn balance(key: &str, resource: &Value) -> Option<Note> {
    if !matches!(resource["kind"].as_str(), Some("balance" | "amount"))
        || key == "rateLimitResets"
        || resource["unit"] == "resets"
    {
        return None;
    }
    let scale = scale_of(resource);
    let amount = resource["available"]
        .as_f64()
        .or_else(|| resource["amount"].as_f64())
        .or_else(|| resource["remaining"].as_f64())?
        / scale;
    let unit = unit_of(resource)?;
    let optional = ["gift", "frozen", "creditLimit", "creditAvailable"]
        .iter()
        .any(|field| key.ends_with(&format!("/{field}")));
    if optional && amount == 0.0 {
        return None;
    }
    let approx = resource["usd"]
        .as_f64()
        .filter(|_| unit == Unit::Credits)
        .map(|usd| (usd, Currency::USD));
    Some(Note::Balance {
        key: plain(key),
        label: balance_label(key, resource, unit),
        amount,
        unit,
        approx,
    })
}

fn resets(body: &Value, provider: UsageProvider, now: Timestamp) -> Option<Note> {
    let queued_ready = provider == UsageProvider::Zai;
    let mut credits: Vec<ResetCredit> = body["resets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|grant| {
            let at = grant["expiresAt"].as_i64()?;
            let live = at > now.0;
            let usable = grant["usable"] == true && live;
            Some(ResetCredit {
                kind: plain(grant["kind"].as_str().unwrap_or("reset")),
                ready: if queued_ready { live } else { usable },
                redeemable: usable,
                expires_at: Timestamp(at),
            })
        })
        .collect();
    if credits.is_empty() {
        credits = body["resources"]["rateLimitResets"]["expires_at"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .map(|at| ResetCredit {
                kind: "reset".to_owned(),
                ready: at > now.0,
                redeemable: at > now.0,
                expires_at: Timestamp(at),
            })
            .collect();
    }
    (!credits.is_empty()).then_some(Note::Resets(credits))
}

fn count(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        let number = value.as_f64().filter(|n| n.is_finite() && *n >= 0.0)?;
        u64::try_from(number.round() as i64).ok()
    })
}

fn days(value: &Value) -> u8 {
    count(value)
        .and_then(|days| u8::try_from(days).ok())
        .unwrap_or(0)
}

fn activity(range: &Value) -> Option<Activity> {
    let totals = &range["totals"];
    let tokens = count(&totals["tokens"])?;
    let mut models: Vec<(String, u64)> = range
        .get("by_model")
        .or_else(|| range.get("byModel"))
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(name, usage)| Some((plain(name), count(&usage["turns"])?)))
        .collect();
    models.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    Some(Activity {
        tokens,
        turns: count(&totals["turns"]).unwrap_or(0),
        cached_in: count(&totals["cacheReadTokens"]).unwrap_or(0),
        input: count(&totals["inputTokens"]).unwrap_or(0),
        output: count(&totals["outputTokens"]).unwrap_or(0),
        observed: (days(&range["observedDays"]), days(&range["expectedDays"])),
        models,
    })
}

fn tokens(body: &Value) -> Option<Note> {
    let ranges = &body["history"]["ranges"];
    let (today, yesterday, last30) = if ranges.is_object() {
        (
            count(&ranges["today"]["totals"]["tokens"]),
            activity(&ranges["yesterday"]),
            activity(&ranges["last30"]),
        )
    } else {
        let log = &body["activity"];
        let prior = json!({
            "totals": log["prior30Days"],
            "observedDays": log["observed_days"],
            "expectedDays": log["expected_days"],
        });
        (None, activity(&log["yesterday"]), activity(&prior))
    };
    (today.is_some() || yesterday.is_some() || last30.is_some()).then_some(Note::Tokens {
        today,
        yesterday,
        last30,
    })
}

fn spend(body: &Value) -> Option<Note> {
    let period = |key: &str, field: &str| body["period_spend"][key][field].as_f64();
    let (today, week, month) = (
        period("today", "spend"),
        period("thisWeek", "spend"),
        period("thisMonth", "spend"),
    );
    if today.is_none() && week.is_none() && month.is_none() {
        return None;
    }
    Some(Note::Spend {
        today,
        week,
        month,
        byok: period("thisMonth", "byokSpend"),
        currency: Currency::USD,
    })
}

fn facts(body: &Value) -> Vec<Fact> {
    let text = |key: &str| body[key].as_str().map(plain);
    let mut facts = Vec::new();
    facts.extend(text("region").map(Fact::Region));
    facts.extend(body["auto_renew"].as_bool().map(Fact::AutoRenew));
    facts.extend(body["claw_enabled"].as_bool().map(Fact::MimoClaw));
    match body["authMethod"].as_str() {
        Some("key") => facts.push(Fact::ApiKeyAuth),
        Some("device") => facts.push(Fact::DeviceAuth),
        _ => {}
    }
    facts.extend(text("project").map(Fact::GoogleProject));
    facts.extend(text("key_type").map(Fact::KeyType));
    facts.extend(
        body["period_start"]
            .as_i64()
            .map(|at| Fact::PeriodStart(Timestamp(at))),
    );
    facts
}

fn attention(
    groups: &[ProviderGroup],
    critical_at: UsedFraction,
    now: Timestamp,
) -> Vec<Attention> {
    let mut entries: Vec<Attention> = groups
        .iter()
        .flat_map(|group| &group.accounts)
        .filter(|account| matches!(account.reading, Reading::Failed { .. }))
        .map(|account| Attention::Failing {
            account: account.id,
        })
        .collect();
    entries.extend(near_limit(groups, critical_at, now));
    entries.extend(duplicates(groups));
    entries
}

fn near_limit(
    groups: &[ProviderGroup],
    critical_at: UsedFraction,
    now: Timestamp,
) -> Vec<Attention> {
    type Key = (UsageProvider, String, String, u8, Option<i64>);
    let mut findings: Vec<(Key, Vec<Uuid>)> = Vec::new();
    for group in groups {
        for account in &group.accounts {
            for meter in account.meters() {
                if !meter.live(now) || !meter.fraction().at_least(critical_at) {
                    continue;
                }
                let key = (
                    group.provider,
                    account.label.clone(),
                    meter.name.clone(),
                    meter.fraction().rounded(),
                    meter.resets_at.map(|at| at.0),
                );
                match findings.iter_mut().find(|(known, _)| *known == key) {
                    Some((_, accounts)) => accounts.push(account.id),
                    None => findings.push((key, vec![account.id])),
                }
            }
        }
    }
    findings
        .into_iter()
        .map(|((_, _, meter, _, _), accounts)| Attention::NearLimit { accounts, meter })
        .collect()
}

fn duplicates(groups: &[ProviderGroup]) -> Vec<Attention> {
    let mut entries = Vec::new();
    for group in groups {
        let mut seen: BTreeSet<Uuid> = BTreeSet::new();
        for account in group
            .accounts
            .iter()
            .filter(|account| account.is_duplicate())
        {
            if seen.contains(&account.id) {
                continue;
            }
            let members: Vec<&AccountQuota> = group
                .accounts
                .iter()
                .filter(|other| other.id == account.id || account.duplicates.contains(&other.id))
                .collect();
            seen.extend(members.iter().map(|member| member.id));
            let remove = members
                .iter()
                .find(|member| matches!(member.reading, Reading::Failed { .. }))
                .or_else(|| members.last())
                .map_or(account.id, |member| member.id);
            entries.push(Attention::Duplicate {
                accounts: members.iter().map(|member| member.id).collect(),
                remove,
            });
        }
    }
    entries
}
