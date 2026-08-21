// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `OpenCode` Go subscription-limit adapter.
//!
//! `OpenCode` exposes one API credential in `auth.json` and a provider-owned
//! rolling/weekly/monthly response. The response does not expose a durable
//! non-secret account identity, so this adapter deliberately keeps the account
//! provisional and never derives identity from the bearer key.

use super::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus, UsageSource,
    UsageSurface, UsageViewInput, bucket, parse_iso_epoch, provider_http_client, timed_bucket,
    usage_view,
};
use serde::Deserialize;
use std::fs;
use std::path::Path;

const OPENCODE_USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

#[derive(Debug, Deserialize)]
struct OpenCodeAuthEntry {
    #[serde(rename = "type")]
    kind: Option<String>,
    key: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeUsageResponse {
    usage: OpenCodeUsageWindows,
}

#[derive(Debug, Deserialize)]
struct OpenCodeUsageWindows {
    rolling: OpenCodeUsageWindow,
    weekly: OpenCodeUsageWindow,
    monthly: OpenCodeUsageWindow,
}

#[derive(Debug, Deserialize)]
struct OpenCodeUsageWindow {
    status: String,
    percent: i64,
    #[serde(rename = "resetsAt")]
    resets_at: String,
}

#[derive(Debug)]
pub(crate) struct OpenCodeQuota {
    pub(crate) buckets: Vec<QuotaBucketView>,
    pub(crate) rate_limited: bool,
}

pub(crate) fn load_opencode_api_key(path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "OpenCode auth.json is missing".to_owned()
        } else {
            "OpenCode auth.json is unreadable".to_owned()
        }
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "OpenCode auth.json is malformed".to_owned())?;
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

pub(crate) fn fetch_opencode_usage(path: &Path) -> Result<OpenCodeQuota, String> {
    let token = load_opencode_api_key(path)?;
    let client = provider_http_client()?;
    let response = client
        .get(OPENCODE_USAGE_URL)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .map_err(|error| format!("OpenCode usage request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("OpenCode usage HTTP {status}"));
    }
    let response = response
        .json::<serde_json::Value>()
        .map_err(|error| format!("OpenCode usage decode failed: {error}"))?;
    parse_opencode_usage(response, chrono::Utc::now().timestamp())
}

pub(crate) fn parse_opencode_usage(
    value: serde_json::Value,
    now: i64,
) -> Result<OpenCodeQuota, String> {
    let response: OpenCodeUsageResponse = serde_json::from_value(value)
        .map_err(|_| "OpenCode usage response is malformed".to_owned())?;
    let windows = [
        ("Rolling", response.usage.rolling),
        ("Weekly", response.usage.weekly),
        ("Monthly", response.usage.monthly),
    ];
    let mut rate_limited = false;
    let mut buckets = Vec::with_capacity(windows.len());
    for (label, window) in windows {
        if !(0..=100).contains(&window.percent) {
            return Err(format!("OpenCode {label} percentage is invalid"));
        }
        let status = match window.status.as_str() {
            "ok" => UsageSnapshotStatus::Fresh,
            "rate-limited" => {
                rate_limited = true;
                UsageSnapshotStatus::Unavailable
            }
            _ => return Err(format!("OpenCode {label} status is unsupported")),
        };
        let reset_at = parse_iso_epoch(&window.resets_at)
            .ok_or_else(|| format!("OpenCode {label} reset timestamp is invalid"))?;
        let percent = u8::try_from(window.percent)
            .map_err(|_| format!("OpenCode {label} percentage is invalid"))?;
        let remaining = 100u8.saturating_sub(percent);
        buckets.push(timed_bucket(
            label,
            None,
            None,
            Some(remaining),
            Some(reset_at),
            now,
            None,
            status,
        ));
    }
    Ok(OpenCodeQuota {
        buckets,
        rate_limited,
    })
}

pub(crate) fn opencode_profile_snapshot(
    agent: &str,
    auth_path: &Path,
    now: i64,
) -> FocusedUsageView {
    let result = fetch_opencode_usage(auth_path);
    let (buckets, status, error) = match result {
        Ok(quota) => (
            quota.buckets,
            if quota.rate_limited {
                UsageSnapshotStatus::Unavailable
            } else {
                UsageSnapshotStatus::Fresh
            },
            None,
        ),
        Err(error) => {
            let status = if error.contains("HTTP 401") || error.contains("unauthorized") {
                UsageSnapshotStatus::NeedsLogin
            } else if error.contains("HTTP 403") {
                UsageSnapshotStatus::Unsupported
            } else if error.contains("missing") {
                UsageSnapshotStatus::NeedsLogin
            } else {
                UsageSnapshotStatus::Error
            };
            (
                vec![bucket(
                    "Usage",
                    None,
                    None,
                    None,
                    None,
                    Some(error.as_str()),
                    status,
                )],
                status,
                Some(error),
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
        credential_origin: Some("API token · opencode-go".to_owned()),
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
