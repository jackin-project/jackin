// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::support::Harness;
use jackin::ExitClass;
use jackin_core::{Clock, SecretBytes};
use jackin_testkit::{TestResult, World, guard};
use jackin_usage::accounts::{
    AccountRegistry, CandidateIdentity, ConnectionError, CredentialVault, UsageProvider,
};
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug)]
struct WriteOnly;

impl CredentialVault for WriteOnly {
    fn store(&self, _: Uuid, _: &SecretBytes) -> Result<(), ConnectionError> {
        Ok(())
    }
    fn load(&self, _: Uuid) -> Result<SecretBytes, ConnectionError> {
        Err(ConnectionError::MissingCredential)
    }
    fn delete(&self, _: Uuid) -> Result<(), ConnectionError> {
        Ok(())
    }
}

#[tokio::test]
async fn offline_snapshot_filters_accounts_and_preserves_absence_and_zero() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let cli = Harness::new(&world);
        let empty = cli.run(&["usage", "--json"]).await?;
        assert_eq!(empty.exit, ExitClass::NoUsageSnapshot);
        assert_eq!(
            serde_json::from_str::<Value>(&empty.stdout)?["accounts"],
            json!([])
        );
        let root = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config directory")?;
        let mut db =
            AccountRegistry::with_vault(&root.join("usage/accounts.sqlite3"), Arc::new(WriteOnly))?;
        let secret = SecretBytes::new(vec![]);
        let now = world.clock.now().0;
        let id = db.add_with_snapshot(
            UsageProvider::Claude,
            "Work",
            &CandidateIdentity::OAuth(None),
            &secret,
            now - 600,
            &json!({
                "nextGrantId":"private-receipt",
                "resources":{"session":{"kind":"consumption","unit":"percent","used":0,"limit":100}}
            }),
        )?;
        let pending = db.add(
            UsageProvider::Grok,
            "Personal",
            &CandidateIdentity::OAuth(None),
            &secret,
        )?;
        db.add_with_snapshot(
            UsageProvider::OpenCode,
            "Hidden workspace",
            &CandidateIdentity::OAuth(None),
            &secret,
            now,
            &json!({"notice":"Reconnect this workspace","resources":{}}),
        )?;
        drop(db);
        let overview = cli.run(&["usage"]).await?;
        overview.ok()?;
        assert!(overview.stdout.contains("claude · Work"));
        assert!(overview.stdout.contains("Session · 100% remaining · stale"));
        assert!(
            overview
                .stdout
                .contains("grok · Personal · Waiting for a first usage reading")
        );
        assert!(
            overview
                .stdout
                .contains("opencode · Hidden workspace · Reconnect this workspace")
        );
        assert_eq!(
            overview.stdout.matches("Reconnect this workspace").count(),
            1
        );
        assert!(!overview.stderr.contains("Reconnect this workspace"));
        let db =
            AccountRegistry::with_vault(&root.join("usage/accounts.sqlite3"), Arc::new(WriteOnly))?;
        let mut settings = db.dashboard_settings()?;
        settings.conceal_names = true;
        db.set_dashboard_settings(&settings)?;
        drop(db);
        let concealed = cli.run(&["usage"]).await?;
        concealed.ok()?;
        assert!(
            concealed
                .stdout
                .contains("grok · Account 2 · Waiting for a first usage reading")
        );
        assert!(
            concealed
                .stdout
                .contains("opencode · Account 3 · Current usage unavailable.")
        );
        for output in [&concealed.stdout, &concealed.stderr] {
            assert!(
                !output.contains("Personal")
                    && !output.contains("Hidden workspace")
                    && !output.contains("Reconnect this workspace")
            );
        }
        let all = cli.run(&["usage", "--json"]).await?;
        all.ok()?;
        let snapshot: Value = serde_json::from_str(&all.stdout)?;
        assert_eq!(snapshot["schema"], 1);
        assert_eq!(snapshot["generated_at"], now);
        assert_eq!(
            snapshot["accounts"][1]["account"]["id"],
            pending.to_string()
        );
        assert!(snapshot["accounts"][1]["usage"].is_null());
        let selected = cli
            .run(&[
                "usage",
                "snapshot",
                "--provider",
                "claude",
                "--account",
                &id.to_string(),
            ])
            .await?;
        selected.ok()?;
        assert!(selected.stderr.is_empty());
        assert!(!selected.stdout.contains("private-receipt"));
        let selected: Value = serde_json::from_str(&selected.stdout)?;
        assert_eq!(selected["accounts"].as_array().map(Vec::len), Some(1));
        let row = &selected["accounts"][0];
        assert_eq!(row["account"]["id"], id.to_string());
        assert_eq!(row["stale"], true);
        assert_eq!(row["expires_at"], now);
        assert_eq!(row["usage"]["resources"]["session"]["used"], 0);
        for provider in ["gemini", "unknown"] {
            let result = cli
                .run(&["usage", "snapshot", "--provider", provider])
                .await?;
            assert_eq!(result.exit, ExitClass::Usage);
            assert!(result.stdout.is_empty());
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn provider_catalog_includes_disconnected_subscriptions_and_counts_each_account() -> TestResult
{
    guard(async {
        let world = World::builder().build()?;
        let cli = Harness::new(&world);
        let path = cli
            .config_path
            .as_path()
            .parent()
            .ok_or("config directory")?
            .join(jackin_usage::accounts::DATABASE_PATH);
        let empty = cli.run(&["usage", "providers", "--json"]).await?;
        empty.ok()?;
        let data: Value = serde_json::from_str(&empty.stdout)?;
        let providers = data["providers"].as_array().ok_or("providers")?;
        for id in [
            "claude",
            "codex",
            "muse",
            "grok",
            "kimi",
            "zai",
            "minimax",
            "mimo",
            "opencode",
            "antigravity",
        ] {
            let row = providers
                .iter()
                .find(|p| p["id"] == id)
                .ok_or("missing provider")?;
            assert_eq!(row["status"], "not_connected");
            assert_eq!(row["account_count"], 0);
        }
        let google = cli.run(&["usage", "providers", "google-ai-ultra"]).await?;
        google.ok()?;
        assert!(
            google.stdout.contains("Google AI Ultra") && google.stdout.contains("Usage Limits")
        );
        let row = providers
            .iter()
            .find(|p| p["id"] == "google-ai-ultra")
            .ok_or("Google AI Ultra")?;
        assert_eq!(row["status"], "unavailable");
        assert!(
            google
                .stdout
                .contains(row["unavailable_reason"].as_str().ok_or("reason")?)
        );
        assert!(!path.exists());
        jackin_testkit::seed_usage_profile(&path, world.clock.now().0, true)?;
        let overview = cli.run(&["usage", "--json"]).await?;
        overview.ok()?;
        let data: Value = serde_json::from_str(&overview.stdout)?;
        for id in ["claude", "codex"] {
            let rows = data["providers"].as_array().ok_or("overview providers")?;
            let row = rows.iter().find(|p| p["id"] == id).ok_or("provider")?;
            assert_eq!(row["account_count"], 2);
            assert_eq!(row["status"], "connected");
            let text = cli.run(&["usage", "providers", id]).await?;
            text.ok()?;
            assert!(text.stdout.contains("2 connected accounts"));
        }
        Ok(())
    })
    .await
}
