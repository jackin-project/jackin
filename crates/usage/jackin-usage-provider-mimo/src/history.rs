// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{Auth, Http, ProviderError};
use serde_json::{Map, Value, json};
use time::{Date, macros::format_description};

#[derive(Clone, Copy, Debug)]
pub enum DailySource {
    Consumption,
    Bills,
}

impl DailySource {
    fn path(self) -> &'static str {
        match self {
            Self::Consumption => "usage",
            Self::Bills => "billing",
        }
    }
}

const INVALID: ProviderError = ProviderError::Decode("invalid MiMo daily record");

pub async fn fetch_daily(
    http: &Http,
    key: &str,
    source: DailySource,
    start: Date,
    end: Date,
) -> Result<Vec<Value>, ProviderError> {
    if !key.starts_with("sk-") || key.len() == 3 {
        return Err(ProviderError::Unsupported(
            "MiMo daily usage requires a regular sk- key",
        ));
    }
    if !(0..31).contains(&(end - start).whole_days()) {
        return Err(ProviderError::Unsupported(
            "select up to 31 inclusive UTC days",
        ));
    }
    let url = format!(
        "https://api.xiaomimimo.com/v1/platform/{}/daily?start_date={start}&end_date={end}",
        source.path()
    );
    let reply = http.get(&url, Auth::Bearer(key), &[]).await?;
    if reply["object"] != "list" {
        return Err(INVALID);
    }
    reply["data"]
        .as_array()
        .ok_or(INVALID)?
        .iter()
        .map(|v| record(source, start, end, v))
        .collect()
}

fn record(source: DailySource, start: Date, end: Date, v: &Value) -> Result<Value, ProviderError> {
    let date = v["date"]
        .as_str()
        .and_then(|s| {
            Date::parse(
                s,
                format_description!("[year]-[month padding:none]-[day padding:none]"),
            )
            .ok()
        })
        .filter(|d| (start..=end).contains(d))
        .ok_or(INVALID)?;
    let currency = v["currency"]
        .as_str()
        .filter(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase()))
        .ok_or(INVALID)?;
    let mut row = Map::from_iter([
        ("date".into(), json!(date.to_string())),
        ("currency".into(), json!(currency)),
    ]);
    match source {
        DailySource::Consumption => {
            let kind = v["resource_type"]
                .as_str()
                .filter(|k| matches!(*k, "model" | "plugin"))
                .ok_or(INVALID)?;
            let name = v["name"]
                .as_str()
                .filter(|n| !n.trim().is_empty())
                .ok_or(INVALID)?;
            row.insert("resource_type".into(), json!(kind));
            row.insert("name".into(), json!(name));
            row.insert(
                "request_count".into(),
                json!(v["request_count"].as_u64().ok_or(INVALID)?),
            );
            money(&mut row, v, "consumed_amount", 6)?;
            if kind == "model" {
                for field in [
                    "input_cache_hit_amount",
                    "input_cache_miss_amount",
                    "output_amount",
                ] {
                    if !v[field].is_null() {
                        money(&mut row, v, field, 6)?;
                    }
                }
                for field in [
                    "total_tokens",
                    "input_cache_hit_tokens",
                    "input_cache_miss_tokens",
                    "output_tokens",
                    "audio_transcription_seconds",
                ] {
                    if !v[field].is_null() {
                        row.insert(field.into(), json!(v[field].as_u64().ok_or(INVALID)?));
                    }
                }
                if let Some(kind) = v["inference_type"].as_str() {
                    row.insert("inference_type".into(), json!(kind));
                }
            }
        }
        DailySource::Bills => {
            for field in [
                "original_amount",
                "discount_amount",
                "gift_amount",
                "final_paid_amount",
            ] {
                money(&mut row, v, field, 2)?;
            }
        }
    }
    Ok(Value::Object(row))
}

fn money(
    row: &mut Map<String, Value>,
    v: &Value,
    field: &str,
    precision: usize,
) -> Result<(), ProviderError> {
    let n = super::balance::fixed(v[field].as_str().ok_or(INVALID)?, precision)?;
    row.insert(field.into(), json!(n));
    Ok(())
}
