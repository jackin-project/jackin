//! `Amp` usage snapshot.
//!
//! Carved out of `usage.rs` during codebase-health-enforcement Workstream W5
//! (file-size ratchet). Items in this module are `pub(crate)` so the
//! coordinator (`usage.rs`) can re-export them.

#[allow(clippy::wildcard_imports)]
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
    let status = if api_usage.is_some() || cli_usage.is_some() {
        UsageSnapshotStatus::Fresh
    } else if has_auth {
        UsageSnapshotStatus::Unsupported
    } else {
        UsageSnapshotStatus::NeedsLogin
    };
    let account_label = api_usage
        .as_ref()
        .and_then(|usage| usage.account_label.clone())
        .or_else(|| {
            cli_usage
                .as_ref()
                .and_then(|usage| usage.account_label.clone())
        })
        .unwrap_or_else(|| {
            if has_auth {
                "local Amp auth".to_owned()
            } else {
                "needs Amp login".to_owned()
            }
        });
    let buckets = api_usage
        .as_ref()
        .map(|usage| usage.buckets(now))
        .or_else(|| cli_usage.as_ref().map(|usage| usage.buckets(now)))
        .filter(|buckets| !buckets.is_empty())
        .unwrap_or_else(|| {
            vec![bucket(
                "Amp Free",
                None,
                None,
                None,
                None,
                provider_error
                    .as_deref()
                    .or(Some("Amp API/CLI usage unavailable")),
                status,
            )]
        });
    usage_view(UsageViewInput {
        agent,
        provider: None,
        surface: UsageSurface::Amp,
        account_label,
        username: None,
        plan_label: (api_usage.is_some() || cli_usage.is_some()).then_some("Amp Free".to_owned()),
        credential_origin,
        buckets,
        status,
        source: if api_usage.is_some() {
            UsageSource::ProviderApi
        } else if cli_usage.is_some() {
            UsageSource::Cli
        } else {
            UsageSource::None
        },
        confidence: if api_usage.is_some() || cli_usage.is_some() {
            UsageConfidence::Authoritative
        } else if has_auth {
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

#[derive(Debug, Clone, Default)]
pub(crate) struct AmpApiUsage {
    pub(crate) account_label: Option<String>,
    pub(crate) free_remaining: Option<f64>,
    pub(crate) free_limit: Option<f64>,
    pub(crate) hourly_replenishment: Option<f64>,
    pub(crate) individual_credits: Option<f64>,
}

impl AmpApiUsage {
    pub(crate) fn from_value(value: serde_json::Value) -> Option<Self> {
        let root = value.get("result").unwrap_or(&value);
        if let Some(display_text) = root.get("displayText").and_then(serde_json::Value::as_str)
            && let Some(usage) = parse_amp_usage_output(display_text)
        {
            return Some(Self::from_cli_usage(usage));
        }
        let usage = Self {
            account_label: first_string_key(root, "email")
                .or_else(|| first_string_key(root, "accountEmail"))
                .or_else(|| first_string_key(root, "userEmail")),
            free_remaining: first_number_key(root, "ampFreeRemaining")
                .or_else(|| first_number_key(root, "freeRemaining"))
                .or_else(|| first_number_key(root, "remainingBalance")),
            free_limit: first_number_key(root, "ampFreeLimit")
                .or_else(|| first_number_key(root, "freeLimit"))
                .or_else(|| first_number_key(root, "limitBalance")),
            hourly_replenishment: first_number_key(root, "hourlyReplenishment")
                .or_else(|| first_number_key(root, "replenishmentRate")),
            individual_credits: first_number_key(root, "individualCredits")
                .or_else(|| first_number_key(root, "individualBalance")),
        };
        (usage.free_remaining.is_some()
            || usage.free_limit.is_some()
            || usage.individual_credits.is_some())
        .then_some(usage)
    }

    pub(crate) fn from_cli_usage(usage: AmpCliUsage) -> Self {
        Self {
            account_label: usage.account_label,
            free_remaining: usage.free_remaining,
            free_limit: usage.free_limit,
            hourly_replenishment: usage.hourly_replenishment,
            individual_credits: usage.individual_credits,
        }
    }

    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        if let (Some(remaining), Some(limit)) = (self.free_remaining, self.free_limit) {
            let used = (limit - remaining).max(0.0);
            let remaining_percent = if limit > 0.0 {
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "clamped to 0.0..=100.0 above"
                )]
                {
                    Some(((remaining / limit) * 100.0).round().clamp(0.0, 100.0) as u8)
                }
            } else {
                None
            };
            buckets.push(bucket(
                "Amp Free",
                Some(format_currency(used)),
                Some(format_currency(limit)),
                remaining_percent,
                amp_free_reset_label(remaining, limit, self.hourly_replenishment, now),
                None,
                UsageSnapshotStatus::Fresh,
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
        buckets
    }
}

pub(crate) fn fetch_amp_api_usage(token: &str) -> Result<AmpApiUsage, String> {
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
    AmpApiUsage::from_value(value)
        .ok_or_else(|| "Amp usage response did not include balance info".to_owned())
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

#[derive(Debug, Clone, Default)]
pub(crate) struct AmpCliUsage {
    pub(crate) account_label: Option<String>,
    pub(crate) free_remaining: Option<f64>,
    pub(crate) free_limit: Option<f64>,
    pub(crate) hourly_replenishment: Option<f64>,
    pub(crate) individual_credits: Option<f64>,
}

impl AmpCliUsage {
    pub(crate) fn buckets(&self, now: i64) -> Vec<QuotaBucketView> {
        let mut buckets = Vec::new();
        if let (Some(remaining), Some(limit)) = (self.free_remaining, self.free_limit) {
            let used = (limit - remaining).max(0.0);
            let remaining_percent = if limit > 0.0 {
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "clamped to 0.0..=100.0 above"
                )]
                {
                    Some(((remaining / limit) * 100.0).round().clamp(0.0, 100.0) as u8)
                }
            } else {
                None
            };
            buckets.push(bucket(
                "Amp Free",
                Some(format_currency(used)),
                Some(format_currency(limit)),
                remaining_percent,
                amp_free_reset_label(remaining, limit, self.hourly_replenishment, now),
                None,
                UsageSnapshotStatus::Fresh,
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
        buckets
    }
}

pub(crate) fn amp_free_reset_label(
    remaining: f64,
    limit: f64,
    hourly_replenishment: Option<f64>,
    now: i64,
) -> Option<String> {
    let hourly_replenishment = hourly_replenishment?;
    if !remaining.is_finite()
        || !limit.is_finite()
        || !hourly_replenishment.is_finite()
        || remaining >= limit
        || hourly_replenishment <= 0.0
    {
        return None;
    }
    let seconds = (((limit - remaining).max(0.0) / hourly_replenishment) * 3_600.0).ceil() as i64;
    let reset_at = now.saturating_add(seconds);
    Some(format!(
        "Resets in {} ({})",
        compact_duration_label(seconds),
        local_timestamp_label(reset_at)
    ))
}

pub(crate) fn fetch_amp_cli_usage() -> Result<AmpCliUsage, String> {
    let output = run_cli_with_timeout("amp", &["--no-color", "usage"], PROVIDER_CLI_TIMEOUT)?;
    parse_amp_usage_output(&output)
        .ok_or_else(|| "Amp CLI usage output was not recognized".to_owned())
}

pub(crate) fn parse_amp_usage_output(text: &str) -> Option<AmpCliUsage> {
    let mut usage = AmpCliUsage::default();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(rest) = line.strip_prefix("Signed in as ") {
            usage.account_label = Some(rest.split(" - ").next().unwrap_or(rest).trim().to_owned());
            continue;
        }
        if let Some(rest) = line.strip_prefix("Amp Free:") {
            let amounts = dollar_amounts(rest);
            if amounts.len() >= 2 {
                usage.free_remaining = Some(amounts[0]);
                usage.free_limit = Some(amounts[1]);
            }
            if let Some(replenishment) = rest
                .split("replenishes")
                .nth(1)
                .and_then(|value| dollar_amounts(value).first().copied())
            {
                usage.hourly_replenishment = Some(replenishment);
            }
            continue;
        }
        if line.starts_with("Individual credits:") {
            usage.individual_credits = dollar_amounts(line).first().copied();
        }
    }
    (usage.free_remaining.is_some() || usage.individual_credits.is_some()).then_some(usage)
}
