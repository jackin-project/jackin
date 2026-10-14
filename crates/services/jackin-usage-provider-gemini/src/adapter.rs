// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use std::path::PathBuf;

use jackin_usage_provider_core::{
    UsageSurface, UsageViewInput, bucket, compact_count, env_value, epoch_seconds_from_maybe_ms,
    home_path, humanize_plan_label, json_number, parse_iso_epoch, timed_bucket, usage_view,
};

pub const GEMINI_CONSUMER_OAUTH_END: i64 = 1_781_740_800;

pub fn gemini_consumer_oauth_retired(now: i64) -> bool {
    now >= GEMINI_CONSUMER_OAUTH_END
}

pub fn gemini_oauth_creds_path() -> PathBuf {
    let dir = env_value("GEMINI_CLI_HOME").map_or_else(
        || home_path(".gemini"),
        |home| PathBuf::from(home).join(".gemini"),
    );
    dir.join("oauth_creds.json")
}

pub fn gemini_credential_presence() -> (bool, bool) {
    let oauth = gemini_oauth_creds_path().is_file();
    let api_key = env_value("GEMINI_API_KEY")
        .or_else(|| env_value("GOOGLE_API_KEY"))
        .is_some();
    (oauth, api_key)
}

pub fn gemini_credential_origin(oauth: bool, api_key: bool) -> String {
    if oauth && api_key {
        "OAuth · oauth_creds.json + API key env".to_owned()
    } else if oauth {
        "OAuth · ~/.gemini/oauth_creds.json".to_owned()
    } else if api_key {
        "API key · env GEMINI_API_KEY".to_owned()
    } else {
        "needs Gemini auth".to_owned()
    }
}

#[derive(Debug, Clone, Default)]
pub struct GeminiEntitlement {
    pub tier_name: Option<String>,
    pub plan_name: Option<String>,
    pub project: Option<String>,

    pub consumer_unsupported: bool,
}

pub fn parse_gemini_entitlement(value: &serde_json::Value) -> GeminiEntitlement {
    let tier_name = ["userTier", "currentTier", "paidTier", "tier"]
        .into_iter()
        .filter_map(|key| value.get(key))
        .filter_map(|tier| tier.get("name").or_else(|| tier.get("id")))
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .find(|name| !name.is_empty())
        .map(humanize_plan_label);
    let plan_name = value
        .get("planInfo")
        .and_then(|info| info.get("planName"))
        .or_else(|| value.get("planName"))
        .or_else(|| value.get("plan"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(humanize_plan_label);
    let project = [
        "cloudaicompanionProject",
        "projectId",
        "project_id",
        "project",
    ]
    .into_iter()
    .filter_map(|key| value.get(key))
    .filter_map(|node| {
        node.as_str().or_else(|| {
            node.get("projectId")
                .or_else(|| node.get("name"))
                .and_then(serde_json::Value::as_str)
        })
    })
    .map(str::trim)
    .find(|project| !project.is_empty())
    .map(str::to_owned);
    GeminiEntitlement {
        tier_name,
        plan_name,
        project,
        consumer_unsupported: gemini_consumer_unsupported(value),
    }
}

fn gemini_consumer_unsupported(value: &serde_json::Value) -> bool {
    if let Some(object) = value.as_object() {
        for (key, flag) in object {
            let lower = key.to_ascii_lowercase();
            if (lower.contains("unsupported")
                || lower.contains("deprecated")
                || lower.contains("retired"))
                && flag.as_bool() == Some(true)
            {
                return true;
            }
        }
    }
    ["userTier", "currentTier", "paidTier", "tier"]
        .into_iter()
        .filter_map(|key| value.get(key))
        .filter_map(|tier| tier.get("id").or_else(|| tier.get("name")))
        .filter_map(serde_json::Value::as_str)
        .any(|tier| tier.to_ascii_lowercase().contains("individual"))
}

pub fn gemini_migration_action(entitlement: Option<&GeminiEntitlement>) -> Option<String> {
    const ACTION: &str = "Consumer Google OAuth ended 2026-06-18; reconnect Gemini CLI with Code Assist Standard/Enterprise";
    entitlement
        .is_some_and(|entitlement| entitlement.consumer_unsupported)
        .then(|| ACTION.to_owned())
}

#[derive(Debug, Clone)]
pub struct GeminiProjectQuota {
    pub model: Option<String>,
    pub kind: String,
    pub limit: Option<f64>,
    pub used: Option<f64>,
    pub reset_at: Option<i64>,
}

pub fn parse_gemini_project_quotas(value: &serde_json::Value) -> Vec<GeminiProjectQuota> {
    let entries = ["quotas", "limits", "quota"]
        .into_iter()
        .find_map(|key| value.get(key).and_then(serde_json::Value::as_array));
    let Some(entries) = entries else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let kind = ["metric", "kind", "name", "limitName", "type"]
                .into_iter()
                .filter_map(|key| entry.get(key).and_then(serde_json::Value::as_str))
                .map(str::trim)
                .find(|kind| !kind.is_empty())?
                .to_owned();
            let non_negative = |keys: &[&str]| {
                keys.iter()
                    .filter_map(|key| entry.get(*key).and_then(json_number))
                    .find(|value| value.is_finite() && *value >= 0.0)
            };
            let limit = non_negative(&["limit", "max", "quota", "allowance"]);
            let used = non_negative(&["usage", "used", "consumed", "current"]).or_else(|| {
                let remaining = non_negative(&["remaining", "left"])?;
                limit.map(|limit| (limit - remaining).max(0.0))
            });
            let reset_at = ["resetTime", "reset_time", "resetsAt", "reset_at", "reset"]
                .into_iter()
                .filter_map(|key| entry.get(key))
                .find_map(|node| {
                    node.as_str()
                        .and_then(|text| parse_iso_epoch(text.trim()))
                        .or_else(|| {
                            json_number(node).map(|n| epoch_seconds_from_maybe_ms(n.floor() as i64))
                        })
                });
            let model = ["model", "modelName", "model_name"]
                .into_iter()
                .filter_map(|key| entry.get(key).and_then(serde_json::Value::as_str))
                .map(str::trim)
                .find(|model| !model.is_empty())
                .map(str::to_owned);
            Some(GeminiProjectQuota {
                model,
                kind,
                limit,
                used,
                reset_at,
            })
        })
        .collect()
}

pub fn gemini_quota_buckets(quotas: &[GeminiProjectQuota], now: i64) -> Vec<QuotaBucketView> {
    quotas
        .iter()
        .map(|quota| gemini_quota_bucket(quota, now))
        .collect()
}

fn gemini_quota_bucket(quota: &GeminiProjectQuota, now: i64) -> QuotaBucketView {
    let kind = humanize_plan_label(&quota.kind);
    let label = quota
        .model
        .as_deref()
        .map_or_else(|| kind.clone(), |model| format!("{model} · {kind}"));
    let count_label = |value: f64| {
        if value.fract() == 0.0 && value >= 0.0 && value <= u64::MAX as f64 {
            #[expect(clippy::cast_sign_loss, reason = "filtered non-negative above")]
            compact_count(value as u64)
        } else {
            format!("{value}")
        }
    };
    let remaining = match (quota.limit, quota.used) {
        (Some(limit), Some(used)) if limit > 0.0 => {
            #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0")]
            {
                Some(((limit - used).clamp(0.0, limit) / limit * 100.0).round() as u8)
            }
        }
        _ => None,
    };
    timed_bucket(
        &label,
        quota.used.map(count_label),
        quota.limit.map(count_label),
        remaining,
        quota.reset_at,
        now,
        None,
        UsageSnapshotStatus::Fresh,
    )
}

pub fn gemini_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    let (has_oauth, has_api_key) = gemini_credential_presence();
    let origin = gemini_credential_origin(has_oauth, has_api_key);
    gemini_snapshot_with_presence(agent, provider, has_oauth, has_api_key, &origin, now)
}

pub fn gemini_snapshot_with_presence(
    agent: &str,
    provider: Option<&str>,
    has_oauth: bool,
    has_api_key: bool,
    credential_origin: &str,
    now: i64,
) -> FocusedUsageView {
    let migration = gemini_migration_action(None);
    let (status, message) = if !has_oauth && !has_api_key {
        (
            UsageSnapshotStatus::NeedsSecret,
            "Gemini auth not available to Capsule",
        )
    } else if let Some(action) = migration.as_deref() {
        (UsageSnapshotStatus::Unsupported, action)
    } else {
        (
            UsageSnapshotStatus::Unsupported,
            "Gemini project quota needs authorized reporting scope",
        )
    };
    usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Google")),
        surface: UsageSurface::Google,
        account_label: String::new(),
        username: None,
        plan_label: None,
        credential_origin: Some(credential_origin.to_owned()),
        buckets: vec![bucket(
            "Eligibility",
            None,
            None,
            None,
            None,
            Some(message),
            status,
        )],
        status,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(message.to_owned()),
    })
}
