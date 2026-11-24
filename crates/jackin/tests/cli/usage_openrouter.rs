// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use jackin_core::{Provider, Secret, SecretBytes};
use jackin_testkit::{Method, Route, TestResult, UsageServer, World, guard};
use jackin_usage::accounts::{
    AccountRegistry, CandidateIdentity, ConnectionError, CredentialVault, UsageProvider,
};
use jackin_usage::connected::KeyConnection;
use serde_json::Value;
use uuid::Uuid;

use super::support::Harness;

const KEY: &str = r#"{"data":{"is_management_key":false,"limit":100.10,"limit_remaining":0.1,
"usage_daily":0.30000000000000004,"usage_weekly":"2.5e-3","usage_monthly":1e-7,
"byok_usage_daily":0.0000001,
"effective_budget":{"limit_usd":10000000000.0000001,"spend_usd":10000000000,"reset_interval":"weekly"},
"free_model_daily_requests":{"used":1,"limit":2}}}"#;

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

#[tokio::test]
async fn a_refreshed_openrouter_account_keeps_every_digit_in_json_and_text() -> TestResult {
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
        let stored: Arc<dyn CredentialVault> = vault;
        let mut db = AccountRegistry::with_vault(&root.join("usage/accounts.sqlite3"), stored)?;
        let key = Secret::new("SECRET_TOKEN_ROUTER".to_owned());
        let id = db.add(
            UsageProvider::OpenRouter,
            "Router",
            &CandidateIdentity::api_key(&key),
            &KeyConnection::OpenRouter.encode(&key)?,
        )?;
        drop(db);
        server.serve(
            Provider::Openrouter,
            Method::Get,
            "/api/v1/key",
            Route::json(200, KEY).requires_header("authorization", "Bearer SECRET_TOKEN_ROUTER"),
        );
        let id = id.to_string();
        let refreshed = cli.run(&["usage", "refresh", "--account", &id]).await?;
        refreshed.ok()?;
        let after: Value = serde_json::from_str(&refreshed.stdout)?;
        let usage = &after["accounts"][0]["usage"];
        assert_eq!(usage["resources"]["budget"]["limit"], "10000000000.0000001");
        assert_eq!(usage["resources"]["budget"]["remaining"], "0.0000001");
        assert_eq!(usage["resources"]["keyLimit"]["used"], "100");
        server.replace(
            Provider::Openrouter,
            Method::Get,
            "/api/v1/key",
            Route::status(500),
        );
        let snapshot = cli.run(&["usage", "--json"]).await?;
        snapshot.ok()?;
        let again: Value = serde_json::from_str(&snapshot.stdout)?;
        assert_eq!(again["accounts"][0]["usage"], *usage);
        let details = cli
            .run(&["usage", "details", "--json", "--account", &id])
            .await?;
        details.ok()?;
        let details: Value = serde_json::from_str(&details.stdout)?;
        let spend = &details["accounts"][0]["usage"]["period_spend"];
        assert_eq!(spend["today"]["spend"], "0.30000000000000004");
        assert_eq!(spend["today"]["byokSpend"], "0.0000001");
        assert_eq!(spend["thisWeek"]["spend"], "0.0025");
        assert_eq!(spend["thisMonth"]["spend"], "0.0000001");
        let text = cli.run(&["usage", "details", "--account", &id]).await?;
        text.ok()?;
        for expected in [
            "Today · 0.30000000000000004 usd",
            "This week · 0.0025 usd",
            "This month · 0.0000001 usd",
            "Limit · 10000000000.0000001 usd",
            "Remaining · 0.0000001 usd",
        ] {
            assert!(
                text.stdout.contains(expected),
                "{expected}\n{}",
                text.stdout
            );
        }
        let overview = cli.run(&["usage"]).await?;
        overview.ok()?;
        assert!(
            overview.stdout.contains("Today · 0.30000000000000004 usd"),
            "{}",
            overview.stdout
        );
        Ok(())
    })
    .await
}
