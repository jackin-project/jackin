// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::sync::Arc;

use jackin_core::{Clock, EnvSnapshot, FileSystem, FsError, ProcessRunner, Provider};
use jackin_testkit::{FakeClock, FsOp, MemFs, Method, Route, ScriptedProcessRunner};
use jackin_usage::{
    HostUsage, ProviderEndpoints, Refresh, UsageDeps, UsageError, UsageOutcome, UsageReport,
};

use crate::support::{
    CODEX_PATH, TestResult, UsageHarness, codex_body, codex_login, guard, host_path, windows_of,
};

const AUTH: &str = include_str!("fixtures/codex/auth_chatgpt.json");
const SECRETS: [&str; 3] = ["SECRET_TOKEN_CODEX", "SECRET_TEXT_BODY", "SECRET_TOKEN_ENV"];

fn assert_clean(surface: &str, text: &str) {
    for secret in SECRETS {
        assert!(
            !text.contains(secret),
            "{secret} leaked into {surface}: {text}"
        );
    }
}

async fn echoing() -> Result<(UsageHarness, UsageReport), Box<dyn std::error::Error>> {
    let mut harness = UsageHarness::new().await?;
    harness.env("OPENROUTER_TEST_KEY", "SECRET_TOKEN_ENV");
    for home in ["ok", "err", "junk"] {
        harness
            .put(&format!("/home/{home}/.codex/auth.json"), AUTH)
            .await?;
    }
    let echo = "\"note\":\"SECRET_TOKEN_CODEX SECRET_TEXT_BODY\"";
    let ok = codex_body(10).replacen('{', &format!("{{{echo},"), 1);
    harness.server.serve(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        Route::json(200, &ok)
            .requires_header("authorization", "Bearer SECRET_TOKEN_CODEX")
            .requires_header("chatgpt-account-id", "ok-id"),
    );
    harness.server.serve(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        Route::json(500, "{\"error\":\"SECRET_TOKEN_CODEX SECRET_TEXT_BODY\"}")
            .requires_header("chatgpt-account-id", "err-id"),
    );
    harness.server.serve(
        Provider::Codex,
        Method::Get,
        CODEX_PATH,
        Route::json(200, "not json SECRET_TOKEN_CODEX SECRET_TEXT_BODY")
            .requires_header("chatgpt-account-id", "junk-id"),
    );
    let accounts = [
        codex_login("echo-ok", "/home/ok/.codex", "ok-id"),
        codex_login("echo-err", "/home/err/.codex", "err-id"),
        codex_login("echo-junk", "/home/junk/.codex", "junk-id"),
    ];
    let report = harness.report(&accounts, Refresh::Cached).await?;
    Ok((harness, report))
}

#[tokio::test]
async fn the_report_its_json_and_its_debug_output_hold_no_secret() -> TestResult {
    guard(async {
        let (_harness, report) = echoing().await?;
        assert_eq!(windows_of(&report.accounts[0].outcome).len(), 1);
        assert!(matches!(
            report.accounts[1].outcome,
            UsageOutcome::Unavailable(_)
        ));
        assert!(matches!(
            report.accounts[2].outcome,
            UsageOutcome::Unavailable(_)
        ));
        assert_clean("the report JSON", &serde_json::to_string(&report)?);
        assert_clean("the report Debug", &format!("{report:?}"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn errors_and_the_dependency_bundle_hold_no_secret() -> TestResult {
    guard(async {
        let harness = UsageHarness::new().await?;
        harness.put("/home/ok/.codex/auth.json", AUTH).await?;
        harness.fs.fail_at(
            FsOp::WriteNew,
            1,
            FsError::Io {
                path: host_path("/state/usage/usage.json"),
                kind: ErrorKind::PermissionDenied,
                message: "denied".to_owned(),
            },
        );
        harness.server.serve(
            Provider::Codex,
            Method::Get,
            CODEX_PATH,
            Route::json(200, &codex_body(10)),
        );
        let failure = harness
            .report(
                &[codex_login("echo-ok", "/home/ok/.codex", "ok-id")],
                Refresh::Cached,
            )
            .await
            .expect_err("the cache write fails");
        for error in [
            failure,
            UsageError::Cancelled,
            UsageError::InvalidPercent(101.0),
        ] {
            assert_clean("a UsageError", &format!("{error} {error:?}"));
        }
        let fs: Arc<dyn FileSystem> = Arc::<MemFs>::clone(&harness.fs);
        let clock: Arc<dyn Clock> = Arc::<FakeClock>::clone(&harness.clock);
        let runner: Arc<dyn ProcessRunner> = Arc::<ScriptedProcessRunner>::clone(&harness.runner);
        let deps = UsageDeps {
            clock,
            fs,
            runner,
            env: EnvSnapshot::from_pairs([("KEY".to_owned(), "SECRET_TOKEN_ENV".to_owned())]),
            endpoints: ProviderEndpoints::real(),
            state_dir: host_path("/state/usage"),
        };
        assert_clean("UsageDeps Debug", &format!("{deps:?}"));
        assert_clean("HostUsage Debug", &format!("{:?}", HostUsage::new(deps)));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_cache_file_holds_no_secret_and_the_credential_files_are_unchanged() -> TestResult {
    guard(async {
        let (harness, _report) = echoing().await?;
        for (path, bytes) in harness.all_files().await? {
            let text = String::from_utf8(bytes)?;
            if path.ends_with("auth.json") {
                assert_eq!(text, AUTH, "{path} changed");
            } else {
                assert_clean(&path, &text);
            }
        }
        Ok(())
    })
    .await
}
