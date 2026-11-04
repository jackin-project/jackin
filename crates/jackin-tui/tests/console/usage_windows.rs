// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_console::domain::account::{IssueCode, Lifecycle, ValidationLevel, ValidationState};
use jackin_console::domain::reason::{Found, IssueReason, RefreshOutcome, UsageReason};
use jackin_console::domain::usage::{Freshness, QuotaStatus, WindowCategory};
use jackin_console::domain::vocabulary::Vocabulary;
use jackin_core::{
    Account, AccountName, ChatgptAccountId, ConfigEdit, CredentialSource, EnvName, OpReference,
    Provider,
};
use jackin_testkit::{
    ACCESS, ACCOUNT_ID, Method, ProcessReply, Route, ScriptedProgram, TestResult, guard,
};

use super::support::{CANARY, Lab};

const PATH: &str = "/backend-api/wham/usage";

fn body(primary: Option<f64>, secondary: f64) -> String {
    let primary = primary.map_or_else(String::new, |used| {
        format!(
            "\"primary_window\":{{\"used_percent\":{used},\"limit_window_seconds\":18000,\"reset_at\":1767232800}},"
        )
    });
    format!(
        "{{\"rate_limit\":{{{primary}\"secondary_window\":{{\"used_percent\":{secondary},\"limit_window_seconds\":604800,\"reset_at\":1767571200}}}}}}"
    )
}

fn route(text: &str) -> Route {
    Route::json(200, text)
        .requires_header("authorization", &format!("Bearer {ACCESS}"))
        .requires_header("chatgpt-account-id", ACCOUNT_ID)
}

async fn serve(lab: &Lab, text: &str) -> TestResult {
    lab.rig
        .world
        .usage
        .get()
        .await?
        .replace(Provider::Codex, Method::Get, PATH, route(text));
    Ok(())
}

async fn refresh(lab: &mut Lab) -> TestResult {
    lab.step("u").await?;
    lab.step("r").await
}

fn windows(lab: &mut Lab, id: &str) -> Vec<(&'static str, Option<u8>, WindowCategory)> {
    lab.world()
        .accounts
        .get(id)
        .map(|account| {
            account
                .usage
                .windows
                .iter()
                .map(|w| (w.id, w.used_pct, w.category))
                .collect()
        })
        .unwrap_or_default()
}

async fn rejected_and_unsupported(lab: &Lab) -> TestResult {
    let other = Account::new(
        AccountName::parse("spare")?,
        Provider::Codex,
        CredentialSource::CodexLogin {
            home: lab.rig.home.clone(),
            chatgpt_account_id: ChatgptAccountId::parse("other-account")?,
        },
    )?;
    let amp = Account::new(
        AccountName::parse("tool")?,
        Provider::Amp,
        CredentialSource::Env {
            var: EnvName::parse("AMP_KEY")?,
        },
    )?;
    lab.edit(vec![
        ConfigEdit::AddAccount(other),
        ConfigEdit::AddAccount(amp),
    ])
    .await
}

#[tokio::test]
async fn refresh_maps_windows_rounds_half_away_from_zero_and_takes_a_new_reading() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        serve(&lab, &body(Some(12.5), 0.4)).await?;
        refresh(&mut lab).await?;
        assert_eq!(
            windows(&mut lab, "work"),
            [
                ("five_hour", Some(13), WindowCategory::Session),
                ("weekly", Some(0), WindowCategory::LongRange)
            ]
        );
        serve(&lab, &body(Some(99.6), 12.4)).await?;
        lab.step("r").await?;
        assert_eq!(
            windows(&mut lab, "work"),
            [
                ("five_hour", Some(100), WindowCategory::Session),
                ("weekly", Some(12), WindowCategory::LongRange)
            ]
        );
        let work = lab.world().accounts.get("work").cloned().ok_or("work")?;
        assert_eq!(work.usage.windows[0].status, QuotaStatus::Exhausted);
        assert_eq!(work.usage.windows[0].reset_secs, Some(1_767_232_800));
        assert_eq!(work.usage.freshness.phase, Freshness::Current);
        assert!(work.issue.is_none());
        let screen = lab.console.screen();
        assert!(screen.contains("100%"), "{screen}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_weekly_only_report_has_no_five_hour_row() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        serve(&lab, &body(None, 33.0)).await?;
        refresh(&mut lab).await?;
        assert_eq!(
            windows(&mut lab, "work"),
            [("weekly", Some(33), WindowCategory::LongRange)]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rejected_login_and_an_unsupported_provider_show_their_reasons_beside_a_good_account()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        rejected_and_unsupported(&lab).await?;
        lab.reopen().await?;
        serve(&lab, &body(Some(20.0), 5.0)).await?;
        refresh(&mut lab).await?;
        assert_eq!(windows(&mut lab, "work").len(), 2);
        let spare = lab.world().accounts.get("spare").cloned().ok_or("spare")?;
        assert_eq!(
            spare.issue.as_ref().map(|i| i.code),
            Some(IssueCode::Unauthorized)
        );
        assert_eq!(spare.usage.windows.len(), 1);
        let words = Vocabulary::openshell();
        let expired = IssueReason::Usage(UsageReason::ExpiredLogin);
        assert_eq!(spare.issue.as_ref().map(|i| &i.reason), Some(&expired));
        assert_eq!(
            spare.usage.windows[0].note.as_deref(),
            Some(words.reason(&expired).as_str())
        );
        let tool = lab.world().accounts.get("tool").cloned().ok_or("tool")?;
        assert_eq!(
            tool.issue.as_ref().map(|i| i.code),
            Some(IssueCode::QuotaUnsupported)
        );
        assert_eq!(tool.usage.windows[0].status, QuotaStatus::Unsupported);
        let unsupported = IssueReason::Usage(UsageReason::Unsupported);
        assert_eq!(tool.issue.as_ref().map(|i| &i.reason), Some(&unsupported));
        assert_eq!(
            tool.usage.windows[0].note.as_deref(),
            Some(words.reason(&unsupported).as_str())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_server_error_after_a_good_report_keeps_the_windows_as_stale() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        serve(&lab, &body(Some(40.0), 10.0)).await?;
        refresh(&mut lab).await?;
        lab.rig.world.usage.get().await?.replace(
            Provider::Codex,
            Method::Get,
            PATH,
            Route::status(500),
        );
        lab.step("r").await?;
        let work = lab.world().accounts.get("work").cloned().ok_or("work")?;
        assert_eq!(work.usage.freshness.phase, Freshness::Stale);
        assert_eq!(work.usage.windows.len(), 2);
        assert_eq!(work.usage.windows[0].used_pct, Some(40));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn validate_reports_quota_access_and_the_rejection_of_a_login() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        rejected_and_unsupported(&lab).await?;
        lab.reopen().await?;
        serve(&lab, &body(Some(20.0), 5.0)).await?;
        lab.step("c").await?;
        lab.step("down").await?;
        lab.step("down").await?;
        lab.step("down").await?;
        lab.step("v").await?;
        let spare = lab.world().accounts.get("spare").cloned().ok_or("spare")?;
        assert_eq!(
            spare.validation,
            ValidationState::Invalid(spare.issue.clone().ok_or("issue")?)
        );
        assert_eq!(
            spare.issue.as_ref().map(|i| i.code),
            Some(IssueCode::Unauthorized)
        );
        assert_eq!(spare.lifecycle, Lifecycle::NeedsLogin);
        lab.step("down").await?;
        lab.step("v").await?;
        let work = lab.world().accounts.get("work").cloned().ok_or("work")?;
        assert_eq!(
            work.validation,
            ValidationState::Valid(ValidationLevel::QuotaReadable)
        );
        assert_eq!(work.lifecycle, Lifecycle::Available);
        assert_eq!(work.usage.windows.len(), 2);
        Ok(())
    })
    .await
}

const CLAUDE_PATH: &str = "/api/oauth/usage";
const CLAUDE_BODY: &str = "{\"five_hour\":{\"utilization\":12,\"resets_at\":\"2026-06-25T19:00:00Z\"},\"seven_day\":{\"utilization\":34,\"resets_at\":\"2026-06-26T14:00:00Z\"}}";

#[tokio::test]
async fn usage_reads_a_one_password_value_only_to_send_it_as_the_bearer_header() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let account = Account::new(
            AccountName::parse("router")?,
            Provider::Claude,
            CredentialSource::OnePassword {
                reference: OpReference::parse("op://vault-1/item-1/key")?,
            },
        )?;
        lab.edit(vec![ConfigEdit::AddAccount(account)]).await?;
        lab.reopen().await?;
        lab.rig.world.usage.get().await?.serve(
            Provider::Claude,
            Method::Get,
            CLAUDE_PATH,
            Route::json(200, CLAUDE_BODY)
                .requires_header("authorization", &format!("Bearer {CANARY}")),
        );
        lab.rig.world.runner.scripted().expect(
            ScriptedProgram::Op,
            &["read", "op://vault-1/item-1/key"],
            ProcessReply::exited(0, format!("{CANARY}\n").as_bytes(), b""),
        );
        let mut frames = vec![lab.console.screen()];
        refresh(&mut lab).await?;
        frames.push(lab.console.screen());
        assert_eq!(
            windows(&mut lab, "router"),
            [
                ("five_hour", Some(12), WindowCategory::Session),
                ("weekly", Some(34), WindowCategory::LongRange)
            ]
        );
        let mismatches = lab.rig.world.usage.get().await?.mismatches();
        assert!(
            mismatches.iter().all(|m| !m.contains("claude")),
            "{mismatches:?}"
        );
        let config = lab.console.config_text().await?;
        assert!(!config.contains(CANARY), "{config}");
        for frame in &frames {
            assert!(!frame.contains(CANARY), "{frame}");
        }
        let accounts = format!("{:?}", lab.world().accounts);
        let op = format!("{:?}", lab.world().op);
        let debug = format!("{accounts}{op}");
        assert!(!debug.contains(CANARY), "{debug}");
        for call in lab.rig.world.runner.scripted().seen() {
            assert!(!format!("{call:?}").contains(CANARY), "{call:?}");
        }
        Ok(())
    })
    .await
}

const CLAUDE_TOKEN: &str = "SECRET_TOKEN_CLAUDE";
const CLAUDE_LOGIN: &str = r#"{"claudeAiOauth":{"accessToken":"SECRET_TOKEN_CLAUDE","refreshToken":"SECRET_TOKEN_CLAUDE_REFRESH","expiresAt":4102444800000,"scopes":["user:inference","user:profile"]}}"#;

#[tokio::test]
async fn a_claude_folder_account_saved_from_the_console_reads_usage() -> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        let folder = lab.code.join("claude-home");
        tokio::fs::create_dir_all(&folder).await?;
        tokio::fs::write(folder.join(".credentials.json"), CLAUDE_LOGIN).await?;
        lab.rig.world.usage.get().await?.replace(
            Provider::Claude,
            Method::Get,
            CLAUDE_PATH,
            Route::json(200, CLAUDE_BODY)
                .requires_header("authorization", &format!("Bearer {CLAUDE_TOKEN}"))
                .requires_header("anthropic-beta", "oauth-2025-04-20"),
        );
        lab.folder_form("claudette", 0, "~/code/claude-home")
            .await?;
        lab.save_form().await?;
        let config = lab.config().await?;
        let saved = config
            .accounts()
            .find(|account| account.name().as_str() == "claudette")
            .ok_or("the account was not saved")?;
        assert_eq!(saved.provider(), Provider::Claude);
        refresh(&mut lab).await?;
        assert_eq!(
            windows(&mut lab, "claudette"),
            [
                ("five_hour", Some(12), WindowCategory::Session),
                ("weekly", Some(34), WindowCategory::LongRange)
            ]
        );
        let account = lab
            .world()
            .accounts
            .get("claudette")
            .cloned()
            .ok_or("claudette")?;
        assert!(account.issue.is_none(), "{:?}", account.issue);
        Ok(())
    })
    .await
}

async fn refresh_codex(lab: &mut Lab) -> Result<String, Box<dyn std::error::Error>> {
    lab.step("r").await?;
    Ok(lab.console.screen())
}

#[tokio::test]
async fn a_good_and_a_stale_refresh_name_their_outcome_in_the_vocabulary_words() -> TestResult {
    guard(async {
        let words = Vocabulary::openshell();
        let mut lab = Lab::open().await?;
        serve(&lab, &body(Some(40.0), 10.0)).await?;
        lab.step("c down down").await?;
        let current = refresh_codex(&mut lab).await?;
        let word = words.refresh_outcome(RefreshOutcome::Current);
        assert!(
            current.contains(&format!("Refreshed Codex · work · {word} · 2 windows")),
            "{current}"
        );
        lab.rig.world.usage.get().await?.replace(
            Provider::Codex,
            Method::Get,
            PATH,
            Route::status(500),
        );
        let stale = refresh_codex(&mut lab).await?;
        let word = words.refresh_outcome(RefreshOutcome::Stale);
        assert!(
            stale.contains(&format!("Refreshed Codex · work · {word} · 2 windows")),
            "{stale}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_refresh_that_never_read_usage_is_named_failed_with_the_provider_reason() -> TestResult {
    guard(async {
        let words = Vocabulary::openshell();
        let mut lab = Lab::open().await?;
        lab.rig.world.usage.get().await?.replace(
            Provider::Codex,
            Method::Get,
            PATH,
            Route::status(500),
        );
        lab.step("c down down").await?;
        let failed = refresh_codex(&mut lab).await?;
        let word = words.refresh_outcome(RefreshOutcome::Failed);
        assert!(
            failed.contains(&format!("Refreshed Codex · work · {word} ·")),
            "{failed}"
        );
        let work = lab.world().accounts.get("work").cloned().ok_or("work")?;
        assert!(
            matches!(
                work.issue.as_ref().map(|i| &i.reason),
                Some(IssueReason::Usage(UsageReason::Provider { .. }))
            ),
            "{:?}",
            work.issue
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_rate_limit_reaches_the_issue_as_a_wait_and_the_note_in_the_vocabulary_words()
-> TestResult {
    guard(async {
        let mut lab = Lab::open().await?;
        lab.rig.world.usage.get().await?.replace(
            Provider::Codex,
            Method::Get,
            PATH,
            Route::status(429).reply_header("retry-after", "120"),
        );
        refresh(&mut lab).await?;
        let work = lab.world().accounts.get("work").cloned().ok_or("work")?;
        let reason = IssueReason::Usage(UsageReason::RateLimited {
            retry_in_secs: Some(120),
        });
        assert_eq!(work.issue.as_ref().map(|i| &i.reason), Some(&reason));
        assert_eq!(
            work.usage.windows[0].note.as_deref(),
            Some(Vocabulary::openshell().reason(&reason).as_str())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn validate_words_its_rows_with_the_vocabulary() -> TestResult {
    guard(async {
        let words = Vocabulary::openshell();
        let mut lab = Lab::open().await?;
        rejected_and_unsupported(&lab).await?;
        lab.reopen().await?;
        serve(&lab, &body(Some(20.0), 5.0)).await?;
        lab.step("c down down down v").await?;
        let rejected = lab.console.screen();
        let expired = words.check(&IssueReason::Usage(UsageReason::ExpiredLogin));
        let start: String = expired.chars().take(13).collect();
        assert!(
            rejected.contains(&format!("quota access · {start}")),
            "{rejected}"
        );
        lab.step("down v").await?;
        let readable = lab.console.screen();
        let quota = words.found(&Found::Level(ValidationLevel::QuotaReadable));
        assert!(
            readable.contains(&format!("✓ quota access · {quota}")),
            "{readable}"
        );
        Ok(())
    })
    .await
}
