// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use jackin_usage_provider_core::{Auth, Http, ProviderError, json_integer, json_number};
use serde_json::{Value, json};
use time::{PlainDateTime, macros::format_description};

pub(crate) const INVENTORY: &str =
    "https://api.z.ai/api/biz/customer-package-reset/list?targetType=PERSONAL";
pub(super) async fn read(
    http: &Http,
    key: &str,
    now: i64,
    previous: Option<&Value>,
    manual: bool,
) -> Result<Value, ProviderError> {
    let body = http
        .get(
            "https://api.z.ai/api/monitor/usage/quota/limit",
            Auth::Bearer(key),
            &[],
        )
        .await?;
    if body["success"] == false {
        return Err(if body["code"] == 401 {
            ProviderError::Unauthorized
        } else if body["msg"]
            .as_str()
            .is_some_and(|s| s.to_lowercase().contains("coding plan"))
        {
            ProviderError::Unsupported("no active GLM Coding Plan")
        } else {
            ProviderError::Decode("GLM quota read failed")
        });
    }
    let invalid = ProviderError::Decode("invalid GLM Coding Plan quota");
    let rows = body.get("data").unwrap_or(&body)["limits"]
        .as_array()
        .ok_or(invalid.clone())?;
    let mut resources = serde_json::Map::new();
    for row in rows {
        let name = row["type"].as_str().or_else(|| row["name"].as_str());
        let resets = json_integer(&row["nextResetTime"]).map(|n| n / 1000);
        let (id, used, limit, unit, seconds) = match name {
            Some("TIME_LIMIT") => (
                "webSearches",
                json_number(&row["currentValue"]),
                json_number(&row["usage"]),
                "searches",
                2_592_000,
            ),
            Some("TOKENS_LIMIT" | "CREDIT_LIMIT") => {
                let n = json_integer(&row["number"])
                    .filter(|n| *n > 0)
                    .ok_or(invalid.clone())?;
                let seconds = match json_integer(&row["unit"]).ok_or(invalid.clone())? {
                    3 => 3600,
                    4 => 86400,
                    5 => 2_592_000,
                    6 => 604800,
                    _ => continue,
                };
                let span = n.checked_mul(seconds).ok_or(invalid.clone())?;
                (
                    if span < 86400 { "session" } else { "weekly" },
                    json_number(&row["percentage"]).map(|n| n.clamp(0.0, 100.0)),
                    Some(100.0),
                    "percent",
                    span,
                )
            }
            _ => continue,
        };
        let used = used
            .filter(|n| n.is_finite() && *n >= 0.0)
            .ok_or(invalid.clone())?;
        let limit = limit
            .filter(|n| n.is_finite() && *n >= 0.0)
            .ok_or(invalid.clone())?;
        resources.insert(
            id.into(),
            json!({"kind":"consumption","unit":unit,"used":used,"limit":limit,
            "remaining":(limit-used).max(0.0),"window_seconds":seconds,"resets_at":resets}),
        );
    }
    let mut out = json!({"resources":resources});
    let previous = previous.unwrap_or(&Value::Null);
    let due = manual
        || previous["secondaryReadAt"]
            .as_i64()
            .is_none_or(|t| now.saturating_sub(t) >= 3600);
    if due {
        out["secondaryReadAt"] = json!(now);
        if let Ok(s) = http
            .get(
                "https://api.z.ai/api/biz/subscription/list",
                Auth::Bearer(key),
                &[],
            )
            .await
            && let Some(plan) = s
                .pointer("/data/0/productName")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
        {
            out["plan"] = json!(plan);
        }
        if let Ok(v) = http.get(INVENTORY, Auth::Bearer(key), &[]).await
            && let Ok(rows) = inventory(&v, now)
        {
            out["resets"] = json!(rows);
        }
    } else {
        for k in ["secondaryReadAt", "plan", "resets"] {
            if let Some(v) = previous.get(k) {
                out[k] = v.clone();
            }
        }
    }
    if let Some(rows) = out["resets"].as_array_mut() {
        rows.retain(|r| r["expiresAt"].as_i64().is_some_and(|at| at > now));
        mark_earliest(rows);
        out["resources"]["rateLimitResets"] =
            json!({"kind":"balance","unit":"resets","available":rows.len()});
    }
    Ok(out)
}

pub(crate) fn inventory(v: &Value, now: i64) -> Result<Vec<Value>, ProviderError> {
    const INVALID: ProviderError = ProviderError::Decode("invalid GLM reset inventory");
    if v["success"] == false {
        return Err(INVALID);
    }
    let data = v["data"].as_object().ok_or(INVALID)?;
    let mut rows = Vec::new();
    for (key, kind, wire) in [
        ("fiveHourResets", "session", "FIVE_HOUR"),
        ("weekResets", "weekly", "WEEK"),
    ] {
        let Some(records) = data.get(key) else {
            continue;
        };
        for item in records.as_array().ok_or(INVALID)? {
            if !item.is_object() {
                return Err(INVALID);
            }
            if item["available"] != true {
                continue;
            }
            let id = item["recordId"].as_i64().ok_or(INVALID)?;
            let date = item["expireTime"].as_str().ok_or(INVALID)?;
            let format = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
            let expiry = PlainDateTime::parse(date, format).map_err(|_| INVALID)?;
            if expiry.format(format).map_err(|_| INVALID)? != date {
                return Err(INVALID);
            }
            let expiry = expiry.assume_utc().unix_timestamp() - 8 * 3600;
            let grant = item["grantType"]
                .as_str()
                .filter(|g| !g.is_empty())
                .ok_or(INVALID)?;
            if expiry > now {
                rows.push(json!({"id":id,"kind":kind,"resetType":wire,"grantType":grant,"expiresAt":expiry,"remaining":1}));
            }
        }
    }
    rows.sort_by_key(|r| (r["expiresAt"].as_i64(), r["id"].as_i64()));
    mark_earliest(&mut rows);
    Ok(rows)
}

fn mark_earliest(rows: &mut [Value]) {
    let mut seen = std::collections::BTreeSet::new();
    for r in rows {
        r["usable"] = json!(seen.insert(r["kind"].as_str().unwrap_or_default().to_owned()));
    }
}
