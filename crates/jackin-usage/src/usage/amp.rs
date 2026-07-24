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

/// The current Amp `userDisplayBalanceInfo.displayText` contract, shared by the
/// API and CLI paths through one parser: account identity, the Amp Free daily
/// remaining percentage, individual credit balance, and per-workspace balances.
#[derive(Debug, Clone, Default)]
pub(crate) struct AmpUsage {
    pub(crate) account_label: Option<String>,
    pub(crate) daily_remaining_percent: Option<u8>,
    pub(crate) individual_credits: Option<f64>,
    pub(crate) workspace_balances: Vec<AmpWorkspaceBalance>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AmpWorkspaceBalance {
    pub(crate) name: String,
    pub(crate) remaining: f64,
}

impl AmpUsage {
    pub(crate) fn from_api_value(value: serde_json::Value) -> Option<Self> {
        let root = value.get("result").unwrap_or(&value);
        let display_text = root
            .get("displayText")
            .and_then(serde_json::Value::as_str)?;
        parse_amp_usage_output(display_text)
    }

    /// `Amp Free` only when the daily line exists; a paid/credit-only balance
    /// never infers a plan.
    pub(crate) fn plan_label(&self) -> Option<String> {
        self.daily_remaining_percent.map(|_| "Amp Free".to_owned())
    }

    pub(crate) fn buckets(&self) -> Vec<QuotaBucketView> {
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
    let buckets = usage.buckets();
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
    let mut usage = AmpUsage::default();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
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
        }
    }
    (usage.daily_remaining_percent.is_some()
        || usage.individual_credits.is_some()
        || !usage.workspace_balances.is_empty())
    .then_some(usage)
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
