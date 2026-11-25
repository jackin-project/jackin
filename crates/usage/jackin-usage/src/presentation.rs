// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::accounts::{AccountRegistry, ConnectionError, StoredSnapshot};
use crate::dashboard::{DashboardRow, DashboardSettings};
use crate::onboarding::OwnedLogin;
use jackin_core::plain;
use serde_json::Value;

pub const DATABASE_PATH: &str = "usage/accounts.sqlite3";
pub const RESOURCES: &str = "resources";
pub const RESETS: &str = "resets";

#[derive(Debug)]
pub struct AccountDetails {
    pub id: String,
    pub label: String,
    pub provider: String,
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

fn resource_detail(name: &str, resource: &Value) -> String {
    let unit = resource.get("unit").and_then(Value::as_str).unwrap_or("");
    let mut values = Vec::new();
    if let Some(fields) = resource.as_object() {
        for (key, value) in fields {
            if value.is_null() || matches!(key.as_str(), "unit" | "kind") {
                continue;
            }
            let value = if key == "resets_at" {
                value.as_i64().map_or_else(
                    || value.to_string(),
                    |epoch| jackin_core::Timestamp(epoch).rfc3339(),
                )
            } else {
                value
                    .as_str()
                    .map_or_else(|| value.to_string(), str::to_owned)
            };
            values.push(format!("{} {value}", key.replace('_', " ")));
        }
    }
    format!("{name} ({unit}) · {}", values.join(" · "))
}

fn detail(
    (account, fetched, payload): StoredSnapshot,
    now: jackin_core::Timestamp,
) -> AccountDetails {
    let mut inventory = Vec::new();
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
        if let Some(plan) = data.get("plan").and_then(Value::as_str) {
            lines.push(format!("Plan · {plan}"));
        }
        if let Some(resources) = data.get("resources").and_then(Value::as_object) {
            for (name, resource) in resources {
                lines.push(resource_detail(name, resource));
            }
        }
        if lines.len() <= 1 {
            lines.push("No limits reported.".into());
        }
    } else {
        lines.push("Waiting for first usage refresh.".into());
    }
    AccountDetails {
        resets: inventory,
        hub: None,
        region,
        device_login,
        id: account.id.to_string(),
        label: plain(account.label),
        provider: account.provider.to_string(),
        details: lines.into_iter().map(plain).collect(),
    }
}

pub fn entries(service: &OwnedLogin) -> Result<Vec<AccountDetails>, ConnectionError> {
    let Some(db) = AccountRegistry::open_read_only(service.database())? else {
        return Ok(Vec::new());
    };
    let concealed = db.dashboard_settings()?.conceal_names;
    db.snapshots()?
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            let id = row.0.id;
            let mut entry = detail(row, service.now());
            entry.hub = db.hub_link(id)?.map(|mirror| mirror.hub.to_string());
            if let Some(fail) = db.failure(id)? {
                entry.details.insert(
                    0,
                    format!(
                        "{} · retry {}",
                        plain(fail.message),
                        jackin_core::Timestamp(fail.retry_at).rfc3339()
                    ),
                );
            }
            if concealed {
                entry.label = format!("Account {}", index + 1);
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

pub fn metric_details(row: &DashboardRow) -> Vec<String> {
    let mut details: Vec<_> = row.details.iter().map(plain).collect();
    if let Some(plan) = &row.plan {
        details.push(format!("Plan · {}", plain(plan)));
    }
    if let Some(reset) = &row.reset {
        details.push(format!("Reset · {}", plain(reset)));
    }
    if let Some(pace) = row.pace_mark {
        details.push(format!("Pace · {:.0}%", pace * 100.0));
    }
    if row.stale {
        details.push("Last known usage · stale".into());
    }
    details
}
pub const NEEDS_ATTENTION: &str = "Needs Attention";
pub const ALL_CLEAR: &str = "All clear";

pub fn caption(first: impl AsRef<str>, second: impl AsRef<str>) -> String {
    format!("{} · {}", plain(first.as_ref()), plain(second.as_ref()))
}

pub fn waiting(label: &str) -> String {
    caption(label, "Waiting for usage")
}
