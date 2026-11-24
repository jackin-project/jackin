// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::support::Harness;
use jackin::ExitClass;
use jackin_testkit::{
    TestResult, World, assert_golden, guard, pin_usage_view_clock, seed_usage_view,
};
use jackin_usage::accounts::DATABASE_PATH;
use serde_json::Value;
use uuid::Uuid;

const GEMINI_APP_REASON: &str = "chat limits are not tracked; there is no usage source";

fn seeded() -> Result<(World, Harness, Vec<Uuid>), Box<dyn std::error::Error>> {
    let world = World::builder().build()?;
    pin_usage_view_clock(&world.clock);
    let cli = Harness::new(&world);
    let path = cli
        .config_path
        .as_path()
        .parent()
        .ok_or("config directory")?
        .join(DATABASE_PATH);
    let ids = seed_usage_view(&path)?.iter().map(|a| a.id).collect();
    Ok((world, cli, ids))
}

#[tokio::test]
async fn the_providers_table_names_every_provider_and_flags_the_duplicate_connection() -> TestResult
{
    guard(async {
        let (_world, cli, ids) = seeded()?;
        let ran = cli.run(&["usage", "providers"]).await?;
        ran.ok()?;
        assert!(!ran.stdout.contains('\x1b'));
        assert!(ran.stderr.is_empty());
        assert_golden("tests/cli/golden/providers.txt", &ran.stdout, &ids)
    })
    .await
}

#[tokio::test]
async fn an_empty_registry_lists_every_provider_as_not_connected() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let cli = Harness::new(&world);
        let ran = cli.run(&["usage", "providers"]).await?;
        ran.ok()?;
        assert!(!ran.stdout.contains('\x1b'));
        assert_golden("tests/cli/golden/providers-empty.txt", &ran.stdout, &[])
    })
    .await
}

#[tokio::test]
async fn one_provider_prints_its_row_without_the_gemini_info() -> TestResult {
    guard(async {
        let (_world, cli, ids) = seeded()?;
        let ran = cli.run(&["usage", "providers", "antigravity"]).await?;
        ran.ok()?;
        assert!(!ran.stdout.contains('\x1b'));
        assert!(!ran.stdout.contains("Gemini app"));
        assert_golden(
            "tests/cli/golden/providers-antigravity.txt",
            &ran.stdout,
            &ids,
        )?;
        let unknown = cli.run(&["usage", "providers", "gemini-app"]).await?;
        assert_eq!(unknown.exit, ExitClass::Usage);
        assert!(unknown.stdout.is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn json_adds_the_info_array_and_keeps_the_provider_keys() -> TestResult {
    guard(async {
        let (_world, cli, _ids) = seeded()?;
        let ran = cli.run(&["usage", "providers", "--json"]).await?;
        ran.ok()?;
        let data: Value = serde_json::from_str(&ran.stdout)?;
        let mut keys: Vec<&str> = data
            .as_object()
            .ok_or("object")?
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["info", "providers", "schema"]);
        assert_eq!(data["schema"], 1);
        let providers = data["providers"].as_array().ok_or("providers")?;
        assert_eq!(providers.len(), 11);
        let claude = providers
            .iter()
            .find(|p| p["id"] == "claude")
            .ok_or("claude")?;
        let mut row: Vec<&str> = claude
            .as_object()
            .ok_or("row")?
            .keys()
            .map(String::as_str)
            .collect();
        row.sort_unstable();
        assert_eq!(row, ["account_count", "id", "name", "status"]);
        assert_eq!(claude["account_count"], 2);
        assert_eq!(claude["status"], "connected");
        let info = data["info"].as_array().ok_or("info")?;
        assert_eq!(info.len(), 1);
        assert_eq!(info[0]["id"], "gemini-app");
        assert_eq!(info[0]["name"], "Gemini app (web)");
        assert_eq!(info[0]["tracked"], false);
        assert_eq!(info[0]["reason"], GEMINI_APP_REASON);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn gemini_code_assist_is_not_a_provider_anywhere() -> TestResult {
    guard(async {
        let (_world, cli, _ids) = seeded()?;
        for args in [
            vec!["usage", "providers"],
            vec!["usage", "providers", "--json"],
            vec!["usage", "providers", "antigravity"],
        ] {
            let ran = cli.run(&args).await?;
            ran.ok()?;
            assert!(
                !ran.stdout.to_lowercase().contains("code assist"),
                "{args:?}"
            );
        }
        for product in jackin_usage::catalog::providers() {
            assert!(
                !product.name.to_lowercase().contains("code assist")
                    && !product.id.contains("code-assist")
                    && !product.id.contains("gemini"),
                "{}",
                product.id
            );
        }
        let text = cli.run(&["usage", "providers"]).await?;
        assert_eq!(text.stdout.matches("Gemini").count(), 1);
        assert!(text.stdout.contains("Info  Gemini app (web): "));
        Ok(())
    })
    .await
}
