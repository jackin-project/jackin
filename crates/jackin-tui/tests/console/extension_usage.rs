// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Provider, Secret};
use jackin_testkit::{Method, Route, TestResult, UsageServer};
use jackin_usage::accounts::UsageProvider;
use jackin_usage::connected::KeyConnection;
use jackin_usage::gemini_login;
use serde_json::{Value, json};

use super::owned_usage::{Expected, inspect, journey};

async fn key_snapshot(
    provider: Provider,
    key: &str,
    path: &str,
    body: Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let server = UsageServer::start().await?;
    server.serve(
        provider,
        Method::Get,
        path,
        Route::json(200, &body.to_string())
            .requires_header("authorization", &format!("Bearer {key}")),
    );
    let region = if provider == Provider::Mimo {
        "sgp"
    } else {
        "global"
    };
    Ok(KeyConnection::for_provider(provider, Some(region))?
        .read(&server.endpoints(), &Secret::new(key.to_owned()))
        .await?)
}

#[tokio::test]
async fn kimi_displays_the_fractional_booster_balance_and_plan_window() -> TestResult {
    let snapshot = key_snapshot(Provider::Kimi, "fixture", "/coding/v1/usages", json!({
        "usages":{"limit_5h":{"used_ratio":0.25}},
        "boosterWallet":{"balance":{"type":"BOOSTER","amount":1000000,"amountLeft":125},"monthlyChargeLimitEnabled":false}
    })).await?;
    inspect(
        UsageProvider::Kimi,
        snapshot,
        Expected {
            account: &[
                "Global",
                "0.000125 cents",
                "75%",
                "Monthly charge limit · Off",
            ],
            board: &["Booster", "0.000125 cents", "75%"],
            absent: &["millionths_of_cent"],
        },
    )
    .await
}

#[tokio::test]
async fn minimax_displays_model_windows_with_the_region() -> TestResult {
    let snapshot=key_snapshot(Provider::Minimax, "fixture", "/v1/token_plan/remains",json!({
        "base_resp":{"status_code":0},"model_remains":[{"model_name":"general","current_weekly_usage_count":10,"current_weekly_total_count":100,"current_weekly_remaining_percent":90}]
    })).await?;
    inspect(
        UsageProvider::MiniMax,
        snapshot,
        Expected {
            account: &["Global", "general weekly"],
            board: &["general weekly"],
            absent: &["current_weekly_total_count"],
        },
    )
    .await
}

#[tokio::test]
async fn mimo_displays_plan_region_renewal_and_remaining_credits() -> TestResult {
    let snapshot=key_snapshot(Provider::Mimo,"tp-fixture","/v1/platform/token_plan",json!({
        "object":"token_plan","plan_code":"Lite","period":{"start_at":"2026-11-01 00:00:00","end_at":"2026-12-01 00:00:00"},
        "quota":{"total_credits":55000000000_i64,"used_credits":4525748922_i64,"remaining_credits":50474251078_i64},"auto_renew":false
    })).await?;
    journey(
        UsageProvider::Mimo,
        snapshot,
        Expected {
            account: &[
                "Singapore",
                "Automatic renewal · Off",
                "50474251078 credits",
            ],
            board: &["50.47B credits"],
            absent: &["auto_renew"],
        },
        Some((
            "Token Plan",
            &["50474251078 credits", "55000000000 credits"],
        )),
    )
    .await
}

#[tokio::test]
async fn gemini_displays_remaining_fraction_as_percent_and_raw_requests_separately() -> TestResult {
    let server = UsageServer::start().await?;
    server.serve(Provider::Gemini,Method::Post,"/v1internal:retrieveUserQuota",
        Route::json(200,r#"{"buckets":[{"modelId":"gemini-flash","tokenType":"REQUESTS","remainingAmount":"290","remainingFraction":0.9}]}"#));
    let http = server.endpoints().http(Provider::Gemini)?;
    let snapshot =
        gemini_login::snapshot(&http, &Secret::new("fixture".into()), "fixture-project").await?;
    inspect(
        UsageProvider::Gemini,
        snapshot,
        Expected {
            account: &["fixture-project", "90%", "290 REQUESTS"],
            board: &["gemini-flash", "90%"],
            absent: &["290%", "remaining_fraction"],
        },
    )
    .await
}

#[tokio::test]
async fn minimax_metered_balance_keeps_small_nonzero_amounts_and_alert_state() -> TestResult {
    let snapshot = key_snapshot(
        Provider::Minimax,
        "sk-api-fixture",
        "/account/query_balance",
        json!({
            "base_resp":{"status_code":0},"available_amount":"0.001251","cash_balance":"0.001251",
            "voucher_balance":"0","credit_balance":"0","owed_amount":"0","balance_alert_switch":true
        }),
    )
    .await?;
    inspect(
        UsageProvider::MiniMax,
        snapshot,
        Expected {
            account: &["Balance alerts · On", "0.001251 monetary units"],
            board: &["Available", "0.001251 monetary units"],
            absent: &["sk-api-fixture"],
        },
    )
    .await
}

#[tokio::test]
async fn mimo_account_shows_balance_currency_partial_history_and_exact_daily_cost() -> TestResult {
    let server = UsageServer::start().await?;
    let provider = Provider::Mimo;
    let auth =
        |body: &str| Route::json(200, body).requires_header("authorization", "Bearer sk-fixture");
    server.serve(provider,Method::Get,"/v1/platform/balance",auth(r#"{"object":"balance","is_available":true,"balance_infos":[{"currency":"CNY","cash_balance":"1.25","gift_balance":"0","frozen_balance":"0","credit_limit":"0","credit_available":"0","total_available":"1.25"}]}"#));
    server.serve(provider,Method::Get,"/v1/platform/usage/daily?start_date=2026-10-24&end_date=2026-11-22",auth(r#"{"object":"list","data":[{"date":"2026-11-22","currency":"CNY","resource_type":"model","name":"MiMo model","consumed_amount":"0.000001","total_tokens":11,"request_count":2}]}"#));
    server.serve(
        provider,
        Method::Get,
        "/v1/platform/billing/daily?start_date=2026-10-24&end_date=2026-11-22",
        Route::status(503),
    );
    let snapshot = KeyConnection::for_provider(provider, None)?
        .read_with_policy(
            &server.endpoints(),
            &Secret::new("sk-fixture".into()),
            jackin_core::Timestamp(1795305600),
            None,
            false,
        )
        .await?;
    inspect(
        UsageProvider::Mimo,
        snapshot,
        Expected {
            account: &[
                "Total available",
                "1.25 CNY",
                "Daily history unavailable",
                "0.000001 CNY",
                "2 requests",
            ],
            board: &[
                "Total available",
                "1.25 CNY",
                "Daily consumption",
                "Daily bills",
            ],
            absent: &["CNY/totalAvailable"],
        },
    )
    .await
}
