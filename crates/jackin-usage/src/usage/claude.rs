//! `Claude` / `Anthropic` usage snapshot.
//!
//! Carved out of `usage.rs` during codebase-health-enforcement Workstream W5
//! (file-size ratchet). Items in this module are `pub(crate)` so the
//! coordinator (`usage.rs`) can re-export them.

#[allow(clippy::wildcard_imports)]
use super::*;
use serde::Deserialize;

/// Claude OAuth credential candidates, home-first — the single source of truth
/// for the path precedence, shared by `claude_snapshot` (token + identity) and
/// `claude_account_identity` (the shared-cache key) so the list can't drift.
pub(crate) fn claude_oauth_candidates(config: &Path) -> [PathBuf; 4] {
    [
        config.join(".credentials.json"),
        home_path(".claude/.credentials.json"),
        home_path(".claude.json"),
        PathBuf::from(CLAUDE_HANDOFF_CREDENTIALS_PATH),
    ]
}

/// Claude account identity (the `oauthAccount` email) from the same credential
/// candidates `claude_snapshot` uses, without fetching usage.
pub(crate) fn claude_account_identity() -> Option<String> {
    let config = env_dir_or_home("CLAUDE_CONFIG_DIR", ".claude");
    claude_oauth_candidates(&config)
        .iter()
        .find_map(|path| load_claude_account_email(path))
}

pub(crate) fn claude_snapshot(agent: &str, provider: Option<&str>, now: i64) -> FocusedUsageView {
    let config = env_dir_or_home("CLAUDE_CONFIG_DIR", ".claude");
    // Resolve the Claude OAuth token, home credentials first (the agent CLI
    // keeps the live token there and refreshes it in place). `~/.claude.json`
    // only carries `oauthAccount` metadata, never the token. The runtime-
    // forwarded handoff at `/jackin/claude/credentials.json` is the last-resort
    // fallback — mirroring the other providers (Codex/Amp/Kimi/Grok) — so the
    // snapshot does not silently drop to the impoverished CLI path when the
    // home copy lacks `claudeAiOauth.accessToken`. Matches CodexBar's order.
    let oauth_candidates = claude_oauth_candidates(&config);
    // One home-first walk yields the OAuth token (with its winning path, for
    // the `Auth:` origin — there is no keychain reader in the capsule, so the
    // origin names the file), the `oauthAccount` email, and the
    // `oauthAccount.organizationType` tier label, reading each file once.
    // account_label is the real email identity — empty when none, never a
    // fabricated auth-method string; the auth source lives on `credential_origin`.
    // `organizationType` (e.g. "claude_enterprise", "claude_max") is the account
    // tier; Enterprise/Team accounts carry a billing-mode `subscriptionType`
    // ("API Usage Billing") in the credentials file that is useless as a plan label.
    let (oauth_resolved, account_email, organization_type) = resolve_identity_with_extra(
        &oauth_candidates,
        claude_oauth_from_value,
        claude_email_from_value,
        claude_organization_type_from_value,
    );
    let (oauth_path, oauth) = oauth_resolved.unzip();
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|v| !v.is_empty());
    let auth_token = std::env::var("ANTHROPIC_AUTH_TOKEN")
        .ok()
        .filter(|v| !v.is_empty());
    let has_local_creds = config.join(".credentials.json").exists();
    let needs_login =
        api_key.is_none() && auth_token.is_none() && oauth.is_none() && !has_local_creds;
    let account = account_email.unwrap_or_default();
    // The displayed numbers come from the OAuth file token (the env keys are
    // never used for the fetch), so name the OAuth path that won first; fall
    // back to the env token only when no OAuth credential resolved.
    let credential_origin = if let Some(path) = oauth_path.as_deref() {
        Some(oauth_origin(path))
    } else if api_key.is_some() {
        Some("API token · env ANTHROPIC_API_KEY".to_owned())
    } else if auth_token.is_some() {
        Some("API token · env ANTHROPIC_AUTH_TOKEN".to_owned())
    } else {
        None
    };
    let (oauth_quota, oauth_error) = split_fetch(
        oauth
            .as_ref()
            .map(|credentials| fetch_claude_oauth_usage(&credentials.access_token)),
    );
    let (cli_usage, cli_error) =
        split_fetch((oauth_quota.is_none() && oauth.is_some()).then(fetch_claude_cli_usage));
    let provider_error = if oauth_quota.is_some() || cli_usage.is_some() {
        None
    } else {
        oauth_error.as_ref().or(cli_error.as_ref()).cloned()
    };
    let status = if needs_login {
        UsageSnapshotStatus::NeedsLogin
    } else if oauth_quota.is_some() || cli_usage.is_some() {
        UsageSnapshotStatus::Fresh
    } else {
        UsageSnapshotStatus::Stale
    };
    let bucket_status = status;
    let buckets = oauth_quota
        .map(|usage| usage.into_buckets(now))
        .or_else(|| cli_usage.as_ref().map(ClaudeCliUsage::buckets))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![
                bucket(
                    "Session",
                    None,
                    None,
                    None,
                    None,
                    provider_error.as_deref().or(Some("provider API pending")),
                    bucket_status,
                ),
                bucket(
                    "Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error.as_deref().or(Some("provider API pending")),
                    bucket_status,
                ),
                bucket(
                    "Daily Routines",
                    None,
                    None,
                    None,
                    None,
                    provider_error.as_deref().or(Some("provider API pending")),
                    bucket_status,
                ),
            ]
        });
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Claude,
        account_label: account,
        username: None,
        plan_label: organization_type
            .or_else(|| oauth.and_then(|credentials| credentials.subscription_type)),
        credential_origin,
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            if cli_usage.is_some() {
                UsageSource::Cli
            } else {
                UsageSource::ProviderApi
            }
        } else {
            UsageSource::None
        },
        confidence: if status == UsageSnapshotStatus::Fresh {
            // Class fix (P0): the rich OAuth snapshot is authoritative; the CLI
            // fallback is a reduced snapshot and must be marked degraded
            // (Estimated) with a reason, never presented as authoritative.
            if cli_usage.is_some() {
                UsageConfidence::Estimated
            } else {
                UsageConfidence::Authoritative
            }
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsLogin => {
                Some("Claude credentials not available to Capsule".to_owned())
            }
            UsageSnapshotStatus::Stale => Some(provider_error.unwrap_or_else(|| {
                "Claude provider usage unavailable; cached quota is stale".to_owned()
            })),
            // Degraded-but-fresh: the reduced CLI snapshot is showing because
            // the OAuth fetch failed — surface why, not a confident silence.
            _ if cli_usage.is_some() => Some(oauth_error.clone().unwrap_or_else(|| {
                "Claude OAuth usage unavailable; showing reduced CLI snapshot".to_owned()
            })),
            _ => None,
        },
    })
}

#[derive(Debug, Clone)]
pub(crate) struct ClaudeOAuthCredentials {
    pub(crate) access_token: String,
    pub(crate) subscription_type: Option<String>,
}

/// Claude account email (F12): `~/.claude.json` carries `oauthAccount` metadata
/// (never the token), and `CodexBar` reads the address from there. Returns the
/// trimmed `oauthAccount.emailAddress`, or `None` when absent.
pub(crate) fn claude_email_from_value(value: &serde_json::Value) -> Option<String> {
    let oauth = value.get("oauthAccount")?;
    oauth
        .get("emailAddress")
        .or_else(|| oauth.get("email_address"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|email| !email.is_empty())
        .map(str::to_owned)
}

/// Claude account tier from `oauthAccount.organizationType` in `~/.claude.json`.
///
/// Enterprise/Team accounts store their billing model in `subscriptionType`
/// ("API Usage Billing"), not the account tier. `organizationType` carries the
/// tier directly (e.g. `"claude_enterprise"`, `"claude_max"`, `"claude_team"`) and is
/// the authoritative source for the plan label shown in the TUI header.
pub(crate) fn claude_organization_type_from_value(value: &serde_json::Value) -> Option<String> {
    value
        .get("oauthAccount")?
        .get("organizationType")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(humanize_plan_label)
}

pub(crate) fn load_claude_account_email(path: &Path) -> Option<String> {
    claude_email_from_value(&read_json_file(path)?)
}

#[cfg(test)]
pub(crate) fn load_claude_organization_type(path: &Path) -> Option<String> {
    claude_organization_type_from_value(&read_json_file(path)?)
}

pub(crate) fn claude_oauth_from_value(value: &serde_json::Value) -> Option<ClaudeOAuthCredentials> {
    let oauth = value.get("claudeAiOauth")?;
    let access_token = oauth
        .get("accessToken")
        .or_else(|| oauth.get("access_token"))
        .and_then(serde_json::Value::as_str)?
        .trim()
        .to_owned();
    if access_token.is_empty() {
        return None;
    }
    let subscription_type = oauth
        .get("subscriptionType")
        .or_else(|| oauth.get("subscription_type"))
        .or_else(|| oauth.get("rateLimitTier"))
        .or_else(|| oauth.get("rate_limit_tier"))
        .and_then(serde_json::Value::as_str)
        .map(humanize_plan_label);
    Some(ClaudeOAuthCredentials {
        access_token,
        subscription_type,
    })
}

#[cfg(test)]
pub(crate) fn load_claude_oauth_credentials(path: &Path) -> Option<ClaudeOAuthCredentials> {
    claude_oauth_from_value(&read_json_file(path)?)
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthUsageResponse {
    #[serde(rename = "five_hour")]
    pub(crate) five_hour: Option<ClaudeOAuthUsageWindow>,
    // `seven_day` is the Weekly window. `seven_day_oauth_apps` is a SEPARATE
    // window the API also returns — it must NOT be aliased here (the API sends
    // both keys, so aliasing collides into a serde "duplicate field" and fails
    // the whole decode). It is not a CodexBar quota window, so it is ignored.
    #[serde(rename = "seven_day")]
    pub(crate) seven_day: Option<ClaudeOAuthUsageWindow>,
    #[serde(rename = "seven_day_sonnet")]
    pub(crate) seven_day_sonnet: Option<ClaudeOAuthUsageWindow>,
    #[serde(rename = "seven_day_opus")]
    pub(crate) seven_day_opus: Option<ClaudeOAuthUsageWindow>,
    #[serde(alias = "seven_day_claude_routines")]
    #[serde(alias = "claude_routines")]
    #[serde(alias = "routines")]
    #[serde(alias = "seven_day_cowork")]
    #[serde(rename = "seven_day_routines")]
    pub(crate) seven_day_routines: Option<ClaudeOAuthUsageWindow>,
    #[serde(rename = "extra_usage")]
    pub(crate) extra_usage: Option<ClaudeOAuthExtraUsage>,
    // The newer, self-describing money object. Preferred over `extra_usage`
    // because it states the unit scale (`exponent`) and currency explicitly, so
    // a minor-unit amount can never be mis-scaled. `extra_usage` is kept as a
    // fallback for responses that predate `spend`.
    #[serde(rename = "spend")]
    pub(crate) spend: Option<ClaudeOAuthSpend>,
    // Catch-all for the remaining keys — chiefly the rotating-codename dollar
    // budget windows (`amber_ladder`, `omelette_promotional`, …). Capturing
    // them generically, rather than enumerating each ephemeral name, is what
    // lets enterprise dollar budgets surface instead of being silently dropped
    // by a fixed-field struct.
    #[serde(flatten)]
    pub(crate) other_windows: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthSpend {
    pub(crate) used: Option<ClaudeOAuthMoney>,
    pub(crate) limit: Option<ClaudeOAuthMoney>,
    pub(crate) percent: Option<u8>,
    pub(crate) severity: Option<String>,
    pub(crate) enabled: Option<bool>,
    #[serde(rename = "disabled_reason")]
    pub(crate) disabled_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthMoney {
    #[serde(rename = "amount_minor")]
    pub(crate) amount_minor: Option<i64>,
    pub(crate) currency: Option<String>,
    pub(crate) exponent: Option<u8>,
}

impl ClaudeOAuthMoney {
    pub(crate) fn into_money(self) -> Option<Money> {
        Some(Money::new(
            self.amount_minor?,
            self.currency.unwrap_or_else(|| "credits".to_owned()),
            self.exponent.unwrap_or(2),
        ))
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthUsageWindow {
    pub(crate) utilization: Option<f64>,
    #[serde(rename = "resets_at")]
    pub(crate) resets_at: Option<String>,
    // Dollar-denominated budget windows (enterprise contractual allocations,
    // carried under rotating codename keys like `amber_ladder`). Named in
    // major-unit dollars by the API, so no `exponent` is supplied.
    #[serde(rename = "limit_dollars")]
    pub(crate) limit_dollars: Option<f64>,
    #[serde(rename = "used_dollars")]
    pub(crate) used_dollars: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthExtraUsage {
    #[serde(rename = "is_enabled")]
    pub(crate) is_enabled: Option<bool>,
    #[serde(rename = "monthly_limit")]
    pub(crate) monthly_limit: Option<f64>,
    #[serde(rename = "used_credits")]
    pub(crate) used_credits: Option<f64>,
    pub(crate) utilization: Option<f64>,
    pub(crate) currency: Option<String>,
    // Unit scale for `used_credits`/`monthly_limit`: they are MINOR units
    // (e.g. cents), so the major value is `value / 10^decimal_places`. Ignoring
    // this is what produced the 100×-too-large spend display.
    #[serde(rename = "decimal_places")]
    pub(crate) decimal_places: Option<u8>,
    #[serde(rename = "disabled_reason")]
    pub(crate) disabled_reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ClaudeCliUsage {
    pub(crate) session_used: Option<f64>,
    pub(crate) weekly_used: Option<f64>,
    pub(crate) sonnet_used: Option<f64>,
}

impl ClaudeCliUsage {
    pub(crate) fn buckets(&self) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        // The impoverished CLI fallback still fills the headline slots — tag them
        // so the status bar renders even when the OAuth fetch failed (the slot
        // is a semantic role, independent of which source produced the window).
        push_claude_cli_bucket(
            &mut buckets,
            "Session",
            Some(StatusSlot::Session),
            self.session_used,
        );
        push_claude_cli_bucket(
            &mut buckets,
            "Weekly",
            Some(StatusSlot::Weekly),
            self.weekly_used,
        );
        push_claude_cli_bucket(&mut buckets, "Sonnet", None, self.sonnet_used);
        buckets
    }
}

pub(crate) fn push_claude_cli_bucket(
    buckets: &mut Vec<QuotaBucketView>,
    label: &str,
    slot: Option<StatusSlot>,
    used: Option<f64>,
) {
    let Some(used) = used else {
        return;
    };
    buckets.push(with_status_slot(
        bucket(
            label,
            used_percent_label(used),
            Some("100%".to_owned()),
            remaining_from_fraction(used),
            None,
            None,
            UsageSnapshotStatus::Fresh,
        ),
        slot,
    ));
}

impl ClaudeOAuthUsageResponse {
    pub(crate) fn into_buckets(self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        push_claude_window(
            &mut buckets,
            "Session",
            Some(StatusSlot::Session),
            self.five_hour,
            now,
        );
        push_claude_window(
            &mut buckets,
            "Weekly",
            Some(StatusSlot::Weekly),
            self.seven_day,
            now,
        );
        push_claude_window(&mut buckets, "Sonnet", None, self.seven_day_sonnet, now);
        push_claude_window(&mut buckets, "Opus", None, self.seven_day_opus, now);
        push_claude_window(
            &mut buckets,
            "Daily Routines",
            None,
            self.seven_day_routines,
            now,
        );
        if let Some(spend) = claude_spend_bucket(self.spend, self.extra_usage) {
            buckets.push(spend);
        }
        push_claude_dollar_windows(&mut buckets, self.other_windows, now);
        buckets
    }
}

/// Surface rotating-codename dollar-budget windows (`amber_ladder` etc.) that a
/// fixed-field struct would drop. Each captured key is parsed as a window; only
/// those carrying a positive `limit_dollars` are real allocations and become a
/// (non-headline) dollar bucket labelled by the title-cased codename (the API
/// supplies no human name for these windows).
pub(crate) fn push_claude_dollar_windows(
    buckets: &mut Vec<QuotaBucketView>,
    other: BTreeMap<String, serde_json::Value>,
    now: i64,
) {
    for (key, value) in other {
        let Ok(window) = serde_json::from_value::<ClaudeOAuthUsageWindow>(value) else {
            continue;
        };
        let Some(limit) = window.limit_dollars.filter(|limit| *limit > 0.0) else {
            continue;
        };
        // `*_dollars` are major-unit dollars; scale to minor for Money.
        let used = window.used_dollars.unwrap_or(0.0).max(0.0);
        let used_money = Money::new((used * 100.0).round() as i64, "USD", 2);
        let limit_money = Money::new((limit * 100.0).round() as i64, "USD", 2);
        // `limit > 0.0` holds (filtered above), so the fraction is well-defined.
        let remaining_percent =
            Some(((1.0 - (used / limit).clamp(0.0, 1.0)) * 100.0).round() as u8);
        let reset_at = window.resets_at.as_deref().and_then(parse_iso_epoch);
        let mut view = timed_bucket(
            &humanize_window_label(&key),
            Some(format!("{used_money} spent")),
            Some(limit_money.to_string()),
            remaining_percent,
            reset_at,
            now,
            remaining_percent
                .map(|remaining| format!("{}% used", 100u8.saturating_sub(remaining)))
                .as_deref(),
            UsageSnapshotStatus::Fresh,
        );
        view.used_money = Some(used_money);
        view.limit_money = Some(limit_money);
        buckets.push(view);
    }
}

/// The normalized inputs for the monetary "Extra usage" bucket, derived from
/// whichever source the API provided.
pub(crate) struct ClaudeSpend {
    pub(crate) used: Money,
    pub(crate) limit: Option<Money>,
    /// Percent of the cap already spent (0..=100).
    pub(crate) used_percent: Option<u8>,
    pub(crate) enabled: bool,
    pub(crate) disabled_reason: Option<String>,
    pub(crate) severity: UsageSeverity,
}

/// Build the monetary spend bucket from the API response.
///
/// Prefers the self-describing `spend{}` object (it carries `amount_minor` +
/// `exponent`, so the scale is unambiguous); falls back to `extra_usage`,
/// scaling `used_credits`/`monthly_limit` by `decimal_places`. Both paths feed
/// one [`Money`]-typed builder, so spend can never be rendered 100× too large
/// regardless of source. A disabled (e.g. out-of-credits) bucket is still
/// surfaced — with its reason — rather than silently dropped, so the cap stays
/// visible the way the web console shows it.
pub(crate) fn claude_spend_bucket(
    spend: Option<ClaudeOAuthSpend>,
    extra: Option<ClaudeOAuthExtraUsage>,
) -> Option<QuotaBucketView> {
    let spend = normalize_claude_spend(spend, extra)?;
    let remaining_percent = spend.used_percent.map(|used| 100u8.saturating_sub(used));
    let used_label = Some(format!("{} spent", spend.used));
    let limit_label = spend.limit.as_ref().map(Money::to_string);
    let pace = if spend.enabled {
        spend.used_percent.map(|used| format!("{used}% used"))
    } else {
        Some(match &spend.disabled_reason {
            Some(reason) => format!("disabled · {}", humanize_reason(reason)),
            None => "disabled".to_owned(),
        })
    };
    let mut view = bucket(
        "Extra usage",
        used_label,
        limit_label,
        remaining_percent,
        None,
        pace.as_deref(),
        UsageSnapshotStatus::Fresh,
    );
    view.status_slot = Some(StatusSlot::Spend);
    view.severity = spend.severity;
    view.used_money = Some(spend.used);
    view.limit_money = spend.limit;
    Some(view)
}

pub(crate) fn normalize_claude_spend(
    spend: Option<ClaudeOAuthSpend>,
    extra: Option<ClaudeOAuthExtraUsage>,
) -> Option<ClaudeSpend> {
    if let Some(spend) = spend
        && let Some(used) = spend.used.and_then(ClaudeOAuthMoney::into_money)
    {
        return Some(ClaudeSpend {
            used,
            limit: spend.limit.and_then(ClaudeOAuthMoney::into_money),
            used_percent: spend.percent.map(|percent| percent.min(100)),
            enabled: spend.enabled.unwrap_or(true),
            disabled_reason: spend.disabled_reason,
            severity: severity_from_label(spend.severity.as_deref()),
        });
    }
    let extra = extra?;
    let used_credits = extra.used_credits?;
    let exponent = extra.decimal_places.unwrap_or(2);
    let currency = extra.currency.unwrap_or_else(|| "credits".to_owned());
    let used = Money::new(used_credits.round() as i64, &currency, exponent);
    let limit = extra
        .monthly_limit
        .map(|limit| Money::new(limit.round() as i64, &currency, exponent));
    Some(ClaudeSpend {
        used,
        limit,
        used_percent: extra.utilization.and_then(used_percent_from_fraction),
        enabled: extra.is_enabled.unwrap_or(true),
        disabled_reason: extra.disabled_reason,
        severity: UsageSeverity::Normal,
    })
}

pub(crate) fn push_claude_window(
    buckets: &mut Vec<QuotaBucketView>,
    label: &str,
    slot: Option<StatusSlot>,
    window: Option<ClaudeOAuthUsageWindow>,
    now: i64,
) {
    let Some(window) = window else {
        return;
    };
    let reset_at = window.resets_at.as_deref().and_then(parse_iso_epoch);
    let window_seconds = claude_window_seconds(label);
    let remaining = window.utilization.and_then(remaining_from_fraction);
    let pace = quota_pace_label(remaining, reset_at, window_seconds, now);
    buckets.push(with_status_slot(
        timed_bucket(
            label,
            window.utilization.and_then(used_percent_label),
            Some("100%".to_owned()),
            remaining,
            reset_at,
            now,
            pace.as_deref(),
            UsageSnapshotStatus::Fresh,
        ),
        slot,
    ));
}

pub(crate) fn claude_window_seconds(label: &str) -> Option<i64> {
    match label {
        "Session" => Some(5 * 60 * 60),
        "Weekly" => Some(7 * 24 * 60 * 60),
        _ => None,
    }
}

pub(crate) fn fetch_claude_oauth_usage(
    access_token: &str,
) -> Result<ClaudeOAuthUsageResponse, String> {
    let user_agent = claude_code_user_agent();
    get_json_bearer(
        "Claude OAuth usage",
        "https://api.anthropic.com/api/oauth/usage",
        access_token,
        &[
            (reqwest::header::CONTENT_TYPE, "application/json"),
            (
                reqwest::header::HeaderName::from_static("anthropic-beta"),
                "oauth-2025-04-20",
            ),
            // The OAuth usage endpoint is gated to the Claude Code client UA;
            // a generic UA is rejected.
            (reqwest::header::USER_AGENT, &user_agent),
        ],
    )
}

pub(crate) fn claude_code_user_agent() -> String {
    // The Claude Code version is stable for the process lifetime, so resolve the
    // UA once instead of spawning `claude --version` on every usage fetch — that
    // per-probe subprocess was a measurable slice of the load latency (Bug 3).
    static CACHED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHED
        .get_or_init(|| {
            claude_code_user_agent_with(|command, args, timeout| {
                run_cli_with_timeout_full(command, args, timeout)
            })
            .unwrap_or_else(|| CLAUDE_CODE_USER_AGENT_FALLBACK.to_owned())
        })
        .clone()
}

pub(crate) fn claude_code_user_agent_with<F>(mut runner: F) -> Option<String>
where
    F: FnMut(&str, &[&str], Duration) -> Result<CliOutput, String>,
{
    let output = runner("claude", &["--version"], CLAUDE_VERSION_TIMEOUT).ok()?;
    if !output.success {
        return None;
    }
    let text = format!("{}\n{}", output.stdout, output.stderr);
    claude_code_version_from_text(&text).map(|version| format!("claude-code/{version}"))
}

pub(crate) fn claude_code_version_from_text(text: &str) -> Option<String> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '-'))
        .find(|part| {
            let mut segments = part.split('.');
            matches!(
                (segments.next(), segments.next(), segments.next()),
                (Some(major), Some(minor), Some(patch))
                    if major.chars().all(|ch| ch.is_ascii_digit())
                        && minor.chars().all(|ch| ch.is_ascii_digit())
                        && patch.chars().all(|ch| ch.is_ascii_digit())
            )
        })
        .map(str::to_owned)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeUsageDiagnostic {
    pub command: String,
    pub args: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub fetched_at_epoch: i64,
}

pub(crate) fn fetch_claude_cli_usage() -> Result<ClaudeCliUsage, String> {
    let diagnostic = run_claude_usage_diagnostic()?;
    if !diagnostic.success {
        return Err(format!(
            "Claude CLI usage exited with status {:?}",
            diagnostic.exit_code
        ));
    }
    parse_claude_usage_output(&diagnostic.stdout)
        .ok_or_else(|| "Claude CLI usage output was not recognized".to_owned())
}
