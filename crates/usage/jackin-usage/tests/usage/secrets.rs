// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::ErrorKind;
use std::sync::Arc;

use jackin_core::{Clock, EnvSnapshot, FileSystem, FsError, ProcessRunner, Provider};
use jackin_testkit::{
    FakeClock, FsOp, MemFs, Method, ProcessReply, Route, ScriptedProcessRunner, ScriptedProgram,
};
use jackin_usage::{
    HostUsage, ProviderEndpoints, Refresh, UnavailableReason, UsageDeps, UsageError, UsageOutcome,
    UsageReport,
};

use crate::providers::openrouter_serve;
use crate::support::{
    CODEX_PATH, TestResult, UsageHarness, account, codex_body, codex_login, env_source,
    file_source, guard, host_path, op_source, windows_of,
};

const AUTH: &str = include_str!("fixtures/codex/auth_chatgpt.json");
const SECRETS: [&str; 6] = [
    "SECRET_TOKEN_CODEX",
    "SECRET_TEXT_BODY",
    "SECRET_TOKEN_ENV",
    "SECRET_TOKEN_FILE",
    "SECRET_TOKEN_OP",
    "SECRET_TOKEN_OP-LEAK",
];

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
    harness.put("/keys/router", "SECRET_TOKEN_FILE\n").await?;
    harness.runner.expect(
        ScriptedProgram::Op,
        &["read", "op://v/i/f"],
        ProcessReply::exited(1, b"", b"[ERROR] item \"SECRET_TOKEN_OP-LEAK\" not found"),
    );
    openrouter_serve(&harness, "SECRET_TOKEN_ENV");
    let router = |name: &str, source| account(name, Provider::Openrouter, source);
    let accounts = [
        codex_login("echo-ok", "/home/ok/.codex", "ok-id"),
        codex_login("echo-err", "/home/err/.codex", "err-id"),
        codex_login("echo-junk", "/home/junk/.codex", "junk-id"),
        router("env-router", env_source("OPENROUTER_TEST_KEY")),
        router("op-router", op_source("op://v/i/f")),
        router("file-router", file_source("/keys/router")),
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
        assert_eq!(windows_of(&report.accounts[3].outcome).len(), 2);
        assert_eq!(
            report.accounts[4].outcome,
            UsageOutcome::Unavailable(UnavailableReason::NoCredentials)
        );
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
            match path.as_str() {
                "/state/usage/usage.json" => assert_clean(&path, &text),
                "/state/usage/usage.json.lock" => assert_eq!(text, "", "{path} holds text"),
                "/keys/router" => assert_eq!(text, "SECRET_TOKEN_FILE\n", "{path} changed"),
                _ => assert_eq!(text, AUTH, "{path} changed"),
            }
        }
        Ok(())
    })
    .await
}

async fn surfaces_of(
    case: &crate::providers::Case,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let echo = format!("{} SECRET_TEXT_BODY", case.token);
    let accounts = std::slice::from_ref(&case.account);
    let mut surfaces = Vec::new();
    let valid = case.harness.report(accounts, Refresh::Force).await?;
    surfaces.push(serde_json::to_string(&valid)?);
    surfaces.push(format!("{valid:?}"));
    let echoes = [
        Route::json(500, &format!("{{\"error\":\"{echo}\"}}")),
        Route::json(200, &format!("not json {echo}")),
        Route::json(200, &format!("{{\"note\":\"{echo}\"}}")),
    ];
    for route in echoes {
        case.harness
            .server
            .replace(case.provider, case.first.0, case.first.1, route);
        let report = case.harness.report(accounts, Refresh::Force).await?;
        surfaces.push(serde_json::to_string(&report)?);
        surfaces.push(format!("{report:?}"));
    }
    surfaces.extend(case.harness.cache_text());
    Ok(surfaces)
}

#[tokio::test]
async fn every_provider_keeps_its_token_and_the_response_text_out_of_the_report_and_the_cache()
-> TestResult {
    guard(async {
        for case in crate::providers::cases().await? {
            let surfaces = surfaces_of(&case).await?;
            let leaked = surfaces.iter().find(|surface| {
                surface.contains(case.token) || surface.contains("SECRET_TEXT_BODY")
            });
            assert!(
                leaked.is_none(),
                "{:?} leaked a secret into {leaked:?}",
                case.provider
            );
        }
        Ok(())
    })
    .await
}
