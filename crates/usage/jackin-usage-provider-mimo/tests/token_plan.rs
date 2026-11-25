// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::Provider;
use jackin_testkit::{Method, Route, UsageServer};
use jackin_usage_provider_core::{Http, ProviderError};
use jackin_usage_provider_mimo::{CreditQuota, Region, TokenPlan, fetch_token_plan};
use serde_json::{Value, json};

const TEST_KEY: &str = "tp-fixture";
const QUOTA_PATH: &str = "/v1/platform/token_plan";

fn plan() -> Value {
    json!({
        "object": "token_plan", "plan_code": "lite",
        "period": {"start_at": "2026-01-01 00:00:00", "end_at": "2026-02-01 00:00:00"},
        "quota": {"total_credits": 9007199254740993_u64, "used_credits": 9007199254740993_u64, "remaining_credits": 0},
        "auto_renew": false
    })
}

#[tokio::test]
async fn reads_exact_credit_counts_and_utc_period() -> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    server.serve(
        Provider::Mimo,
        Method::Get,
        QUOTA_PATH,
        Route::json(200, &plan().to_string()).requires_header("authorization", "Bearer tp-fixture"),
    );
    let http =
        Http::new(Duration::from_secs(5)).with_base(&server.provider_base(Provider::Mimo))?;
    assert_eq!(
        fetch_token_plan(&http, Region::China, TEST_KEY).await?,
        TokenPlan {
            plan_code: "lite".to_owned(),
            starts_at: 1767225600,
            ends_at: 1769904000,
            quota: CreditQuota {
                total_credits: 9007199254740993,
                used_credits: 9007199254740993,
                remaining_credits: 0
            },
            auto_renew: Some(false),
            claw_enabled: None
        }
    );
    Ok(())
}

#[tokio::test]
async fn rejects_wrong_product_incomplete_quota_and_provider_failures() -> Result<(), Box<dyn Error>>
{
    let server = UsageServer::start().await?;
    let http =
        Http::new(Duration::from_secs(5)).with_base(&server.provider_base(Provider::Mimo))?;
    assert!(matches!(
        fetch_token_plan(&http, Region::Europe, "sk-fixture").await,
        Err(ProviderError::Unsupported(_))
    ));
    let mut incomplete = plan();
    incomplete["quota"] = json!({"total_credits": 37, "used_credits": 37});
    for body in [incomplete, json!({"error":{"message":"invalid key"}})] {
        server.replace(
            Provider::Mimo,
            Method::Get,
            QUOTA_PATH,
            Route::json(200, &body.to_string()),
        );
        assert!(matches!(
            fetch_token_plan(&http, Region::Singapore, TEST_KEY).await,
            Err(ProviderError::Decode(_))
        ));
    }
    server.replace(Provider::Mimo, Method::Get, QUOTA_PATH, Route::status(401));
    assert_eq!(
        fetch_token_plan(&http, Region::Europe, TEST_KEY).await,
        Err(ProviderError::Unauthorized)
    );
    Ok(())
}

#[tokio::test]
async fn reads_currency_decimals_and_refuses_token_plan_keys_for_cash_balance()
-> Result<(), Box<dyn Error>> {
    use jackin_usage_provider_mimo::fetch_balance;
    let server = UsageServer::start().await?;
    let http =
        Http::new(Duration::from_secs(4)).with_base(&server.provider_base(Provider::Mimo))?;
    let path = "/v1/platform/balance";
    let mut body = json!({"object":"balance","is_available":false,"balance_infos":[{"currency":"CNY","cash_balance":"-0.01","gift_balance":"0.00","frozen_balance":"17.60","credit_limit":"100.00","credit_available":"10.75","total_available":"10.74"}]});
    server.serve(
        Provider::Mimo,
        Method::Get,
        path,
        Route::json(200, &body.to_string()),
    );
    let balance = fetch_balance(&http, "sk-fixture").await?;
    assert!(!balance.available);
    assert_eq!(balance.balances[0].currency, "CNY");
    assert_eq!(balance.balances[0].cash_cents, -1);
    assert_eq!(balance.balances[0].gift_cents, 0);
    assert_eq!(balance.balances[0].total_available_cents, 1074);
    assert!(matches!(
        fetch_balance(&http, TEST_KEY).await,
        Err(ProviderError::Unsupported(_))
    ));
    body["balance_infos"][0]["cash_balance"] = json!("0.011");
    server.replace(
        Provider::Mimo,
        Method::Get,
        path,
        Route::json(200, &body.to_string()),
    );
    assert!(matches!(
        fetch_balance(&http, "sk-fixture").await,
        Err(ProviderError::Decode(_))
    ));
    Ok(())
}
