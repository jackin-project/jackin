// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Provider;
use jackin_usage::{Refresh, UnavailableReason};

use crate::support::{TestResult, UsageHarness, account, env_source, guard, unavailable};

const UNSUPPORTED: [Provider; 4] = [
    Provider::Amp,
    Provider::Antigravity,
    Provider::Hermes,
    Provider::Muse,
];

#[tokio::test]
async fn each_unsupported_provider_reports_unsupported_without_a_detail() -> TestResult {
    guard(async {
        let harness = UsageHarness::new().await?;
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
    })
    .await
}

#[tokio::test]
async fn an_unsupported_provider_stays_unsupported_when_the_network_is_gone() -> TestResult {
    guard(async {
        let harness = UsageHarness::new().await?;
        harness.server.shutdown().await;
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
    })
    .await
}
