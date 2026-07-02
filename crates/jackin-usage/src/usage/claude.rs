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
    // Authoritative shape for Session / "All models" Weekly / per-model Weekly
    // (Fable, and future model-scoped limits). The API migrated model-specific
    // windows here: the legacy `seven_day_sonnet`/`seven_day_opus` keys are
    // still returned but `null` on current accounts — the data lives only in
    // `limits` as `weekly_scoped` entries. Surfaced generically so a new model
    // codename (Fable today, others tomorrow) appears without per-model code.
    #[serde(default)]
    pub(crate) limits: Vec<ClaudeOAuthLimit>,
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

/// One entry in the `limits` array — the authoritative shape for Session,
/// "All models" Weekly, and per-model Weekly (Fable, and future model-scoped
/// limits). `percent` is already-scaled (0..=100); `kind` selects the bucket
/// (`session` | `weekly_all` | `weekly_scoped`); `scope.model.display_name`
/// labels a `weekly_scoped` window. `severity` mirrors the web console's meter
/// color and maps to [`UsageSeverity`].
#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthLimit {
    pub(crate) kind: Option<String>,
    // `group` ("session"/"weekly") duplicates the window duration `kind`
    // already implies, so it is kept for API-shape completeness, not read.
    #[expect(
        dead_code,
        reason = "API-shape field kept for completeness; window duration derives from `kind`"
    )]
    pub(crate) group: Option<String>,
    pub(crate) percent: Option<u8>,
    pub(crate) severity: Option<String>,
    #[serde(rename = "resets_at")]
    pub(crate) resets_at: Option<String>,
    pub(crate) scope: Option<ClaudeOAuthLimitScope>,
    // Surfaced by the API but not yet rendered: today it flags the currently
    // binding constraint, not whether to show the row (Session/All-models are
    // `false` while a scoped model is `true`), so it must not gate visibility.
    #[serde(rename = "is_active", default)]
    #[expect(
        dead_code,
        reason = "API-shape field kept for completeness; not gated on yet"
    )]
    pub(crate) is_active: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthLimitScope {
    pub(crate) model: Option<ClaudeOAuthLimitModel>,
    #[expect(
        dead_code,
        reason = "API-shape field kept for completeness; not rendered yet"
    )]
    pub(crate) surface: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ClaudeOAuthLimitModel {
    #[expect(
        dead_code,
        reason = "API-shape field kept for completeness; label uses display_name"
    )]
    pub(crate) id: Option<String>,
    #[serde(rename = "display_name")]
    pub(crate) display_name: Option<String>,
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
    /// Per-model weekly windows the CLI prints as `Current week (<model>): …`
    /// (Fable today; future model codenames). Each entry is `(model label,
    /// percent used)`. Distinct from `sonnet_used`, which preserves the legacy
    /// `(Sonnet only)` line and its "Sonnet" bucket label.
    pub(crate) scoped_weekly: Vec<(String, f64)>,
}

impl ClaudeCliUsage {
    pub(crate) fn buckets(&self) -> Vec<QuotaBucketView> {
        // The CLI fallback reuses the same unified window model + builder as
        // the OAuth path, so a CLI "Weekly" line and an OAuth `weekly_all`
        // limit render identically (headline slot, over-cap label). CLI windows
        // carry no timestamps, so `now` is unused for pace/reset formatting.
        let mut windows: Vec<ClaudeQuotaWindow> = Vec::new();
        if let Some(used) = self.session_used {
            windows.push(ClaudeQuotaWindow::headline(
                "Session",
                StatusSlot::Session,
                used,
                Some(CLAUDE_SESSION_WINDOW_SECONDS),
            ));
        }
        if let Some(used) = self.weekly_used {
            windows.push(ClaudeQuotaWindow::headline(
                "Weekly",
                StatusSlot::Weekly,
                used,
                Some(CLAUDE_WEEKLY_WINDOW_SECONDS),
            ));
        }
        if let Some(used) = self.sonnet_used {
            windows.push(ClaudeQuotaWindow::scoped("Sonnet", used));
        }
        for (label, used) in &self.scoped_weekly {
            windows.push(ClaudeQuotaWindow::scoped(label, *used));
        }
        windows.into_iter().map(|w| w.into_bucket(0)).collect()
    }
}

/// Session (5-hour) window duration, shared by every source that produces one.
const CLAUDE_SESSION_WINDOW_SECONDS: i64 = 5 * 60 * 60;
/// Weekly window duration, shared by every source (`weekly_all`,
/// `weekly_scoped`, legacy `seven_day*`).
const CLAUDE_WEEKLY_WINDOW_SECONDS: i64 = 7 * 24 * 60 * 60;

/// One normalized Claude quota window — the single intermediate shape every
/// utilization source feeds before it becomes a [`QuotaBucketView`]. The
/// authoritative `limits` array, the legacy named windows (`seven_day*`), and
/// the `claude -p /usage` CLI fallback all produce `ClaudeQuotaWindow`s, so a
/// Session window, a Fable `weekly_scoped` limit, a legacy Sonnet window, and a
/// CLI "Weekly" line share one builder instead of three near-identical ones.
/// Fable is not a special case here — it is just another `weekly_scoped` entry.
#[derive(Debug, Clone)]
pub(crate) struct ClaudeQuotaWindow {
    pub(crate) label: String,
    pub(crate) slot: Option<StatusSlot>,
    /// Used fraction on the scale the shared helpers expect: a raw
    /// `utilization` (fraction-or-percent) for legacy/CLI sources, or
    /// `f64::from(percent)` for `limits`. `used_percent_label` and
    /// `remaining_from_fraction` resolve the fraction-vs-percent ambiguity, so
    /// both source shapes flow through unchanged.
    pub(crate) used: Option<f64>,
    pub(crate) reset_at: Option<i64>,
    pub(crate) window_seconds: Option<i64>,
    pub(crate) severity: UsageSeverity,
}

impl ClaudeQuotaWindow {
    /// A non-headline window with no reset/pace data (the CLI fallback shape).
    fn scoped(label: &str, used: f64) -> Self {
        Self {
            label: label.to_owned(),
            slot: None,
            used: Some(used),
            reset_at: None,
            window_seconds: None,
            severity: UsageSeverity::Normal,
        }
    }

    /// A headline window with a duration (so pace can be computed when the
    /// source also carries a reset). Used by the CLI Session/Weekly lines.
    fn headline(label: &str, slot: StatusSlot, used: f64, window_seconds: Option<i64>) -> Self {
        Self {
            label: label.to_owned(),
            slot: Some(slot),
            used: Some(used),
            reset_at: None,
            window_seconds,
            severity: UsageSeverity::Normal,
        }
    }

    /// The one bucket builder for every Claude utilization source. The used
    /// label is uncapped (a window over its limit renders `150% used` while
    /// `remaining` clamps at 0); pace is computed only when both a reset and a
    /// window duration are known; severity mirrors the API for meter color.
    pub(crate) fn into_bucket(self, now: i64) -> QuotaBucketView {
        let remaining = self.used.and_then(remaining_from_fraction);
        let pace = quota_pace_label(remaining, self.reset_at, self.window_seconds, now);
        let mut view = timed_bucket(
            &self.label,
            self.used.and_then(used_percent_label),
            Some("100%".to_owned()),
            remaining,
            self.reset_at,
            now,
            pace.as_deref(),
            UsageSnapshotStatus::Fresh,
        );
        view.status_slot = self.slot;
        view.severity = self.severity;
        view
    }
}

impl ClaudeOAuthUsageWindow {
    /// Normalize a legacy named window (`five_hour`, `seven_day*`) into the
    /// unified quota model. `slot` and `window_seconds` carry the semantic the
    /// fixed field name can't (Session/Weekly headline + duration for pace), so
    /// a legacy weekly Sonnet window is paced the same way as a `weekly_scoped`
    /// Fable limit — uniform handling across API generations.
    fn into_quota(
        self,
        label: &str,
        slot: Option<StatusSlot>,
        window_seconds: Option<i64>,
    ) -> ClaudeQuotaWindow {
        ClaudeQuotaWindow {
            label: label.to_owned(),
            slot,
            used: self.utilization,
            reset_at: self.resets_at.as_deref().and_then(parse_iso_epoch),
            window_seconds,
            // Legacy named windows carry no severity field; the API meter
            // color only arrived with `limits`.
            severity: UsageSeverity::Normal,
        }
    }
}

impl ClaudeOAuthLimit {
    /// Normalize a `limits`-array entry into the unified quota model. Returns
    /// `None` for an entry without a usable shape: a missing `percent`, an
    /// unknown `kind`, or a `weekly_scoped` window whose model has no display
    /// name (omitted, never fabricated into an empty-label row).
    fn as_quota(&self) -> Option<ClaudeQuotaWindow> {
        let percent = self.percent?;
        let (label, slot, window_seconds) = match self.kind.as_deref()? {
            "session" => (
                "Session".to_owned(),
                Some(StatusSlot::Session),
                Some(CLAUDE_SESSION_WINDOW_SECONDS),
            ),
            "weekly_all" => (
                "All models".to_owned(),
                Some(StatusSlot::Weekly),
                Some(CLAUDE_WEEKLY_WINDOW_SECONDS),
            ),
            "weekly_scoped" => (
                self.scoped_label()?,
                None,
                Some(CLAUDE_WEEKLY_WINDOW_SECONDS),
            ),
            _ => return None,
        };
        Some(ClaudeQuotaWindow {
            label,
            slot,
            used: Some(f64::from(percent)),
            reset_at: self.resets_at.as_deref().and_then(parse_iso_epoch),
            window_seconds,
            severity: severity_from_label(self.severity.as_deref()),
        })
    }

    /// The model display name for a `weekly_scoped` limit, trimmed and
    /// non-empty; `None` when the API supplied no name.
    fn scoped_label(&self) -> Option<String> {
        self.scope
            .as_ref()
            .and_then(|scope| scope.model.as_ref())
            .and_then(|model| model.display_name.as_deref())
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
    }
}

impl ClaudeOAuthUsageResponse {
    pub(crate) fn into_buckets(self, now: i64) -> Vec<QuotaBucketView> {
        // Destructure so the spend/dollar data is moved out before the
        // utilization windows consume the rest — one source of truth, one
        // builder, regardless of whether the windows came from `limits` or the
        // legacy named keys.
        let Self {
            five_hour,
            seven_day,
            seven_day_sonnet,
            seven_day_opus,
            seven_day_routines,
            limits,
            extra_usage,
            spend,
            other_windows,
        } = self;
        // The `limits` array is authoritative on current accounts; the legacy
        // named windows are the fallback for responses that predate `limits`.
        // This source preference is not a parallel implementation — both arms
        // produce the same `ClaudeQuotaWindow` type — it only avoids
        // double-counting, because the live API returns both carriers with
        // identical data, so they cannot be merged.
        let windows: Vec<ClaudeQuotaWindow> = if limits.is_empty() {
            legacy_claude_quota_windows(
                five_hour,
                seven_day,
                seven_day_sonnet,
                seven_day_opus,
                seven_day_routines,
            )
        } else {
            limits
                .iter()
                .filter_map(ClaudeOAuthLimit::as_quota)
                .collect()
        };
        let mut buckets: Vec<QuotaBucketView> =
            windows.into_iter().map(|w| w.into_bucket(now)).collect();
        if let Some(spend) = claude_spend_bucket(spend, extra_usage) {
            buckets.push(spend);
        }
        push_claude_dollar_windows(&mut buckets, other_windows, now);
        buckets
    }
}

/// Legacy pre-`limits` named windows normalized to the unified quota model, so
/// they share one builder with `limits`-sourced windows. Weekly-scoped windows
/// (Sonnet/Opus/Routines) get the weekly duration so they are paced uniformly
/// with a `weekly_scoped` Fable limit.
#[allow(clippy::too_many_arguments)]
fn legacy_claude_quota_windows(
    five_hour: Option<ClaudeOAuthUsageWindow>,
    seven_day: Option<ClaudeOAuthUsageWindow>,
    seven_day_sonnet: Option<ClaudeOAuthUsageWindow>,
    seven_day_opus: Option<ClaudeOAuthUsageWindow>,
    seven_day_routines: Option<ClaudeOAuthUsageWindow>,
) -> Vec<ClaudeQuotaWindow> {
    let session = Some(CLAUDE_SESSION_WINDOW_SECONDS);
    let weekly = Some(CLAUDE_WEEKLY_WINDOW_SECONDS);
    let mut windows = Vec::new();
    if let Some(window) = five_hour {
        windows.push(window.into_quota("Session", Some(StatusSlot::Session), session));
    }
    if let Some(window) = seven_day {
        windows.push(window.into_quota("Weekly", Some(StatusSlot::Weekly), weekly));
    }
    if let Some(window) = seven_day_sonnet {
        windows.push(window.into_quota("Sonnet", None, weekly));
    }
    if let Some(window) = seven_day_opus {
        windows.push(window.into_quota("Opus", None, weekly));
    }
    if let Some(window) = seven_day_routines {
        windows.push(window.into_quota("Daily Routines", None, weekly));
    }
    windows
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
