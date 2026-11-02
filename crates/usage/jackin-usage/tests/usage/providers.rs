// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Provider, Timestamp};
use jackin_testkit::{Method, Route};
use jackin_usage::{Refresh, UnavailableReason, UsageAccount};

use crate::support::{
    T0, TestResult, UsageHarness, Want, account, assert_windows, env_source, guard, unavailable,
};

mod claude;
mod gemini;
mod kimi;
mod minimax;
mod openrouter;
mod zai;

pub(crate) use openrouter::serve_valid as openrouter_serve;
pub(crate) use openrouter::started as openrouter_started;

pub(crate) struct Case {
    pub(crate) provider: Provider,
    pub(crate) harness: UsageHarness,
    pub(crate) account: UsageAccount,
    pub(crate) first: (Method, &'static str),
    pub(crate) expected: &'static [Want],
}

async fn cases() -> Result<Vec<Case>, Box<dyn std::error::Error>> {
    Ok(vec![
        claude::case().await?,
        gemini::case().await?,
        kimi::case().await?,
        minimax::case().await?,
        openrouter::case().await?,
        zai::case().await?,
    ])
}

fn fail_first(case: &Case, route: Route) {
    case.harness
        .server
        .replace(case.provider, case.first.0, case.first.1, route);
}

#[tokio::test]
async fn every_provider_case_gives_its_literal_windows_for_a_valid_body() -> TestResult {
    guard(async {
        for case in cases().await? {
            let usage = case.harness.one(&case.account, Refresh::Cached).await?;
            assert_windows(&usage.outcome, case.expected);
            let mismatches = case.harness.server.mismatches();
            assert!(mismatches.is_empty(), "{:?}: {mismatches:?}", case.provider);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn every_provider_case_maps_a_401_to_an_expired_login() -> TestResult {
    guard(async {
        for case in cases().await? {
            fail_first(&case, Route::status(401));
            let usage = case.harness.one(&case.account, Refresh::Cached).await?;
            assert_eq!(
                unavailable(&usage.outcome),
                &UnavailableReason::ExpiredLogin,
                "{:?}: {:?}",
                case.provider,
                case.harness.server.mismatches()
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn every_provider_case_maps_a_429_with_seconds_to_a_retry_time() -> TestResult {
    guard(async {
        for case in cases().await? {
            fail_first(&case, Route::status(429).reply_header("Retry-After", "30"));
            let usage = case.harness.one(&case.account, Refresh::Cached).await?;
            assert_eq!(
                unavailable(&usage.outcome),
                &UnavailableReason::RateLimited {
                    retry_at: Some(Timestamp(T0 + 30))
                },
                "{:?}",
                case.provider
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn every_provider_case_maps_a_garbage_body_to_a_provider_error() -> TestResult {
    guard(async {
        for case in cases().await? {
            fail_first(&case, Route::json(200, "not json"));
            let usage = case.harness.one(&case.account, Refresh::Cached).await?;
            assert!(
                matches!(unavailable(&usage.outcome), UnavailableReason::Provider(_)),
                "{:?}",
                case.provider
            );
        }
        Ok(())
    })
    .await
}

const UNSUPPORTED: [Provider; 4] = [
    Provider::Amp,
    Provider::Antigravity,
    Provider::Hermes,
    Provider::Muse,
];

async fn unsupported_after(shutdown: bool) -> TestResult {
    let harness = UsageHarness::new().await?;
    if shutdown {
        harness.server.shutdown().await;
    }
    for provider in UNSUPPORTED {
        let usage = harness
            .one(
                &account("acct", provider, env_source("ANY_KEY")),
                Refresh::Cached,
            )
            .await?;
        assert_eq!(
            unavailable(&usage.outcome),
            &UnavailableReason::Unsupported { detail: None },
            "{provider:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn each_unsupported_provider_reports_unsupported_without_a_detail() -> TestResult {
    guard(unsupported_after(false)).await
}

#[tokio::test]
async fn an_unsupported_provider_stays_unsupported_when_the_network_is_gone() -> TestResult {
    guard(unsupported_after(true)).await
}
