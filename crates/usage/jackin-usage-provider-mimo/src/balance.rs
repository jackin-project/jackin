// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_usage_provider_core::{Auth, Http, ProviderError};
use serde::Deserialize;

const URL: &str = "https://api.xiaomimimo.com/v1/platform/balance";

#[derive(Debug, PartialEq, Eq)]
pub struct Balance {
    pub available: bool,
    pub balances: Vec<CurrencyBalance>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CurrencyBalance {
    pub currency: String,
    pub cash_cents: i64,
    pub gift_cents: i64,
    pub frozen_cents: i64,
    pub credit_limit_cents: i64,
    pub available_credit_cents: i64,
    pub total_available_cents: i64,
}

#[derive(Deserialize)]
struct Response {
    object: String,
    is_available: bool,
    balance_infos: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    currency: String,
    cash_balance: String,
    gift_balance: String,
    frozen_balance: String,
    credit_limit: String,
    credit_available: String,
    total_available: String,
}

fn cents(s: &str) -> Result<i64, ProviderError> {
    fixed(s, 2)
}
pub(super) fn fixed(s: &str, precision: usize) -> Result<i64, ProviderError> {
    let digits = s.strip_prefix('-').unwrap_or(s);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    let valid = |v: &str| v.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || !valid(whole) || !valid(fraction) || fraction.len() > precision {
        return Err(ProviderError::Decode("invalid MiMo currency amount"));
    }
    let sign = if s.starts_with('-') { "-" } else { "" };
    format!("{sign}{whole}{fraction:0<precision$}")
        .parse()
        .map_err(|_| ProviderError::OUT_OF_RANGE)
}

pub async fn fetch_balance(http: &Http, key: &str) -> Result<Balance, ProviderError> {
    if !key.starts_with("sk-") || key.len() == 3 {
        return Err(ProviderError::Unsupported(
            "MiMo balance requires a regular sk- key",
        ));
    }
    let body = http.get(URL, Auth::Bearer(key), &[]).await?;
    let response: Response = serde_json::from_value(body)
        .map_err(|_| ProviderError::Decode("invalid MiMo account balance"))?;
    if response.object != "balance" || response.balance_infos.is_empty() {
        return Err(ProviderError::Decode(
            "MiMo did not return an account balance",
        ));
    }
    let balances = response
        .balance_infos
        .into_iter()
        .map(|item| {
            if item.currency.trim().is_empty() {
                return Err(ProviderError::Decode("missing MiMo currency"));
            }
            Ok(CurrencyBalance {
                currency: item.currency,
                cash_cents: cents(&item.cash_balance)?,
                gift_cents: cents(&item.gift_balance)?,
                frozen_cents: cents(&item.frozen_balance)?,
                credit_limit_cents: cents(&item.credit_limit)?,
                available_credit_cents: cents(&item.credit_available)?,
                total_available_cents: cents(&item.total_available)?,
            })
        })
        .collect::<Result<_, ProviderError>>()?;
    Ok(Balance {
        available: response.is_available,
        balances,
    })
}
