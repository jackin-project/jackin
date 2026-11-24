// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin::ExitClass;
use jackin_core::{Clock, Provider, Secret, SecretBytes};
use jackin_testkit::{Method, Route, TestResult, UsageServer, World, guard};
use jackin_usage::accounts::{
    AccountRegistry, CandidateIdentity, ConnectionError, CredentialVault, UsageProvider,
};
use jackin_usage::onboarding::{LoginPrompt, OwnedLogin, receive_antigravity_statusline};
use serde_json::{Value, json};
use uuid::Uuid;

use super::support::Harness;

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

fn quota_requests() -> &'static AtomicUsize {
    static REQUESTS: AtomicUsize = AtomicUsize::new(0);
    &REQUESTS
}

fn counted_reading(_: &[u8]) -> Value {
    quota_requests().fetch_add(1, Ordering::SeqCst);
    serde_json::from_str(&reading()).unwrap_or(Value::Null)
}

fn record_shared_board(
    id: Uuid,
    account: &Value,
    resources: &serde_json::Map<String, Value>,
    board: &[(Option<String>, String, String, Option<String>)],
) -> TestResult {
    for label in [
        "Gemini weekly",
        "Argon weekly",
        "Claude 5h",
        "Claude weekly",
    ] {
        if !board.iter().any(|(_, title, _, _)| title.contains(label)) {
            return Err(format!("{board:?}").into());
        }
    }
    if !board
        .iter()
        .any(|(account_id, _, _, _)| account_id.as_deref() == Some(id.to_string().as_str()))
    {
        return Err(format!("{board:?}").into());
    }
    if account["usage"]["plan"] != "Google AI Ultra" {
        return Err(format!("{}", account["usage"]["plan"]).into());
    }
    for (key, fraction, reset) in [
        ("gemini-5h", 0.99965, 1_803_996_000_i64),
        ("gemini-weekly", 0.9742, 1_804_496_400_i64),
        ("gemini-argon-wtu-weekly", 0.8, 1_804_550_400_i64),
        ("3p-5h", 0.5, 1_803_999_600_i64),
        ("3p-weekly", 0.4, 1_804_496_400_i64),
    ] {
        if resources[key]["remaining_fraction"] != fraction || resources[key]["resets_at"] != reset
        {
            return Err(format!("{key} {}", resources[key]).into());
        }
    }
    Ok(())
}

fn reading() -> String {
    r#"{"groups":[{"displayName":"Gemini","buckets":[
        {"bucketId":"gemini-5h","remainingFraction":0.99965,"resetTime":"2027-03-02T14:00:00Z"},
        {"bucketId":"gemini-weekly","remainingFraction":0.9742,"resetTime":"2027-03-08T09:00:00Z"},
        {"bucketId":"3p-5h","remainingFraction":0.5,"resetTime":"2027-03-02T15:00:00Z"},
        {"bucketId":"3p-weekly","remainingFraction":0.4,"resetTime":"2027-03-08T09:00:00Z"}
    ]},{"displayName":"Gemini 4 Argon","buckets":[
        {"bucketId":"gemini-argon-wtu-weekly","window":"weekly","remainingFraction":0.8,"resetTime":"2027-03-09T00:00:00Z"}
    ]}]}"#
    .to_owned()
}

#[tokio::test]
async fn cli_refresh_matches_across_launches_and_keeps_a_failed_reading() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let server = UsageServer::start().await?;
        let mut cli = Harness::new(&world);
        cli.usage_endpoints = server.endpoints();
        let vault = Arc::new(Memory::default());
        let shared = Arc::clone(&vault);
        let shared: Arc<dyn CredentialVault> = shared;
        cli.usage_vault = Some(shared);
        let root = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config directory")?;
        let path = root.join("usage/accounts.sqlite3");
        let now = world.clock.now().0;
        let stored = Arc::clone(&vault);
        let stored: Arc<dyn CredentialVault> = stored;
        let mut db = AccountRegistry::with_vault(&path, stored)?;
        let id = db.add_with_snapshot(
            UsageProvider::Antigravity,
            "Personal",
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
        let load = r#"{"paidTier":{"id":"g1-ultra-tier","name":"ignored"},"cloudaicompanionProject":"proj-1"}"#;
        server.serve(
            Provider::Antigravity,
            Method::Post,
            "/production/v1internal:loadCodeAssist",
            Route::json(200, load),
        );
        quota_requests().store(0, Ordering::SeqCst);
        server.serve(
            Provider::Antigravity,
            Method::Post,
            "/daily/v1internal:retrieveUserQuotaSummary",
            Route::json(200, "{}")
                .requires_json_body(json!({"project": "proj-1"}))
                .responding_with(counted_reading),
        );
        let args = ["usage", "refresh", "--json", "--account", &id.to_string()];
        let first = cli.run(&args).await?;
        first.ok()?;
        let after_first = quota_requests().load(Ordering::SeqCst);
        assert!(after_first >= 1, "the first refresh made no quota request");
        let second = cli.run(&args).await?;
        second.ok()?;
        assert!(
            quota_requests().load(Ordering::SeqCst) > after_first,
            "the second refresh made no quota request"
        );
        assert_eq!(first.stdout, second.stdout);
        assert!(!first.stdout.contains("last report"));
        assert!(!first.stderr.contains("last report"));
        let body: Value = serde_json::from_str(&first.stdout)?;
        let account = &body["accounts"][0];
        assert_eq!(account["account"]["id"], id.to_string());
        assert_eq!(account["usage"]["plan"], "Google AI Ultra");
        let resources = account["usage"]["resources"]
            .as_object()
            .ok_or("missing resources")?;
        for key in [
            "gemini-5h",
            "gemini-weekly",
            "gemini-argon-wtu-weekly",
            "3p-5h",
            "3p-weekly",
        ] {
            assert!(resources.contains_key(key), "missing {key}");
        }
        let details = cli
            .run(&["usage", "details", "--account", &id.to_string()])
            .await?;
        details.ok()?;
        let gemini = details.stdout.find("Gemini weekly").ok_or("missing Gemini weekly")?;
        let argon = details.stdout.find("Argon weekly").ok_or("missing Argon")?;
        let claude = details.stdout.find("Claude 5h").ok_or("missing Claude")?;
        assert!(claude < argon && argon < gemini, "{}", details.stdout);
        assert!(!details.stdout.contains('\x1b'));
        assert_eq!(resources["gemini-5h"]["remaining_fraction"], 0.99965);
        assert_eq!(resources["gemini-5h"]["resets_at"], 1_803_996_000_i64);
        assert_eq!(resources["gemini-weekly"]["resets_at"], 1_804_496_400_i64);
        assert_eq!(resources["3p-5h"]["resets_at"], 1_803_999_600_i64);
        assert_eq!(resources["3p-weekly"]["resets_at"], 1_804_496_400_i64);
        assert_eq!(resources["gemini-argon-wtu-weekly"]["resets_at"], 1_804_550_400_i64);
        let clock = Arc::<jackin_testkit::FakeClock>::clone(&world.clock);
        let clock: Arc<dyn Clock> = clock;
        let again = Arc::<Memory>::clone(&vault);
        let again: Arc<dyn CredentialVault> = again;
        let login = OwnedLogin::new(path.clone(), server.endpoints(), clock).with_vault(again);
        let board = jackin_tui::reload_usage_board(&login).await?;
        record_shared_board(id, account, resources, &board)?;
        let fetched = account["fetched_at"].as_i64().ok_or("missing age")?;
        world.clock.advance(Duration::from_secs(120));
        server.replace(
            Provider::Antigravity,
            Method::Post,
            "/daily/v1internal:retrieveUserQuotaSummary",
            Route::status(500),
        );
        server.replace(
            Provider::Antigravity,
            Method::Post,
            "/production/v1internal:retrieveUserQuotaSummary",
            Route::status(500),
        );
        let failed = cli.run(&args).await?;
        assert_ne!(failed.exit, first.exit);
        let failed_body: Value = serde_json::from_str(&failed.stdout)?;
        let failed_account = &failed_body["accounts"][0];
        assert_eq!(failed_account["fetched_at"], fetched);
        assert_eq!(
            failed_account["usage"]["resources"]["gemini-5h"]["remaining_fraction"],
            0.99965
        );
        let message = failed_account["failure"]["message"]
            .as_str()
            .ok_or("missing refresh error")?;
        assert!(message.contains("unavailable"), "{message}");
        assert!(!failed.stdout.contains("fixture-not-a-secret"));
        assert!(!failed.stderr.contains("fixture-not-a-secret"));
        assert!(!failed.stdout.contains("old-access"));
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
                "skipped Antigravity (Google AI) · cached@example.com: updated by the Antigravity status-line hook"
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
