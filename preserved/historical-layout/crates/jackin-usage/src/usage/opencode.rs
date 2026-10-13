// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `OpenCode` Go subscription-limit adapter.
//!
//! `OpenCode` exposes one API credential in `auth.json` and a provider-owned
//! rolling/weekly/monthly response. The response does not expose a durable
//! non-secret account identity, so this adapter deliberately keeps the account
//! provisional and never derives identity from the Bearer [REDACTED] A valid key without a
//! Go subscription fails with a typed entitlement error (distinct from a key
//! failure), and per-model quota or Zen balance fields are never invented:
//! unknown payload fields render nothing.

use super::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus, UsageSource,
    UsageSurface, UsageViewInput, bucket, parse_iso_epoch, provider_http_client, timed_bucket,
    usage_view,
};
use serde::Deserialize;
use std::fs;
use std::path::Path;

const OPENCODE_USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

// No `Debug`: this carries the live Go API key and must never be formatted
// into a log or error.
#[derive(Deserialize)]
struct OpenCodeAuthEntry {
    #[serde(rename = "type")]
    kind: Option<String>,
    key: Option<String>,
}

#[derive(Debug)]
pub(crate) struct OpenCodeQuota {
    pub(crate) buckets: Vec<QuotaBucketView>,
    pub(crate) rate_limited: bool,
}

/// Typed `zen/go/v1/usage` failure: a key problem (401 / `AuthError`) needs a
/// login, while a valid key without a Go subscription (403 / `EntitlementError`)
/// is an honest unsupported state — never conflated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpenCodeUsageError {
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

/// Classify a non-2xx `zen/go/v1/usage` failure. The in-band error type wins
/// over the status code: `{error: {type: "EntitlementError"}}` is an
/// entitlement failure even on an unexpected status, and `AuthError` is a key
/// failure. Pure so the 403-vs-key distinction is unit-testable without I/O.
/// (Research §16: a valid key without Go entitlement gets a distinct 403 —
/// the taxonomy mirrors the server, it is not inferred.)
pub(crate) fn classify_opencode_http_error(status: u16, body: &str) -> OpenCodeUsageError {
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

pub(crate) fn fetch_opencode_usage(path: &Path) -> Result<OpenCodeQuota, OpenCodeUsageError> {
    let token = load_opencode_api_key(path).map_err(|error| {
        if error.contains("missing") {
            OpenCodeUsageError::Key(error)
        } else {
            OpenCodeUsageError::Schema(error)
        }
    })?;
    let client = provider_http_client().map_err(OpenCodeUsageError::Transport)?;
    let response = client
        .get(OPENCODE_USAGE_URL)
        .bearer_auth(token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .map_err(|error| {
            OpenCodeUsageError::Transport(format!("OpenCode usage request failed: {error}"))
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

/// Tolerant used-percent resolution: `percent`, then the `usagePercent` /
/// `usage_percent` spellings, then a `used` / `limit` fallback. `1` means 1%,
/// never 100% (research §16: the first-party Go route reports 0–100
/// percents, not fractions).
fn opencode_window_percent(window: &serde_json::Value) -> Option<f64> {
    window
        .get("percent")
        .or_else(|| window.get("usagePercent"))
        .or_else(|| window.get("usage_percent"))
        .and_then(serde_json::Value::as_f64)
        .filter(|percent| percent.is_finite())
        .or_else(|| {
            let used = window
                .get("used")?
                .as_f64()
                .filter(|used| used.is_finite())?;
            let limit = window
                .get("limit")?
                .as_f64()
                .filter(|limit| limit.is_finite() && *limit > 0.0)?;
            Some((used / limit) * 100.0)
        })
}

/// Tolerant reset resolution: ISO `resetsAt` / `resetAt` / `reset_at` /
/// `resets_at`, then `now + resetInSec` variants. Monthly anchors the
/// subscription anniversary — no duration is invented here.
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

pub(crate) fn parse_opencode_usage(
    value: serde_json::Value,
    now: i64,
) -> Result<OpenCodeQuota, String> {
    let usage = value
        .get("usage")
        .ok_or_else(|| "OpenCode usage response is malformed".to_owned())?;
    let windows = [
        ("Rolling", "rolling"),
        ("Weekly", "weekly"),
        ("Monthly", "monthly"),
    ];
    let mut rate_limited = false;
    let mut buckets = Vec::with_capacity(windows.len());
    for (label, key) in windows {
        let window = usage
            .get(key)
            .ok_or_else(|| format!("OpenCode {label} window is missing"))?;
        let status_raw = window
            .get("status")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("OpenCode {label} status is missing"))?;
        let status = match status_raw {
            "ok" => UsageSnapshotStatus::Fresh,
            "rate-limited" => {
                rate_limited = true;
                UsageSnapshotStatus::Unavailable
            }
            _ => return Err(format!("OpenCode {label} status is unsupported")),
        };
        let percent = opencode_window_percent(window)
            .ok_or_else(|| format!("OpenCode {label} percentage is missing"))?;
        if percent < 0.0 {
            return Err(format!("OpenCode {label} percentage is invalid"));
        }
        let reset_at = opencode_window_reset(window, now)
            .ok_or_else(|| format!("OpenCode {label} reset timestamp is invalid"))?;
        // Over-cap is a valid reading, not a schema error: the raw figure
        // stays in the label (`101% used`) while only the bar clamps to 0
        // remaining (T02). One over-cap window must never fail the others.
        #[expect(
            clippy::cast_sign_loss,
            reason = "floored at 0.0 and clamped to 100.0 before cast"
        )]
        let used = percent.round().clamp(0.0, 100.0) as u8;
        let used_label = (percent > 100.0).then(|| opencode_used_label(percent));
        buckets.push(timed_bucket(
            label,
            used_label,
            None,
            Some(100u8.saturating_sub(used)),
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

/// Over-cap label preserving the raw provider figure (`101% used`) — the
/// Muse/Codex lanes render the same form.
fn opencode_used_label(used_percent: f64) -> String {
    if used_percent.fract() == 0.0 {
        format!("{used_percent:.0}% used")
    } else {
        format!("{used_percent:.1}% used")
    }
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

#[cfg(test)]
mod tests;
