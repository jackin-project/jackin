// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use jackin_usage_provider_core::{ProviderError, json_epoch, json_number};
use serde_json::{Value, json};

use super::{INVALID, Read, SESSION, WEEK, blank, consumption, percent};

pub(crate) const HEADERS: [(&str, &str); 3] = [
    ("anthropic-beta", "oauth-2025-04-20"),
    ("Content-Type", "application/json"),
    ("User-Agent", "claude-cli/2.1.294 (external, cli)"),
];
pub(crate) const GRANTS: &str =
    "https://api.anthropic.com/api/oauth/usage?cedar_ember=1&skip_spend=1";

pub(super) async fn read(ctx: &Read<'_>) -> Result<Value, ProviderError> {
    let body = ctx
        .get("https://api.anthropic.com/api/oauth/usage", &HEADERS)
        .await?;
    body.as_object().ok_or(INVALID)?;
    let mut out = blank();
    for (key, id, period) in [
        ("five_hour", "session", SESSION),
        ("seven_day", "weekly", WEEK),
        ("seven_day_sonnet", "sonnet", WEEK),
    ] {
        let window = &body[key];
        if let Some(used) = json_number(&window["utilization"]) {
            out["resources"][id] = percent(used, json_epoch(&window["resets_at"]), period);
        }
    }
    if let Some(fable) = body["limits"].as_array().into_iter().flatten().find(|w| {
        w["kind"] == "weekly_scoped"
            && w.pointer("/scope/model/display_name")
                .and_then(Value::as_str)
                == Some("Fable")
    }) && let Some(used) = json_number(&fable["percent"])
    {
        out["resources"]["fable"] = percent(used, json_epoch(&fable["resets_at"]), WEEK);
    }
    let extra = &body["extra_usage"];
    if extra["is_enabled"] == true {
        let unit = extra["currency"]
            .as_str()
            .unwrap_or("USD")
            .to_ascii_lowercase();
        let scale = 10_f64.powf(json_number(&extra["decimal_places"]).unwrap_or(2.0));
        if !scale.is_finite() || scale < 1.0 {
            return Err(INVALID);
        }
        let used = json_number(&extra["used_credits"]).unwrap_or(0.0).round() / scale;
        let cap = json_number(&extra["monthly_limit"])
            .filter(|n| *n > 0.0)
            .map(|n| n.round() / scale);
        out["resources"]["extraUsage"] = consumption(used, &unit, cap, None, None);
    }
    ctx.restore(&mut out, &["resets", "resetsReadAt", "nextGrantId"]);
    let mut held = false;
    if ctx.due("resetsReadAt", 3600, 900) {
        match ctx.get(GRANTS, &HEADERS).await {
            Ok(v) => {
                if let Some((rows, next)) = inventory(&v["cedar_ember"]) {
                    out["resets"] = Value::Array(rows);
                    out["nextGrantId"] = next;
                } else if let Some(obj) = out.as_object_mut() {
                    obj.remove("resets");
                    obj.remove("nextGrantId");
                }
                out["resetsReadAt"] = json!(ctx.now);
            }
            Err(ProviderError::Network(_)) => {}
            Err(ProviderError::RateLimited { .. }) => {
                held = true;
                out["resetsReadAt"] = json!(ctx.now);
            }
            Err(_) => out["resetsReadAt"] = json!(ctx.now),
        }
    }
    if let Some(rows) = out["resets"].as_array_mut() {
        rows.retain(|r| r["expiresAt"].as_i64().is_none_or(|at| at > ctx.now));
        let count = rows
            .iter()
            .try_fold(0u64, |sum, r| sum.checked_add(r["remaining"].as_u64()?))
            .ok_or(INVALID)?;
        out["resources"]["rateLimitResets"] =
            json!({"kind":"balance","unit":"resets","available":count});
    }
    if !held && ctx.due("planReadAt", 3600, 0) {
        if let Ok(profile) = ctx
            .get("https://api.anthropic.com/api/oauth/profile", &[HEADERS[0]])
            .await
            && let Some(name) = plan(&profile["organization"])
        {
            out["plan"] = json!(name);
        }
        out["planReadAt"] = json!(ctx.now);
    } else {
        ctx.restore(&mut out, &["plan", "planReadAt"]);
    }
    if out["plan"]
        .as_str()
        .is_some_and(|s| s.starts_with("Enterprise"))
        && out["resources"]["extraUsage"].is_object()
    {
        out["resources"]["extraUsage"]["label"] = json!("Spend");
    }
    Ok(out)
}

fn plan(org: &Value) -> Option<String> {
    let raw = org["organization_type"]
        .as_str()?
        .trim()
        .trim_start_matches("claude_");
    if raw.is_empty() {
        return None;
    }
    let base = super::title(raw, ' ');
    let tier = org["rate_limit_tier"].as_str().unwrap_or_default();
    if base == "Ultra" || (base == "Max" && (tier.contains("ultra") || tier.contains("20x"))) {
        return Some("Ultra".into());
    }
    if base != "Max"
        && let Some(multiplier) = tier
            .split(|c: char| !(c.is_ascii_digit() || c == 'x'))
            .find(|s| {
                s.strip_suffix('x')
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            })
    {
        return Some(format!("{base} {multiplier}"));
    }
    Some(base)
}

pub(crate) fn inventory(block: &Value) -> Option<(Vec<Value>, Value)> {
    let obj = block.as_object()?;
    let mut rows = Vec::new();
    if block["eligible"] == true {
        for grant in obj
            .get("grants")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = grant["id"].as_str().filter(|s| !s.is_empty())?;
            let remaining = json_number(&grant["resets_left"])?.floor().max(0.0);
            if remaining < 1.0 {
                continue;
            }
            let clears: Vec<_> = grant["clears"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let kind = if grant["gives_back_weekly"] == true {
                "fable"
            } else {
                match grant["type"].as_str() {
                    Some(kind @ ("session" | "weekly" | "full")) => kind,
                    _ => match (
                        clears.contains(&"five_hour"),
                        clears.iter().any(|s| *s != "five_hour"),
                    ) {
                        (true, false) => "session",
                        (false, true) => "weekly",
                        _ => "full",
                    },
                }
            };
            let mut row = json!({"id":id,"kind":kind,"remaining":u64::try_from(remaining as i64).ok()?,"usable":grant["usable_now"]==true && grant["paused"]!=true,"clears":clears,"givesBackWeekly":grant["gives_back_weekly"]==true});
            if let Some(t) = json_epoch(&grant["ends_at"]) {
                row["expiresAt"] = json!(t);
            }
            rows.push(row);
        }
    }
    Some((
        rows,
        block["next_grant_id"]
            .as_str()
            .map_or(Value::Null, Value::from),
    ))
}
