// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::support::Harness;
use super::usage_view::view_text;
use jackin::ExitClass;
use jackin_core::{Clock, SecretBytes};
use jackin_testkit::{
    TestResult, World, guard, pin_usage_view_clock, seed_usage_profile, seed_usage_view,
};
use jackin_usage::accounts::{
    AccountRegistry, CandidateIdentity, ConnectionError, CredentialVault, DATABASE_PATH,
    UsageProvider,
};
use jackin_usage::view::{Report, ViewFilter};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn details_reads_all_twelve_saved_accounts_and_filters_by_exact_identity() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let cli = Harness::new(&world);
        let path = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config dir")?
            .join(DATABASE_PATH);
        let no_data = cli.run(&["usage", "details", "--json"]).await?;
        assert_eq!(no_data.exit, ExitClass::NoUsageSnapshot);
        assert!(!path.exists());
        let accounts = seed_usage_profile(&path, world.clock.now().0, true)?;
        let report = cli.run(&["usage", "details", "--json"]).await?;
        report.ok()?;
        assert!(
            !report.stdout.contains("private-reset-id")
                && !report.stdout.contains("synthetic-team-reset")
        );
        let all: Value = serde_json::from_str(&report.stdout)?;
        let listed = all["accounts"].as_array().ok_or("accounts")?;
        assert_eq!(listed.len(), 12);
        for (shown, account) in listed.iter().zip(&accounts) {
            assert_eq!(shown["account"]["id"], account.id.to_string());
            assert_eq!(shown["account"]["label"], account.label);
            assert!(shown["usage"]["resources"].is_object());
        }
        let codex = cli
            .run(&["usage", "--json", "details", "--provider", "codex"])
            .await?;
        codex.ok()?;
        let codex: Value = serde_json::from_str(&codex.stdout)?;
        let rows = codex["accounts"].as_array().ok_or("codex rows")?;
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0]["usage"]["history"]["counts"]["days"]["2026-01-01"]["by_model"]["model-fixture"]
                ["turns"],
            2.0
        );
        assert_eq!(
            rows[1]["usage"]["history"]["ranges"]["today"]["observedDays"],
            0
        );
        let selected = accounts
            .iter()
            .find(|a| a.label == "Claude Team")
            .ok_or("team")?;
        let one = cli
            .run(&["usage", "details", "--account", &selected.id.to_string()])
            .await?;
        one.ok()?;
        assert!(
            one.stdout.contains("140 of 200 EUR used · 60 EUR left")
                && one.stdout.contains("30% left")
                && one
                    .stdout
                    .contains("1  full   ready       expires Tue 1 Jan 2030 00:00"),
            "{}",
            one.stdout
        );
        assert!(!one.stdout.contains('\x1b'));
        assert!(!one.stdout.contains("Claude Home"));
        let mismatch = cli
            .run(&[
                "usage",
                "details",
                "--account",
                &selected.id.to_string(),
                "--provider",
                "codex",
            ])
            .await?;
        assert_eq!(mismatch.exit, ExitClass::Usage);
        Ok(())
    })
    .await
}

#[derive(Debug)]
struct Offline;
impl CredentialVault for Offline {
    fn store(&self, _: Uuid, _: &SecretBytes) -> Result<(), ConnectionError> {
        Ok(())
    }
    fn delete(&self, _: Uuid) -> Result<(), ConnectionError> {
        Ok(())
    }
    fn load(&self, _: Uuid) -> Result<SecretBytes, ConnectionError> {
        Err(ConnectionError::MissingCredential)
    }
}

#[tokio::test]
async fn daily_details_report_exact_amounts_optional_failures_and_no_private_fields() -> TestResult
{
    guard(async {
        let world=World::builder().build()?;
        let cli=Harness::new(&world);
        let path=cli.config_path.as_path().parent().ok_or("config dir")?.join(DATABASE_PATH);
        let mut db=AccountRegistry::with_vault(&path,Arc::new(Offline))?;
        let data=json!({
            "access_token":"PRIVATE-TOKEN", "nextGrantId":"PRIVATE-GRANT", "is_available":true,
            "daily":{"consumption":{"start":"2026-11-01","end":"2026-11-22","rows":[{
                "currency":"CNY","name":"MiMo fixture","date":"2026-11-22","resource_type":"model",
                "consumed_amount":1,"request_count":2,"total_tokens":11,"input_cache_hit_amount":1,
                "internal_id":"PRIVATE-ROW"
            }]},"bills":{"unavailable":true,"attemptedAt":77}},
            "resources":{"CNY/cash":{"kind":"balance","unit":"minor_currency","currency":"CNY","exponent":2,"available":0},
                "daily/consumption":{"kind":"history","unit":"records","label":"Daily consumption","amount":1,"details":["2026-11-01 to 2026-11-22 UTC","2026-11-22 · MiMo fixture 0.000001 CNY · 2 requests · 11 tokens"]}}
        });
        db.add_with_snapshot(&jackin_core::CriticalWrite::begin(), UsageProvider::Mimo,"Daily",&CandidateIdentity::api_key(&jackin_core::Secret::new("test-only".into())),&SecretBytes::new(vec![1]), (world.clock.now().0-900, &data))?;
        let id=db.add(&jackin_core::CriticalWrite::begin(), UsageProvider::Claude,"Awaiting usage",&CandidateIdentity::OAuth(None),&SecretBytes::new(vec![1]))?;
        let pending=cli.run(&["usage","details","--account",&id.to_string(),"--json"]).await?;
        assert_eq!(pending.exit,ExitClass::NoUsageSnapshot);
        let record:Value=serde_json::from_str(&pending.stdout)?;
        assert!(record["accounts"][0]["usage"].is_null());
        let text=cli.run(&["usage","details"]).await?;
        text.ok()?;
        assert!(!text.stdout.contains('\x1b'));
        assert!(!text.stdout.contains("PRIVATE-") && !text.stdout.contains("attemptedAt"),"{}", text.stdout);
        assert!(text.stdout.contains("Daily usage    1 record · 2026-11-01 to 2026-11-22 UTC") && text.stdout.contains("2026-11-22 · MiMo fixture 0.000001 CNY"),"{}", text.stdout);
        assert!(text.stdout.contains("Cash           0 CNY") && text.stdout.contains("not fetched yet"),"{}", text.stdout);
        let report=cli.run(&["usage","details","--json"]).await?;
        report.ok()?;
        assert!(report.stdout.contains("0.000001 CNY") && report.stdout.contains("Daily history unavailable"),"{}", report.stdout);
        assert!(!report.stdout.contains("PRIVATE-") && !report.stdout.contains("attemptedAt"));
        let data:Value=serde_json::from_str(&report.stdout)?;
        let a=&data["accounts"][0];
        assert_eq!(a["stale"],true);
        assert_eq!(a["usage"]["resources"]["CNY/cash"]["available"],0);
        assert_eq!(a["usage"]["daily"]["consumption"]["rows"][0]["input_cache_hit_amount"],1);
        assert_eq!(a["usage"]["daily"]["bills"],json!({"unavailable":true}));
        Ok(())
    }).await
}

#[tokio::test]
async fn details_text_is_the_details_report_without_color_and_json_keeps_its_keys() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        pin_usage_view_clock(&world.clock);
        let cli = Harness::new(&world);
        let path = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config dir")?
            .join(DATABASE_PATH);
        let accounts = seed_usage_view(&path)?;
        let text = cli.run(&["usage", "details"]).await?;
        text.ok()?;
        assert!(!text.stdout.contains('\x1b'));
        assert!(text.stderr.is_empty(), "{}", text.stderr);
        assert!(
            text.stdout.starts_with("Usage details · "),
            "{}",
            text.stdout
        );
        assert_eq!(
            text.stdout,
            view_text(&cli, &ViewFilter::default(), Report::Details)?
        );
        let json = cli.run(&["usage", "details", "--json"]).await?;
        json.ok()?;
        assert!(!json.stdout.contains('\x1b'));
        let report: Value = serde_json::from_str(&json.stdout)?;
        assert_eq!(report["schema"], 1);
        assert_eq!(report["generated_at"], 1_795_534_620);
        let listed = report["accounts"].as_array().ok_or("accounts")?;
        assert_eq!(listed.len(), 14);
        for (shown, account) in listed.iter().zip(&accounts) {
            assert_eq!(shown["account"]["id"], account.id.to_string());
            assert!(shown["details"].is_array());
            assert!(shown["usage"]["resources"].is_object());
            assert!(shown.get("stale").is_some());
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn details_text_for_one_account_is_that_accounts_report_and_unknown_ones_are_rejected()
-> TestResult {
    guard(async {
        let world = World::builder().build()?;
        pin_usage_view_clock(&world.clock);
        let cli = Harness::new(&world);
        let path = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config dir")?
            .join(DATABASE_PATH);
        let accounts = seed_usage_view(&path)?;
        let codex = accounts
            .iter()
            .find(|account| account.provider == UsageProvider::ChatGpt)
            .ok_or("codex account")?;
        let one = cli
            .run(&["usage", "details", "--account", &codex.id.to_string()])
            .await?;
        one.ok()?;
        assert!(!one.stdout.contains('\x1b'));
        let filter = ViewFilter {
            provider: None,
            account: Some(codex.id),
        };
        assert_eq!(one.stdout, view_text(&cli, &filter, Report::Details)?);
        let everything = view_text(&cli, &ViewFilter::default(), Report::Details)?;
        assert_ne!(one.stdout, everything);
        let unknown = cli
            .run(&["usage", "details", "--account", &Uuid::new_v4().to_string()])
            .await?;
        assert_eq!(unknown.exit, ExitClass::Usage);
        assert!(unknown.stdout.is_empty());
        let none = cli
            .run(&[
                "usage",
                "details",
                "--provider",
                "mimo",
                "--account",
                &codex.id.to_string(),
            ])
            .await?;
        assert_eq!(none.exit, ExitClass::Usage);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn details_text_without_saved_usage_prints_nothing_and_reports_no_snapshot() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let cli = Harness::new(&world);
        let empty = cli.run(&["usage", "details"]).await?;
        assert_eq!(empty.exit, ExitClass::NoUsageSnapshot);
        assert!(empty.stdout.is_empty());
        Ok(())
    })
    .await
}
