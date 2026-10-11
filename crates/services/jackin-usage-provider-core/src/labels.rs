// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Provider labels, storage names, and reason text.

use std::path::PathBuf;

use jackin_protocol::control::{
    FocusedUsageView, UsageConfidence, UsageSeverity, UsageSnapshotStatus, UsageSource,
};

use super::UsageSurface;
use super::format::home_path;
use super::resolve_surface;

pub fn env_dir_or_home(env_var: &str, home_default: &str) -> PathBuf {
    std::env::var(env_var).map_or_else(|_| home_path(home_default), PathBuf::from)
}

pub fn resolved_usage_provider_label(
    agent: &str,
    focused_provider: Option<&str>,
) -> Option<&'static str> {
    let surface = resolve_surface(agent, focused_provider);
    (surface != UsageSurface::Unsupported).then_some(surface.label())
}

/// Closed host-broker surface id for a Capsule refresh target.
#[must_use]
pub fn broker_surface_id(agent: &str, focused_provider: Option<&str>) -> Option<&'static str> {
    resolve_surface(agent, focused_provider).id()
}

/// Shared provider display remap for Capsule tabs and jackin❯ desktop overview.
///
/// Single mapping so Desktop never grows a second Swift-side provider rename.
#[must_use]
pub fn provider_display_label(label: &str) -> &str {
    match label {
        "Codex" | "OpenAI / Codex" => "OpenAI",
        "Claude" | "Anthropic / Claude" => "Anthropic",
        "Grok Build" | "xAI / Grok" => "xAI",
        "GLM / Z.AI" => "Z.AI",
        other => other,
    }
}

/// Honesty caption when numbers are estimated / local-log derived.
#[must_use]
pub fn estimate_caption(view: &FocusedUsageView) -> Option<String> {
    if matches!(view.confidence, UsageConfidence::Estimated)
        || matches!(view.source, UsageSource::LocalLogs)
    {
        Some("Estimated from token usage · not a subscription bill".to_owned())
    } else {
        None
    }
}

pub fn usage_status_storage_label(status: UsageSnapshotStatus) -> &'static str {
    match status {
        UsageSnapshotStatus::Fresh => "fresh",
        UsageSnapshotStatus::Stale => "stale",
        UsageSnapshotStatus::NeedsLogin => "needs_login",
        UsageSnapshotStatus::NeedsSecret => "needs_secret",
        UsageSnapshotStatus::Unsupported => "unsupported",
        UsageSnapshotStatus::Unavailable => "unavailable",
        UsageSnapshotStatus::Error => "error",
    }
}

pub fn usage_source_storage_label(source: UsageSource) -> &'static str {
    match source {
        UsageSource::ProviderApi => "provider_api",
        UsageSource::Cli => "cli",
        UsageSource::LocalLogs => "local_logs",
        UsageSource::Cache => "cache",
        UsageSource::None => "none",
    }
}

pub fn usage_confidence_storage_label(confidence: UsageConfidence) -> &'static str {
    match confidence {
        UsageConfidence::Authoritative => "authoritative",
        UsageConfidence::Estimated => "estimated",
        UsageConfidence::PresenceOnly => "presence_only",
        UsageConfidence::None => "none",
    }
}

pub fn severity_from_label(label: Option<&str>) -> UsageSeverity {
    match label.map(str::to_ascii_lowercase).as_deref() {
        Some("warn" | "warning" | "elevated") => UsageSeverity::Warn,
        Some("danger" | "critical" | "exceeded") => UsageSeverity::Danger,
        _ => UsageSeverity::Normal,
    }
}

/// Turn an API reason slug (`out_of_credits`) into a human phrase
/// (`out of credits`) for the disabled-spend pace label.
pub fn humanize_reason(reason: &str) -> String {
    reason.replace(['_', '-'], " ")
}

/// Title-case a codename window key (`amber_ladder` → `Amber Ladder`) for use as
/// a bucket label. Distinct from [`humanize_reason`] (which yields a lowercase
/// phrase for inline pace text); a window label is a proper-noun-style heading
/// shown beside `Session`/`Weekly`.
pub fn humanize_window_label(key: &str) -> String {
    key.split(['_', '-'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
