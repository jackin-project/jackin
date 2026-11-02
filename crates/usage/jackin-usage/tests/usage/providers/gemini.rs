// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, UsageAccount, UsageOutcome, WindowKind};
use serde_json::json;

use super::Case;
use crate::support::{
    Left, TestResult, UsageHarness, Want, account, assert_windows, env_source, file_source, guard,
    unavailable, want,
};

const TOKEN: &str = "SECRET_TOKEN_GEMINI";
const LOAD_PATH: &str = "/v1internal:loadCodeAssist";
const QUOTA_PATH: &str = "/v1internal:retrieveUserQuota";
const LOAD: &str = include_str!("../fixtures/gemini/load_code_assist.json");
const NO_PROJECT: &str = include_str!("../fixtures/gemini/load_code_assist_no_project.json");
const QUOTA: &str = include_str!("../fixtures/gemini/retrieve_user_quota.json");
const EMPTY_BUCKETS: &str =
    include_str!("../fixtures/gemini/retrieve_user_quota_empty_buckets.json");
const EMPTY_OBJECT: &str = include_str!("../fixtures/gemini/retrieve_user_quota_empty_object.json");
const PARTIAL: &str = include_str!("../fixtures/gemini/retrieve_user_quota_partial.json");
const CREDS: &str = include_str!("../fixtures/gemini/oauth_creds.json");
const EXPIRED: &str = include_str!("../fixtures/gemini/oauth_creds_expired.json");
const CREDS_PATH: &str = "/home/work/.gemini/oauth_creds.json";
const REASON: &str = "Gemini API keys have no quota method";

const PRO: Want = want(
    WindowKind::Other,
    "gemini-2.5-pro requests",
    Some(25.0),
    Left::Percent(75.0),
    Some(1_761_148_875),
);
const FLASH: Want = want(
    WindowKind::Other,
    "gemini-2.5-flash requests",
    Some(10.0),
    Left::Percent(90.0),
    Some(1_761_148_875),
);

fn serve(harness: &UsageHarness, load: Route, quota: Route) {
    let bearer = format!("Bearer {TOKEN}");
    let metadata = json!({"metadata": {
        "ideType": "IDE_UNSPECIFIED",
        "platform": "PLATFORM_UNSPECIFIED",
        "pluginType": "GEMINI",
    }});
    harness.server.replace(
        Provider::Gemini,
        Method::Post,
        LOAD_PATH,
        load.requires_header("authorization", &bearer)
            .requires_header("content-type", "application/json")
            .requires_json_body(metadata),
    );
    harness.server.replace(
        Provider::Gemini,
        Method::Post,
        QUOTA_PATH,
        quota
            .requires_header("authorization", &bearer)
            .requires_header("content-type", "application/json")
            .requires_json_body(json!({"project": "my-cloudcode-project"})),
    );
}

async fn started_with(
    load: &str,
    quota: &str,
) -> Result<(UsageHarness, UsageAccount), Box<dyn std::error::Error>> {
    let mut harness = UsageHarness::new().await?;
    harness.env("GEMINI_TEST_KEY", TOKEN);
    serve(&harness, Route::json(200, load), Route::json(200, quota));
    Ok((
        harness,
        account("gemini", Provider::Gemini, env_source("GEMINI_TEST_KEY")),
    ))
}

pub(crate) async fn case() -> Result<Case, Box<dyn std::error::Error>> {
    let (harness, account) = started_with(LOAD, QUOTA).await?;
    Ok(Case {
        provider: Provider::Gemini,
        harness,
        account,
        first: (Method::Post, LOAD_PATH),
        expected: &[PRO, FLASH],
    })
}

async fn outcome(load: &str, quota: &str) -> Result<UsageOutcome, Box<dyn std::error::Error>> {
    let (harness, account) = started_with(load, quota).await?;
    Ok(harness.one(&account, Refresh::Cached).await?.outcome)
}

fn assert_provider(outcome: &UsageOutcome) {
    assert!(
        matches!(unavailable(outcome), UnavailableReason::Provider(_)),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn the_project_of_the_first_request_selects_the_buckets_of_the_second() -> TestResult {
    guard(async {
        assert_windows(&outcome(LOAD, QUOTA).await?, &[PRO, FLASH]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_api_key_is_unsupported_with_a_reason_and_no_request() -> TestResult {
    guard(async {
        let mut harness = UsageHarness::new().await?;
        harness.env("GEMINI_TEST_KEY", "AIzaSECRET_TOKEN_GEMINI");
        serve(&harness, Route::json(200, LOAD), Route::json(200, QUOTA));
        let account = account("gemini", Provider::Gemini, env_source("GEMINI_TEST_KEY"));
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::Unsupported {
                detail: Some(REASON.to_owned())
            }
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_file_gives_its_access_token() -> TestResult {
    guard(async {
        let (harness, _) = started_with(LOAD, QUOTA).await?;
        harness.put(CREDS_PATH, CREDS).await?;
        let login = account("gemini", Provider::Gemini, file_source(CREDS_PATH));
        let usage = harness.one(&login, Refresh::Cached).await?;
        assert_windows(&usage.outcome, &[PRO, FLASH]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_login_whose_expiry_date_has_passed_is_an_expired_login_without_a_request() -> TestResult
{
    guard(async {
        let (harness, _) = started_with(LOAD, QUOTA).await?;
        harness.put(CREDS_PATH, EXPIRED).await?;
        let login = account("gemini", Provider::Gemini, file_source(CREDS_PATH));
        let usage = harness.one(&login, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::ExpiredLogin
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_account_without_a_project_is_a_provider_error_and_the_quota_route_is_not_reached()
-> TestResult {
    guard(async {
        assert_provider(&outcome(NO_PROJECT, QUOTA).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_401_on_the_first_request_is_an_expired_login() -> TestResult {
    guard(async {
        let (harness, account) = started_with(LOAD, QUOTA).await?;
        harness.server.replace(
            Provider::Gemini,
            Method::Post,
            LOAD_PATH,
            Route::status(401),
        );
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::ExpiredLogin
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_401_on_the_second_request_is_an_expired_login() -> TestResult {
    guard(async {
        let (harness, account) = started_with(LOAD, QUOTA).await?;
        harness.server.replace(
            Provider::Gemini,
            Method::Post,
            QUOTA_PATH,
            Route::status(401),
        );
        let usage = harness.one(&account, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::ExpiredLogin
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn buckets_without_a_model_or_a_fraction_are_dropped() -> TestResult {
    guard(async {
        let kept = want(
            WindowKind::Other,
            "gemini-2.5-pro requests",
            Some(25.0),
            Left::Percent(75.0),
            Some(1_761_148_875),
        );
        assert_windows(&outcome(LOAD, PARTIAL).await?, &[kept]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_empty_bucket_list_is_a_provider_error() -> TestResult {
    guard(async {
        assert_provider(&outcome(LOAD, EMPTY_BUCKETS).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_empty_quota_object_is_a_provider_error() -> TestResult {
    guard(async {
        assert_provider(&outcome(LOAD, EMPTY_OBJECT).await?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_missing_login_file_is_no_credentials() -> TestResult {
    guard(async {
        let (harness, _) = started_with(LOAD, QUOTA).await?;
        let login = account("gemini", Provider::Gemini, file_source(CREDS_PATH));
        let usage = harness.one(&login, Refresh::Cached).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::NoCredentials
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_expiry_date_at_the_edge_of_the_integers_does_not_panic() -> TestResult {
    guard(async {
        let (harness, _) = started_with(LOAD, QUOTA).await?;
        let far = CREDS.replace("1767232800000", "9223372036854775807");
        let past = CREDS.replace("1767232800000", "-9223372036854775808");
        harness.put(CREDS_PATH, &far).await?;
        let login = account("gemini", Provider::Gemini, file_source(CREDS_PATH));
        assert_windows(
            &harness.one(&login, Refresh::Cached).await?.outcome,
            &[PRO, FLASH],
        );
        harness.put("/home/work/.gemini/past.json", &past).await?;
        let login = account(
            "gemini-past",
            Provider::Gemini,
            file_source("/home/work/.gemini/past.json"),
        );
        let usage = harness.one(&login, Refresh::Force).await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::ExpiredLogin
        );
        Ok(())
    })
    .await
}
