// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{
    CanonicalUsedPercent, FocusedUsageView, PercentObservationOrigin, QuotaBucketView,
    UsageConfidence, UsageSnapshotStatus, UsageSource,
};
use jackin_usage_provider_core::{
    UsageSurface, UsageViewInput, bucket, parse_iso_epoch, provider_http_client, timed_bucket,
    usage_view, with_used_percent,
};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderValue};
use serde::Deserialize;
use std::fs;
use std::path::Path;

const OPENCODE_USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

#[derive(Deserialize)]
struct OpenCodeAuthEntry {
    #[serde(rename = "type")]
    kind: Option<String>,
    key: Option<String>,
}

#[derive(Debug)]
pub struct OpenCodeQuota {
    pub buckets: Vec<QuotaBucketView>,
    pub rate_limited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenCodeUsageError {
    Key(String),
    Entitlement(String),
    Http(String),
    Transport(String),
    Decode(String),
    Schema(String),
}

impl std::fmt::Display for OpenCodeUsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Key(message)
            | Self::Entitlement(message)
            | Self::Http(message)
            | Self::Transport(message)
            | Self::Decode(message)
            | Self::Schema(message) => f.write_str(message),
        }
    }
}

impl OpenCodeUsageError {
    fn message(&self) -> &str {
        match self {
            Self::Key(message)
            | Self::Entitlement(message)
            | Self::Http(message)
            | Self::Transport(message)
            | Self::Decode(message)
            | Self::Schema(message) => message,
        }
    }
}

pub fn classify_opencode_http_error(status: u16, body: &str) -> OpenCodeUsageError {
    let error_type = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
    let message = |detail: &str| format!("OpenCode usage HTTP {status} ({detail})");
    match error_type.as_deref() {
        Some("EntitlementError") => {
            OpenCodeUsageError::Entitlement(message("Go subscription required"))
        }
        Some("AuthError") => OpenCodeUsageError::Key(message("invalid API key")),
        _ => match status {
            401 => OpenCodeUsageError::Key(message("invalid API key")),
            403 => OpenCodeUsageError::Entitlement(message("Go subscription required")),
            _ => OpenCodeUsageError::Http(format!("OpenCode usage HTTP {status}")),
        },
    }
}

pub fn load_opencode_api_key(path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "OpenCode auth.json is missing".to_owned()
        } else {
            "OpenCode auth.json is unreadable".to_owned()
        }
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "OpenCode auth.json is malformed".to_owned())?;
    let entries = value
        .as_object()
        .ok_or_else(|| "OpenCode auth.json is malformed".to_owned())?;
    if entries.len() != 1 {
        return Err("OpenCode auth.json has unsupported multiple credentials".to_owned());
    }
    let entry = value
        .get("opencode-go")
        .ok_or_else(|| "OpenCode opencode-go credential is missing".to_owned())?;
    let entry: OpenCodeAuthEntry = serde_json::from_value(entry.clone())
        .map_err(|_| "OpenCode opencode-go credential is malformed".to_owned())?;
    if entry.kind.as_deref() != Some("api") {
        return Err("OpenCode opencode-go credential is not an API key".to_owned());
    }
    entry
        .key
        .filter(|key| !key.trim().is_empty())
        .map(|key| key.trim().to_owned())
        .ok_or_else(|| "OpenCode opencode-go API key is empty".to_owned())
}

pub fn fetch_opencode_usage(path: &Path) -> Result<OpenCodeQuota, OpenCodeUsageError> {
    let token = load_opencode_api_key(path).map_err(|error| {
        if error.contains("missing") {
            OpenCodeUsageError::Key(error)
        } else {
            OpenCodeUsageError::Schema(error)
        }
    })?;
    fetch_opencode_usage_with_key(&token)
}

fn fetch_opencode_usage_with_key(key: &str) -> Result<OpenCodeQuota, OpenCodeUsageError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(OpenCodeUsageError::Key(
            "OpenCode API key is empty".to_owned(),
        ));
    }
    let client = provider_http_client().map_err(OpenCodeUsageError::Transport)?;
    let request = build_opencode_usage_request(&client, key)?;
    let response = client.execute(request).map_err(|error| {
        let message = format!("OpenCode usage request failed: {error}");
        OpenCodeUsageError::Transport(message.replace(key, "[redacted]"))
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(classify_opencode_http_error(status.as_u16(), &body));
    }
    let response = response.json::<serde_json::Value>().map_err(|error| {
        OpenCodeUsageError::Decode(format!("OpenCode usage decode failed: {error}"))
    })?;
    parse_opencode_usage(response, chrono::Utc::now().timestamp())
        .map_err(OpenCodeUsageError::Schema)
}

pub(crate) fn build_opencode_usage_request(
    client: &reqwest::blocking::Client,
    key: &str,
) -> Result<reqwest::blocking::Request, OpenCodeUsageError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(OpenCodeUsageError::Key(
            "OpenCode API key is empty".to_owned(),
        ));
    }
    let mut authorization = HeaderValue::from_str(&format!("Bearer {key}"))
        .map_err(|_| OpenCodeUsageError::Key("OpenCode API key is malformed".to_owned()))?;
    authorization.set_sensitive(true);
    client
        .get(OPENCODE_USAGE_URL)
        .header(AUTHORIZATION, authorization)
        .header(ACCEPT, "application/json")
        .build()
        .map_err(|_| {
            OpenCodeUsageError::Transport("OpenCode usage request could not be built".to_owned())
        })
}

fn opencode_window_percent(
    window: &serde_json::Value,
) -> Result<Option<(CanonicalUsedPercent, PercentObservationOrigin)>, ()> {
    for key in ["percent", "usagePercent", "usage_percent"] {
        let Some(value) = window.get(key) else {
            continue;
        };
        let Some(percent) = value.as_f64() else {
            return Err(());
        };
        return CanonicalUsedPercent::new(percent)
            .map(|percent| Some((percent, PercentObservationOrigin::ProviderReported)))
            .map_err(|_| ());
    }

    let Some(used) = window.get("used") else {
        return Ok(None);
    };
    let Some(limit) = window.get("limit") else {
        return Ok(None);
    };
    let Some(used) = used.as_f64() else {
        return Err(());
    };
    let Some(limit) = limit.as_f64() else {
        return Err(());
    };
    if !limit.is_finite() || limit <= 0.0 {
        return Err(());
    }
    CanonicalUsedPercent::new((used / limit) * 100.0)
        .map(|percent| Some((percent, PercentObservationOrigin::Derived)))
        .map_err(|_| ())
}

fn opencode_window_reset(window: &serde_json::Value, now: i64) -> Option<i64> {
    window
        .get("resetsAt")
        .or_else(|| window.get("resetAt"))
        .or_else(|| window.get("reset_at"))
        .or_else(|| window.get("resets_at"))
        .and_then(serde_json::Value::as_str)
        .and_then(parse_iso_epoch)
        .or_else(|| {
            window
                .get("resetInSec")
                .or_else(|| window.get("reset_in_sec"))
                .or_else(|| window.get("resetInSeconds"))
                .or_else(|| window.get("reset_in_seconds"))
                .and_then(serde_json::Value::as_i64)
                .filter(|secs| *secs >= 0)
                .and_then(|secs| now.checked_add(secs))
        })
}

pub fn parse_opencode_usage(value: serde_json::Value, now: i64) -> Result<OpenCodeQuota, String> {
    let usage = value
        .get("usage")
        .ok_or_else(|| "OpenCode usage response is malformed".to_owned())?;
    if !usage.is_object() {
        return Err("OpenCode usage response is malformed".to_owned());
    }
    let windows = [
        ("Rolling", "rolling"),
        ("Weekly", "weekly"),
        ("Monthly", "monthly"),
    ];
    let mut rate_limited = false;
    let mut buckets = Vec::with_capacity(windows.len());
    for (label, key) in windows {
        let Some(window) = usage.get(key) else {
            buckets.push(timed_bucket(
                label,
                None,
                None,
                None,
                None,
                now,
                Some("No data"),
                UsageSnapshotStatus::Unavailable,
            ));
            continue;
        };
        let status_raw = window.get("status").and_then(serde_json::Value::as_str);
        let status = match status_raw {
            Some("ok") => UsageSnapshotStatus::Fresh,
            Some("rate-limited") => {
                rate_limited = true;
                UsageSnapshotStatus::Unavailable
            }
            _ => {
                buckets.push(timed_bucket(
                    label,
                    None,
                    None,
                    None,
                    opencode_window_reset(window, now),
                    now,
                    Some("OpenCode window status is unavailable"),
                    UsageSnapshotStatus::Unavailable,
                ));
                continue;
            }
        };
        let reset_at = opencode_window_reset(window, now);
        let percent = match opencode_window_percent(window) {
            Ok(Some(percent)) => percent,
            Ok(None) => {
                buckets.push(timed_bucket(
                    label,
                    None,
                    None,
                    None,
                    reset_at,
                    now,
                    Some("No data"),
                    UsageSnapshotStatus::Unavailable,
                ));
                continue;
            }
            Err(()) => {
                buckets.push(timed_bucket(
                    label,
                    None,
                    None,
                    None,
                    reset_at,
                    now,
                    Some("OpenCode usage percentage is invalid"),
                    UsageSnapshotStatus::Unavailable,
                ));
                continue;
            }
        };
        let mut view = timed_bucket(
            label,
            Some(percent.0.label()),
            Some("100%".to_owned()),
            None,
            reset_at,
            now,
            None,
            status,
        );
        view = with_used_percent(view, Some(percent));
        buckets.push(view);
    }
    Ok(OpenCodeQuota {
        buckets,
        rate_limited,
    })
}

pub fn opencode_profile_snapshot(agent: &str, auth_path: &Path, now: i64) -> FocusedUsageView {
    let result = fetch_opencode_usage(auth_path);
    opencode_profile_view(agent, result, now)
}

pub fn opencode_api_key_snapshot(agent: &str, key: &str, now: i64) -> FocusedUsageView {
    let result = fetch_opencode_usage_with_key(key);
    opencode_api_key_view(agent, result, now)
}

pub(crate) fn opencode_profile_view(
    agent: &str,
    result: Result<OpenCodeQuota, OpenCodeUsageError>,
    now: i64,
) -> FocusedUsageView {
    opencode_view(agent, "API token · opencode-go", result, now)
}

pub(crate) fn opencode_api_key_view(
    agent: &str,
    result: Result<OpenCodeQuota, OpenCodeUsageError>,
    now: i64,
) -> FocusedUsageView {
    opencode_view(agent, "API key · env OPENCODE_API_KEY", result, now)
}

fn opencode_view(
    agent: &str,
    credential_origin: &str,
    result: Result<OpenCodeQuota, OpenCodeUsageError>,
    now: i64,
) -> FocusedUsageView {
    let (buckets, status, error) = match result {
        Ok(quota) => {
            let has_observation = quota
                .buckets
                .iter()
                .any(opencode_bucket_has_quantitative_observation);
            let mut diagnostics = quota
                .buckets
                .iter()
                .filter_map(|bucket| bucket.pace_label.as_deref())
                .filter(|message| !message.is_empty() && *message != "No data")
                .map(str::to_owned)
                .collect::<Vec<_>>();
            if quota.rate_limited {
                diagnostics.push("OpenCode usage is rate limited".to_owned());
            }
            let status = if has_observation && !quota.rate_limited {
                UsageSnapshotStatus::Fresh
            } else {
                UsageSnapshotStatus::Unavailable
            };
            if !has_observation && diagnostics.is_empty() {
                diagnostics.push(
                    "OpenCode usage response contained no quantitative observation".to_owned(),
                );
            }
            (
                quota.buckets,
                status,
                (!diagnostics.is_empty()).then(|| diagnostics.join("; ")),
            )
        }
        Err(error) => {
            let status = match &error {
                OpenCodeUsageError::Key(_) => UsageSnapshotStatus::NeedsLogin,
                OpenCodeUsageError::Entitlement(_) => UsageSnapshotStatus::Unsupported,
                _ => UsageSnapshotStatus::Error,
            };
            let message = error.message().to_owned();
            (
                vec![bucket(
                    "Usage",
                    None,
                    None,
                    None,
                    None,
                    Some(message.as_str()),
                    status,
                )],
                status,
                Some(message),
            )
        }
    };
    usage_view(UsageViewInput {
        agent,
        provider: Some("OpenCode"),
        surface: UsageSurface::OpenCode,
        account_label: "OpenCode account (unresolved)".to_owned(),
        username: None,
        plan_label: Some("OpenCode Go".to_owned()),
        credential_origin: Some(credential_origin.to_owned()),
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            UsageSource::ProviderApi
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: error,
    })
}

fn opencode_bucket_has_quantitative_observation(bucket: &QuotaBucketView) -> bool {
    bucket.status == UsageSnapshotStatus::Fresh
        && (bucket.used_percent.is_some()
            || bucket.remaining_percent.is_some()
            || bucket.used_money.is_some()
            || bucket.limit_money.is_some()
            || bucket.count_quota.is_some_and(|quota| {
                quota.used.is_some() || quota.limit.is_some() || quota.remaining.is_some()
            }))
}
