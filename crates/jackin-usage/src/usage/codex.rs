//! `Codex` / `OpenAI` usage snapshot.
//!
//! Carved out of `usage.rs` during codebase-health-enforcement Workstream W5
//! (file-size ratchet). Items in this module are `pub(crate)` so the
//! coordinator (`usage.rs`) can re-export them.

#[allow(clippy::wildcard_imports)]
use super::*;
use serde::Deserialize;

/// Codex auth credential candidates (home auth first, forwarded handoff last) —
/// shared by `codex_snapshot` and `codex_account_identity`.
pub(crate) fn codex_auth_candidates(codex_home: &Path) -> [PathBuf; 2] {
    [
        codex_home.join("auth.json"),
        PathBuf::from(CODEX_HANDOFF_AUTH_PATH),
    ]
}

/// Codex account identity (`account_id`, else the account label) from the same
/// auth candidates `codex_snapshot` uses, without fetching usage.
pub(crate) fn codex_account_identity() -> Option<String> {
    let codex_home = env_dir_or_home("CODEX_HOME", ".codex");
    codex_auth_candidates(&codex_home).iter().find_map(|path| {
        let creds = codex_oauth_from_value(&read_json_file(path)?)?;
        creds.account_id.or(creds.account_label)
    })
}

/// Map a Codex/`ChatGPT` `plan_type` to its display name, mirroring `CodexBar`'s
/// `CodexPlanFormatting.displayName` (F7a): `pro` → `Pro 20x`, the pro-lite
/// variants → `Pro 5x`, machine identifiers humanized (`enterprise_cbp_usage_based`
/// → `Enterprise CBP Usage Based`), already-readable text preserved. Returns
/// `None` for blank input so an unknown plan is omitted, never shown as `pro`.
pub(crate) fn codex_plan_display_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(exact) = codex_plan_exact_display(trimmed) {
        return Some(exact);
    }
    // Strip boilerplate words (claude/codex/account/plan) the way CodexBar's
    // `UsageFormatter.cleanPlanName` does, then re-check the exact map.
    let cleaned = trimmed
        .split_whitespace()
        .filter(|word| {
            !matches!(
                word.to_ascii_lowercase().as_str(),
                "claude" | "codex" | "account" | "plan"
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return Some(trimmed.to_owned());
    }
    if let Some(exact) = codex_plan_exact_display(cleaned) {
        return Some(exact);
    }
    let formatted = humanize_words_with(cleaned, codex_plan_word_display);
    if formatted.is_empty() {
        return Some(cleaned.to_owned());
    }
    Some(formatted)
}

pub(crate) fn codex_plan_exact_display(value: &str) -> Option<String> {
    match value.to_ascii_lowercase().as_str() {
        "pro" => Some("Pro 20x".to_owned()),
        "prolite" | "pro_lite" | "pro-lite" | "pro lite" => Some("Pro 5x".to_owned()),
        _ => None,
    }
}

pub(crate) fn codex_plan_word_display(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if matches!(lower.as_str(), "cbp" | "k12") {
        return lower.to_ascii_uppercase();
    }
    // Preserve existing acronyms (all-caps with a letter, e.g. "AI").
    if raw == raw.to_ascii_uppercase() && raw.chars().any(char::is_alphabetic) {
        return raw.to_owned();
    }
    titlecase_ascii(raw)
}

pub(crate) fn codex_snapshot(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    rpc_gate: &mut ManagedCliLaunchGate,
) -> FocusedUsageView {
    let codex_home = env_dir_or_home("CODEX_HOME", ".codex");
    // Home auth first, runtime-forwarded handoff last; one walk yields the
    // credential (with its winning path, for the `Auth:` origin) and the account
    // label, reading each file once.
    let codex_candidates = codex_auth_candidates(&codex_home);
    let (resolved, account_from_file) = resolve_identity(
        &codex_candidates,
        codex_oauth_from_value,
        codex_account_from_value,
    );
    let (oauth_path, credentials) = resolved.unzip();
    // account_label is the email identity only; the auth source (the resolver
    // arm that actually won) goes on `credential_origin`.
    let auth_email = credentials
        .as_ref()
        .and_then(|credentials| credentials.account_label.clone())
        .or(account_from_file);
    let has_env_key = std::env::var("OPENAI_API_KEY").is_ok_and(|v| !v.is_empty());
    let needs_login = credentials.is_none() && auth_email.is_none() && !has_env_key;
    let credential_origin = if let Some(path) = oauth_path.as_deref() {
        Some(oauth_origin(path))
    } else if has_env_key {
        Some("API token · env OPENAI_API_KEY".to_owned())
    } else {
        None
    };
    let (rpc_usage, rpc_error) = match fetch_codex_rpc_usage(rpc_gate) {
        Ok(usage) => (Some(usage), None),
        Err(error) => (None, Some(error)),
    };
    let rpc_quota = rpc_usage.as_ref().map(|usage| &usage.response);
    let (oauth_quota, oauth_error) = split_fetch(credentials.as_ref().map(|credentials| {
        fetch_codex_oauth_usage_refreshing(credentials, &codex_home).map(|mut usage| {
            usage.reset_credits = fetch_codex_oauth_reset_credits(credentials, &codex_home)
                .inspect_err(|error| {
                    crate::cdebug!("codex reset-credits fetch failed: {error}");
                })
                .ok();
            usage
        })
    }));
    let provider_error = rpc_error.as_ref().or(oauth_error.as_ref()).cloned();
    let quota = rpc_quota.or(oauth_quota.as_ref());
    let account = rpc_usage
        .as_ref()
        .and_then(|usage| usage.account_label.clone())
        .or(auth_email)
        .unwrap_or_default();
    let status = if needs_login {
        UsageSnapshotStatus::NeedsLogin
    } else if quota.is_some() {
        UsageSnapshotStatus::Fresh
    } else if provider_error
        .as_deref()
        .is_some_and(usage_error_is_unauthorized)
    {
        // The on-disk token is present but rejected (expired/revoked). Codex
        // refreshes its own token on launch; jackin reads the token as-is, so a
        // stale `auth.json` 401s here. Surface an honest "login" rather than a
        // blank/stale meter — the root cause (no in-process refresh) is named in
        // FINDINGS §9.2 E2.
        UsageSnapshotStatus::NeedsLogin
    } else {
        UsageSnapshotStatus::Stale
    };
    let buckets = quota
        .map(|usage| usage.buckets(now))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![
                bucket(
                    "Session",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("app-server/OAuth quota pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
                bucket(
                    "Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error
                        .as_deref()
                        .or(Some("app-server/OAuth quota pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
                bucket(
                    "Codex Spark 5-hour",
                    None,
                    None,
                    None,
                    None,
                    provider_error.as_deref().or(Some("provider API pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
                bucket(
                    "Codex Spark Weekly",
                    None,
                    None,
                    None,
                    None,
                    provider_error.as_deref().or(Some("provider API pending")),
                    UsageSnapshotStatus::Unsupported,
                ),
            ]
        });
    usage_view(UsageViewInput {
        agent,
        provider,
        surface: UsageSurface::Codex,
        account_label: account,
        username: None,
        plan_label: quota
            .and_then(|usage| usage.plan_type.as_deref())
            .and_then(codex_plan_display_name),
        credential_origin,
        buckets,
        status,
        source: if status == UsageSnapshotStatus::Fresh {
            if rpc_quota.is_some() {
                UsageSource::Cli
            } else {
                UsageSource::ProviderApi
            }
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
            UsageSnapshotStatus::NeedsLogin => {
                Some("Codex auth not available to Capsule".to_owned())
            }
            UsageSnapshotStatus::Stale => Some(provider_error.unwrap_or_else(|| {
                "Codex provider usage unavailable; cached quota is stale".to_owned()
            })),
            _ => None,
        },
    })
}

#[derive(Debug, Clone)]
pub(crate) struct CodexOAuthCredentials {
    pub(crate) access_token: String,
    pub(crate) account_id: Option<String>,
    pub(crate) account_label: Option<String>,
    /// OAuth refresh token, when present, used to re-mint a rejected
    /// `access_token` in place for a single retry (see
    /// `fetch_codex_oauth_usage_refreshing`).
    pub(crate) refresh_token: Option<String>,
}

#[cfg(test)]
pub(crate) fn load_codex_oauth_credentials(path: &Path) -> Option<CodexOAuthCredentials> {
    codex_oauth_from_value(&read_json_file(path)?)
}

pub(crate) fn codex_oauth_from_value(value: &serde_json::Value) -> Option<CodexOAuthCredentials> {
    if let Some(api_key) = value
        .get("OPENAI_API_KEY")
        .and_then(serde_json::Value::as_str)
        && !api_key.trim().is_empty()
    {
        return Some(CodexOAuthCredentials {
            access_token: api_key.trim().to_owned(),
            account_id: None,
            account_label: Some("OPENAI_API_KEY".to_owned()),
            // A static API key cannot be refreshed; there is nothing to re-mint.
            refresh_token: None,
        });
    }
    let tokens = value.get("tokens")?;
    let access_token = tokens
        .get("access_token")
        .or_else(|| tokens.get("accessToken"))
        .and_then(serde_json::Value::as_str)?
        .trim()
        .to_owned();
    if access_token.is_empty() {
        return None;
    }
    let account_id = tokens
        .get("account_id")
        .or_else(|| tokens.get("accountId"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let account_label = tokens
        .get("id_token")
        .or_else(|| tokens.get("idToken"))
        .and_then(serde_json::Value::as_str)
        .and_then(codex_account_label_from_id_token)
        .or_else(|| {
            tokens
                .get("account_id")
                .or_else(|| tokens.get("accountId"))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        });
    let refresh_token = tokens
        .get("refresh_token")
        .or_else(|| tokens.get("refreshToken"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    Some(CodexOAuthCredentials {
        access_token,
        account_id,
        account_label,
        refresh_token,
    })
}

pub(crate) fn codex_account_label_from_id_token(token: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    first_string_key(&value, "email")
        .or_else(|| first_string_key(&value, "preferred_username"))
        .or_else(|| first_string_key(&value, "name"))
        .or_else(|| first_string_key(&value, "sub").map(|sub| format!("ChatGPT account {sub}")))
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexUsageResponse {
    #[serde(rename = "plan_type")]
    pub(crate) plan_type: Option<String>,
    #[serde(rename = "rate_limit")]
    pub(crate) rate_limit: Option<CodexRateLimitDetails>,
    pub(crate) credits: Option<CodexCreditDetails>,
    #[serde(rename = "additional_rate_limits")]
    pub(crate) additional_rate_limits: Option<Vec<CodexAdditionalRateLimit>>,
    #[serde(skip)]
    pub(crate) reset_credits: Option<CodexResetCredits>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRateLimitDetails {
    #[serde(rename = "primary_window")]
    pub(crate) primary_window: Option<CodexWindowSnapshot>,
    #[serde(rename = "secondary_window")]
    pub(crate) secondary_window: Option<CodexWindowSnapshot>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexWindowSnapshot {
    #[serde(rename = "used_percent")]
    pub(crate) used_percent: Option<u8>,
    #[serde(rename = "reset_at")]
    pub(crate) reset_at: Option<i64>,
    #[serde(rename = "limit_window_seconds")]
    pub(crate) limit_window_seconds: Option<i64>,
    #[serde(skip)]
    pub(crate) window_duration_mins: Option<i64>,
}

impl CodexWindowSnapshot {
    pub(crate) fn from_rpc(window: CodexRpcRateLimitWindow) -> Self {
        Self {
            used_percent: {
                #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0 above")]
                {
                    Some(window.used_percent.round().clamp(0.0, 100.0) as u8)
                }
            },
            reset_at: window.resets_at,
            limit_window_seconds: None,
            window_duration_mins: window.window_duration_mins,
        }
    }

    pub(crate) fn window_label(&self) -> Option<String> {
        let minutes = self
            .window_duration_mins
            .or_else(|| self.limit_window_seconds.map(|seconds| seconds / 60))?;
        window_minutes_label(minutes)
    }

    pub(crate) fn window_seconds(&self) -> Option<i64> {
        self.limit_window_seconds
            .or_else(|| self.window_duration_mins.map(|minutes| minutes * 60))
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexCreditDetails {
    #[serde(rename = "has_credits")]
    pub(crate) has_credits: Option<bool>,
    pub(crate) unlimited: Option<bool>,
    pub(crate) balance: Option<serde_json::Value>,
}

impl CodexCreditDetails {
    pub(crate) fn from_rpc(credits: CodexRpcCredits) -> Self {
        Self {
            has_credits: Some(credits.has_credits),
            unlimited: Some(credits.unlimited),
            balance: credits.balance.map(serde_json::Value::String),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexAdditionalRateLimit {
    #[serde(rename = "limit_name")]
    pub(crate) limit_name: Option<String>,
    #[serde(rename = "metered_feature")]
    pub(crate) metered_feature: Option<String>,
    #[serde(rename = "rate_limit")]
    pub(crate) rate_limit: Option<CodexRateLimitDetails>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcAccountResponse {
    pub(crate) account: Option<CodexRpcAccountDetails>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum CodexRpcAccountDetails {
    #[serde(rename = "apikey")]
    ApiKey,
    Chatgpt {
        email: Option<String>,
        #[serde(rename = "planType")]
        plan_type: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcRateLimitsResponse {
    #[serde(rename = "rateLimits")]
    pub(crate) rate_limits: CodexRpcRateLimits,
    // Per-limit-id windows. Every entry other than the main "codex" limit
    // (already surfaced as Session/Weekly) is an extra limit — the
    // "…Codex-Spark" entry carries the Codex Spark 5-hour/Weekly windows.
    #[serde(rename = "rateLimitsByLimitId", default)]
    pub(crate) rate_limits_by_limit_id: BTreeMap<String, CodexRpcLimitEntry>,
    #[serde(rename = "rateLimitResetCredits")]
    pub(crate) reset_credits: Option<CodexRpcResetCredits>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcLimitEntry {
    #[serde(rename = "limitId")]
    pub(crate) limit_id: Option<String>,
    #[serde(rename = "limitName")]
    pub(crate) limit_name: Option<String>,
    pub(crate) primary: Option<CodexRpcRateLimitWindow>,
    pub(crate) secondary: Option<CodexRpcRateLimitWindow>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcResetCredits {
    #[serde(rename = "availableCount")]
    pub(crate) available_count: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcRateLimits {
    pub(crate) primary: Option<CodexRpcRateLimitWindow>,
    pub(crate) secondary: Option<CodexRpcRateLimitWindow>,
    pub(crate) credits: Option<CodexRpcCredits>,
    #[serde(rename = "planType")]
    pub(crate) plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcRateLimitWindow {
    #[serde(rename = "usedPercent")]
    pub(crate) used_percent: f64,
    #[serde(rename = "windowDurationMins")]
    pub(crate) window_duration_mins: Option<i64>,
    #[serde(rename = "resetsAt")]
    pub(crate) resets_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRpcCredits {
    #[serde(rename = "hasCredits")]
    pub(crate) has_credits: bool,
    pub(crate) unlimited: bool,
    pub(crate) balance: Option<String>,
}

pub(crate) struct CodexRpcUsage {
    pub(crate) response: CodexUsageResponse,
    pub(crate) account_label: Option<String>,
}

impl CodexRpcUsage {
    pub(crate) fn from_rpc(
        limits: CodexRpcRateLimitsResponse,
        account: Option<CodexRpcAccountResponse>,
    ) -> Self {
        let account_details = account.and_then(|response| response.account);
        let account_label = match &account_details {
            Some(CodexRpcAccountDetails::Chatgpt { email, .. }) => email.clone(),
            Some(CodexRpcAccountDetails::ApiKey) => Some("Codex API key".to_owned()),
            None => None,
        };
        let account_plan = match account_details {
            Some(CodexRpcAccountDetails::Chatgpt { plan_type, .. }) => plan_type,
            _ => None,
        };
        let CodexRpcRateLimitsResponse {
            rate_limits,
            rate_limits_by_limit_id,
            reset_credits: rpc_reset_credits,
        } = limits;
        // Every per-limit-id entry except the main "codex" limit is an extra
        // rate limit (e.g. Codex Spark); its primary/secondary become the
        // "<label> 5-hour"/"Weekly" buckets in `buckets()`.
        let additional_rate_limits: Vec<CodexAdditionalRateLimit> = rate_limits_by_limit_id
            .into_values()
            .filter(|entry| entry.limit_id.as_deref() != Some("codex"))
            .filter_map(|entry| {
                let primary = entry.primary.map(CodexWindowSnapshot::from_rpc);
                let secondary = entry.secondary.map(CodexWindowSnapshot::from_rpc);
                if primary.is_none() && secondary.is_none() {
                    return None;
                }
                Some(CodexAdditionalRateLimit {
                    limit_name: entry.limit_name,
                    metered_feature: None,
                    rate_limit: Some(CodexRateLimitDetails {
                        primary_window: primary,
                        secondary_window: secondary,
                    }),
                })
            })
            .collect();
        let reset_credits = rpc_reset_credits
            .filter(|reset| reset.available_count > 0)
            .map(|reset| CodexResetCredits {
                credits: Vec::new(),
                available_count: reset.available_count,
            });
        let response = CodexUsageResponse {
            plan_type: account_plan.or(rate_limits.plan_type),
            rate_limit: Some(CodexRateLimitDetails {
                primary_window: rate_limits.primary.map(CodexWindowSnapshot::from_rpc),
                secondary_window: rate_limits.secondary.map(CodexWindowSnapshot::from_rpc),
            }),
            credits: rate_limits.credits.map(CodexCreditDetails::from_rpc),
            additional_rate_limits: (!additional_rate_limits.is_empty())
                .then_some(additional_rate_limits),
            reset_credits,
        };
        Self {
            response,
            account_label,
        }
    }
}

impl CodexUsageResponse {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        if let Some(rate_limit) = &self.rate_limit {
            push_codex_window(
                &mut buckets,
                "Session",
                Some(StatusSlot::Session),
                rate_limit.primary_window.as_ref(),
                now,
            );
            push_codex_window(
                &mut buckets,
                "Weekly",
                Some(StatusSlot::Weekly),
                rate_limit.secondary_window.as_ref(),
                now,
            );
        }
        for limit in self.additional_rate_limits.iter().flatten() {
            let label = limit
                .limit_name
                .as_deref()
                .or(limit.metered_feature.as_deref())
                .map_or_else(|| "Codex extra limit".to_owned(), codex_limit_label);
            if let Some(rate_limit) = &limit.rate_limit {
                // Extra per-feature limits are detail rows, never the headline.
                push_codex_window(
                    &mut buckets,
                    &format!("{label} 5-hour"),
                    None,
                    rate_limit.primary_window.as_ref(),
                    now,
                );
                push_codex_window(
                    &mut buckets,
                    &format!("{label} Weekly"),
                    None,
                    rate_limit.secondary_window.as_ref(),
                    now,
                );
            }
        }
        if let Some(reset_credits) = &self.reset_credits
            && reset_credits.available_count > 0
        {
            let detail = reset_credits.detail_label(now);
            buckets.push(bucket(
                "Limit Reset Credits",
                None,
                None,
                None,
                None,
                Some(detail.as_str()),
                UsageSnapshotStatus::Fresh,
            ));
        }
        if let Some(credits) = &self.credits
            && credits.has_credits.unwrap_or(false)
        {
            let balance = credits.balance.as_ref().and_then(json_number);
            buckets.push(bucket(
                "Credits",
                None,
                balance.map(|value| format_amount_with_unit(value, "credits")),
                credits.unlimited.unwrap_or(false).then_some(100),
                None,
                credits.unlimited.unwrap_or(false).then_some("unlimited"),
                UsageSnapshotStatus::Fresh,
            ));
        }
        buckets
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexResetCredits {
    pub(crate) credits: Vec<CodexResetCredit>,
    #[serde(rename = "available_count")]
    pub(crate) available_count: i64,
}

impl CodexResetCredits {
    pub(crate) fn detail_label(&self, now: i64) -> String {
        let count = if self.available_count == 1 {
            "1 manual reset available".to_owned()
        } else {
            format!("{} manual resets available", self.available_count)
        };
        let Some(expires_at) = self.next_expiring_available_epoch(now) else {
            return count;
        };
        format!("{count} · Next expires {}", expiry_label(expires_at, now))
    }

    pub(crate) fn next_expiring_available_epoch(&self, now: i64) -> Option<i64> {
        self.credits
            .iter()
            .filter(|credit| credit.status.as_deref() == Some("available"))
            .filter_map(|credit| {
                credit
                    .expires_at
                    .as_deref()
                    .and_then(parse_iso_epoch)
                    .filter(|epoch| *epoch > now)
            })
            .min()
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexResetCredit {
    pub(crate) status: Option<String>,
    #[serde(rename = "expires_at")]
    pub(crate) expires_at: Option<String>,
}

pub(crate) fn push_codex_window(
    buckets: &mut Vec<QuotaBucketView>,
    label: &str,
    slot: Option<StatusSlot>,
    window: Option<&CodexWindowSnapshot>,
    now: i64,
) {
    let Some(window) = window else {
        return;
    };
    let used = window.used_percent.map(|value| value.min(100));
    let remaining = used.map(|value| 100u8.saturating_sub(value));
    let window_seconds = window.window_seconds();
    let pace = quota_pace_label(remaining, window.reset_at, window_seconds, now)
        .or_else(|| window.window_label());
    buckets.push(with_status_slot(
        timed_bucket(
            label,
            used.map(|value| format!("{value}% used")),
            Some("100%".to_owned()),
            remaining,
            window.reset_at,
            now,
            pace.as_deref(),
            UsageSnapshotStatus::Fresh,
        ),
        slot,
    ));
}

pub(crate) fn fetch_codex_rpc_usage(
    gate: &mut ManagedCliLaunchGate,
) -> Result<CodexRpcUsage, String> {
    gate.can_launch("Codex app-server", Instant::now())?;
    let mut child = match Command::new("codex")
        .args(["-s", "read-only", "-a", "untrusted", "app-server"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            let message = format!("codex app-server failed to start: {err}");
            gate.record_launch_failure(message.clone());
            return Err(message);
        }
    };

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "codex app-server stdin unavailable".to_owned())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "codex app-server stdout unavailable".to_owned())?;
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let result: Result<CodexRpcUsage, String> = (|| {
        drop(codex_rpc_request(
            &mut stdin,
            &rx,
            1,
            "initialize",
            serde_json::json!({
                "clientInfo": {
                    "name": "jackin-capsule",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
            CODEX_RPC_INIT_TIMEOUT,
        )?);
        codex_rpc_notification(&mut stdin, "initialized")?;
        let limits_value = codex_rpc_request(
            &mut stdin,
            &rx,
            2,
            "account/rateLimits/read",
            serde_json::json!({}),
            CODEX_RPC_REQUEST_TIMEOUT,
        )?;
        // The account label is non-essential (rate limits already succeeded), so
        // an RPC failure here degrades to no label rather than failing the whole
        // snapshot. Logged at the firehose tier (visible under JACKIN_DEBUG): an
        // absent account is usually a legitimate plan shape, not a fault, so this
        // does not warrant always-on `clog!` noise on every refresh.
        let account_value = codex_rpc_request(
            &mut stdin,
            &rx,
            3,
            "account/read",
            serde_json::json!({}),
            CODEX_RPC_REQUEST_TIMEOUT,
        )
        .inspect_err(|error| crate::cdebug!("codex account/read RPC failed: {error}"))
        .ok();
        let limits = serde_json::from_value::<CodexRpcRateLimitsResponse>(limits_value)
            .map_err(|err| format!("Codex app-server rate limit decode failed: {err}"))?;
        let account = account_value
            .map(serde_json::from_value::<CodexRpcAccountResponse>)
            .transpose()
            .map_err(|err| format!("Codex app-server account decode failed: {err}"))?;
        Ok(CodexRpcUsage::from_rpc(limits, account))
    })();

    drop(stdin);
    drop(child.kill());
    drop(child.wait());
    drop(reader.join());

    if result.is_ok() {
        gate.record_success();
    } else if let Err(message) = &result {
        gate.record_launch_failure(message.clone());
    }
    result
}

pub(crate) fn codex_rpc_request(
    stdin: &mut impl Write,
    rx: &mpsc::Receiver<String>,
    id: i64,
    method: &str,
    params: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    let payload = serde_json::json!({
        "id": id,
        "method": method,
        "params": params,
    });
    write_json_line(
        stdin,
        &payload,
        "Codex app-server request encode failed",
        "Codex app-server request write failed",
    )?;

    let started = Instant::now();
    loop {
        let remaining = timeout
            .checked_sub(started.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if remaining.is_zero() {
            return Err(format!("Codex app-server timed out waiting for {method}"));
        }
        let line = rx
            .recv_timeout(remaining)
            .map_err(|_| format!("Codex app-server timed out waiting for {method}"))?;
        let value: serde_json::Value = serde_json::from_str(&line)
            .map_err(|err| format!("Codex app-server response decode failed: {err}"))?;
        if value.get("id").and_then(serde_json::Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            let message = error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown error");
            return Err(format!("Codex app-server {method} failed: {message}"));
        }
        return value
            .get("result")
            .cloned()
            .ok_or_else(|| format!("Codex app-server {method} response missing result"));
    }
}

pub(crate) fn codex_rpc_notification(stdin: &mut impl Write, method: &str) -> Result<(), String> {
    let payload = serde_json::json!({
        "method": method,
        "params": {},
    });
    write_json_line(
        stdin,
        &payload,
        "Codex app-server notification encode failed",
        "Codex app-server notification write failed",
    )
}

pub(crate) fn fetch_codex_oauth_usage(
    credentials: &CodexOAuthCredentials,
    codex_home: &Path,
) -> Result<CodexUsageResponse, String> {
    let mut headers = vec![(reqwest::header::USER_AGENT, "jackin-capsule/usage")];
    if let Some(account_id) = &credentials.account_id {
        headers.push((
            reqwest::header::HeaderName::from_static("chatgpt-account-id"),
            account_id.as_str(),
        ));
    }
    get_json_bearer(
        "Codex OAuth usage",
        &resolve_codex_usage_url(codex_home),
        &credentials.access_token,
        &headers,
    )
}

/// Body for the `refresh_token` grant. Pure so the request shape is unit-tested
/// without a live endpoint.
pub(crate) fn codex_refresh_request_body(refresh_token: &str) -> serde_json::Value {
    serde_json::json!({
        "client_id": CODEX_OAUTH_CLIENT_ID,
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "scope": "openid profile email",
    })
}

/// Extract the re-minted access token from a token-endpoint response. Pure.
pub(crate) fn codex_access_token_from_response(value: &serde_json::Value) -> Option<String> {
    value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
}

pub(crate) fn refresh_codex_access_token(refresh_token: &str) -> Result<String, String> {
    let client = provider_http_client()?;
    let response = client
        .post(CODEX_OAUTH_TOKEN_URL)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::ACCEPT, "application/json")
        .json(&codex_refresh_request_body(refresh_token))
        .send()
        .map_err(|err| format!("Codex token refresh request failed: {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("Codex token refresh HTTP {status}"));
    }
    let value: serde_json::Value = response
        .json()
        .map_err(|err| format!("Codex token refresh decode failed: {err}"))?;
    codex_access_token_from_response(&value)
        .ok_or_else(|| "Codex token refresh response missing access_token".to_owned())
}

/// Fetch Codex usage, transparently re-minting the access token once if the
/// on-disk token is rejected (HTTP 401/403).
///
/// Root cause this addresses: jackin' reads `auth.json` as-is, while the Codex
/// CLI refreshes that token only on its own launch — so a token that expired
/// since the last CLI run would 401 here indefinitely. The refresh is used only
/// for this read-only fetch and deliberately NOT written back to `auth.json`
/// (avoiding any risk of corrupting the operator's live credential file); the
/// CLI re-mints and persists its own copy on next launch.
pub(crate) fn fetch_codex_oauth_usage_refreshing(
    credentials: &CodexOAuthCredentials,
    codex_home: &Path,
) -> Result<CodexUsageResponse, String> {
    match fetch_codex_oauth_usage(credentials, codex_home) {
        Err(error) if usage_error_is_unauthorized(&error) => {
            let Some(refresh_token) = credentials.refresh_token.as_deref() else {
                return Err(error);
            };
            let access_token = refresh_codex_access_token(refresh_token)?;
            let refreshed = CodexOAuthCredentials {
                access_token,
                account_id: credentials.account_id.clone(),
                account_label: credentials.account_label.clone(),
                refresh_token: credentials.refresh_token.clone(),
            };
            fetch_codex_oauth_usage(&refreshed, codex_home)
        }
        other => other,
    }
}

pub(crate) fn fetch_codex_oauth_reset_credits(
    credentials: &CodexOAuthCredentials,
    codex_home: &Path,
) -> Result<CodexResetCredits, String> {
    let mut headers = vec![
        (reqwest::header::USER_AGENT, "jackin-capsule/usage"),
        (
            reqwest::header::HeaderName::from_static("openai-beta"),
            "codex-1",
        ),
        (
            reqwest::header::HeaderName::from_static("originator"),
            "Codex Desktop",
        ),
    ];
    if let Some(account_id) = &credentials.account_id {
        headers.push((
            reqwest::header::HeaderName::from_static("chatgpt-account-id"),
            account_id.as_str(),
        ));
    }
    let credits: CodexResetCredits = get_json_bearer(
        "Codex reset credits",
        &resolve_codex_reset_credits_url(codex_home),
        &credentials.access_token,
        &headers,
    )?;
    if credits.available_count < 0 {
        return Err("Codex reset credits invalid available count".to_owned());
    }
    Ok(credits)
}

pub(crate) fn resolve_codex_usage_url(codex_home: &Path) -> String {
    let normalized = resolve_codex_base_url(codex_home);
    let path = if normalized.contains("/backend-api") {
        "/wham/usage"
    } else {
        "/api/codex/usage"
    };
    format!("{normalized}{path}")
}

pub(crate) fn resolve_codex_reset_credits_url(codex_home: &Path) -> String {
    format!(
        "{}/wham/rate-limit-reset-credits",
        resolve_codex_base_url(codex_home)
    )
}

pub(crate) fn resolve_codex_base_url(codex_home: &Path) -> String {
    let config_path = codex_home.join("config.toml");
    let contents = match fs::read_to_string(&config_path) {
        Ok(contents) => Some(contents),
        Err(error) => {
            // A config.toml that exists but is unreadable silently drops the
            // operator's custom base-URL override back to the public default.
            if error.kind() != std::io::ErrorKind::NotFound {
                crate::clog!(
                    "codex config.toml read failed for {}: {error}",
                    config_path.display()
                );
            }
            None
        }
    };
    let base = contents
        .and_then(|contents| parse_chatgpt_base_url(&contents))
        .unwrap_or_else(|| "https://chatgpt.com/backend-api".to_owned());
    let mut normalized = base.trim().trim_end_matches('/').to_owned();
    if normalized.is_empty() {
        normalized = "https://chatgpt.com/backend-api".to_owned();
    }
    if (normalized.starts_with("https://chatgpt.com")
        || normalized.starts_with("https://chat.openai.com"))
        && !normalized.contains("/backend-api")
    {
        normalized.push_str("/backend-api");
    }
    normalized
}
