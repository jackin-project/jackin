// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `Amp` usage snapshot.
//!
//! Carved out of `usage.rs` for the file-size ratchet. Items in this module
//! are `pub(crate)` so the coordinator (`usage.rs`) can re-export them.

#[cfg_attr(
    not(test),
    expect(clippy::wildcard_imports, reason = "target-dependent")
)]
use super::*;

pub(crate) fn amp_snapshot(agent: &str, now: i64) -> FocusedUsageView {
    let data = home_path(".local/share/amp");
    let amp_secrets = data.join("secrets.json");
    let handoff_secrets = Path::new(AMP_HANDOFF_SECRETS_PATH);
    let amp_env_key = env_value("AMP_API_KEY");
    let env_present = amp_env_key.is_some();
    // Resolve the file key only when the env var is absent (env wins), capturing
    // the winning path so the origin names the file that actually produced the
    // key instead of re-`stat`ing and guessing. Home secrets first, handoff last.
    let amp_file = if env_present {
        None
    } else {
        first_credential_with_path(
            &[amp_secrets.clone(), handoff_secrets.to_path_buf()],
            load_amp_api_key,
        )
    };
    let amp_api_key = amp_env_key
        .clone()
        .or_else(|| amp_file.as_ref().map(|(_, key)| key.clone()));
    let (api_usage, api_error) = split_fetch(amp_api_key.as_deref().map(fetch_amp_api_usage));
    let (cli_usage, cli_error) = split_fetch(api_usage.is_none().then(fetch_amp_cli_usage));
    let provider_error = api_error.as_ref().or(cli_error.as_ref()).cloned();
    let has_auth = amp_api_key.is_some() || amp_secrets.exists() || handoff_secrets.exists();
    // credential_origin names the file that actually produced the key (env wins
    // first). A present-but-unparseable home `secrets.json` no longer mislabels a
    // key that actually resolved from the handoff.
    let credential_origin = if env_present {
        Some("API key · env AMP_API_KEY".to_owned())
    } else if let Some((path, _)) = amp_file.as_ref() {
        Some(if path.as_path() == handoff_secrets {
            format!("API key · {AMP_HANDOFF_SECRETS_PATH}")
        } else {
            "API key · amp secrets.json".to_owned()
        })
    } else {
        None
    };

    // Success path is one shared, credential-free view builder so the plan-label
    // and detail-only credit contract is executable without provider I/O.
    if let Some(usage) = api_usage {
        return amp_view_from_usage(
            AmpSuccessContext {
                agent,
                credential_origin,
                source: UsageSource::ProviderApi,
            },
            usage,
            now,
        );
    }
    if let Some(usage) = cli_usage {
        return amp_view_from_usage(
            AmpSuccessContext {
                agent,
                credential_origin,
                source: UsageSource::Cli,
            },
            usage,
            now,
        );
    }

    let status = if has_auth {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsLogin
    };
    let account_label = if has_auth {
        "local Amp auth".to_owned()
    } else {
        "needs Amp login".to_owned()
    };
    let buckets = vec![bucket(
        "Amp Free",
        None,
        None,
        None,
        None,
        provider_error
            .as_deref()
            .or(Some("Amp API/CLI usage unavailable")),
        status,
    )];
    usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Amp,
        account_label,
        username: None,
        plan_label: None,
        credential_origin,
        buckets,
        status,
        source: UsageSource::None,
        confidence: if has_auth {
            UsageConfidence::PresenceOnly
        } else {
            UsageConfidence::None
        },
        now,
        last_error: match status {
            UsageSnapshotStatus::NeedsLogin => Some("Amp auth not available to Capsule".to_owned()),
            UsageSnapshotStatus::Unsupported => Some(
                provider_error
                    .unwrap_or_else(|| "Amp API/CLI usage unavailable to Capsule".to_owned()),
            ),
            _ => None,
        },
    })
}

pub(crate) fn amp_api_key_snapshot(agent: &str, key: &str, now: i64) -> FocusedUsageView {
    match fetch_amp_api_usage(key) {
        Ok(usage) => amp_view_from_usage(
            AmpSuccessContext {
                agent,
                credential_origin: Some("API key · configured source".to_owned()),
                source: UsageSource::ProviderApi,
            },
            usage,
            now,
        ),
        Err(error) => usage_view(UsageViewInput {
            agent,
            provider: None,
            surface: UsageSurface::Amp,
            account_label: String::new(),
            username: None,
            plan_label: None,
            credential_origin: Some("API key · configured source".to_owned()),
            buckets: vec![bucket(
                "Amp Free",
                None,
                None,
                None,
                None,
                Some(&error),
                UsageSnapshotStatus::Error,
            )],
            status: UsageSnapshotStatus::Error,
            source: UsageSource::None,
            confidence: UsageConfidence::None,
            now,
            last_error: Some(error),
        }),
    }
}

/// The current Amp `userDisplayBalanceInfo.displayText` contract, shared by the
/// API and CLI paths through one parser: account identity, the Amp Free daily
/// remaining percentage, the monthly subscription pools (Agent dollars + Orb
/// hours, Tier or legacy Subscription shape), individual credit balance, and
/// per-workspace balances. Each pool keeps its own unit — dollars, hours, and
/// percents are never compressed into one metric.
#[derive(Debug, Clone, Default)]
pub(crate) struct AmpUsage {
    pub(crate) account_label: Option<String>,
    pub(crate) daily_remaining_percent: Option<u8>,
    pub(crate) individual_credits: Option<f64>,
    pub(crate) workspace_balances: Vec<AmpWorkspaceBalance>,
    pub(crate) subscription: Option<AmpSubscription>,
    pub(crate) renewal: Option<AmpRenewal>,
    pub(crate) billing_period: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AmpWorkspaceBalance {
    pub(crate) name: String,
    pub(crate) remaining: f64,
}

/// One monthly Amp subscription: the plan name (the funding route — which
/// subscription pays) plus the Agent/Orb pools in their native units.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AmpSubscription {
    pub(crate) plan: String,
    pub(crate) kind: AmpSubscriptionKind,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AmpSubscriptionKind {
    /// `Amp <plan> Tier: agent usage $<rem> of $<limit> remaining …` with an
    /// optional independent `orb usage <R>h of <L>h …` segment. Dollar/hours
    /// balances are authoritative; no rounded CLI percent is used.
    Tier {
        agent_remaining: f64,
        agent_limit: f64,
        orb_remaining_hours: Option<f64>,
        orb_limit_hours: Option<f64>,
    },
    /// Legacy `Subscription <plan>: <N>% other usage and <M>% orb usage
    /// remaining …` (or `Amp <plan> Subscription:`) percent pools.
    Legacy {
        agent_remaining_percent: u8,
        orb_remaining_percent: u8,
    },
}

/// Subscription renewal countdown (`resets upon renewal in <N> days|months`).
/// Months are calendar-approximated as 30 days; the pace label always shows
/// the raw countdown so the approximation is visible.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AmpRenewal {
    pub(crate) value: u64,
    pub(crate) months: bool,
}

impl AmpRenewal {
    pub(crate) fn label(&self) -> String {
        let unit = if self.months { "month" } else { "day" };
        let plural = if self.value == 1 { "" } else { "s" };
        format!("renews in {} {unit}{plural}", self.value)
    }

    pub(crate) fn resets_at(&self, now: i64) -> i64 {
        let days = if self.months {
            self.value.saturating_mul(30)
        } else {
            self.value
        };
        let seconds = days.saturating_mul(86_400);
        now.saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX))
    }
}

impl AmpUsage {
    pub(crate) fn from_api_value(value: serde_json::Value) -> Option<Self> {
        let root = value.get("result").unwrap_or(&value);
        let display_text = root
            .get("displayText")
            .and_then(serde_json::Value::as_str)?;
        parse_amp_usage_output(display_text)
    }

    /// The subscription plan names the funding route, so it wins; `Amp Free`
    /// only when the daily line exists; a paid/credit-only balance never
    /// infers a plan.
    pub(crate) fn plan_label(&self) -> Option<String> {
        if let Some(subscription) = &self.subscription {
            return Some(format!("Amp {}", subscription.plan));
        }
        self.daily_remaining_percent.map(|_| "Amp Free".to_owned())
    }

    /// Pace/detail line shared by the subscription buckets: the renewal
    /// countdown plus the billing period when the CLI reported one.
    fn subscription_pace(&self) -> Option<String> {
        let mut parts = Vec::new();
        if let Some(renewal) = &self.renewal {
            parts.push(renewal.label());
        }
        if let Some(period) = &self.billing_period {
            parts.push(format!("period {period}"));
        }
        (!parts.is_empty()).then(|| parts.join(" · "))
    }

    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        if let Some(remaining) = self.daily_remaining_percent {
            buckets.push(with_status_slot(
                bucket(
                    "Amp Free",
                    None,
                    None,
                    Some(remaining),
                    Some("Resets daily".to_owned()),
                    None,
                    UsageSnapshotStatus::Fresh,
                ),
                Some(StatusSlot::Daily),
            ));
        }
        if let Some(subscription) = &self.subscription {
            let reset_at = self.renewal.as_ref().map(|renewal| renewal.resets_at(now));
            let pace = self.subscription_pace();
            match &subscription.kind {
                AmpSubscriptionKind::Tier {
                    agent_remaining,
                    agent_limit,
                    orb_remaining_hours,
                    orb_limit_hours,
                } => {
                    push_amp_agent_dollar_bucket(
                        &mut buckets,
                        *agent_remaining,
                        *agent_limit,
                        reset_at,
                        now,
                        pace.as_deref(),
                    );
                    if let (Some(remaining), Some(limit)) = (orb_remaining_hours, orb_limit_hours) {
                        push_amp_orb_bucket(
                            &mut buckets,
                            *remaining,
                            *limit,
                            reset_at,
                            now,
                            pace.as_deref(),
                        );
                    }
                }
                AmpSubscriptionKind::Legacy {
                    agent_remaining_percent,
                    orb_remaining_percent,
                } => {
                    for (label, remaining) in [
                        ("Agent usage", *agent_remaining_percent),
                        ("Orb usage", *orb_remaining_percent),
                    ] {
                        buckets.push(timed_bucket(
                            label,
                            Some(format!("{}% used", 100u8.saturating_sub(remaining))),
                            Some("100%".to_owned()),
                            Some(remaining),
                            reset_at,
                            now,
                            pace.as_deref(),
                            UsageSnapshotStatus::Fresh,
                        ));
                    }
                }
            }
        }
        if let Some(credits) = self.individual_credits {
            buckets.push(bucket(
                "Individual credits",
                None,
                Some(format_currency(credits)),
                None,
                None,
                Some(&format!("Individual credits: {}", format_currency(credits))),
                UsageSnapshotStatus::Fresh,
            ));
        }
        for balance in &self.workspace_balances {
            let label = format!("Workspace {}", balance.name);
            let detail = format!("{label}: {}", format_currency(balance.remaining));
            buckets.push(bucket(
                &label,
                None,
                Some(format_currency(balance.remaining)),
                None,
                None,
                Some(&detail),
                UsageSnapshotStatus::Fresh,
            ));
        }
        buckets
    }
}

/// Tier Agent dollar pool: structured `Money` on the `Spend` slot plus a
/// remaining percent from the full-precision balances (never a rounded CLI
/// percent). The Amp surface headline stays Daily-only, so this never leaks
/// into the status bar.
fn push_amp_agent_dollar_bucket(
    buckets: &mut Vec<QuotaBucketView>,
    remaining: f64,
    limit: f64,
    reset_at: Option<i64>,
    now: i64,
    pace: Option<&str>,
) {
    let used = (limit - remaining).max(0.0);
    let used_money = Money::new((used * 100.0).round() as i64, "USD", 2);
    let limit_money = Money::new((limit * 100.0).round() as i64, "USD", 2);
    #[expect(
        clippy::cast_sign_loss,
        reason = "fraction clamped to 0.0..=1.0; percent is rounded f64→u8"
    )]
    let remaining_percent = Some(((remaining / limit).clamp(0.0, 1.0) * 100.0).round() as u8);
    let mut view = timed_bucket(
        "Agent usage",
        Some(format!("{used_money} used")),
        Some(limit_money.to_string()),
        remaining_percent,
        reset_at,
        now,
        pace,
        UsageSnapshotStatus::Fresh,
    );
    view.status_slot = Some(StatusSlot::Spend);
    view.used_money = Some(used_money);
    view.limit_money = Some(limit_money);
    buckets.push(view);
}

/// Tier Orb hour pool: whole a1.small-equivalent hours rounded down, positive
/// sub-hour balances shown as `< 1h`; the remaining percent keeps full
/// precision.
fn push_amp_orb_bucket(
    buckets: &mut Vec<QuotaBucketView>,
    remaining: f64,
    limit: f64,
    reset_at: Option<i64>,
    now: i64,
    pace: Option<&str>,
) {
    let used = (limit - remaining).max(0.0);
    #[expect(
        clippy::cast_sign_loss,
        reason = "fraction clamped to 0.0..=1.0; percent is rounded f64→u8"
    )]
    let remaining_percent = Some(((remaining / limit).clamp(0.0, 1.0) * 100.0).round() as u8);
    buckets.push(timed_bucket(
        "Orb usage",
        Some(format!("{} used", format_orb_hours(used))),
        Some(format_orb_hours(limit)),
        remaining_percent,
        reset_at,
        now,
        pace,
        UsageSnapshotStatus::Fresh,
    ));
}

fn format_orb_hours(hours: f64) -> String {
    if hours < 1.0 {
        if hours > 0.0 {
            "< 1h".to_owned()
        } else {
            "0h".to_owned()
        }
    } else {
        format!("{}h", hours.floor())
    }
}

/// Non-usage inputs the shared Amp success view builder needs: the agent, the
/// resolved credential origin, and which fetch path produced the usage.
pub(crate) struct AmpSuccessContext<'a> {
    pub(crate) agent: &'a str,
    pub(crate) credential_origin: Option<String>,
    pub(crate) source: UsageSource,
}

/// Build the Fresh, Authoritative Amp success view from parsed usage without
/// touching credentials or provider I/O, so the plan-label and detail-only
/// credit contract is unit-testable.
pub(crate) fn amp_view_from_usage(
    context: AmpSuccessContext<'_>,
    usage: AmpUsage,
    now: i64,
) -> FocusedUsageView {
    let account_label = usage
        .account_label
        .clone()
        .unwrap_or_else(|| "local Amp auth".to_owned());
    let plan_label = usage.plan_label();
    let buckets = usage.buckets(now);
    usage_view(UsageViewInput {
        agent: context.agent,
        provider: None,
        surface: UsageSurface::Amp,
        account_label,
        username: None,
        plan_label,
        credential_origin: context.credential_origin,
        buckets,
        status: UsageSnapshotStatus::Fresh,
        source: context.source,
        confidence: UsageConfidence::Authoritative,
        now,
        last_error: None,
    })
}

pub(crate) fn fetch_amp_api_usage(token: &str) -> Result<AmpUsage, String> {
    provider_request(
        jackin_telemetry::schema::enums::ProviderName::Amp,
        "POST",
        "/api/internal",
        || {
            let client = provider_http_client()?;
            let response = client
                .post("https://ampcode.com/api/internal?userDisplayBalanceInfo")
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "application/json")
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .json(&serde_json::json!({
                    "method": "userDisplayBalanceInfo",
                    "params": {}
                }))
                .send()
                .map_err(|err| format!("Amp usage request failed: {err}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("Amp usage HTTP {status}"));
            }
            let value = response
                .json::<serde_json::Value>()
                .map_err(|err| format!("Amp usage decode failed: {err}"))?;
            AmpUsage::from_api_value(value)
                .ok_or_else(|| "Amp usage response did not include balance info".to_owned())
        },
    )
}

pub(crate) fn load_amp_api_key(path: &Path) -> Option<String> {
    let value = read_json_file(path)?;
    value
        .as_object()?
        .iter()
        .find_map(|(key, value)| {
            key.starts_with("apiKey@")
                .then(|| value.as_str())
                .flatten()
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| {
            value
                .as_object()?
                .values()
                .filter_map(|value| value.as_str().map(str::trim))
                .find(|token| !token.is_empty())
                .map(ToOwned::to_owned)
        })
}

pub(crate) fn fetch_amp_cli_usage() -> Result<AmpUsage, String> {
    let output = run_cli_with_timeout("amp", &["--no-color", "usage"], PROVIDER_CLI_TIMEOUT)?;
    parse_amp_usage_output(&output)
        .ok_or_else(|| "Amp CLI usage output was not recognized".to_owned())
}

/// The one parser for the current Amp `displayText`/CLI usage contract. Rejects
/// the retired `$remaining/$limit (replenishes +$N/hour)` line entirely.
pub(crate) fn parse_amp_usage_output(text: &str) -> Option<AmpUsage> {
    // The API `displayText` may carry Markdown bold markers; the CLI never
    // does, and stripping them is a no-op for CLI output.
    let cleaned = text.replace("**", "");
    let mut usage = AmpUsage::default();
    for line in cleaned
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(rest) = line.strip_prefix("Signed in as ") {
            let identity = rest.split(" (").next().unwrap_or(rest).trim();
            if !identity.is_empty() {
                usage.account_label = Some(identity.to_owned());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("Amp Free:") {
            if let Some(percent) = parse_amp_daily_percent(rest) {
                usage.daily_remaining_percent = Some(percent);
            }
            continue;
        }
        if line.starts_with("Individual credits:") {
            usage.individual_credits = dollar_amounts(line).first().copied();
            continue;
        }
        if let Some(rest) = line.strip_prefix("Workspace ")
            && let Some(balance) = parse_amp_workspace(rest)
        {
            usage.workspace_balances.push(balance);
            continue;
        }
        // Tier wins over a legacy line when both appear (they never do on a
        // real account); a legacy line only fills an empty subscription.
        if let Some((subscription, renewal, period)) = parse_amp_tier_line(line) {
            usage.subscription = Some(subscription);
            usage.renewal = renewal;
            usage.billing_period = period;
            continue;
        }
        if usage.subscription.is_none()
            && let Some((subscription, renewal)) = parse_amp_legacy_subscription_line(line)
        {
            usage.subscription = Some(subscription);
            usage.renewal = renewal;
        }
    }
    (usage.daily_remaining_percent.is_some()
        || usage.individual_credits.is_some()
        || !usage.workspace_balances.is_empty()
        || usage.subscription.is_some())
    .then_some(usage)
}

/// Parse `Amp <plan> Tier: agent usage $<rem> of $<limit> remaining …` plus
/// the optional independent `orb usage <R>h of <L>h …` segment, renewal
/// countdown, and billing period. Agent dollars are required; every other
/// segment is optional — unrecognized Orb data never hides the Agent pool.
fn parse_amp_tier_line(
    line: &str,
) -> Option<(AmpSubscription, Option<AmpRenewal>, Option<String>)> {
    let after_amp = line.strip_prefix("Amp ")?;
    let (plan, segment) = after_amp.split_once(" Tier:")?;
    let plan = plan.trim();
    if plan.is_empty() {
        return None;
    }
    let segment = segment.strip_prefix(" agent usage ")?;
    if !segment.contains("remaining") {
        return None;
    }
    let amounts = dollar_amounts(segment);
    let (remaining, limit) = (*amounts.first()?, *amounts.get(1)?);
    if !remaining.is_finite() || !limit.is_finite() || remaining < 0.0 || limit <= 0.0 {
        return None;
    }
    let (orb_remaining_hours, orb_limit_hours) =
        parse_amp_orb_hours(segment).map_or((None, None), |(r, l)| (Some(r), Some(l)));
    let subscription = AmpSubscription {
        plan: plan.to_owned(),
        kind: AmpSubscriptionKind::Tier {
            agent_remaining: remaining,
            agent_limit: limit,
            orb_remaining_hours,
            orb_limit_hours,
        },
    };
    Some((
        subscription,
        parse_amp_renewal(segment),
        parse_amp_period(segment),
    ))
}

/// Parse the Tier `orb usage <R>h of <L>h a1.small orb hours remaining`
/// segment. `None` when the segment is absent or malformed — the caller keeps
/// the Agent pool regardless.
fn parse_amp_orb_hours(segment: &str) -> Option<(f64, f64)> {
    let after = segment.split_once("orb usage ")?.1;
    let (remaining_token, rest) = after.split_once('h')?;
    let rest = rest.strip_prefix(" of ")?;
    let (limit_token, rest) = rest.split_once('h')?;
    if !rest.contains("orb hours") {
        return None;
    }
    let remaining = parse_amp_number(remaining_token)?;
    let limit = parse_amp_number(limit_token)?;
    if !remaining.is_finite() || !limit.is_finite() || remaining < 0.0 || limit <= 0.0 {
        return None;
    }
    Some((remaining, limit))
}

/// Parse `resets upon renewal in <N> days|months` (trailing URL tolerated).
fn parse_amp_renewal(segment: &str) -> Option<AmpRenewal> {
    let after = segment.split_once("resets upon renewal in ")?.1;
    let mut parts = after.split_whitespace();
    let value: u64 = parts.next()?.replace(',', "").parse().ok()?;
    let unit = parts.next()?.to_ascii_lowercase();
    let months = if unit.starts_with("day") {
        false
    } else if unit.starts_with("month") {
        true
    } else {
        return None;
    };
    Some(AmpRenewal { value, months })
}

/// Parse the Tier `period YYYY-MM-DD to YYYY-MM-DD` billing dates. Strict
/// day shape, end after start; anything else is ignored (never guessed).
fn parse_amp_period(segment: &str) -> Option<String> {
    let after = segment.split_once("period ")?.1;
    let start = after.get(..10)?;
    let end = after.get(10..)?.strip_prefix(" to ")?.get(..10)?;
    if !is_amp_period_date(start) || !is_amp_period_date(end) || end <= start {
        return None;
    }
    Some(format!("{start} to {end}"))
}

fn is_amp_period_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
}

/// Parse the legacy `Subscription <plan>: <N>% other usage and <M>% orb usage
/// remaining …` line (or the `Amp <plan> Subscription:` variant). The two
/// percents are required; the renewal countdown is optional.
fn parse_amp_legacy_subscription_line(line: &str) -> Option<(AmpSubscription, Option<AmpRenewal>)> {
    let (plan, segment) = if let Some(rest) = line.strip_prefix("Subscription ") {
        let (plan, segment) = rest.split_once(':')?;
        (plan, segment)
    } else {
        let after_amp = line.strip_prefix("Amp ")?;
        after_amp.split_once(" Subscription:")?
    };
    let plan = plan.trim();
    if plan.is_empty() {
        return None;
    }
    let (agent_part, orb_segment) = segment.split_once("% other usage and ")?;
    let (orb_number, orb_rest) = orb_segment.split_once("% orb usage")?;
    if !orb_rest.contains("remaining") {
        return None;
    }
    let agent = parse_amp_trailing_percent(agent_part)?;
    let orb = parse_amp_trailing_percent(orb_number)?;
    let subscription = AmpSubscription {
        plan: plan.to_owned(),
        kind: AmpSubscriptionKind::Legacy {
            agent_remaining_percent: agent,
            orb_remaining_percent: orb,
        },
    };
    Some((subscription, parse_amp_renewal(segment)))
}

/// The number token before a `%` marker: last whitespace-separated token of
/// the text preceding it (`" 61"` → 61). Round then clamp to `0..=100`.
fn parse_amp_trailing_percent(before_percent: &str) -> Option<u8> {
    let token = before_percent.split_whitespace().last()?;
    let percent = parse_amp_number(token)?;
    if !percent.is_finite() {
        return None;
    }
    #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0")]
    Some(percent.round().clamp(0.0, 100.0) as u8)
}

/// A plain comma-tolerant number token (`"1,234.5"` → 1234.5).
fn parse_amp_number(token: &str) -> Option<f64> {
    let cleaned: String = token.chars().filter(|ch| *ch != ',').collect();
    cleaned.trim().parse().ok()
}

/// Parse `<N>% remaining today (resets daily)`: round then clamp to `0..=100`.
/// The retired dollar line carries no `%` and yields `None`.
fn parse_amp_daily_percent(rest: &str) -> Option<u8> {
    let (value, _) = rest.trim().split_once('%')?;
    let percent: f64 = value.trim().parse().ok()?;
    if !percent.is_finite() {
        return None;
    }
    #[expect(clippy::cast_sign_loss, reason = "clamped to 0.0..=100.0 below")]
    Some(percent.round().clamp(0.0, 100.0) as u8)
}

/// Parse `<name>: $<N> remaining` after the `Workspace ` prefix. Requires a
/// non-empty name and a finite, non-negative amount.
fn parse_amp_workspace(rest: &str) -> Option<AmpWorkspaceBalance> {
    let (name, amount_part) = rest.split_once(':')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let remaining = dollar_amounts(amount_part).first().copied()?;
    if !remaining.is_finite() || remaining < 0.0 {
        return None;
    }
    Some(AmpWorkspaceBalance {
        name: name.to_owned(),
        remaining,
    })
}
