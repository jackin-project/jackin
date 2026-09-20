// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Gemini CLI` eligibility + project-quota snapshot.
//!
//! Gemini CLI is a separate account from Antigravity even though both ride
//! Google OAuth. Eligibility changed: consumer Google OAuth access through
//! Gemini CLI ended 2026-06-18 (targeted deprecation notice); Standard and
//! Enterprise remain supported. A discovered old consumer login therefore gets
//! a concrete reconnect/migration action — a 403 on a managed route must never
//! be blind-mapped to that migration.
//!
//! Gemini/Vertex API-key billing is separate again: quotas are project-scoped
//! RPM/TPM/daily/model limits, keys in one project share quota, and
//! `usageMetadata` is request consumption, never remaining allowance. This
//! module parses project-quota responses into count buckets without inventing
//! denominators, and classifies credential routes so the broker never attaches
//! a project observation to the wrong billing identity.

#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;

/// 2026-06-18T00:00:00Z: consumer Google OAuth through Gemini CLI ended.
/// (`parse_iso_epoch` cross-checks this constant in tests.)
pub(crate) const GEMINI_CONSUMER_OAUTH_END: i64 = 1_781_740_800;

/// True once consumer OAuth is retired (at/after the deprecation instant).
pub(crate) fn gemini_consumer_oauth_retired(now: i64) -> bool {
    now >= GEMINI_CONSUMER_OAUTH_END
}

/// `GEMINI_CLI_HOME` is the parent to which `.gemini` is appended.
pub(crate) fn gemini_oauth_creds_path() -> PathBuf {
    let dir = env_value("GEMINI_CLI_HOME").map_or_else(
        || home_path(".gemini"),
        |home| PathBuf::from(home).join(".gemini"),
    );
    dir.join("oauth_creds.json")
}

/// Credential presence: `(oauth_creds_file, api_key_env)`. Presence only —
/// secret values are never read here. The `GOOGLE_API_KEY` alias is the
/// S1-recorded decision (`accounts/discovery.rs` recognizes it as the same
/// key), not a lane invention.
pub(crate) fn gemini_credential_presence() -> (bool, bool) {
    let oauth = gemini_oauth_creds_path().is_file();
    let api_key = env_value("GEMINI_API_KEY")
        .or_else(|| env_value("GOOGLE_API_KEY"))
        .is_some();
    (oauth, api_key)
}

pub(crate) fn gemini_credential_origin(oauth: bool, api_key: bool) -> String {
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

/// Parsed entitlement response: tier/plan identity, project scope, and any
/// consumer-unsupported markers the server sent.
#[derive(Debug, Clone, Default)]
pub(crate) struct GeminiEntitlement {
    pub(crate) tier_name: Option<String>,
    pub(crate) plan_name: Option<String>,
    pub(crate) project: Option<String>,
    /// True when the response explicitly marks consumer/individual access
    /// unsupported, deprecated, or retired.
    pub(crate) consumer_unsupported: bool,
}

/// Parse an entitlement response. Tier names come from the Google tier object
/// (`userTier`/`currentTier`/`paidTier`); the inherited `planInfo.planName` is
/// kept only as a plan label, never as tier evidence.
pub(crate) fn parse_gemini_entitlement(value: &serde_json::Value) -> GeminiEntitlement {
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

/// True when the response explicitly flags consumer access unsupported. Only
/// explicit server flags (or an `individual`-family tier id, the deprecated
/// product family) count — a missing tier is unknown, never consumer.
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

/// Concrete reconnect action for a retired consumer login, or `None` when the
/// entitlement gives no migration signal. Only an explicit server
/// `consumer_unsupported` flag triggers the targeted notice: a bare OAuth
/// credential past the retirement instant may be a managed
/// Standard/Enterprise login that is already eligible, so credential
/// presence + wall-clock alone must never classify it as a to-be-migrated
/// consumer login.
pub(crate) fn gemini_migration_action(entitlement: Option<&GeminiEntitlement>) -> Option<String> {
    const ACTION: &str = "Consumer Google OAuth ended 2026-06-18; reconnect Gemini CLI with Code Assist Standard/Enterprise";
    entitlement
        .is_some_and(|entitlement| entitlement.consumer_unsupported)
        .then(|| ACTION.to_owned())
}

/// True only for a 403-class error on a consumer-route credential past the
/// retirement instant. Every other 403 (managed route, API key, pre-retirement)
/// is an auth failure, never a migration — no blind mapping.
pub(crate) fn gemini_error_needs_migration(error: &str, consumer_route: bool, now: i64) -> bool {
    if !consumer_route || !gemini_consumer_oauth_retired(now) {
        return false;
    }
    let lower = error.to_ascii_lowercase();
    lower.contains("403") || lower.contains("forbidden")
}

/// One project-scoped quota entry (RPM/TPM/daily/model limit). Counts stay
/// counts: `remaining` exists only when the response supplies a denominator.
#[derive(Debug, Clone)]
pub(crate) struct GeminiProjectQuota {
    pub(crate) model: Option<String>,
    pub(crate) kind: String,
    pub(crate) limit: Option<f64>,
    pub(crate) used: Option<f64>,
    pub(crate) reset_at: Option<i64>,
}

pub(crate) fn parse_gemini_project_quotas(value: &serde_json::Value) -> Vec<GeminiProjectQuota> {
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

pub(crate) fn gemini_quota_buckets(
    quotas: &[GeminiProjectQuota],
    now: i64,
) -> Vec<QuotaBucketView> {
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
        // Rate limits are detail rows: no headline slot, no invented pace.
        None,
        UsageSnapshotStatus::Fresh,
    )
}

pub(crate) fn gemini_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    let (has_oauth, has_api_key) = gemini_credential_presence();
    let origin = gemini_credential_origin(has_oauth, has_api_key);
    gemini_snapshot_with_presence(agent, provider, has_oauth, has_api_key, &origin, now)
}

/// Snapshot from broker-routed evidence instead of ambient files: profile
/// refresh passes the discovery-proven OAuth presence, the env arm the
/// configured key. The result stays a typed gap (never a zero balance) until
/// an entitlement endpoint lands.
pub(crate) fn gemini_snapshot_with_presence(
    agent: &str,
    provider: Option<&str>,
    has_oauth: bool,
    has_api_key: bool,
    credential_origin: &str,
    now: i64,
) -> FocusedUsageView {
    // No entitlement endpoint is wired yet, so no migration signal exists:
    // every authenticated user gets the generic reporting-gap message until a
    // fetched entitlement says `consumer_unsupported`.
    let migration = gemini_migration_action(None);
    let (status, message) = if !has_oauth && !has_api_key {
        (
            UsageSnapshotStatus::NeedsSecret,
            "Gemini auth not available to Capsule",
        )
    } else if let Some(action) = migration.as_deref() {
        (UsageSnapshotStatus::Unsupported, action)
    } else {
        // API-key/Vertex route (or managed OAuth without a fetched
        // entitlement): project reporting needs an authorized scope no pinned
        // endpoint covers yet — a typed gap, never a zero balance.
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

#[cfg(test)]
mod tests;
