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
        drop(db);
        let overview = cli.run(&["usage"]).await?;
        overview.ok()?;
        assert!(overview.stdout.contains("claude · Work"));
        assert!(overview.stdout.contains("Session · 100% remaining · stale"));
        assert!(
            overview
                .stderr
                .contains("Waiting for a first usage reading")
        );
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
