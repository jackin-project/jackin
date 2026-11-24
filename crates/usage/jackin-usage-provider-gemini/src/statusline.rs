// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{ProviderError, parse_iso_epoch};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

const SESSION: i64 = 5 * 60 * 60;
const WEEK: i64 = 7 * 24 * 60 * 60;
const WINDOWS: [(&str, &str, i64); 5] = [
    ("gemini-5h", "Gemini 5h", SESSION),
    ("gemini-weekly", "Gemini weekly", WEEK),
    ("gemini-argon-wtu-weekly", "Gemini Argon weekly", WEEK),
    ("3p-5h", "Claude 5h", SESSION),
    ("3p-weekly", "Claude weekly", WEEK),
];

#[derive(Debug)]
pub struct AntigravityReading {
    pub email: String,
    pub snapshot: Value,
}

#[derive(Deserialize)]
struct StatusLine<'a> {
    product: String,
    email: Option<String>,
    plan_tier: Option<String>,
    #[serde(borrow, default)]
    quota: BTreeMap<String, &'a RawValue>,
}

#[derive(Deserialize)]
struct Quota {
    remaining_fraction: Option<f64>,
    #[serde(default, deserialize_with = "optional_reset_field")]
    reset_time: Option<String>,
    #[serde(default, deserialize_with = "optional_reset_field")]
    reset_in_seconds: Option<i64>,
    disabled: Option<bool>,
}

fn optional_reset_field<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let value = <&RawValue>::deserialize(deserializer)?;
    Ok(serde_json::from_str(value.get()).ok())
}

impl Quota {
    fn resource(self, label: &str, period: i64, observed_at: i64) -> Option<Value> {
        if self.disabled == Some(true) {
            return None;
        }
        let fraction = self
            .remaining_fraction
            .filter(|value| value.is_finite())?
            .clamp(0., 1.);
        let reset = self
            .reset_time
            .as_deref()
            .and_then(parse_iso_epoch)
            .or_else(|| {
                let seconds = self.reset_in_seconds.filter(|&seconds| seconds >= 0)?;
                observed_at.checked_add(seconds)
            });
        let mut resource = json!({
            "label":label, "kind":"quota", "unit":"percent",
            "remaining_fraction":fraction, "window_seconds":period,
        });
        if let Some(reset) = reset {
            resource["resets_at"] = reset.into();
        }
        Some(resource)
    }
}

pub fn antigravity_statusline(
    input: &[u8],
    observed_at: i64,
) -> Result<Option<AntigravityReading>, ProviderError> {
    let status: StatusLine<'_> = serde_json::from_slice(input)
        .map_err(|_| ProviderError::Decode("invalid Antigravity status line"))?;
    if status.product != "antigravity" {
        return Err(ProviderError::Decode("expected an Antigravity status line"));
    }
    let Some(email) = status
        .email
        .as_deref()
        .map(str::trim)
        .filter(|email| !email.is_empty())
    else {
        return Ok(None);
    };
    let email = antigravity_account_email(email)?;
    let mut resources = Map::new();
    for (id, label, period) in WINDOWS {
        if let Some(value) = status.quota.get(id)
            && let Ok(quota) = serde_json::from_str::<Quota>(value.get())
            && let Some(reading) = quota.resource(label, period, observed_at)
        {
            resources.insert(id.to_owned(), reading);
        }
    }
    if resources.is_empty() {
        return Ok(None);
    }
    let mut snapshot = json!({"resources":resources});
    if let Some(tier) = status
        .plan_tier
        .as_deref()
        .map(str::trim)
        .filter(|tier| !tier.is_empty())
    {
        if tier.len() > 128 || tier.chars().any(char::is_control) {
            return Err(ProviderError::Decode("invalid Antigravity plan tier"));
        }
        snapshot["plan"] = match tier.to_ascii_lowercase().as_str() {
            "ultra" | "g1-ultra-tier" => "Google AI Ultra",
            "ultra lite" | "g1-ultra-lite-tier" => "Google AI Ultra Lite",
            "pro" | "g1-pro-tier" => "Google AI Pro",
            _ => tier,
        }
        .into();
    }
    Ok(Some(AntigravityReading { email, snapshot }))
}

pub fn antigravity_account_email(value: &str) -> Result<String, ProviderError> {
    let email = value.trim();
    if email.len() > 320
        || email.chars().any(|c| c.is_control() || c.is_whitespace())
        || !email.split_once('@').is_some_and(|(user, domain)| {
            !user.is_empty() && !domain.is_empty() && !domain.contains('@')
        })
    {
        return Err(ProviderError::Decode(
            "enter the Google account email used in Antigravity",
        ));
    }
    Ok(email.to_ascii_lowercase())
}
