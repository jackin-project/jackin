// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::time::Duration;

use jackin_core::Provider;
use jackin_testkit::{Method, Route, UsageServer};
use jackin_usage_provider_core::{Http, ProviderError};
use jackin_usage_provider_gemini::{fetch_buckets, identify, load_project};
use serde_json::json;

#[tokio::test]
async fn resolves_project_and_retains_amount_only_and_fraction_only_buckets()
-> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    let http =
        Http::new(Duration::from_secs(4)).with_base(&server.provider_base(Provider::Gemini))?;
    let load = "/v1internal:loadCodeAssist";
    server.serve(
        Provider::Gemini,
        Method::Get,
        "/oauth2/v2/userinfo",
        Route::json(200, r#"{"id":"subject","email":"example@test.invalid"}"#),
    );
    assert_eq!(identify(&http, "fixture").await?.subject, "subject");
    server.serve(
        Provider::Gemini,
        Method::Post,
        load,
        Route::json(
            200,
            r#"{"currentTier":{"id":"free"},"cloudaicompanionProject":"managed-project"}"#,
        ),
    );
    assert_eq!(
        load_project(&http, "fixture", None).await?.id,
        "managed-project"
    );
    server.replace(
        Provider::Gemini,
        Method::Post,
        load,
        Route::json(200, r#"{"currentTier":{"id":"standard"}}"#),
    );
    assert!(matches!(
        load_project(&http, "fixture", None).await,
        Err(ProviderError::Unsupported(_))
    ));
    let project = load_project(&http, "fixture", Some("selected")).await?;
    assert_eq!(project.id, "selected");
    server.serve(Provider::Gemini,Method::Post,"/v1internal:retrieveUserQuota",Route::json(200,r#"{"buckets":[{"modelId":"pro","tokenType":"REQUESTS","remainingAmount":"0"},{"modelId":"flash","remainingFraction":0.25}]}"#).requires_json_body(json!({"project":"selected"})));
    let buckets = fetch_buckets(&http, "fixture", &project.id).await?;
    assert_eq!(buckets.len(), 2);
    assert_eq!(buckets[0].remaining_amount, Some(0));
    assert_eq!(buckets[0].token_type.as_deref(), Some("REQUESTS"));
    assert!(buckets[0].remaining_fraction.is_none());
    assert!(buckets[1].remaining_amount.is_none());
    assert!(matches!(buckets[1].remaining_fraction,Some(n) if (n-0.25).abs() < f64::EPSILON));
    Ok(())
}

#[tokio::test]
async fn enrolls_eligible_tier_and_only_polls_google_operations() -> Result<(), Box<dyn Error>> {
    let server = UsageServer::start().await?;
    let p = Provider::Gemini;
    let http = Http::new(Duration::from_secs(5)).with_base(&server.provider_base(p))?;
    let load = "/v1internal:loadCodeAssist";
    let onboard = "/v1internal:onboardUser";
    let default = |id: &str| {
        Route::json(
            200,
            &json!({"allowedTiers":[{"id":id,"isDefault":true}]}).to_string(),
        )
    };
    server.serve(p, Method::Post, load, default("free-tier"));
    let pending =
        Route::json(200, r#"{"name":"operations/abc-id","done":false}"#).matching_json(|body| {
            body["tierId"] == "free-tier"
                && body.get("cloudaicompanionProject").is_none()
                && body["metadata"].get("duetProject").is_none()
        });
    server.serve(p, Method::Post, onboard, pending);
    server.serve(
        p,
        Method::Get,
        "/v1internal/operations/abc-id",
        Route::json(
            200,
            r#"{"done":true,"response":{"cloudaicompanionProject":{"id":"managed"}}}"#,
        )
        .requires_header("authorization", "Bearer grant"),
    );
    let http2 = http.clone();
    let task = tokio::spawn(async move { load_project(&http2, "grant", None).await });
    assert_eq!(task.await??.id, "managed");
    for name in [
        "https://bad.example/token",
        "operations/../../secret",
        "operations/%2e%2e",
        "operations/..",
    ] {
        server.replace(
            p,
            Method::Post,
            onboard,
            Route::json(200, &json!({"name":name,"done":false}).to_string()),
        );
        assert!(matches!(
            load_project(&http, "grant", None).await,
            Err(ProviderError::Decode(_))
        ));
    }
    server.replace(
        p,
        Method::Post,
        onboard,
        Route::json(200, r#"{"done":true,"error":{"code":403}}"#),
    );
    assert!(matches!(
        load_project(&http, "grant", None).await,
        Err(ProviderError::Unsupported(_))
    ));
    server.replace(p, Method::Post, load, default("standard-tier"));
    assert!(matches!(
        load_project(&http, "grant", None).await,
        Err(ProviderError::Unsupported(_))
    ));
    server.replace(
        p,
        Method::Post,
        onboard,
        Route::json(
            200,
            r#"{"done":true,"response":{"cloudaicompanionProject":{"id":"other"}}}"#,
        ),
    );
    assert!(matches!(
        load_project(&http, "grant", Some("wanted")).await,
        Err(ProviderError::Unsupported(_))
    ));
    assert!(server.mismatches().is_empty());
    Ok(())
}
