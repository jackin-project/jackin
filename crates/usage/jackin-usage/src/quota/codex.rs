// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use super::{INVALID, Read, SESSION, WEEK, blank, consumption, history, percent, title};
use jackin_usage_provider_core::{Auth, JsonReply, ProviderError, json_epoch, json_number};
use serde_json::{Value, json};

pub(crate) const RESET_URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";

pub(super) async fn read(ctx: &Read<'_>) -> Result<Value, ProviderError> {
    let mut headers = Vec::new();
    if let Some(org) = ctx.org {
        headers.push(("ChatGPT-Account-Id", org));
    }
    let reply = ctx
        .http
        .get_reply(
            "https://chatgpt.com/backend-api/wham/usage",
            Auth::Bearer(ctx.access),
            &headers,
        )
        .await?;
    reply.ensure_success()?;
    reply.body.as_object().ok_or(INVALID)?;
    if !recognized_quota(&reply.body) {
        return Err(INVALID);
    }
    let mut out = blank();
    let body = &reply.body;
    windows(
        &mut out,
        &body["rate_limit"],
        ["session", "weekly"],
        [
            header_number(&reply, "x-codex-primary-used-percent"),
            header_number(&reply, "x-codex-secondary-used-percent"),
        ],
        ctx.now,
    );
    if let Some(spark) = body["additional_rate_limits"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|entry| {
            ["limit_name", "metered_feature"].iter().any(|key| {
                entry[key]
                    .as_str()
                    .is_some_and(|s| s.to_lowercase().contains("spark"))
            })
        })
    {
        windows(
            &mut out,
            &spark["rate_limit"],
            ["spark", "sparkWeekly"],
            [None, None],
            ctx.now,
        );
    }
    let credits = &body["credits"];
    let enabled = credits["has_credits"]
        .as_bool()
        .or_else(|| reply.header("x-codex-credits-has-credits")?.parse().ok());
    if enabled != Some(false)
        && let Some(credits) = json_number(&credits["balance"])
            .or_else(|| header_number(&reply, "x-codex-credits-balance"))
    {
        let n = credits.floor().max(0.0);
        out["resources"]["credits"] =
            json!({"kind":"balance","unit":"credits","available":n,"usd":n*0.04});
    }
    let individual = &body["spend_control"]["individual_limit"];
    if let (Some(used), Some(cap)) = (
        json_number(&individual["used"]).filter(|n| *n >= 0.0),
        json_number(&individual["limit"]).filter(|n| *n >= 0.0),
    ) {
        out["resources"]["creditLimit"] = consumption(
            used,
            "credits",
            Some(cap),
            json_epoch(&individual["reset_at"]),
            None,
        );
    }
    let raw = body["plan_type"].as_str().unwrap_or_default().trim();
    if !raw.is_empty() {
        out["plan"] = json!(match raw.to_ascii_lowercase().as_str() {
            "prolite" => "Pro 100".into(),
            "pro" => "Pro 200".into(),
            "promax" => "Pro 500".into(),
            "self_serve_business_prolite" => "Business Premium".into(),
            _ => title(raw, '_'),
        });
    }
    ctx.restore(&mut out, &["resetDetail", "resetsReadAt"]);
    if ctx.due("resetsReadAt", 3600, 0) {
        let mut extra = headers.clone();
        extra.extend([("OpenAI-Beta", "codex-1"), ("originator", "Codex Desktop")]);
        out["resetDetail"] = ctx
            .get(RESET_URL, &extra)
            .await
            .ok()
            .and_then(|details| inventory(&details))
            .unwrap_or(Value::Null);
        out["resetsReadAt"] = json!(ctx.now);
    }
    let embedded = inventory(&body["rate_limit_reset_credits"]);
    let detailed = &out["resetDetail"];
    let chosen = match &embedded {
        Some(entry) if entry["count"] != detailed["count"] => entry,
        Some(entry) if !detailed.is_object() => entry,
        _ => detailed,
    };
    if let Some(count) = chosen["count"].as_u64() {
        let rows = if count == 0 {
            json!([])
        } else {
            chosen["items"].clone()
        };
        out["resources"]["rateLimitResets"] =
            json!({"kind":"balance","unit":"resets","available":count});
        out["resets"] = rows;
    }
    history::read(ctx, &headers, raw, &mut out).await;
    Ok(out)
}

fn recognized_quota(body: &Value) -> bool {
    body["rate_limit"]["primary_window"].is_object()
        || body["rate_limit"]["secondary_window"].is_object()
        || body["credits"].is_object()
        || body["additional_rate_limits"]
            .as_array()
            .is_some_and(|limits| !limits.is_empty())
        || body["spend_control"]["individual_limit"].is_object()
        || body["rate_limit_reset_credits"].is_object()
        || body["plan_type"]
            .as_str()
            .is_some_and(|plan| !plan.trim().is_empty())
}

fn header_number(reply: &JsonReply, key: &str) -> Option<f64> {
    reply
        .header(key)?
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
}

fn windows(out: &mut Value, block: &Value, ids: [&str; 2], percents: [Option<f64>; 2], now: i64) {
    let slots = [&block["primary_window"], &block["secondary_window"]];
    for (i, period) in [SESSION, WEEK].into_iter().enumerate() {
        let duration = |w: &Value| json_number(&w["limit_window_seconds"]).map(|n| n as i64);
        let exact = slots.iter().position(|w| duration(w) == Some(period));
        let Some(j) =
            exact.or_else(|| (!matches!(duration(slots[i]), Some(SESSION | WEEK))).then_some(i))
        else {
            continue;
        };
        let window = slots[j];
        if let Some(used) = json_number(&window["used_percent"]).or(percents[j]) {
            let at = json_epoch(&window["reset_at"])
                .or_else(|| now.checked_add(json_number(&window["reset_after_seconds"])? as i64));
            out["resources"][ids[i]] = percent(used, at, duration(window).unwrap_or(period));
        }
    }
}

pub(crate) fn inventory(v: &Value) -> Option<Value> {
    let count = json_number(&v["available_count"])
        .filter(|n| *n >= 0.0)?
        .floor() as i64;
    let count = u64::try_from(count).ok()?;
    let rows=v["credits"].as_array().into_iter().flatten().filter(|c|c["status"].as_str().is_none_or(|s|s=="available")).filter_map(|c|{
        let at=json_epoch(&c["expires_at"])?;
        Some(json!({"kind":"full","id":c["id"].as_str(),"expiresAt":at,"usable":true,"remaining":1}))
    }).collect::<Vec<_>>();
    Some(json!({"count":count,"items":rows}))
}
