// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use jackin::ExitClass;
use jackin_core::{Clock, SecretBytes};
use jackin_testkit::{MemoryVault, TestResult, World, guard};
use jackin_usage::accounts::{AccountRegistry, CandidateIdentity, CredentialVault, UsageProvider};
use jackin_usage::onboarding::receive_antigravity_statusline;
use serde_json::json;

use super::support::{Failure, Harness, ScriptedTerminal, Step};

const CONNECT: [&str; 5] = [
    "account",
    "usage",
    "connect",
    "antigravity",
    "--status-line",
];

fn report(email: &str, fraction: f64) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&json!({
        "product":"antigravity","email":email,"plan_tier":"ultra",
        "quota":{"3p-weekly":{"remaining_fraction":fraction}}
    }))
}

async fn deliver(
    path: &std::path::Path,
    vault: &Arc<dyn CredentialVault>,
    input: &[u8],
    now: i64,
) -> Result<(), Failure> {
    for _ in 0..600 {
        if receive_antigravity_statusline(path, input, now, Arc::clone(vault))? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("the enrollment never became pending".into())
}

fn terminal(email: &str) -> Result<ScriptedTerminal, Failure> {
    let (terminal, _) = ScriptedTerminal::new(vec![
        Step::Line(email.to_owned()),
        Step::Gate(Arc::new(tokio::sync::Notify::new())),
    ])?;
    Ok(terminal)
}

fn lab(world: &World) -> Result<(Harness, std::path::PathBuf, Arc<dyn CredentialVault>), Failure> {
    let mut cli = Harness::new(world);
    let vault: Arc<dyn CredentialVault> = Arc::new(MemoryVault::default());
    cli.usage_vault = Arc::clone(&vault);
    let path = cli
        .config_path
        .as_path()
        .parent()
        .ok_or("config directory")?
        .join("usage/accounts.sqlite3");
    Ok((cli, path, vault))
}

#[tokio::test]
async fn status_line_connect_prints_the_agy_command_and_waits_for_a_report() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (cli, path, vault) = lab(&world)?;
        let now = world.clock.now().0;
        let input = report("Cached@Example.com", 0.05)?;
        let run = cli
            .command(&CONNECT)
            .terminal(terminal("Cached@Example.com")?)
            .run();
        let (ran, delivered) = tokio::join!(run, deliver(&path, &vault, &input, now));
        delivered?;
        let ran = ran?;
        assert_eq!(ran.exit, ExitClass::Ok, "{}", ran.stderr);
        assert!(ran.stderr.contains("agy --settings"), "{}", ran.stderr);
        assert!(
            ran.stderr
                .contains("usage antigravity-statusline --database"),
            "{}",
            ran.stderr
        );
        assert!(ran.stderr.contains("no setup is needed"), "{}", ran.stderr);
        let listed = cli.run(&["account", "usage", "ls"]).await?;
        let row = listed
            .stdout
            .lines()
            .find(|line| line.contains("cached@example.com"))
            .ok_or("missing row")?;
        assert!(row.contains("status-line"), "{row}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn the_flag_is_rejected_for_every_other_provider() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (cli, path, _) = lab(&world)?;
        for provider in ["claude", "codex", "openrouter"] {
            let ran = cli
                .run(&["account", "usage", "connect", provider, "--status-line"])
                .await?;
            assert_eq!(ran.exit, ExitClass::Usage, "{provider}: {}", ran.stderr);
            assert!(
                ran.stderr
                    .contains("--status-line applies only to antigravity"),
                "{provider}: {}",
                ran.stderr
            );
        }
        assert!(!path.exists());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_direct_row_without_a_credential_becomes_the_status_line_row() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (cli, path, vault) = lab(&world)?;
        let now = world.clock.now().0;
        let elsewhere: Arc<dyn CredentialVault> = Arc::new(MemoryVault::default());
        let mut db = AccountRegistry::with_vault(&path, elsewhere)?;
        let id = db.add_with_snapshot(
            &jackin_core::CriticalWrite::begin(),
            UsageProvider::Antigravity,
            "Personal",
            &CandidateIdentity::OAuth(Some("cached@example.com".into())),
            &SecretBytes::new(vec![1]),
            (now - 60, &json!({"plan":"stale","resources":{}})),
        )?;
        drop(db);
        let first = report("cached@example.com", 0.05)?;
        let run = cli
            .command(&CONNECT)
            .terminal(terminal("cached@example.com")?)
            .run();
        let (ran, delivered) = tokio::join!(run, deliver(&path, &vault, &first, now));
        delivered?;
        let ran = ran?;
        assert_eq!(ran.exit, ExitClass::Ok, "{}", ran.stderr);
        assert_eq!(ran.stdout.trim(), id.to_string());
        let later = report("cached@example.com", 0.02)?;
        assert!(receive_antigravity_statusline(
            &path,
            &later,
            now + 60,
            Arc::clone(&vault)
        )?);
        let shown = cli
            .run(&["usage", "details", "--json", "--account", &id.to_string()])
            .await?;
        shown.ok()?;
        let body: serde_json::Value = serde_json::from_str(&shown.stdout)?;
        let account = &body["accounts"][0];
        assert_eq!(account["fetched_at"], now + 60);
        assert_eq!(
            account["usage"]["resources"]["3p-weekly"]["remaining_fraction"],
            0.02
        );
        let listed = cli.run(&["account", "usage", "ls"]).await?;
        let rows = listed
            .stdout
            .lines()
            .filter(|line| line.starts_with("Antigravity"))
            .count();
        assert_eq!(rows, 1, "{}", listed.stdout);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn direct_sign_in_without_an_authorized_registration_points_at_the_status_line() -> TestResult
{
    guard(async {
        let world = World::builder().build()?;
        let (cli, _, _) = lab(&world)?;
        let (terminal, _) = ScriptedTerminal::new(Vec::new())?;
        let ran = cli
            .command(&["account", "usage", "connect", "antigravity"])
            .terminal(terminal)
            .run()
            .await?;
        assert_ne!(ran.exit, ExitClass::Ok);
        assert!(
            ran.stderr.contains("connect antigravity --status-line"),
            "{}",
            ran.stderr
        );
        Ok(())
    })
    .await
}
