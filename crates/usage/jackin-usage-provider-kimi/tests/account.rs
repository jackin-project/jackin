// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::{Provider, Secret};
use jackin_testkit::{Method, Route, UsageServer};
use jackin_usage_provider_core::{DevicePoll, DevicePollError, Http, ProviderError, poll_device};
use jackin_usage_provider_kimi::{Region, fetch_plan, identify, renew, start_device};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn region_scoped_identity_windows_and_booster() -> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    let http =
        Http::new(Duration::from_secs(4)).with_base(&server.provider_base(Provider::Kimi))?;
    server.serve(
        Provider::Kimi,
        Method::Get,
        "/coding/v1/me",
        Route::json(200, r#"{"user_id":"stable","nickname":"Example"}"#),
    );
    server.serve(Provider::Kimi, Method::Get, "/coding/v1/usages", Route::json(200, r#"{"usages":{"limit_month_code":{"used_ratio":0}},"boosterWallet":{"balance":{"type":"BOOSTER","amount":99000000,"amountLeft":"0"},"monthlyUsed":{"priceInCents":0,"currency":"USD"}}}"#));
    assert_eq!(
        identify(&http, Region::Global, "fixture").await?.user_id,
        "stable"
    );
    let plan = fetch_plan(&http, Region::Global, "fixture").await?;
    assert_eq!(plan.windows.len(), 1);
    let wallet = plan.booster.ok_or("missing booster")?;
    assert_eq!(wallet.remaining_millionths_of_cent, Some(0));
    assert_eq!(wallet.monthly_used.ok_or("missing spend")?.currency, "USD");
    assert!(wallet.monthly_limit.is_none());
    server.replace(
        Provider::Kimi,
        Method::Get,
        "/coding/v1/me",
        Route::json(200, r#"{"nickname":"Example"}"#),
    );
    assert!(matches!(
        identify(&http, Region::Mainland, "fixture").await,
        Err(ProviderError::Decode(_))
    ));
    Ok(())
}

#[tokio::test]
async fn device_prompt_renewal_and_pre_cancelled_wait() -> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    let http =
        Http::new(Duration::from_secs(4)).with_base(&server.provider_base(Provider::Kimi))?;
    server.serve(Provider::Kimi,Method::Post,"/api/oauth/device_authorization",Route::json(200,r#"{"verification_uri_complete":"https://auth.kimi.ai/device","user_code":"EXAMPLE","device_code":"device-fixture","expires_in":300,"interval":5}"#));
    server.serve(Provider::Kimi,Method::Post,"/api/oauth/token",Route::json(200,r#"{"access_token":"new-access-fixture","refresh_token":"new-refresh-fixture","expires_in":120}"#));
    let prompt = start_device(&http, Region::Global, "fixture-client").await?;
    assert_eq!(prompt.user_code, "EXAMPLE");
    assert_eq!(prompt.verification_url.host_str(), Some("auth.kimi.ai"));
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(matches!(
        prompt.wait(&http, &cancel).await,
        Err(DevicePollError::Cancelled)
    ));
    let token = renew(
        &http,
        Region::Global,
        "fixture-client",
        &Secret::new("old-fixture".into()),
    )
    .await?;
    assert_eq!(token.refresh_token.expose(), "new-refresh-fixture");
    assert_eq!(token.expires_in, Duration::from_secs(120));
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn slowdown_changes_subsequent_intervals_and_expiry_ends_an_inflight_poll() {
    let start = Instant::now();
    let cancel = CancellationToken::new();
    let mut count = 0;
    let granted = poll_device(
        Duration::from_secs(2),
        start + Duration::from_secs(30),
        &cancel,
        || {
            count += 1;
            std::future::ready(Ok(match count {
                1 => DevicePoll::Pending,
                2 => DevicePoll::SlowDown,
                _ => DevicePoll::Granted(17),
            }))
        },
    )
    .await;
    assert!(matches!(granted, Ok(17)));
    assert_eq!(Instant::now() - start, Duration::from_secs(11));
    let result = poll_device(
        Duration::from_secs(1),
        Instant::now() + Duration::from_secs(5),
        &cancel,
        std::future::pending::<Result<DevicePoll<()>, ProviderError>>,
    )
    .await;
    assert!(matches!(result, Err(DevicePollError::Expired)));
    let denied = poll_device(
        Duration::from_secs(1),
        Instant::now() + Duration::from_secs(8),
        &cancel,
        || std::future::ready(Ok(DevicePoll::<()>::Denied)),
    )
    .await;
    assert!(matches!(denied, Err(DevicePollError::Denied)));
}
