// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin::ExitClass;
use jackin_core::{Clock, Provider, Secret, SecretBytes};
use jackin_testkit::{Method, Route, TestResult, UsageServer, World, guard};
use jackin_usage::accounts::{
    AccountRegistry, CandidateIdentity, ConnectionError, CredentialVault, UsageProvider,
};
use jackin_usage::onboarding::{LoginPrompt, OwnedLogin, receive_antigravity_statusline};
use jackin_usage::view::text;
use serde_json::{Value, json};
use uuid::Uuid;

use super::support::{Failure, Harness, Ran, ScriptedTerminal};

#[derive(Debug, Default)]
struct Memory {
    values: Mutex<HashMap<Uuid, Vec<u8>>>,
}

impl CredentialVault for Memory {
    fn store(&self, id: Uuid, secret: &SecretBytes) -> Result<(), ConnectionError> {
        self.values
            .lock()
            .map_err(|_| ConnectionError::Vault)?
            .insert(id, secret.expose().to_vec());
        Ok(())
    }
    fn load(&self, id: Uuid) -> Result<SecretBytes, ConnectionError> {
        self.values
            .lock()
            .map_err(|_| ConnectionError::Vault)?
            .get(&id)
            .cloned()
            .map(SecretBytes::new)
            .ok_or(ConnectionError::MissingCredential)
    }
    fn delete(&self, id: Uuid) -> Result<(), ConnectionError> {
        self.values
            .lock()
            .map_err(|_| ConnectionError::Vault)?
            .remove(&id);
        Ok(())
    }
}

const READING: &str = r#"{"groups":[{"displayName":"Gemini","buckets":[
        {"bucketId":"gemini-5h","remainingFraction":0.99965,"resetTime":"2027-03-02T14:00:00Z"},
        {"bucketId":"gemini-weekly","remainingFraction":0.9742,"resetTime":"2027-03-08T09:00:00Z"},
        {"bucketId":"3p-5h","remainingFraction":0.5,"resetTime":"2027-03-02T15:00:00Z"},
        {"bucketId":"3p-weekly","remainingFraction":0.4,"resetTime":"2027-03-08T09:00:00Z"}
    ]},{"displayName":"Gemini 4 Argon","buckets":[
        {"bucketId":"gemini-argon-wtu-weekly","window":"weekly","remainingFraction":0.8,"resetTime":"2027-03-09T00:00:00Z"}
    ]}]}"#;

const DENIED: &str = r#"{"error":{"code":403,"status":"PERMISSION_DENIED","details":[{"reason":"SERVICE_DISABLED"}]}}"#;

const LOAD: &str =
    r#"{"paidTier":{"id":"g1-ultra-tier","name":"ignored"},"cloudaicompanionProject":"proj-1"}"#;

const BUCKETS: [(&str, f64, i64); 5] = [
    ("gemini-5h", 0.99965, 1_803_996_000),
    ("gemini-weekly", 0.9742, 1_804_496_400),
    ("gemini-argon-wtu-weekly", 0.8, 1_804_550_400),
    ("3p-5h", 0.5, 1_803_999_600),
    ("3p-weekly", 0.4, 1_804_496_400),
];

const QUOTA: &str = "/daily/v1internal:retrieveUserQuotaSummary";
const QUOTA_FALLBACK: &str = "/production/v1internal:retrieveUserQuotaSummary";

struct Lab {
    world: World,
    server: UsageServer,
    cli: Harness,
    vault: Arc<Memory>,
    path: PathBuf,
    id: Uuid,
}

impl Lab {
    async fn open() -> Result<Lab, Failure> {
        Lab::open_as("Personal").await
    }

    async fn open_as(label: &str) -> Result<Lab, Failure> {
        let world = World::builder().build()?;
        let server = UsageServer::start().await?;
        let mut cli = Harness::new(&world);
        cli.usage_endpoints = server.endpoints();
        let vault = Arc::new(Memory::default());
        let shared = Arc::clone(&vault);
        let shared: Arc<dyn CredentialVault> = shared;
        cli.usage_vault = Some(shared);
        let path = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config directory")?
            .join("usage/accounts.sqlite3");
        let now = world.clock.now().0;
        let stored = Arc::clone(&vault);
        let stored: Arc<dyn CredentialVault> = stored;
        let mut db = AccountRegistry::with_vault(&path, stored)?;
        let id = db.add_with_snapshot(
            UsageProvider::Antigravity,
            label,
            &CandidateIdentity::OAuth(Some("ultra@example.com".into())),
            &SecretBytes::new(serde_json::to_vec(&json!({
                "product": "antigravity",
                "client_id": "jackin-fixture-desktop.apps.googleusercontent.com",
                "client_secret": "fixture-not-a-secret",
                "access": "old-access",
                "refresh": "refresh-1",
                "expires_at": now + 10_000
            }))?),
            now - 60,
            &json!({"plan":"stale","resources":{"gemini-5h":{"kind":"quota","unit":"percent","remaining_fraction":0.1,"window_seconds":18000}}}),
        )?;
        drop(db);
        server.serve(
            Provider::Antigravity,
            Method::Post,
            "/production/v1internal:loadCodeAssist",
            Route::json(200, LOAD),
        );
        let lab = Lab {
            world,
            server,
            cli,
            vault,
            path,
            id,
        };
        lab.serve_quota(READING);
        Ok(lab)
    }

    fn serve_quota(&self, body: &str) {
        self.server.replace(
            Provider::Antigravity,
            Method::Post,
            QUOTA,
            Route::json(200, body).requires_json_body(json!({"project": "proj-1"})),
        );
    }

    fn fail_quota(&self, route: fn() -> Route) {
        for path in [QUOTA, QUOTA_FALLBACK] {
            self.server
                .replace(Provider::Antigravity, Method::Post, path, route());
        }
    }

    async fn refresh(&self) -> Result<Ran, Failure> {
        self.cli
            .run(&[
                "usage",
                "refresh",
                "--json",
                "--account",
                &self.id.to_string(),
            ])
            .await
    }

    async fn succeeded_then_failed(&self) -> Result<(Ran, Ran), Failure> {
        let first = self.refresh().await?;
        first.ok()?;
        self.world.clock.advance(Duration::from_secs(120));
        self.fail_quota(|| Route::status(500));
        let failed = self.refresh().await?;
        Ok((first, failed))
    }
}

#[tokio::test]
async fn cli_refresh_prints_the_google_plan_and_every_quota_bucket_with_its_reset() -> TestResult {
    guard(async {
        let lab = Lab::open().await?;
        let ran = lab.refresh().await?;
        ran.ok()?;
        let body = ran.value()?;
        let account = &body["accounts"][0];
        assert_eq!(account["account"]["id"], lab.id.to_string());
        assert_eq!(account["usage"]["plan"], "Google AI Ultra");
        let resources = &account["usage"]["resources"];
        assert_eq!(resources.as_object().map(serde_json::Map::len), Some(5));
        for (key, fraction, reset) in BUCKETS {
            assert_eq!(resources[key]["remaining_fraction"], fraction, "{key}");
            assert_eq!(resources[key]["resets_at"], reset, "{key}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_later_launch_replaces_the_saved_reading_with_the_new_quota() -> TestResult {
    guard(async {
        let lab = Lab::open().await?;
        let first = lab.refresh().await?;
        first.ok()?;
        assert_eq!(
            first.value()?["accounts"][0]["usage"]["resources"]["gemini-5h"]["remaining_fraction"],
            0.99965
        );
        lab.serve_quota(&READING.replace("0.99965", "0.25"));
        let second = lab.refresh().await?;
        second.ok()?;
        assert_eq!(
            second.value()?["accounts"][0]["usage"]["resources"]["gemini-5h"]["remaining_fraction"],
            0.25
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn details_lists_claude_5h_before_argon_weekly_before_gemini_weekly() -> TestResult {
    guard(async {
        let lab = Lab::open().await?;
        lab.refresh().await?.ok()?;
        let details = lab
            .cli
            .run(&["usage", "details", "--account", &lab.id.to_string()])
            .await?;
        details.ok()?;
        assert!(!details.stdout.contains('\x1b'));
        let at = |label: &str| {
            details
                .stdout
                .find(label)
                .ok_or_else(|| format!("missing {label}\n{}", details.stdout))
        };
        at("Claude weekly")?;
        assert!(at("Claude 5h")? < at("Argon weekly")?);
        assert!(at("Argon weekly")? < at("Gemini weekly")?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_console_board_lists_every_bucket_of_the_account_the_cli_refreshed() -> TestResult {
    guard(async {
        let lab = Lab::open().await?;
        lab.refresh().await?.ok()?;
        let clock = Arc::<jackin_testkit::FakeClock>::clone(&lab.world.clock);
        let clock: Arc<dyn Clock> = clock;
        let vault = Arc::<Memory>::clone(&lab.vault);
        let vault: Arc<dyn CredentialVault> = vault;
        let login =
            OwnedLogin::new(lab.path.clone(), lab.server.endpoints(), clock).with_vault(vault);
        let board = jackin_tui::reload_usage_board(&login).await?;
        let titles: Vec<&str> = board
            .iter()
            .map(|(_, title, _, _)| title.as_str())
            .collect();
        for label in [
            "Gemini weekly",
            "Argon weekly",
            "Claude 5h",
            "Claude weekly",
        ] {
            assert!(
                titles.iter().any(|title| title.contains(label)),
                "{label} in {titles:?}"
            );
        }
        let id = lab.id.to_string();
        assert!(
            board
                .iter()
                .any(|(account, _, _, _)| account.as_deref() == Some(id.as_str())),
            "{board:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_refresh_keeps_the_last_reading_and_its_fetch_time() -> TestResult {
    guard(async {
        let lab = Lab::open().await?;
        let (first, failed) = lab.succeeded_then_failed().await?;
        let fetched = first.value()?["accounts"][0]["fetched_at"].clone();
        assert!(fetched.is_i64(), "{fetched}");
        let body = failed.value()?;
        let account = &body["accounts"][0];
        assert_eq!(account["fetched_at"], fetched);
        assert_eq!(
            account["usage"]["resources"]["gemini-5h"]["remaining_fraction"],
            0.99965
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_refresh_exits_with_failure_and_says_the_quota_is_unavailable() -> TestResult {
    guard(async {
        let lab = Lab::open().await?;
        let (_, failed) = lab.succeeded_then_failed().await?;
        assert_eq!(failed.exit, ExitClass::Failure, "{}", failed.stderr);
        let body = failed.value()?;
        let account = &body["accounts"][0];
        let message = account["failure"]["message"]
            .as_str()
            .ok_or("missing refresh error")?;
        assert!(message.contains("unavailable"), "{message}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_refresh_prints_no_credential_material() -> TestResult {
    guard(async {
        let lab = Lab::open().await?;
        let (_, failed) = lab.succeeded_then_failed().await?;
        assert!(
            failed.stdout.contains(&lab.id.to_string()),
            "{}",
            failed.stdout
        );
        for secret in ["fixture-not-a-secret", "old-access", "refresh-1"] {
            assert!(!failed.stdout.contains(secret), "{secret}");
            assert!(!failed.stderr.contains(secret), "{secret}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_disabled_quota_api_is_a_limitation_that_waits_an_hour_and_never_escalates() -> TestResult
{
    guard(async {
        let lab = Lab::open().await?;
        lab.fail_quota(|| Route::json(403, DENIED));
        let command = format!("jackin account usage reconnect {}", lab.id);
        for round in 0..2 {
            let refreshed = lab
                .cli
                .run(&["usage", "refresh", "--account", &lab.id.to_string()])
                .await?;
            assert_eq!(
                refreshed.exit,
                ExitClass::Failure,
                "{round}: {}",
                refreshed.stderr
            );
            let usage = lab.cli.run(&["usage"]).await?;
            assert!(
                usage
                    .stdout
                    .contains("HTTP 403 SERVICE_DISABLED · next retry in 1h"),
                "{round}: {}",
                usage.stdout
            );
            assert!(
                !usage.stdout.contains(&command),
                "{round}: {}",
                usage.stdout
            );
            assert!(
                !usage.stdout.contains("retry due"),
                "{round}: {}",
                usage.stdout
            );
            lab.world.clock.advance(Duration::from_hours(2));
        }
        Ok(())
    })
    .await
}

async fn statusline_account(
    world: &World,
    cli: &Harness,
    path: &std::path::Path,
) -> Result<(Uuid, i64), Box<dyn std::error::Error>> {
    let clock = Arc::clone(&world.clock);
    let clock: Arc<dyn Clock> = clock;
    let service = OwnedLogin::new(path.to_path_buf(), cli.usage_endpoints.clone(), clock);
    let cancel = tokio_util::sync::CancellationToken::new();
    let (send, mut prompts) = tokio::sync::mpsc::channel(2);
    let pending = tokio::spawn(async move {
        service
            .connect_antigravity_statusline(None, send, &cancel)
            .await
    });
    let LoginPrompt::Field { reply, .. } = prompts.recv().await.ok_or("missing email prompt")?
    else {
        return Err("expected an email prompt".into());
    };
    reply
        .send(Secret::new("cached@example.com".into()))
        .map_err(|_| "prompt closed")?;
    let LoginPrompt::AntigravityStatusLine { .. } =
        prompts.recv().await.ok_or("missing status line")?
    else {
        return Err("expected the status-line command".into());
    };
    let observed = world.clock.now().0;
    let input = json!({"product":"antigravity","email":"cached@example.com","plan_tier":"ultra","quota":{"3p-weekly":{"remaining_fraction":0.05}}});
    assert!(receive_antigravity_statusline(
        path,
        &serde_json::to_vec(&input)?,
        observed
    )?);
    let id = pending.await?.map_err(|error| error.to_string())?;
    Ok((id, observed))
}

#[tokio::test]
async fn cli_refresh_of_a_statusline_snapshot_is_not_a_network_success() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let cli = Harness::new(&world);
        let root = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config directory")?;
        let path = root.join("usage/accounts.sqlite3");
        let (id, observed) = statusline_account(&world, &cli, &path).await?;
        let args = ["usage", "refresh", "--json", "--account", &id.to_string()];
        let ran = cli.run(&args).await?;
        assert_eq!(ran.exit, ExitClass::Ok);
        assert!(!ran.stderr.contains("last report"));
        assert!(!ran.stdout.contains("last report"));
        let body: Value = serde_json::from_str(&ran.stdout)?;
        let account = &body["accounts"][0];
        assert_eq!(account["fetched_at"], observed);
        assert_eq!(
            account["usage"]["resources"]["3p-weekly"]["remaining_fraction"],
            0.05
        );
        let message = account["failure"]["message"]
            .as_str()
            .ok_or("missing refresh error")?;
        assert!(message.contains("status-line hook"), "{message}");
        assert!(ran.stderr.contains("status-line hook"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn refresh_prints_a_summary_by_default_and_json_only_with_the_flag() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let cli = Harness::new(&world);
        let path = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config directory")?
            .join("usage/accounts.sqlite3");
        let (id, _) = statusline_account(&world, &cli, &path).await?;
        let account = id.to_string();
        let human = cli
            .run(&["usage", "refresh", "--account", &account])
            .await?;
        assert_eq!(human.exit, ExitClass::Ok);
        assert!(
            serde_json::from_str::<Value>(&human.stdout).is_err(),
            "{}",
            human.stdout
        );
        assert!(
            human
                .stdout
                .starts_with("Refreshed 0 of 1 accounts · 1 skipped\n"),
            "{}",
            human.stdout
        );
        assert!(!human.stdout.contains("failed"), "{}", human.stdout);
        assert!(
            human.stdout.contains(
                "  skipped Antigravity (Google AI) · cached@example.com:\n          updated by the Antigravity status-line hook\n"
            ),
            "{}",
            human.stdout
        );
        assert!(
            human.stdout.contains("cached@example.com"),
            "{}",
            human.stdout
        );
        assert!(
            human.stdout.contains("status-line hook"),
            "{}",
            human.stdout
        );
        assert!(
            human.stdout.contains("jackin usage details"),
            "{}",
            human.stdout
        );
        assert!(human.stderr.is_empty(), "{}", human.stderr);
        let json = cli
            .run(&["usage", "refresh", "--json", "--account", &account])
            .await?;
        let body: Value = serde_json::from_str(&json.stdout)?;
        assert_eq!(body["accounts"][0]["account"]["id"], account);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_real_failure_beside_a_skipped_status_line_account_still_fails_the_refresh() -> TestResult
{
    guard(async {
        let world = World::builder().build()?;
        let mut cli = Harness::new(&world);
        let empty: Arc<dyn CredentialVault> = Arc::new(Memory::default());
        cli.usage_vault = Some(empty);
        let path = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config directory")?
            .join("usage/accounts.sqlite3");
        statusline_account(&world, &cli, &path).await?;
        let stored: Arc<dyn CredentialVault> = Arc::new(Memory::default());
        let mut db = AccountRegistry::with_vault(&path, stored)?;
        db.add_with_snapshot(
            UsageProvider::Antigravity,
            "Personal",
            &CandidateIdentity::OAuth(Some("ultra@example.com".into())),
            &SecretBytes::new(vec![1]),
            world.clock.now().0,
            &json!({"plan":"Ultra","resources":{}}),
        )?;
        drop(db);
        let human = cli.run(&["usage", "refresh"]).await?;
        assert_ne!(human.exit, ExitClass::Ok);
        assert!(
            human
                .stdout
                .starts_with("Refreshed 0 of 2 accounts · 1 skipped · 1 failed\n"),
            "{}",
            human.stdout
        );
        assert!(
            human.stdout.contains("failed  Antigravity"),
            "{}",
            human.stdout
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_status_line_reading_that_went_stale_says_which_hook_feeds_it() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let cli = Harness::new(&world);
        let path = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config directory")?
            .join("usage/accounts.sqlite3");
        statusline_account(&world, &cli, &path).await?;
        world.clock.advance(Duration::from_hours(25));
        let usage = cli.run(&["usage"]).await?;
        assert!(
            usage
                .stdout
                .contains("updated by the Antigravity status-line hook · last update 1d 1h ago"),
            "{}",
            usage.stdout
        );
        let listed = cli.run(&["account", "usage", "ls"]).await?;
        let row = listed
            .stdout
            .lines()
            .find(|line| line.contains("cached@example.com"))
            .ok_or("missing row")?;
        assert!(
            row.contains("stale 1d 1h") && row.contains("status-line"),
            "{row}"
        );
        assert!(
            listed.stdout.contains("status-line hook runs"),
            "{}",
            listed.stdout
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_missing_credential_failure_names_the_sign_in_command_and_no_retry() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let mut cli = Harness::new(&world);
        let empty: Arc<dyn CredentialVault> = Arc::new(Memory::default());
        cli.usage_vault = Some(empty);
        let path = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config directory")?
            .join("usage/accounts.sqlite3");
        let stored: Arc<dyn CredentialVault> = Arc::new(Memory::default());
        let mut db = AccountRegistry::with_vault(&path, stored)?;
        let id = db.add_with_snapshot(
            UsageProvider::Antigravity,
            "Personal",
            &CandidateIdentity::OAuth(Some("ultra@example.com".into())),
            &SecretBytes::new(vec![1]),
            world.clock.now().0,
            &json!({"plan":"Ultra","resources":{}}),
        )?;
        drop(db);
        let refreshed = cli
            .run(&["usage", "refresh", "--account", &id.to_string()])
            .await?;
        assert!(
            refreshed
                .stdout
                .contains("the credential is missing; sign in again"),
            "{}",
            refreshed.stdout
        );
        let command = format!("jackin account usage reconnect {id}");
        assert!(refreshed.stdout.contains(&command), "{}", refreshed.stdout);
        let usage = cli.run(&["usage"]).await?;
        assert!(usage.stdout.contains(&command), "{}", usage.stdout);
        assert!(!usage.stdout.contains("retry"), "{}", usage.stdout);
        let listed = cli.run(&["account", "usage", "ls"]).await?;
        assert!(
            !listed.stdout.contains(&id.to_string()),
            "{}",
            listed.stdout
        );
        assert!(
            listed
                .stdout
                .contains("jackin account usage reconnect <id>"),
            "{}",
            listed.stdout
        );
        Ok(())
    })
    .await
}

async fn mixed_refresh_lab(world: &World) -> Result<(Harness, Uuid), Failure> {
    let mut cli = Harness::new(world);
    cli.env.push(("NO_COLOR".to_owned(), "1".to_owned()));
    let empty: Arc<dyn CredentialVault> = Arc::new(Memory::default());
    cli.usage_vault = Some(empty);
    let path = cli
        .config_path
        .as_path()
        .parent()
        .ok_or("config directory")?
        .join("usage/accounts.sqlite3");
    statusline_account(world, &cli, &path).await?;
    let stored: Arc<dyn CredentialVault> = Arc::new(Memory::default());
    let mut db = AccountRegistry::with_vault(&path, stored)?;
    let id = db.add_with_snapshot(
        UsageProvider::Antigravity,
        "Personal",
        &CandidateIdentity::OAuth(Some("ultra.quota.owner@example.com".into())),
        &SecretBytes::new(vec![1]),
        world.clock.now().0,
        &json!({"plan":"Ultra","resources":{}}),
    )?;
    drop(db);
    Ok((cli, id))
}

fn is_command_line(line: &str) -> bool {
    line.trim_start().starts_with("jackin ")
}

fn assert_summary_fits(stdout: &str, width: usize, id: Uuid) {
    assert!(!stdout.contains('\u{1b}'), "{stdout:?}");
    let wide: Vec<String> = stdout
        .lines()
        .filter(|line| !is_command_line(line))
        .filter(|line| text::width(line) > width)
        .map(|line| format!("{:>3}  {line}", text::width(line)))
        .collect();
    assert!(
        wide.is_empty(),
        "lines wider than {width} columns:\n{}",
        wide.join("\n")
    );
    for tag in ["  failed  ", "  skipped "] {
        let start = stdout
            .lines()
            .position(|line| line.starts_with(tag))
            .unwrap_or(usize::MAX);
        assert_ne!(start, usize::MAX, "{stdout}");
        let held = stdout
            .lines()
            .skip(start + 1)
            .take_while(|line| line.starts_with(&" ".repeat(10)))
            .count();
        assert!(held > 0, "{tag:?} has no hanging continuation:\n{stdout}");
    }
    assert!(stdout.contains(&id.to_string()), "{stdout}");
    assert!(stdout.contains("sign in again"), "{stdout}");
    assert!(stdout.contains("status-line hook"), "{stdout}");
}

#[tokio::test]
async fn the_refresh_summary_wraps_to_80_columns_on_a_pipe() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (cli, id) = mixed_refresh_lab(&world).await?;
        let human = cli.run(&["usage", "refresh"]).await?;
        assert_ne!(human.exit, ExitClass::Ok);
        assert_summary_fits(&human.stdout, 80, id);
        let command = format!("jackin account usage reconnect {id}");
        assert!(
            human.stdout.lines().any(|line| line.trim() == command),
            "{}",
            human.stdout
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_refresh_summary_wraps_to_60_columns_on_a_terminal() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (cli, id) = mixed_refresh_lab(&world).await?;
        let terminal = ScriptedTerminal::new(Vec::new())?.0.sized(60)?;
        let human = cli
            .command(&["usage", "refresh"])
            .terminal(terminal)
            .run()
            .await?;
        assert_ne!(human.exit, ExitClass::Ok);
        assert_summary_fits(&human.stdout, 60, id);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_long_ok_row_wraps_inside_60_columns_with_a_hanging_indent() -> TestResult {
    guard(async {
        let label = "a very long account label that has to wrap on the ok row";
        let mut lab = Lab::open_as(label).await?;
        lab.cli.env.push(("NO_COLOR".to_owned(), "1".to_owned()));
        let terminal = ScriptedTerminal::new(Vec::new())?.0.sized(60)?;
        let human = lab
            .cli
            .command(&["usage", "refresh", "--account", &lab.id.to_string()])
            .terminal(terminal)
            .run()
            .await?;
        assert_eq!(human.exit, ExitClass::Ok, "{}", human.stdout);
        let lines: Vec<&str> = human.stdout.lines().collect();
        let start = lines
            .iter()
            .position(|line| line.starts_with("  ok      "))
            .ok_or_else(|| format!("no ok row:\n{}", human.stdout))?;
        let held = lines[start + 1..]
            .iter()
            .take_while(|line| line.starts_with(&" ".repeat(10)))
            .count();
        assert!(
            held > 0,
            "the ok row has no hanging continuation:\n{}",
            human.stdout
        );
        let wide: Vec<&&str> = lines.iter().filter(|line| text::width(line) > 60).collect();
        assert!(wide.is_empty(), "lines wider than 60 columns: {wide:?}");
        Ok(())
    })
    .await
}

fn assert_every_line_fits(label: &str, output: &str, width: usize) {
    let wide: Vec<String> = output
        .lines()
        .filter(|line| text::width(line) > width)
        .map(|line| format!("{:>3}  {line}", text::width(line)))
        .collect();
    assert!(
        wide.is_empty(),
        "{label} has lines wider than {width} columns:\n{}",
        wide.join("\n")
    );
}

#[tokio::test]
async fn the_final_error_line_of_a_failed_refresh_fits_80_piped_and_60_on_a_terminal() -> TestResult
{
    guard(async {
        let mut lab = Lab::open().await?;
        lab.cli.env.push(("NO_COLOR".to_owned(), "1".to_owned()));
        lab.fail_quota(|| Route::status(500));
        let account = lab.id.to_string();
        let args = ["usage", "refresh", "--account", account.as_str()];
        let piped = lab.cli.run(&args).await?;
        assert_ne!(piped.exit, ExitClass::Ok);
        assert_every_line_fits("piped stdout", &piped.stdout, 80);
        assert_every_line_fits("piped stderr", &piped.stderr, 80);
        assert!(
            piped
                .stderr
                .lines()
                .any(|line| line == "error: refresh failed for 1 account"),
            "{}",
            piped.stderr
        );

        let terminal = ScriptedTerminal::new(Vec::new())?.0.sized(60)?;
        let tty = lab.cli.command(&args).terminal(terminal).run().await?;
        assert_ne!(tty.exit, ExitClass::Ok);
        assert_every_line_fits("terminal stdout", &tty.stdout, 60);
        assert_every_line_fits("terminal stderr", &tty.stderr, 60);

        let json = lab
            .cli
            .run(&["usage", "refresh", "--json", "--account", &account])
            .await?;
        assert!(
            json.stderr
                .contains("Antigravity usage is temporarily unavailable"),
            "{}",
            json.stderr
        );
        Ok(())
    })
    .await
}

async fn failing_hub_lab(world: &World) -> Result<(Harness, UsageServer), Failure> {
    use jackin_usage::onboarding::OwnedLogin as Login;
    use tokio_util::sync::CancellationToken;
    let server = UsageServer::start().await?;
    let mut cli = Harness::new(world);
    cli.usage_endpoints = server.endpoints();
    cli.env.push(("NO_COLOR".to_owned(), "1".to_owned()));
    let vault: Arc<dyn CredentialVault> = Arc::new(Memory::default());
    cli.usage_vault = Some(Arc::clone(&vault));
    let path = cli
        .config_path
        .as_path()
        .parent()
        .ok_or("config directory")?
        .join("usage/accounts.sqlite3");
    let listing = "/v0/management/auth-files";
    server.serve(
        Provider::Claude,
        Method::Get,
        listing,
        Route::json(200, r#"{"files":[]}"#).requires_header("authorization", "Bearer fixture"),
    );
    let clock = Arc::clone(&world.clock);
    let clock: Arc<dyn Clock> = clock;
    Login::new(path.clone(), cli.usage_endpoints.clone(), clock)
        .with_vault(vault)
        .connect_hub(
            &format!("{}/management.html", server.provider_base(Provider::Claude)),
            &Secret::new("fixture".into()),
            None,
            &CancellationToken::new(),
        )
        .await?;
    statusline_account(world, &cli, &path).await?;
    server.replace(Provider::Claude, Method::Get, listing, Route::status(500));
    Ok((cli, server))
}

#[tokio::test]
async fn a_failed_hub_sync_note_and_error_fit_80_piped_and_60_on_a_terminal() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (cli, _server) = failing_hub_lab(&world).await?;
        let piped = cli.run(&["usage", "refresh"]).await?;
        assert_ne!(piped.exit, ExitClass::Ok, "{}", piped.stderr);
        assert!(!piped.stderr.is_empty());
        assert_every_line_fits("piped stderr", &piped.stderr, 80);
        assert_every_line_fits("piped stdout", &piped.stdout, 80);
        assert!(
            piped
                .stderr
                .lines()
                .any(|line| line == "error: refresh failed for 1 hub"),
            "{}",
            piped.stderr
        );

        let terminal = ScriptedTerminal::new(Vec::new())?.0.sized(60)?;
        let tty = cli
            .command(&["usage", "refresh"])
            .terminal(terminal)
            .run()
            .await?;
        assert_ne!(tty.exit, ExitClass::Ok);
        assert_every_line_fits("terminal stderr", &tty.stderr, 60);
        assert_every_line_fits("terminal stdout", &tty.stdout, 60);
        Ok(())
    })
    .await
}
