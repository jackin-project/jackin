// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::Provider;
use jackin_testkit::{Method, Route, UsageServer};
use jackin_usage_provider_core::{Http, ProviderError};
use jackin_usage_provider_minimax::{Region, fetch_plan};

#[tokio::test]
async fn plan_key_and_per_model_windows() -> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    let http =
        Http::new(Duration::from_secs(4)).with_base(&server.provider_base(Provider::Minimax))?;
    let path = "/v1/token_plan/remains";
    let body = r#"{"base_resp":{"status_code":0},"model_remains":[{"model_name":"general","current_weekly_usage_count":10,"current_weekly_total_count":100,"current_weekly_remaining_percent":90},{"model_name":"image","current_interval_status":1,"current_weekly_status":3},{"model_name":"video","current_interval_status":3,"current_interval_total_count":0,"current_weekly_status":3,"current_weekly_total_count":0}]}"#;
    server.serve(
        Provider::Minimax,
        Method::Get,
        path,
        Route::json(200, body).requires_header("authorization", "Bearer fixture"),
    );
    let quotas = fetch_plan(&http, Region::China, "fixture").await?;
    assert_eq!(quotas[0].weekly.remaining_count, Some(90));
    assert_eq!(quotas[1].weekly.status, Some(3));
    assert!(!quotas[1].excluded_from_plan);
    assert!(quotas[2].excluded_from_plan);
    assert!(quotas[1].interval.remaining_percent.is_none());
    assert!(matches!(
        fetch_plan(&http, Region::Global, "sk-api-fixture").await,
        Err(ProviderError::Unsupported(_))
    ));
    server.replace(
        Provider::Minimax,
        Method::Get,
        path,
        Route::json(
            200,
            r#"{"base_resp":{"status_code":1004},"model_remains":[]}"#,
        ),
    );
    assert!(fetch_plan(&http, Region::Global, "fixture").await.is_err());
    Ok(())
}
