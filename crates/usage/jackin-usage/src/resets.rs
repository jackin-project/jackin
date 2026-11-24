// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use std::fmt;

use jackin_core::{Secret, Timestamp};
use jackin_usage_provider_core::{Auth, Http, ProviderError, json_epoch};
use serde_json::{Value, json};
use thiserror::Error;
use uuid::Uuid;

use crate::accounts::UsageProvider;
use crate::connected::zai;
use crate::quota::{claude, codex};

#[derive(Debug, Error)]
pub enum ResetError {
    #[error("this account has no such available reset")]
    Ineligible,
    #[error("the selected reset has expired")]
    Expired,
    #[error("the selected reset is no longer available; refresh usage and select again")]
    Changed,
    #[error(
        "the previous reset request may have succeeded; refresh usage before choosing another reset"
    )]
    Unconfirmed,
    #[error("could not identify the organization for this reset")]
    Organization,
    #[error(transparent)]
    Provider(#[from] ProviderError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Claude {
        grant: String,
        remaining: u64,
    },
    ChatGpt {
        credit: Option<String>,
    },
    Zai {
        record: i64,
        grant: String,
        window: String,
    },
}

#[derive(Clone, Debug)]
pub struct ResetSelection {
    pub(crate) account: Uuid,
    pub(crate) provider: UsageProvider,
    kind: &'static str,
    expiry: i64,
    target: Target,
    detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimOutcome {
    Spent,
    SpentRefreshFailed,
    NothingToReset,
    NoCredit,
    NotNextReset,
    UnavailableNow { until: Option<i64> },
    Unconfirmed,
}

impl fmt::Display for ClaimOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spent => f.write_str("Reset redeemed."),
            Self::SpentRefreshFailed => {
                f.write_str("Reset redeemed. Usage could not be refreshed.")
            }
            Self::NothingToReset => f.write_str("The account has no limit to reset."),
            Self::NoCredit => f.write_str("This reset is no longer available."),
            Self::NotNextReset => f.write_str("The provider requires a different reset first."),
            Self::UnavailableNow { until: Some(t) } => write!(
                f,
                "The reset is unavailable until {}.",
                Timestamp(*t).rfc3339()
            ),
            Self::UnavailableNow { until: None } => {
                f.write_str("The reset is currently unavailable.")
            }
            Self::Unconfirmed => {
                f.write_str("The provider has not confirmed whether the reset was spent.")
            }
        }
    }
}

impl ResetSelection {
    pub fn from_snapshot(
        account: Uuid,
        provider: UsageProvider,
        snapshot: &Value,
        index: usize,
        now: Timestamp,
    ) -> Result<Self, ResetError> {
        let item = snapshot["resets"]
            .get(index)
            .ok_or(ResetError::Ineligible)?;
        Self::from_item(account, provider, item, now)
    }

    fn from_item(
        account: Uuid,
        provider: UsageProvider,
        row: &Value,
        now: Timestamp,
    ) -> Result<Self, ResetError> {
        let expiry = row["expiresAt"].as_i64().ok_or(ResetError::Ineligible)?;
        if expiry <= now.0 {
            return Err(ResetError::Expired);
        }
        if row["usable"] != true {
            return Err(ResetError::Ineligible);
        }
        let kind = match row["kind"].as_str() {
            Some("session") => "Session",
            Some("weekly") => "Weekly",
            Some("fable") => "Fable",
            Some("full") => "Full",
            _ => return Err(ResetError::Ineligible),
        };
        let target = match provider {
            UsageProvider::Claude => {
                let grant = row["id"]
                    .as_str()
                    .filter(|g| {
                        !g.is_empty()
                            && g.len() <= 40
                            && g.bytes().all(|b| {
                                b.is_ascii_lowercase()
                                    || b.is_ascii_digit()
                                    || matches!(b, b'-' | b'_')
                            })
                    })
                    .ok_or(ResetError::Ineligible)?;
                Target::Claude {
                    grant: grant.to_owned(),
                    remaining: row["remaining"]
                        .as_u64()
                        .filter(|n| *n > 0)
                        .ok_or(ResetError::Ineligible)?,
                }
            }
            UsageProvider::ChatGpt => Target::ChatGpt {
                credit: row["id"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            },
            UsageProvider::Zai => Target::Zai {
                record: row["id"].as_i64().ok_or(ResetError::Ineligible)?,
                grant: row["grantType"]
                    .as_str()
                    .ok_or(ResetError::Ineligible)?
                    .into(),
                window: row["resetType"]
                    .as_str()
                    .ok_or(ResetError::Ineligible)?
                    .into(),
            },
            _ => return Err(ResetError::Ineligible),
        };
        let detail = if provider == UsageProvider::Claude {
            let mapped: Option<Vec<_>> = row["clears"].as_array().and_then(|a| {
                a.iter()
                    .map(|w| match w.as_str() {
                        Some("five_hour") => Some("session"),
                        Some("seven_day") => Some("weekly"),
                        Some("seven_day_overage_included") => Some("Fable"),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>()
            });
            let mut message = mapped.filter(|w| !w.is_empty()).map_or_else(
                || "Refill the affected limits".to_owned(),
                |w| format!("Refill the {} limit(s)", w.join(", ")),
            );
            if row["givesBackWeekly"] == true {
                message.push_str(" and restore the weekly quota used by Fable");
            }
            message
        } else if kind == "Full" {
            "Refill all rate limits".into()
        } else {
            format!("Refill the {kind} limit")
        };
        Ok(Self {
            account,
            provider,
            kind,
            expiry,
            target,
            detail,
        })
    }

    pub fn description(&self) -> String {
        format!(
            "{} using the {} reset expiring {}. This cannot be undone.",
            self.detail,
            self.kind,
            Timestamp(self.expiry).rfc3339()
        )
    }

    pub(crate) fn unresolved_expiry(&self) -> Option<String> {
        matches!(self.target, Target::ChatGpt { credit: None }).then(|| format!("/{}", self.expiry))
    }

    pub(crate) fn bind_replay(&self, key: &str) -> Result<Self, ResetError> {
        let mut result = self.clone();
        if let Some(expiry) = self.unresolved_expiry() {
            let credit = key
                .strip_prefix("codex/")
                .and_then(|s| s.strip_suffix(&expiry))
                .filter(|s| !s.is_empty())
                .ok_or(ResetError::Changed)?;
            result.target = Target::ChatGpt {
                credit: Some(credit.to_owned()),
            };
        }
        Ok(result)
    }

    pub(crate) fn pending_scope(&self) -> Option<String> {
        match &self.target {
            Target::Claude { grant, .. } => Some(format!("claude/{grant}/")),
            _ => None,
        }
    }

    pub(crate) fn key(&self) -> String {
        match &self.target {
            Target::Claude { grant, remaining } => {
                format!("claude/{grant}/{remaining}/{}", self.expiry)
            }
            Target::ChatGpt {
                credit: Some(credit),
            } => format!("codex/{credit}/{}", self.expiry),
            Target::ChatGpt { credit: None } => format!("codex/expiry/{}/", self.expiry),
            Target::Zai { record, window, .. } => format!("zai/{window}/{record}/{}", self.expiry),
        }
    }

    pub(crate) async fn recheck(
        &self,
        http: &Http,
        access: &Secret,
        org: Option<&str>,
        now: Timestamp,
    ) -> Result<Self, ResetError> {
        if self.expiry <= now.0 {
            return Err(ResetError::Expired);
        }
        let auth = Auth::Bearer(access.expose());
        let rows = match self.target {
            Target::Claude { .. } => {
                let body = http.get(claude::GRANTS, auth, &claude::HEADERS).await?;
                let (rows, _) =
                    claude::inventory(&body["cedar_ember"]).ok_or(ResetError::Changed)?;
                rows
            }
            Target::ChatGpt { .. } => {
                let body = http
                    .get(codex::RESET_URL, auth, &codex_headers(org))
                    .await?;
                let inventory = codex::inventory(&body)
                    .filter(|v| v["count"].as_u64().is_some_and(|n| n > 0))
                    .ok_or(ResetError::Changed)?;
                inventory["items"].as_array().cloned().unwrap_or_default()
            }
            Target::Zai { .. } => {
                zai::inventory(&http.get(zai::INVENTORY, auth, &[]).await?, now.0)?
            }
        };
        let mut candidates = rows
            .iter()
            .filter_map(|r| Self::from_item(self.account, self.provider, r, now).ok())
            .filter(|r| {
                (r.target == self.target
                    || (matches!(self.target, Target::ChatGpt { credit: None })
                        && matches!(r.target, Target::ChatGpt { credit: Some(_) })))
                    && !matches!(r.target, Target::ChatGpt { credit: None })
                    && r.expiry == self.expiry
                    && r.kind == self.kind
                    && r.detail == self.detail
            });
        let chosen = candidates.next().ok_or(ResetError::Changed)?;
        if candidates.any(|r| r.target != chosen.target) {
            return Err(ResetError::Changed);
        }
        Ok(chosen)
    }

    pub(crate) async fn submit(
        &self,
        http: &Http,
        access: &Secret,
        org: Option<&str>,
        request: Uuid,
    ) -> Result<ClaimOutcome, ResetError> {
        let auth = Auth::Bearer(access.expose());
        match &self.target {
            Target::Claude { grant, .. } => {
                let org = org
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .ok_or(ResetError::Organization)?;
                let endpoint =
                    format!("https://api.anthropic.com/api/organizations/{org}/reset_rate_limits");
                let body = http
                    .post(
                        &endpoint,
                        auth,
                        &claude::HEADERS,
                        &json!({"program":"cedar_ember","grant_id":grant,"request_id":request}),
                    )
                    .await?;
                let result = body["result"].as_str();
                let code = if result == Some("unavailable") {
                    body["reason"].as_str()
                } else {
                    result
                };
                Ok(match code {
                    Some("reset" | "already_used") => ClaimOutcome::Spent,
                    Some("not_limited") => ClaimOutcome::NothingToReset,
                    Some("ineligible" | "no_grant" | "unknown_grant") => ClaimOutcome::NoCredit,
                    Some("not_next_grant") => ClaimOutcome::NotNextReset,
                    Some("cooldown" | "paused" | "low_priority_active") => {
                        ClaimOutcome::UnavailableNow {
                            until: json_epoch(&body["cooldown_until"]),
                        }
                    }
                    _ => ClaimOutcome::Unconfirmed,
                })
            }
            Target::ChatGpt { credit: None } => Err(ResetError::Changed),
            Target::ChatGpt {
                credit: Some(credit),
            } => {
                let body = http
                    .post(
                        "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume",
                        auth,
                        &codex_headers(org),
                        &json!({"credit_id":credit,"redeem_request_id":request}),
                    )
                    .await?;
                Ok(match body["code"].as_str() {
                    Some("reset" | "already_redeemed") => ClaimOutcome::Spent,
                    Some("nothing_to_reset") => ClaimOutcome::NothingToReset,
                    Some("no_credit") => ClaimOutcome::NoCredit,
                    _ => ClaimOutcome::Unconfirmed,
                })
            }
            Target::Zai {
                record,
                grant,
                window,
            } => {
                let body=http.post("https://api.z.ai/api/biz/customer-package-reset/use",auth,&[],&json!({"targetType":"PERSONAL","resetType":window,"recordId":record,"requestId":request,"grantType":grant})).await?;
                if body["success"] == true {
                    Ok(ClaimOutcome::Spent)
                } else {
                    Err(ResetError::Provider(ProviderError::Decode(
                        "Z.ai refused the reset",
                    )))
                }
            }
        }
    }
}

fn codex_headers(org: Option<&str>) -> Vec<(&str, &str)> {
    let mut headers = vec![("OpenAI-Beta", "codex-1"), ("originator", "Codex Desktop")];
    if let Some(org) = org {
        headers.push(("ChatGPT-Account-Id", org));
    }
    headers
}
