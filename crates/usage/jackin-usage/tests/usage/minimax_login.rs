// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{CredentialSource, Provider};
use jackin_usage::{Refresh, UnavailableReason, UsageOutcome};

use crate::support::{TestResult, UsageHarness, account, host_path};

const ROOT: &str = "/home/work/minimax";
const EN: &str = "/home/work/minimax/auth/prod/en/mcode-public/auth.json";
const CN: &str = "/home/work/minimax/auth/prod/cn/mcode-public/auth.json";
const LOGIN: &str = "{\"access_token\":\"SECRET_TOKEN_MINIMAX\"}";
const OUTER: Duration = Duration::from_secs(120);

async fn report(harness: &UsageHarness) -> Result<UsageOutcome, Box<dyn std::error::Error>> {
    let account = account(
        "minimax",
        Provider::Minimax,
        CredentialSource::HostLogin {
            path: host_path(ROOT),
        },
    );
    let host = harness.host();
    let finished = tokio::time::timeout(
        OUTER,
        harness.report_on(&host, std::slice::from_ref(&account), Refresh::Force),
    )
    .await
    .map_err(|_| "the report never finished")?;
    Ok(finished?.accounts.remove(0).outcome)
}

fn message_of(outcome: &UsageOutcome) -> &str {
    match outcome {
        UsageOutcome::Unavailable(UnavailableReason::Provider(message)) => message,
        other => panic!("expected a provider refusal, found {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_login_path_that_never_resolves_is_given_up_on() -> TestResult {
    let harness = UsageHarness::new().await?;
    harness.put(EN, LOGIN).await?;
    let probe = harness
        .fs
        .stall_canonicalize_of("en/mcode-public/auth.json");
    let outcome = report(&harness).await?;
    assert_eq!(
        message_of(&outcome),
        "the credential file could not be read"
    );
    assert_eq!(probe.entered(), 1);
    assert_eq!(probe.abandoned(), 1);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_login_file_that_never_reads_is_given_up_on() -> TestResult {
    let harness = UsageHarness::new().await?;
    harness.put(EN, LOGIN).await?;
    let probe = harness.fs.stall_reads_of("auth.json");
    let outcome = report(&harness).await?;
    assert_eq!(
        message_of(&outcome),
        "the credential file could not be read"
    );
    assert_eq!(probe.entered(), 1);
    assert_eq!(probe.abandoned(), 1);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn a_second_login_that_stalls_makes_the_first_unusable() -> TestResult {
    let harness = UsageHarness::new().await?;
    harness.put(EN, LOGIN).await?;
    harness.put(CN, LOGIN).await?;
    let probe = harness
        .fs
        .stall_canonicalize_of("cn/mcode-public/auth.json");
    let outcome = report(&harness).await?;
    assert_eq!(
        message_of(&outcome),
        "the credential file could not be read"
    );
    assert_eq!(probe.entered(), 1);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn the_logins_keep_their_meaning_when_nothing_stalls() -> TestResult {
    let harness = UsageHarness::new().await?;
    let outcome = report(&harness).await?;
    assert_eq!(
        outcome,
        UsageOutcome::Unavailable(UnavailableReason::NoCredentials)
    );

    harness.put(EN, "  \n").await?;
    harness.put(CN, LOGIN).await?;
    let outcome = report(&harness).await?;
    assert_eq!(
        outcome,
        UsageOutcome::Unavailable(UnavailableReason::NoCredentials),
        "an empty login file hides the other region as before"
    );

    harness.fs.create(&host_path(EN), LOGIN.as_bytes().to_vec());
    let outcome = report(&harness).await?;
    assert_eq!(
        message_of(&outcome),
        "the MiniMax store holds more than one login"
    );
    Ok(())
}
