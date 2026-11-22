// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::Region;
use jackin_usage_provider_core::{Auth, Http, ProviderError};
use serde_json::Value;

#[derive(Debug)]
pub struct Balance {
    pub amounts: Vec<(&'static str, i64, usize)>,
    pub alerts: Option<bool>,
}

pub async fn fetch_balance(
    http: &Http,
    region: Region,
    key: &str,
) -> Result<Balance, ProviderError> {
    let url = match region {
        Region::Global => "https://api.minimax.io/account/query_balance",
        Region::China => "https://api.minimax.cn/account/query_balance",
    };
    let body = http.get(url, Auth::Bearer(key), &[]).await?;
    if body
        .pointer("/base_resp/status_code")
        .and_then(Value::as_i64)
        != Some(0)
    {
        return Err(ProviderError::Decode(
            "MiniMax rejected the balance request",
        ));
    }
    let mut amounts = Vec::new();
    for (field, label) in [
        ("available_amount", "Available"),
        ("cash_balance", "Cash"),
        ("voucher_balance", "Vouchers"),
        ("credit_balance", "Credit"),
        ("owed_amount", "Owed"),
        ("balance_alert_threshold", "Alert threshold"),
    ] {
        let s = body[field].as_str().unwrap_or_default();
        if s.is_empty() && field == "balance_alert_threshold" {
            continue;
        }
        let digits = s.strip_prefix('-').unwrap_or(s);
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty()
            || fraction.len() > 9
            || !whole
                .bytes()
                .chain(fraction.bytes())
                .all(|b| b.is_ascii_digit())
        {
            return Err(ProviderError::Decode("invalid MiniMax balance amount"));
        }
        let n = s
            .replace('.', "")
            .parse::<i64>()
            .map_err(|_| ProviderError::OUT_OF_RANGE)?;
        amounts.push((label, n, fraction.len()));
    }
    Ok(Balance {
        amounts,
        alerts: body["balance_alert_switch"].as_bool(),
    })
}
