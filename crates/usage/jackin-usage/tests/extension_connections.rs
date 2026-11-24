// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::{Provider, Secret, Timestamp};
use jackin_testkit::{Method, Route, UsageServer};
use jackin_usage::connected::KeyConnection;
use serde_json::json;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[tokio::test]
async fn regional_and_balance_credentials_reopen_the_same_usage_source() -> Result<()> {
    let server = UsageServer::start().await?;
    let key = Secret::new("sk-fixture".to_owned());
    for (provider, region) in [
        (Provider::Kimi, Some("cn")),
        (Provider::Kimi, Some("global")),
        (Provider::Minimax, Some("cn")),
        (Provider::Minimax, Some("global")),
        (Provider::Mimo, None),
        (Provider::Mimo, Some("ams")),
    ] {
        let method = KeyConnection::for_provider(provider, region)?;
        let (source, credential) = KeyConnection::decode(provider, &method.encode(&key)?)?;
        assert_eq!(source, method);
        if provider == Provider::Mimo && region.is_none() {
            server.serve(Provider::Mimo,Method::Get,"/v1/platform/balance",Route::json(200,r#"{"object":"balance","is_available":false,"balance_infos":[{"currency":"CNY","cash_balance":"0.00","gift_balance":"0.00","frozen_balance":"0.00","credit_limit":"1.04","credit_available":"0.00","total_available":"0.00"}]}"#).requires_header("authorization","Bearer sk-fixture"));
            let snapshot = source.read(&server.endpoints(), &credential).await?;
            assert_eq!(snapshot["resources"]["CNY/cash"]["available"], 0);
            assert_eq!(snapshot["resources"]["CNY/creditLimit"]["available"], 104);
            assert_eq!(snapshot["resources"]["CNY/cash"]["currency"], "CNY");
        }
    }
    Ok(())
}

#[tokio::test]
async fn mimo_daily_history_preserves_partial_reads_and_exact_units() -> Result<()> {
    let server = UsageServer::start().await?;
    let provider = Provider::Mimo;
    let auth = |s: &str| Route::json(200, s).requires_header("authorization", "Bearer sk-fixture");
    server.serve(provider,Method::Get,"/v1/platform/balance",auth(r#"{"object":"balance","is_available":true,"balance_infos":[{"currency":"CNY","cash_balance":"0","gift_balance":"0","frozen_balance":"0","credit_limit":"0","credit_available":"0","total_available":"0"}]}"#));
    let now = time::macros::datetime!(2026-11-22 0:00 UTC).unix_timestamp();
    let usage = "/v1/platform/usage/daily?start_date=2026-10-24&end_date=2026-11-22";
    let bills = "/v1/platform/billing/daily?start_date=2026-10-24&end_date=2026-11-22";
    server.serve(provider,Method::Get,usage,auth(r#"{"object":"list","data":[{"date":"2026-11-22","currency":"CNY","resource_type":"model","name":"test","consumed_amount":"0.000001","total_tokens":9007199254740993,"request_count":2,"masked_api_key":"sensitive"},{"date":"2026-10-24","currency":"USD","resource_type":"plugin","name":"tool","consumed_amount":"1.200001","request_count":1}]}"#));
    server.serve(provider, Method::Get, bills, Route::status(500));
    let method = KeyConnection::MimoBalance;
    let key = Secret::new("sk-fixture".to_owned());
    let endpoints = server.endpoints();
    let first = method
        .read_with_policy(&endpoints, &key, Timestamp(now), None, false)
        .await?;
    assert_eq!(
        first["daily"]["consumption"]["rows"][0]["consumed_amount"],
        1
    );
    assert_eq!(
        first["daily"]["consumption"]["rows"][0]["total_tokens"],
        9007199254740993_u64
    );
    assert!(
        first["daily"]["consumption"]["rows"][1]
            .get("total_tokens")
            .is_none()
    );
    assert!(!first.to_string().contains("sensitive"));
    assert_eq!(first["daily"]["bills"]["unavailable"], true);
    let details = &first["resources"]["daily/consumption"]["details"];
    assert!(details.to_string().contains("Total 1.200001 USD"));
    assert!(details.to_string().contains("Total 0.000001 CNY"));
    server.replace(provider, Method::Get, usage, Route::status(500));
    server.replace(provider,Method::Get,bills,auth(r#"{"object":"list","data":[{"date":"2026-11-22","currency":"USD","original_amount":"1.50","discount_amount":"0.25","gift_amount":"0.11","final_paid_amount":"1.14"}]}"#));
    let second = method
        .read_with_policy(&endpoints, &key, Timestamp(now + 300), Some(&first), false)
        .await?;
    assert_eq!(
        second["daily"]["bills"]["rows"][0]["final_paid_amount"],
        114
    );
    assert_eq!(
        second["daily"]["consumption"],
        first["daily"]["consumption"]
    );
    let partial = method
        .read_with_policy(
            &endpoints,
            &key,
            Timestamp(now + 3601),
            Some(&second),
            false,
        )
        .await?;
    assert_eq!(
        partial["daily"]["consumption"]["rows"],
        first["daily"]["consumption"]["rows"]
    );
    assert_eq!(partial["daily"]["consumption"]["unavailable"], true);
    server.replace(
        provider,
        Method::Get,
        usage,
        auth(r#"{"object":"list","data":[]}"#),
    );
    let recovered = method
        .read_with_policy(
            &endpoints,
            &key,
            Timestamp(now + 3902),
            Some(&partial),
            false,
        )
        .await?;
    assert_eq!(recovered["daily"]["consumption"]["rows"], json!([]));
    assert!(server.mismatches().is_empty());
    Ok(())
}

#[tokio::test]
async fn minimax_metered_key_selects_regional_balance_and_keeps_provider_units() -> Result<()> {
    let s = UsageServer::start().await?;
    let p = Provider::Minimax;
    let path = "/account/query_balance";
    let response = json!({"available_amount":"0.00","cash_balance":"-1.02","voucher_balance":"0.001","credit_balance":"10","owed_amount":"2.50","balance_alert_switch":false,"balance_alert_threshold":"","base_resp":{"status_code":0}});
    s.serve(
        p,
        Method::Get,
        path,
        Route::json(200, &response.to_string())
            .requires_header("authorization", "Bearer sk-api-key"),
    );
    let method = KeyConnection::for_provider(p, Some("cn"))?;
    let key = Secret::new("sk-api-key".into());
    let snapshot = method.read(&s.endpoints(), &key).await?;
    let resources = &snapshot["resources"];
    assert_eq!(resources["balance/totalAvailable"]["available"], 0);
    assert_eq!(resources["balance/cash"]["available"], -102);
    assert_eq!(resources["balance/vouchers"]["exponent"], 3);
    assert!(
        resources["balance/totalAvailable"]
            .get("currency")
            .is_none()
    );
    let mut missing = response;
    missing["cash_balance"] = serde_json::Value::Null;
    s.replace(p, Method::Get, path, Route::json(200, &missing.to_string()));
    method
        .read(&s.endpoints(), &key)
        .await
        .expect_err("missing cash balance must reject the candidate");
    assert!(s.mismatches().is_empty());
    Ok(())
}
