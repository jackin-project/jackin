// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Antigravity` (`agy`) usage snapshot.
//!
//! Official read-only commands (changelog-verified, no model turn):
//! `agy -p /usage --output-format json` and `agy -p /credits --output-format json`.
//! Version-gated at >= 1.1.11: older binaries interpret an unknown slash
//! command as a prompt, so the gate is a correctness boundary, not a nicety.
//! At >= 1.2.2 the official commands are preferred over the local
//! `LanguageServerService` loopback API (tokenless break per `CodexBar`); this
//! module implements the official commands only.
//!
//! Response families (see `ref-contracts-C.md` §1):
//!
//! * Quota summary (`groups[].buckets[]`, bare or `{response: {groups}}`
//!   loopback-wrapped; `pools[]`/`buckets[]` aliases): exact `bucketId` match
//!   `gemini-5h` / `gemini-weekly` / `3p-5h` / `3p-weekly`. A parsed summary
//!   (even empty) wins over legacy shapes.
//! * Legacy per-model quota (`models{}` map with `quotaInfo`): collapses to
//!   the worst (lowest) remaining fraction per family and is 5h-only; weekly
//!   reads "No data". Internal (`isInternal`) and empty-label models are
//!   dropped, as are availability-only rows: model availability fractions are
//!   not quota, so an all-available response never becomes fabricated 100%.
//!
//! The command response may omit identity: the account label is then empty and
//!! the credential origin says so, so the broker binds the observation to the
//! runtime that produced it instead of attaching it to an arbitrary account.

#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;

/// Minimum `agy` version with read-only `/usage|/credits --output-format json`.
pub(crate) const ANTIGRAVITY_MIN_JSON_VERSION: (u64, u64, u64) = (1, 1, 11);

const ANTIGRAVITY_SESSION_WINDOW_SECONDS: i64 = 5 * 60 * 60;
const ANTIGRAVITY_WEEKLY_WINDOW_SECONDS: i64 = 7 * 24 * 60 * 60;

/// Parse `agy --version` output into `(major, minor, patch)`. Accepts a bare
/// `1.2.5` or a decorated line (`agy version 1.2.5 (build …)`); `None` when no
/// `N.N.N` triple is present.
pub(crate) fn parse_agy_version(text: &str) -> Option<(u64, u64, u64)> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '-'))
        .find_map(|part| {
            let mut segments = part.split('.');
            match (segments.next(), segments.next(), segments.next()) {
                (Some(major), Some(minor), Some(patch))
                    if major.chars().all(|ch| ch.is_ascii_digit())
                        && minor.chars().all(|ch| ch.is_ascii_digit())
                        && patch.chars().all(|ch| ch.is_ascii_digit()) =>
                {
                    Some((
                        major.parse().ok()?,
                        minor.parse().ok()?,
                        patch.parse().ok()?,
                    ))
                }
                _ => None,
            }
        })
}

/// True when `version` supports the official JSON usage commands.
pub(crate) fn agy_version_supports_json(version: (u64, u64, u64)) -> bool {
    version >= ANTIGRAVITY_MIN_JSON_VERSION
}

/// Probe `agy --version` and enforce the JSON gate. `Err` carries the exact
/// non-secret reason (binary missing, unparseable version, too old).
pub(crate) fn antigravity_cli_version() -> Result<(u64, u64, u64), String> {
    let text = run_cli_with_timeout("agy", &["--version"], PROVIDER_CLI_TIMEOUT)
        .map_err(|error| format!("Antigravity CLI unavailable: {error}"))?;
    let version = parse_agy_version(&text)
        .ok_or_else(|| "Antigravity CLI version was not recognized".to_owned())?;
    if !agy_version_supports_json(version) {
        return Err(format!(
            "Antigravity CLI {}.{}.{} predates JSON usage (needs >= 1.1.11)",
            version.0, version.1, version.2
        ));
    }
    Ok(version)
}

pub(crate) fn fetch_antigravity_cli_usage() -> Result<AntigravityUsage, String> {
    antigravity_cli_version()?;
    let output = run_cli_with_timeout(
        "agy",
        &["-p", "/usage", "--output-format", "json"],
        PROVIDER_CLI_TIMEOUT,
    )
    .map_err(|error| format!("Antigravity /usage request failed: {error}"))?;
    parse_antigravity_usage_output(&output)
}

pub(crate) fn fetch_antigravity_cli_credits() -> Result<AntigravityCredits, String> {
    antigravity_cli_version()?;
    let output = run_cli_with_timeout(
        "agy",
        &["-p", "/credits", "--output-format", "json"],
        PROVIDER_CLI_TIMEOUT,
    )
    .map_err(|error| format!("Antigravity /credits request failed: {error}"))?;
    parse_antigravity_credits_output(&output)
}

/// Quota family: Gemini models vs every non-Gemini model (Claude, GPT-OSS, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AntigravityFamily {
    Gemini,
    Other,
}

/// Quota window: the 5-hour session pool vs the weekly pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AntigravityWindow {
    Session,
    Weekly,
}

/// One normalized Antigravity quota pool.
#[derive(Debug, Clone)]
pub(crate) struct AntigravityPool {
    pub(crate) family: AntigravityFamily,
    pub(crate) window: Option<AntigravityWindow>,
    /// Remaining percent. `None` only when the source carried no quota signal
    /// at all for a pool whose identity is known (kept distinct from 0, which
    /// is a genuinely depleted pool).
    pub(crate) remaining_percent: Option<u8>,
    pub(crate) reset_at: Option<i64>,
    /// Source label for pools whose window could not be determined (kept as a
    /// detail row under their own id rather than dropped or mis-slotted).
    pub(crate) source_label: Option<String>,
}

/// Parsed `/usage` output: quota pools plus optional identity/plan.
#[derive(Debug, Clone, Default)]
pub(crate) struct AntigravityUsage {
    pub(crate) pools: Vec<AntigravityPool>,
    pub(crate) identity: Option<String>,
    pub(crate) plan: Option<String>,
    /// True when pools came from the legacy per-model shape (5h-only); weekly
    /// buckets then render "No data" instead of invented allowance.
    pub(crate) legacy_fallback: bool,
}

/// Parse `agy -p /usage --output-format json` output. `Err` only when the text
/// is not JSON at all; a well-formed response with no quota pools is `Ok` with
/// empty pools (the snapshot renders an honest placeholder, never 100%).
pub(crate) fn parse_antigravity_usage_output(text: &str) -> Result<AntigravityUsage, String> {
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|_| "Antigravity /usage output was not recognized".to_owned())?;
    let identity = antigravity_identity_from_value(&value);
    let plan = antigravity_plan_from_value(&value);
    if let Some(entries) = antigravity_summary_entries(&value) {
        let pools = entries
            .iter()
            .filter_map(antigravity_pool_from_summary_entry)
            .collect();
        return Ok(AntigravityUsage {
            pools,
            identity,
            plan,
            legacy_fallback: false,
        });
    }
    let pools = antigravity_pools_from_legacy_models(&value);
    Ok(AntigravityUsage {
        pools,
        identity,
        plan,
        legacy_fallback: true,
    })
}

/// Summary-bucket containers, in precedence order. `Some` (even when empty)
/// means a summary shape is present and wins over legacy models.
fn antigravity_summary_entries(value: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    for groups in [
        value.get("groups"),
        value.get("response").and_then(|inner| inner.get("groups")),
    ] {
        if let Some(groups) = groups.and_then(serde_json::Value::as_array) {
            let entries = groups
                .iter()
                .filter_map(|group| group.get("buckets"))
                .filter_map(serde_json::Value::as_array)
                .flatten()
                .cloned()
                .collect::<Vec<_>>();
            return Some(entries);
        }
    }
    for key in ["pools", "buckets", "quotaBuckets"] {
        if let Some(entries) = value.get(key).and_then(serde_json::Value::as_array) {
            return Some(entries.clone());
        }
    }
    None
}

/// Map one summary entry to a pool. Exact `bucketId` match first; otherwise a
/// family/window keyword read of the entry id. Entries without quota identity
/// are dropped (never fabricated into an empty-label row).
fn antigravity_pool_from_summary_entry(entry: &serde_json::Value) -> Option<AntigravityPool> {
    let id = ["bucketId", "bucket_id", "id", "poolId", "pool", "name"]
        .into_iter()
        .filter_map(|key| entry.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|id| !id.is_empty())?;
    // A summary entry with no quota signal is unknown, never depleted: the
    // `/usage` schema is unauthenticated and unverified, so absence ⇒ 0%
    // would invent exhaustion (F05 keeps unknown vs exhausted distinct; only
    // an explicit depleted marker yields 0).
    let remaining_percent = antigravity_remaining_from_entry(entry);
    let reset_at = antigravity_reset_from_entry(entry);
    let (family, window) = match id {
        "gemini-5h" => (AntigravityFamily::Gemini, Some(AntigravityWindow::Session)),
        "gemini-weekly" => (AntigravityFamily::Gemini, Some(AntigravityWindow::Weekly)),
        "3p-5h" => (AntigravityFamily::Other, Some(AntigravityWindow::Session)),
        "3p-weekly" => (AntigravityFamily::Other, Some(AntigravityWindow::Weekly)),
        _ => {
            let lower = id.to_ascii_lowercase();
            let family = if lower.contains("gemini") {
                AntigravityFamily::Gemini
            } else {
                AntigravityFamily::Other
            };
            let window = if lower.contains("week") {
                Some(AntigravityWindow::Weekly)
            } else if lower.contains("5h")
                || lower.contains("5-h")
                || lower.contains("session")
                || lower.contains("hour")
            {
                Some(AntigravityWindow::Session)
            } else {
                None
            };
            (family, window)
        }
    };
    Some(AntigravityPool {
        family,
        window,
        remaining_percent,
        reset_at,
        source_label: Some(id.to_owned()),
    })
}

/// Remaining percent from a quota entry: `remainingFraction` (0-1),
/// `remainingPercent` (0-100), or an inverted used signal. `None` when the
/// entry carries no quota signal at all.
fn antigravity_remaining_from_entry(entry: &serde_json::Value) -> Option<u8> {
    let quota = entry.get("quotaInfo").unwrap_or(entry);
    for (key, is_fraction) in [
        ("remainingFraction", true),
        ("remaining_fraction", true),
        ("remainingPercent", false),
        ("remaining_percent", false),
        ("remaining", false),
    ] {
        if let Some(raw) = quota.get(key).and_then(json_number)
            && raw.is_finite()
            && raw >= 0.0
        {
            let percent = if is_fraction { raw * 100.0 } else { raw };
            #[expect(
                clippy::cast_sign_loss,
                reason = "filtered non-negative; clamped 0..=100"
            )]
            {
                return Some(percent.round().clamp(0.0, 100.0) as u8);
            }
        }
    }
    for key in ["usedPercent", "used_percent", "utilization", "used"] {
        if let Some(raw) = quota.get(key).and_then(json_number)
            && raw.is_finite()
            && raw >= 0.0
        {
            let used = if raw <= 1.0 { raw * 100.0 } else { raw };
            #[expect(
                clippy::cast_sign_loss,
                reason = "filtered non-negative; clamped 0..=100"
            )]
            {
                return Some(100u8.saturating_sub(used.round().clamp(0.0, 100.0) as u8));
            }
        }
    }
    None
}

fn antigravity_reset_from_entry(entry: &serde_json::Value) -> Option<i64> {
    let quota = entry.get("quotaInfo").unwrap_or(entry);
    for key in [
        "resetTime",
        "reset_time",
        "resetsAt",
        "resets_at",
        "reset_at",
        "reset",
    ] {
        if let Some(node) = quota.get(key) {
            if let Some(text) = node.as_str()
                && let Some(epoch) = parse_iso_epoch(text.trim())
            {
                return Some(epoch);
            }
            if let Some(number) = json_number(node) {
                return Some(epoch_seconds_from_maybe_ms(number.floor() as i64));
            }
        }
    }
    None
}

/// Legacy per-model quota collapsed to the worst (lowest) remaining fraction
/// per family. Internal, empty-label, and availability-only rows are dropped:
/// a model that only reports availability contributes no pool.
fn antigravity_pools_from_legacy_models(value: &serde_json::Value) -> Vec<AntigravityPool> {
    let Some(models) = value.get("models").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let mut worst: [Option<(u8, Option<i64>)>; 2] = [None, None];
    for (id, model) in models {
        if model.get("isInternal").and_then(serde_json::Value::as_bool) == Some(true) {
            continue;
        }
        let label = ["displayName", "label", "model"]
            .into_iter()
            .filter_map(|key| model.get(key).and_then(serde_json::Value::as_str))
            .map(str::trim)
            .find(|label| !label.is_empty());
        // Empty display label (no displayName/label/model) is not a pool,
        // even when the map key is non-empty.
        if label.is_none() {
            continue;
        }
        // Availability-only rows carry no quota signal: skip, never invent.
        let Some(remaining) = antigravity_remaining_from_entry(model) else {
            continue;
        };
        let name = format!("{id} {}", label.unwrap_or_default()).to_ascii_lowercase();
        let slot = usize::from(!name.contains("gemini"));
        let reset_at = antigravity_reset_from_entry(model);
        let replace = worst[slot].is_none_or(|(current, _)| remaining < current);
        if replace {
            worst[slot] = Some((remaining, reset_at));
        }
    }
    [
        (AntigravityFamily::Gemini, worst[0]),
        (AntigravityFamily::Other, worst[1]),
    ]
    .into_iter()
    .filter_map(|(family, entry)| {
        let (remaining, reset_at) = entry?;
        Some(AntigravityPool {
            family,
            window: Some(AntigravityWindow::Session),
            remaining_percent: Some(remaining),
            reset_at,
            source_label: None,
        })
    })
    .collect()
}

/// Identity from a `/usage` response, if present. The command may omit it; the
/// caller must then bind the observation to its runtime, not to an account.
pub(crate) fn antigravity_identity_from_value(value: &serde_json::Value) -> Option<String> {
    for key in ["email", "emailAddress", "accountEmail", "userEmail", "user"] {
        if let Some(node) = value.get(key) {
            if let Some(text) = node.as_str()
                && !text.trim().is_empty()
            {
                return Some(text.trim().to_owned());
            }
            if let Some(text) = node
                .get("email")
                .or_else(|| node.get("emailAddress"))
                .and_then(serde_json::Value::as_str)
                && !text.trim().is_empty()
            {
                return Some(text.trim().to_owned());
            }
        }
    }
    None
}

/// Plan label: prefer the Google tier name over the Windsurf-inherited plan
/// name (always "Pro" when paid, so it carries no tier signal).
pub(crate) fn antigravity_plan_from_value(value: &serde_json::Value) -> Option<String> {
    for key in ["userTier", "currentTier", "paidTier", "tier"] {
        if let Some(name) = value
            .get(key)
            .and_then(|tier| tier.get("name").or_else(|| tier.get("id")))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            return Some(humanize_plan_label(name));
        }
    }
    value
        .get("planInfo")
        .and_then(|info| info.get("planName"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(humanize_plan_label)
}

pub(crate) fn antigravity_buckets(usage: &AntigravityUsage, now: i64) -> Vec<QuotaBucketView> {
    let mut buckets = Vec::new();
    for pool in &usage.pools {
        buckets.push(antigravity_pool_bucket(pool, now));
    }
    // Legacy per-model rows are 5h-only: weekly reads "No data" for families
    // with a session pool but no weekly pool.
    if usage.legacy_fallback {
        for family in [AntigravityFamily::Gemini, AntigravityFamily::Other] {
            let has_session = usage.pools.iter().any(|pool| {
                pool.family == family && pool.window == Some(AntigravityWindow::Session)
            });
            let has_weekly = usage.pools.iter().any(|pool| {
                pool.family == family && pool.window == Some(AntigravityWindow::Weekly)
            });
            if has_session && !has_weekly {
                buckets.push(bucket(
                    antigravity_family_label(family, Some(AntigravityWindow::Weekly)).as_str(),
                    None,
                    None,
                    None,
                    None,
                    Some("No data"),
                    UsageSnapshotStatus::Fresh,
                ));
            }
        }
    }
    buckets
}

fn antigravity_pool_bucket(pool: &AntigravityPool, now: i64) -> QuotaBucketView {
    let label = match (&pool.window, &pool.source_label) {
        (Some(window), _) => antigravity_family_label(pool.family, Some(*window)),
        (None, Some(source)) => format!("Quota · {source}"),
        (None, None) => "Quota".to_owned(),
    };
    let remaining = pool.remaining_percent;
    let used_label = remaining.map(|left| format!("{}% used", 100u8.saturating_sub(left)));
    let window_seconds = match pool.window {
        Some(AntigravityWindow::Session) => Some(ANTIGRAVITY_SESSION_WINDOW_SECONDS),
        Some(AntigravityWindow::Weekly) => Some(ANTIGRAVITY_WEEKLY_WINDOW_SECONDS),
        None => None,
    };
    let pace = quota_pace_label(remaining, pool.reset_at, window_seconds, now);
    // No quota signal: an honest "No data" detail row (the legacy path's
    // convention), never a fabricated 0% or 100%.
    let pace = pace.or_else(|| remaining.is_none().then(|| "No data".to_owned()));
    let mut view = timed_bucket(
        &label,
        used_label,
        Some("100%".to_owned()),
        remaining,
        pool.reset_at,
        now,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    // The Gemini family fills the headline slots; the non-Gemini pools are
    // detail rows the headline ignores.
    view.status_slot = match (pool.family, pool.window) {
        (AntigravityFamily::Gemini, Some(AntigravityWindow::Session)) => Some(StatusSlot::Session),
        (AntigravityFamily::Gemini, Some(AntigravityWindow::Weekly)) => Some(StatusSlot::Weekly),
        _ => None,
    };
    view
}

fn antigravity_family_label(
    family: AntigravityFamily,
    window: Option<AntigravityWindow>,
) -> String {
    let family_label = match family {
        AntigravityFamily::Gemini => "Gemini",
        AntigravityFamily::Other => "Other models",
    };
    match window {
        Some(AntigravityWindow::Session) => format!("{family_label} · 5h"),
        Some(AntigravityWindow::Weekly) => format!("{family_label} · Weekly"),
        None => family_label.to_owned(),
    }
}

/// Parsed `/credits` output. Money attaches only when the response states an
/// explicit minor-unit amount + exponent; plain numbers stay labels so an
/// unknown scale can never render 100× off.
#[derive(Debug, Clone, Default)]
pub(crate) struct AntigravityCredits {
    pub(crate) used_money: Option<Money>,
    pub(crate) limit_money: Option<Money>,
    pub(crate) balance_label: Option<String>,
    pub(crate) limit_label: Option<String>,
    pub(crate) remaining_percent: Option<u8>,
}

pub(crate) fn parse_antigravity_credits_output(text: &str) -> Result<AntigravityCredits, String> {
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|_| "Antigravity /credits output was not recognized".to_owned())?;
    let node = value.get("credits").unwrap_or(&value);
    let currency = ["currency", "unit"]
        .into_iter()
        .filter_map(|key| node.get(key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|currency| !currency.is_empty())
        .unwrap_or("credits")
        .to_owned();
    let exponent = node
        .get("exponent")
        .and_then(json_number)
        .map_or(2, |value| {
            #[expect(
                clippy::cast_sign_loss,
                reason = "filtered non-negative; clamped 0..=6"
            )]
            {
                value.round().clamp(0.0, 6.0) as u8
            }
        });
    let minor = |key: &str| {
        node.get(key)
            .and_then(json_number)
            .map(|value| value.round() as i64)
    };
    let (used_money, limit_money) = match (minor("used_minor"), minor("limit_minor")) {
        (Some(used), Some(limit)) => (
            Some(Money::new(used, currency.clone(), exponent)),
            Some(Money::new(limit, currency.clone(), exponent)),
        ),
        (Some(used), None) => (Some(Money::new(used, currency.clone(), exponent)), None),
        _ => (None, None),
    };
    let major = |keys: &[&str]| {
        keys.iter()
            .filter_map(|key| node.get(*key).and_then(json_number))
            .find(|value| value.is_finite() && *value >= 0.0)
    };
    let balance = major(&["balance", "remaining", "available"]);
    let limit = major(&["limit", "total", "allowance"]);
    let remaining_percent = match (balance, limit) {
        (Some(left), Some(total)) if total > 0.0 => {
            #[expect(
                clippy::cast_sign_loss,
                reason = "filtered non-negative; clamped 0..=100"
            )]
            {
                Some((left.clamp(0.0, total) / total * 100.0).round() as u8)
            }
        }
        _ => antigravity_remaining_from_entry(node),
    };
    let money_label = |money: Option<&Money>| money.map(Money::to_string);
    Ok(AntigravityCredits {
        balance_label: money_label(used_money.as_ref())
            .or_else(|| balance.map(|value| format!("{value} {currency}"))),
        limit_label: money_label(limit_money.as_ref())
            .or_else(|| limit.map(|value| format!("{value} {currency}"))),
        used_money,
        limit_money,
        remaining_percent,
    })
}

pub(crate) fn antigravity_credits_bucket(credits: &AntigravityCredits) -> Option<QuotaBucketView> {
    if credits.used_money.is_none()
        && credits.limit_money.is_none()
        && credits.balance_label.is_none()
        && credits.limit_label.is_none()
    {
        return None;
    }
    let mut view = bucket(
        "Credits",
        credits.balance_label.clone(),
        credits.limit_label.clone(),
        credits.remaining_percent,
        None,
        None,
        UsageSnapshotStatus::Fresh,
    );
    // The Spend slot only when structured money backs it; a label-only credit
    // balance is a detail row, never a headline figure.
    if credits.used_money.is_some() {
        view.status_slot = Some(StatusSlot::Spend);
        view.used_money = credits.used_money.clone();
        view.limit_money = credits.limit_money.clone();
    }
    Some(view)
}

pub(crate) fn antigravity_snapshot(
    agent: &str,
    provider: Option<&str>,
    now: i64,
) -> FocusedUsageView {
    match antigravity_cli_version() {
        Ok(_) => {}
        Err(error) => {
            return antigravity_status_view(
                agent,
                provider,
                now,
                antigravity_version_error_status(&error),
                &error,
            );
        }
    }
    let (usage, usage_error) = split_fetch(Some(fetch_antigravity_cli_usage()));
    let (credits, credits_error) = split_fetch(Some(fetch_antigravity_cli_credits()));
    let mut buckets = usage
        .as_ref()
        .map(|usage| antigravity_buckets(usage, now))
        .unwrap_or_default();
    let credits_bucket = credits.as_ref().and_then(antigravity_credits_bucket);
    if let Some(bucket) = &credits_bucket {
        buckets.push(bucket.clone());
    }
    if buckets.is_empty() {
        let error = usage_error
            .as_deref()
            .or(Some("Antigravity CLI returned no quota pools"));
        buckets.push(bucket(
            "Quota",
            None,
            None,
            None,
            None,
            error,
            UsageSnapshotStatus::Stale,
        ));
    }
    let status = antigravity_snapshot_status(usage.as_ref(), credits_bucket.as_ref());
    let mut view = usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Antigravity")),
        // No UsageSurface variant exists for Antigravity yet (this lane is
        // constrained to mod lines + re-exports in usage.rs); patch the
        // provider label until the surface wiring lands.
        surface: UsageSurface::Unsupported,
        account_label: usage
            .as_ref()
            .and_then(|usage| usage.identity.clone())
            .unwrap_or_default(),
        username: None,
        plan_label: usage.as_ref().and_then(|usage| usage.plan.clone()),
        credential_origin: Some("CLI · agy (identity unverified)".to_owned()),
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            UsageSource::Cli
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            UsageConfidence::Authoritative
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::Fresh => credits_error,
            _ => usage_error,
        },
    });
    view.account.provider_label = "Antigravity".to_owned();
    view
}

/// View status for a passed version gate: `Fresh` only when the response
/// carried quota signal (parsed pools or a credits row). A parsed-but-
/// pool-less response is `Stale` — never `Fresh` with a lone `Stale`
/// placeholder bucket inside.
pub(crate) fn antigravity_snapshot_status(
    usage: Option<&AntigravityUsage>,
    credits_bucket: Option<&QuotaBucketView>,
) -> UsageSnapshotStatus {
    let has_signal = usage.is_some_and(|usage| !usage.pools.is_empty()) || credits_bucket.is_some();
    if has_signal {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Stale
    }
}

/// Status for a version-gate failure: a too-old binary predates JSON usage
/// (`Unsupported`), but a missing/unparseable binary is `NeedsSecret` — a
/// login cannot install a binary (Cursor/Gemini lanes agree). Pure so the
/// mapping is unit-testable without a live `agy`.
pub(crate) fn antigravity_version_error_status(error: &str) -> UsageSnapshotStatus {
    if error.contains("predates JSON") {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsSecret
    }
}

fn antigravity_status_view(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    status: UsageSnapshotStatus,
    error: &str,
) -> FocusedUsageView {
    let mut view = usage_view(UsageViewInput {
        agent,
        provider: provider.or(Some("Antigravity")),
        surface: UsageSurface::Unsupported,
        account_label: String::new(),
        username: None,
        plan_label: None,
        credential_origin: Some("CLI · agy".to_owned()),
        buckets: vec![bucket("Quota", None, None, None, None, Some(error), status)],
        status,
        source: UsageSource::None,
        confidence: UsageConfidence::None,
        now,
        last_error: Some(error.to_owned()),
    });
    view.account.provider_label = "Antigravity".to_owned();
    view
}

#[cfg(test)]
mod tests;
