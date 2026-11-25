// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

pub(crate) mod claude;
pub(crate) mod codex;
mod history;

use jackin_core::{Secret, Timestamp};
use jackin_usage_provider_core::{Auth, Http, ProviderError, json_epoch, json_number};
use serde_json::{Value, json};

use crate::accounts::UsageProvider;

const SESSION: i64 = 18_000;
const WEEK: i64 = 604_800;
const MONTH: i64 = 2_592_000;
const INVALID: ProviderError = ProviderError::Decode("invalid subscription usage response");

pub async fn read(
    provider: UsageProvider,
    http: &Http,
    access: &Secret,
    organization: Option<&str>,
    now: Timestamp,
    previous: Option<&Value>,
) -> Result<Value, ProviderError> {
    read_with_policy(provider, http, access, organization, now, previous, false).await
}

pub async fn read_with_policy(
    provider: UsageProvider,
    http: &Http,
    access: &Secret,
    organization: Option<&str>,
    now: Timestamp,
    previous: Option<&Value>,
    manual: bool,
) -> Result<Value, ProviderError> {
    let ctx = Read {
        http,
        access: access.expose(),
        org: organization,
        now: now.0,
        prior: previous.unwrap_or(&Value::Null),
        manual,
    };
    match provider {
        UsageProvider::Claude => claude::read(&ctx).await,
        UsageProvider::ChatGpt => codex::read(&ctx).await,
        UsageProvider::Muse => muse(&ctx).await,
        UsageProvider::OpenCode => opencode(&ctx).await,
        UsageProvider::Grok => grok(&ctx).await,
        _ => Err(ProviderError::Unsupported(
            "this account does not use a subscription sign-in",
        )),
    }
}

struct Read<'a> {
    http: &'a Http,
    access: &'a str,
    org: Option<&'a str>,
    now: i64,
    prior: &'a Value,
    manual: bool,
}

impl Read<'_> {
    fn due(&self, key: &str, interval: i64, manual_interval: i64) -> bool {
        self.prior[key].as_i64().is_none_or(|t| {
            self.now.saturating_sub(t)
                >= if self.manual {
                    manual_interval
                } else {
                    interval
                }
        })
    }

    async fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<Value, ProviderError> {
        self.http.get(url, Auth::Bearer(self.access), headers).await
    }

    fn restore(&self, result: &mut Value, fields: &[&str]) {
        for key in fields {
            if let Some(v) = self.prior.get(key) {
                result[key] = v.clone();
            }
        }
    }
}

fn consumption(
    used: f64,
    unit: &str,
    limit: Option<f64>,
    reset: Option<i64>,
    period: Option<i64>,
) -> Value {
    let mut v = json!({"kind":"consumption","unit":unit,"used":used});
    if let Some(limit) = limit {
        v["limit"] = json!(limit);
        v["remaining"] = json!((limit - used).max(0.0));
        if limit > 0.0 {
            v["utilization"] = json!((used / limit).clamp(0.0, 1.0));
        }
    }
    if let Some(reset) = reset {
        v["resetsAt"] = json!(reset);
    }
    if let Some(period) = period {
        v["windowSeconds"] = json!(period);
    }
    v
}

fn percent(used: f64, reset: Option<i64>, period: i64) -> Value {
    consumption(
        used.clamp(0.0, 100.0),
        "percent",
        Some(100.0),
        reset,
        Some(period),
    )
}

fn blank() -> Value {
    json!({"resources":{}})
}

async fn muse(ctx: &Read<'_>) -> Result<Value, ProviderError> {
    let body = ctx
        .http
        .post(
            "https://api.meta.ai/muse-code/key",
            Auth::Bearer(ctx.access),
            &[("x-api-version", "1.0.0")],
            &json!({"onboard":false}),
        )
        .await?;
    let mut out = blank();
    if !body["is_subs_active"].as_bool().ok_or(INVALID)? {
        out["notice"] = json!("No Muse Code subscription on this account.");
        return Ok(out);
    }
    if let Some(plan) = body["subs_tier_name"].as_str() {
        out["plan"] = json!(plan);
    }
    let usage = &body["subs_usage"];
    if !usage.is_null() && !usage.is_object() {
        return Err(INVALID);
    }
    for (key, field, period) in [("session", "window", SESSION), ("weekly", "weekly", WEEK)] {
        let window = &usage[field];
        let previous = &ctx.prior["resources"][key];
        let reading = if !window.is_null() {
            percent(
                json_number(&window["used_percent"]).ok_or(INVALID)?,
                json_epoch(&window["resets_at"]),
                period,
            )
        } else if usage.is_null()
            && previous.is_object()
            && previous["resetsAt"].as_i64().is_none_or(|t| t > ctx.now)
        {
            previous.clone()
        } else {
            percent(0.0, None, period)
        };
        out["resources"][key] = reading;
    }
    Ok(out)
}

async fn opencode(ctx: &Read<'_>) -> Result<Value, ProviderError> {
    let org = ctx
        .org
        .filter(|v| !v.is_empty())
        .ok_or(ProviderError::Decode("missing OpenCode workspace"))?;
    let reply = ctx
        .http
        .get_reply(
            "https://opencode.ai/zen/go/v1/usage",
            Auth::Bearer(ctx.access),
            &[("x-opencode-org-id", org)],
        )
        .await?;
    if reply.status == 403
        && reply.body.pointer("/error/type").and_then(Value::as_str) == Some("EntitlementError")
    {
        return Ok(json!({"resources":{},"notice":"No OpenCode Go subscription on this account."}));
    }
    reply.ensure_success()?;
    let captured = reply
        .header("date")
        .and_then(|s| chrono::DateTime::parse_from_rfc2822(s).ok())
        .map_or(ctx.now, |d| d.timestamp());
    let mut out = blank();
    for (id, key, period) in [
        ("session", "rolling", SESSION),
        ("weekly", "weekly", WEEK),
        ("monthly", "monthly", MONTH),
    ] {
        let window = &reply.body["usage"][key];
        let used = json_number(&window["percent"])
            .ok_or(INVALID)?
            .clamp(0.0, 100.0);
        let reset = json_epoch(&window["resetsAt"]).ok_or(INVALID)?;
        let placeholder =
            id == "session" && used == 0.0 && reset.abs_diff(captured.saturating_add(period)) <= 2;
        out["resources"][id] = percent(used, (!placeholder).then_some(reset), period);
    }
    if ctx.due("planReadAt", 3600, 0) {
        if let Ok(plan) = ctx
            .get(
                "https://opencode.ai/console/api/go/status",
                &[("x-org-id", org)],
            )
            .await
        {
            let name = match plan["product"].as_str() {
                Some("go") => Some("Go"),
                Some("go-plus") => Some("Go Plus"),
                Some("go-teams") => Some("Go Teams"),
                _ => None,
            };
            if let Some(name) = name {
                out["plan"] = json!(name);
            }
        }
        out["planReadAt"] = json!(ctx.now);
    } else {
        ctx.restore(&mut out, &["plan", "planReadAt"]);
    }
    Ok(out)
}

async fn grok(ctx: &Read<'_>) -> Result<Value, ProviderError> {
    let headers = [("X-XAI-Token-Auth", "xai-grok-cli")];
    let reply = ctx
        .http
        .get_reply(
            "https://cli-chat-proxy.grok.com/v1/billing?format=credits",
            Auth::Bearer(ctx.access),
            &headers,
        )
        .await?;
    let mut out = blank();
    if reply.status == 412
        && reply.body["error"]
            .as_str()
            .is_some_and(|s| s.to_ascii_lowercase().contains("no personal team"))
    {
        out["notice"] = json!("Grok team accounts have no personal quota.");
    } else {
        reply.ensure_success()?;
        let config = &reply.body["config"];
        let period = &config["currentPeriod"];
        let start = json_epoch(&period["start"]).ok_or(INVALID)?;
        let end = json_epoch(&period["end"])
            .filter(|t| *t > start)
            .ok_or(INVALID)?;
        let used = config
            .get("creditUsagePercent")
            .map(|v| json_number(v).ok_or(INVALID))
            .transpose()?
            .unwrap_or(0.0);
        let kind = period["type"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or(INVALID)?;
        if kind == "USAGE_PERIOD_TYPE_WEEKLY" {
            out["resources"]["weekly"] = percent(used, Some(end), end - start);
        }
        if let Some(cap) = config.get("onDemandCap") {
            let cap = cap
                .as_object()
                .ok_or(INVALID)?
                .get("val")
                .map(|v| json_number(v).ok_or(INVALID))
                .transpose()?
                .unwrap_or(0.0);
            if cap > 0.0 {
                out["extraUsageCap"] = json!({"unit":"usd","limit":cap/100.0});
            }
        }
    }
    if ctx.due("planReadAt", 3600, 0) {
        if let Ok(settings) = ctx
            .get("https://cli-chat-proxy.grok.com/v1/settings", &headers)
            .await
            && let Some(plan) = settings["subscription_tier_display"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
        {
            out["plan"] = json!(plan);
        }
        out["planReadAt"] = json!(ctx.now);
    } else {
        ctx.restore(&mut out, &["plan", "planReadAt"]);
    }
    Ok(out)
}

fn title(raw: &str, sep: char) -> String {
    raw.split(sep)
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut chars = s.chars();
            chars.next().map_or(String::new(), |c| {
                format!("{}{}", c.to_uppercase(), chars.as_str().to_lowercase())
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}
