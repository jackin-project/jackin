// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::support::{Failure, Harness};
use jackin::ExitClass;
use jackin_testkit::{
    TestResult, World, assert_golden, guard, pin_usage_view_clock, seed_usage_view,
};
use jackin_usage::accounts::DATABASE_PATH;
use serde_json::Value;
use uuid::Uuid;

const LEGACY_KEYS: [&str; 5] = ["id", "provider", "label", "position", "preferences"];

fn seeded(world: &World, harness: &Harness) -> Result<Vec<Uuid>, Failure> {
    pin_usage_view_clock(&world.clock);
    let path = harness
        .config_path
        .as_path()
        .parent()
        .ok_or("config dir")?
        .join(DATABASE_PATH);
    Ok(seed_usage_view(&path)?
        .into_iter()
        .map(|account| account.id)
        .collect())
}

#[tokio::test]
async fn ls_prints_the_connection_table_without_escapes_or_stderr_noise() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        let ids = seeded(&world, &harness)?;
        let ran = harness.run(&["account", "usage", "ls"]).await?;
        ran.ok()?;
        assert!(!ran.stdout.contains('\u{1b}'));
        assert!(ran.stderr.is_empty(), "{}", ran.stderr);
        assert_golden("tests/cli/golden/ls.txt", &ran.stdout, &ids)
    })
    .await
}

#[tokio::test]
async fn ls_wide_adds_fetch_time_ids_and_the_last_error() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        let ids = seeded(&world, &harness)?;
        let ran = harness.run(&["account", "usage", "ls", "--wide"]).await?;
        ran.ok()?;
        assert!(!ran.stdout.contains('\u{1b}'));
        assert!(ran.stdout.lines().all(|line| !line.ends_with(' ')));
        for id in &ids {
            assert!(
                ran.stdout.contains(&id.hyphenated().to_string()),
                "missing id {id}"
            );
        }
        assert!(ran.stdout.contains("HTTP 403 SERVICE_DISABLED"));
        assert_golden("tests/cli/golden/ls-wide.txt", &ran.stdout, &ids)
    })
    .await
}

#[tokio::test]
async fn ls_json_keeps_the_existing_keys_and_adds_the_readable_fields() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        let ids = seeded(&world, &harness)?;
        let ran = harness.run(&["account", "usage", "ls", "--json"]).await?;
        ran.ok()?;
        let rows = ran.value()?;
        let rows = rows.as_array().ok_or("rows")?;
        assert_eq!(rows.len(), ids.len());
        for row in rows {
            for key in LEGACY_KEYS {
                assert!(row.get(key).is_some(), "missing {key}: {row}");
            }
            for key in [
                "provider_name",
                "plan",
                "fetched_at",
                "status",
                "last_error",
                "duplicate_of",
            ] {
                assert!(row.get(key).is_some(), "missing {key}: {row}");
            }
        }
        let failing = rows
            .iter()
            .find(|row| row["status"] == "failing")
            .ok_or("failing row")?;
        assert_eq!(failing["last_error"], "HTTP 403 SERVICE_DISABLED");
        assert_eq!(failing["provider_name"], "Antigravity (Google AI)");
        let twin = rows
            .iter()
            .find(|row| row["provider"] == "antigravity" && row["id"] != failing["id"])
            .ok_or("twin row")?;
        assert_eq!(failing["duplicate_of"], serde_json::json!([twin["id"]]));
        assert_eq!(twin["duplicate_of"], serde_json::json!([failing["id"]]));
        assert_eq!(twin["status"], "stale");
        assert_eq!(twin["last_error"], Value::Null);
        let claude = rows
            .iter()
            .find(|row| row["provider"] == "claude")
            .ok_or("claude row")?;
        assert_eq!(claude["plan"], "Ultra");
        assert_eq!(claude["duplicate_of"], serde_json::json!([]));
        assert!(claude["fetched_at"].is_i64());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn ls_with_no_connections_says_so_and_json_stays_an_empty_array() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let harness = Harness::new(&world);
        let text = harness.run(&["account", "usage", "ls"]).await?;
        text.ok()?;
        assert_eq!(
            text.stdout,
            "No usage accounts connected.\nConnect one: jackin account usage connect <provider>\n"
        );
        let json = harness.run(&["account", "usage", "ls", "--json"]).await?;
        json.ok()?;
        assert_eq!(json.value()?, Value::Array(Vec::new()));
        assert_eq!(text.exit, ExitClass::Ok);
        Ok(())
    })
    .await
}
