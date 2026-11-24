// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::accounts::{AccountRegistry, ConnectionError, StoredSnapshot};
use crate::dashboard::{DashboardRow, DashboardSettings};
use crate::onboarding::OwnedLogin;
use jackin_core::plain;
use serde_json::Value;

pub const RESOURCES: &str = "resources";
pub const RESETS: &str = "resets";

pub const ALL_USAGE_TITLE: &str = "All Usage";

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
    let Some(db) = AccountRegistry::open_read_only(service.database())? else {
        return Ok(Vec::new());
    };
    let settings = db.dashboard_settings()?;
    db.snapshots()?
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let id = row.0.id;
            let mut entry = detail(row, service.now(), &settings);
            entry.hub = db.hub_link(id)?.map(|mirror| mirror.hub.to_string());
            if let Some(fail) = db.failure(id)? {
                entry.summary.insert(0, plain(&fail.message));
                entry.details.insert(
                    0,
                    format!(
                        "{} · retry {}",
                        plain(fail.message),
                        jackin_core::Timestamp(fail.retry_at).rfc3339()
                    ),
                );
            }
            if settings.conceal_names {
                entry.label = crate::dashboard::concealed_label(index);
            }
            Ok(entry)
        })
        .collect()
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
    default_all=>"Open All Usage by default", remaining=>"Show remaining usage", exact_resets=>"Show exact reset times", pace_marks=>"Show pace marks",
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
pub const NEEDS_ATTENTION: &str = "Needs Attention";
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
