// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::Secret;
use jackin_usage_provider_core::{
    Auth, Bucket, DevicePoll, DevicePollError, Http, PostFields, ProviderError, json_integer,
    json_text, poll_device,
};
use serde_json::Value;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::{GLOBAL_CODING_BASE, MAINLAND_CODING_BASE};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    Mainland,
    Global,
}

impl Region {
    fn api(self) -> &'static str {
        match self {
            Self::Mainland => MAINLAND_CODING_BASE,
            Self::Global => GLOBAL_CODING_BASE,
        }
    }

    fn auth(self) -> &'static str {
        match self {
            Self::Mainland => "https://auth.kimi.com",
            Self::Global => "https://auth.kimi.ai",
        }
    }
}

pub struct DeviceLogin {
    pub user_code: String,
    pub verification_url: Url,
    pub deadline: Instant,
    device_code: Secret,
    client_id: String,
    region: Region,
    interval: Duration,
}

#[derive(Debug)]
pub struct Tokens {
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub expires_in: Duration,
}

#[derive(Debug)]
pub struct Profile {
    pub user_id: String,
    pub nickname: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug)]
pub struct KimiPlan {
    pub windows: Vec<Bucket>,
    pub booster: Option<BoosterWallet>,
}

#[derive(Debug)]
pub struct BoosterWallet {
    pub total_millionths_of_cent: Option<i64>,
    pub remaining_millionths_of_cent: Option<i64>,
    pub monthly_charge_limit_enabled: Option<bool>,
    pub monthly_limit: Option<Charge>,
    pub monthly_used: Option<Charge>,
}

#[derive(Debug)]
pub struct Charge {
    pub cents: i64,
    pub currency: String,
}

fn required_text<'a>(body: &'a Value, key: &str) -> Result<&'a str, ProviderError> {
    body.get(key)
        .and_then(json_text)
        .ok_or(ProviderError::Decode("incomplete Kimi account response"))
}

fn tokens(body: &Value) -> Result<Tokens, ProviderError> {
    Ok(Tokens {
        access_token: Secret::new(required_text(body, "access_token")?.to_owned()),
        refresh_token: Secret::new(required_text(body, "refresh_token")?.to_owned()),
        expires_in: Duration::from_secs(
            body.get("expires_in")
                .and_then(Value::as_u64)
                .filter(|s| *s > 0)
                .ok_or(ProviderError::Decode("invalid Kimi token lifetime"))?,
        ),
    })
}

pub async fn start_device(
    http: &Http,
    region: Region,
    client_id: &str,
) -> Result<DeviceLogin, ProviderError> {
    let url = format!("{}/api/oauth/device_authorization", region.auth());
    let reply = http
        .post_fields(&url, PostFields::Form(&[("client_id", client_id)]))
        .await?;
    reply.ensure_success()?;
    let body = reply.body;
    let verification_url = Url::parse(required_text(&body, "verification_uri_complete")?)
        .ok()
        .filter(|url| {
            url.scheme() == "https"
                && url.has_host()
                && url.username().is_empty()
                && url.password().is_none()
        })
        .ok_or(ProviderError::Decode("invalid Kimi verification URL"))?;
    let duration = match body.get("expires_in") {
        Some(v) => v
            .as_u64()
            .filter(|s| *s > 0)
            .ok_or(ProviderError::Decode("invalid Kimi device-code lifetime"))?
            .min(900),
        None => 900,
    };
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(duration))
        .ok_or(ProviderError::OUT_OF_RANGE)?;
    let interval = body
        .get("interval")
        .and_then(Value::as_u64)
        .unwrap_or(5)
        .clamp(1, duration);
    Ok(DeviceLogin {
        user_code: required_text(&body, "user_code")?.to_owned(),
        verification_url,
        deadline,
        device_code: Secret::new(required_text(&body, "device_code")?.to_owned()),
        client_id: client_id.to_owned(),
        region,
        interval: Duration::from_secs(interval),
    })
}

impl DeviceLogin {
    pub async fn wait(
        self,
        http: &Http,
        cancel: &CancellationToken,
    ) -> Result<Tokens, DevicePollError> {
        poll_device(self.interval, self.deadline, cancel, || self.poll(http)).await
    }

    async fn poll(&self, http: &Http) -> Result<DevicePoll<Tokens>, ProviderError> {
        let url = format!("{}/api/oauth/token", self.region.auth());
        let reply = http
            .post_fields(
                &url,
                PostFields::Form(&[
                    ("client_id", &self.client_id),
                    ("device_code", self.device_code.expose()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ]),
            )
            .await?;
        if reply.status < 500 {
            match reply.body.get("error").and_then(json_text) {
                Some("authorization_pending") => return Ok(DevicePoll::Pending),
                Some("slow_down") => return Ok(DevicePoll::SlowDown),
                Some("expired_token") => return Ok(DevicePoll::Expired),
                Some("access_denied") => return Ok(DevicePoll::Denied),
                _ => {}
            }
        }
        reply.ensure_success()?;
        Ok(DevicePoll::Granted(tokens(&reply.body)?))
    }
}

pub async fn renew(
    http: &Http,
    region: Region,
    client_id: &str,
    refresh: &Secret,
) -> Result<Tokens, ProviderError> {
    let url = format!("{}/api/oauth/token", region.auth());
    let reply = http
        .post_fields(
            &url,
            PostFields::Form(&[
                ("grant_type", "refresh_token"),
                ("client_id", client_id),
                ("refresh_token", refresh.expose()),
            ]),
        )
        .await?;
    if reply.body.get("error").and_then(json_text) == Some("invalid_grant") {
        return Err(ProviderError::Unauthorized);
    }
    reply.ensure_success()?;
    tokens(&reply.body)
}

pub async fn identify(http: &Http, region: Region, token: &str) -> Result<Profile, ProviderError> {
    let body = http
        .get(&format!("{}/me", region.api()), Auth::Bearer(token), &[])
        .await?;
    Ok(Profile {
        user_id: required_text(&body, "user_id")?.to_owned(),
        nickname: body.get("nickname").and_then(json_text).map(str::to_owned),
        plan: body
            .get("user_level_name")
            .and_then(json_text)
            .map(str::to_owned),
    })
}

pub async fn fetch_plan(
    http: &Http,
    region: Region,
    token: &str,
) -> Result<KimiPlan, ProviderError> {
    let body = http
        .get(
            &format!("{}/usages", region.api()),
            Auth::Bearer(token),
            &[],
        )
        .await?;
    if !body.get("usages").is_some_and(Value::is_object) {
        return Err(ProviderError::Decode("Kimi did not return account usage"));
    }
    let booster = body
        .get("boosterWallet")
        .filter(|wallet| wallet.pointer("/balance/type").and_then(json_text) == Some("BOOSTER"))
        .map(|wallet| BoosterWallet {
            total_millionths_of_cent: wallet.pointer("/balance/amount").and_then(json_integer),
            remaining_millionths_of_cent: wallet
                .pointer("/balance/amountLeft")
                .and_then(json_integer),
            monthly_charge_limit_enabled: wallet
                .get("monthlyChargeLimitEnabled")
                .and_then(Value::as_bool),
            monthly_limit: wallet.get("monthlyChargeLimit").and_then(charge),
            monthly_used: wallet.get("monthlyUsed").and_then(charge),
        });
    Ok(KimiPlan {
        windows: crate::windows(&body)?,
        booster,
    })
}

fn charge(value: &Value) -> Option<Charge> {
    Some(Charge {
        cents: value.get("priceInCents").and_then(json_integer)?,
        currency: value.get("currency").and_then(json_text)?.to_owned(),
    })
}

impl std::fmt::Debug for DeviceLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceLogin")
            .field("region", &self.region)
            .finish_non_exhaustive()
    }
}
