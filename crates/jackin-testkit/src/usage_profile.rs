// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Clock, Secret, SecretBytes};
use jackin_usage::accounts::{
    AccountRegistry, CandidateIdentity, ConnectionError, CredentialVault, FailureKind,
    UsageProvider,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

use crate::host::FakeClock;

pub const USAGE_VIEW_NOW: i64 = 1_795_534_620;
pub const USAGE_VIEW_OFFSET: i32 = 25_200;

#[derive(Debug)]
pub struct ProfileAccount {
    pub id: Uuid,
    pub provider: UsageProvider,
    pub label: String,
    pub snapshot: Value,
}

#[derive(Debug)]
struct ProfileVault;

impl CredentialVault for ProfileVault {
    fn store(&self, _id: Uuid, _secret: &SecretBytes) -> Result<(), ConnectionError> {
        Ok(())
    }
    fn delete(&self, _id: Uuid) -> Result<(), ConnectionError> {
        Ok(())
    }
    fn load(&self, _id: Uuid) -> Result<SecretBytes, ConnectionError> {
        Err(ConnectionError::MissingCredential)
    }
}

#[derive(Default)]
pub struct MemoryVault {
    values: Mutex<HashMap<Uuid, Vec<u8>>>,
}

impl std::fmt::Debug for MemoryVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MemoryVault(<redacted>)")
    }
}

impl CredentialVault for MemoryVault {
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

pub fn seed_usage_profile(
    path: &Path,
    now: i64,
    openrouter: bool,
) -> Result<Vec<ProfileAccount>, Box<dyn std::error::Error>> {
    let source: Vec<Value> =
        serde_json::from_str(include_str!("../tests/fixtures/mixed_usage.json"))?;
    let mut db = AccountRegistry::with_vault(path, Arc::new(ProfileVault))?;
    let mut accounts = Vec::new();
    for row in source {
        let provider: UsageProvider = row["provider"]
            .as_str()
            .ok_or("fixture provider missing")?
            .parse()?;
        if provider == UsageProvider::OpenRouter && !openrouter {
            continue;
        }
        let label = row["label"]
            .as_str()
            .ok_or("fixture label missing")?
            .to_owned();
        let identity = match provider {
            UsageProvider::Kimi
            | UsageProvider::Zai
            | UsageProvider::MiniMax
            | UsageProvider::Mimo
            | UsageProvider::OpenRouter => {
                CandidateIdentity::api_key(&Secret::new(format!("synthetic-{label}")))
            }
            _ => CandidateIdentity::OAuth(Some(format!("synthetic-subject-{label}"))),
        };
        let snapshot = row["snapshot"].clone();
        let id = db.add_with_snapshot(
            provider,
            &label,
            &identity,
            &SecretBytes::new(vec![1]),
            now,
            &snapshot,
        )?;
        accounts.push(ProfileAccount {
            id,
            provider,
            label,
            snapshot,
        });
    }
    Ok(accounts)
}

pub fn seed_usage_view(path: &Path) -> Result<Vec<ProfileAccount>, Box<dyn std::error::Error>> {
    let source: Vec<Value> =
        serde_json::from_str(include_str!("../tests/fixtures/usage_view.json"))?;
    let mut db = AccountRegistry::with_vault(path, Arc::new(ProfileVault))?;
    let mut accounts = Vec::new();
    for (index, row) in source.into_iter().enumerate() {
        let provider: UsageProvider = row["provider"]
            .as_str()
            .ok_or("fixture provider missing")?
            .parse()?;
        let label = row["label"]
            .as_str()
            .ok_or("fixture label missing")?
            .to_owned();
        let fetched_at = row["fetched_at"]
            .as_i64()
            .ok_or("fixture fetched_at missing")?;
        let identity = match (provider, row["identity"].as_str()) {
            (_, Some(shared)) => CandidateIdentity::OAuth(Some(shared.to_owned())),
            (
                UsageProvider::Kimi
                | UsageProvider::Zai
                | UsageProvider::MiniMax
                | UsageProvider::Mimo
                | UsageProvider::OpenRouter,
                None,
            ) => CandidateIdentity::api_key(&Secret::new(format!("synthetic-{index}-{label}"))),
            _ => CandidateIdentity::OAuth(Some(format!("synthetic-subject-{index}-{label}"))),
        };
        let snapshot = row["snapshot"].clone();
        let id = db.add_with_snapshot(
            provider,
            &label,
            &identity,
            &SecretBytes::new(vec![1]),
            fetched_at,
            &snapshot,
        )?;
        if let Some(failure) = row.get("failure") {
            let kind: FailureKind = serde_json::from_value(failure["kind"].clone())?;
            let failed_at = failure["failed_at"]
                .as_i64()
                .ok_or("fixture failed_at missing")?;
            let message = failure["message"]
                .as_str()
                .ok_or("fixture failure message missing")?;
            let version = db.version(id)?;
            db.record_failure(id, version, failed_at, kind, None, message)?;
        }
        accounts.push(ProfileAccount {
            id,
            provider,
            label,
            snapshot,
        });
    }
    Ok(accounts)
}

#[derive(Debug)]
pub struct SeedRow<'a> {
    pub provider: UsageProvider,
    pub label: &'a str,
    pub identity: Option<&'a str>,
    pub fetched_at: i64,
    pub snapshot: Value,
    pub failure: Option<(FailureKind, &'a str, i64)>,
}

pub fn seed_usage_rows(
    path: &Path,
    rows: Vec<SeedRow<'_>>,
) -> Result<Vec<ProfileAccount>, Box<dyn std::error::Error>> {
    let mut db = AccountRegistry::with_vault(path, Arc::new(ProfileVault))?;
    let mut accounts = Vec::new();
    for (index, row) in rows.into_iter().enumerate() {
        let identity = match (row.provider, row.identity) {
            (_, Some(shared)) => CandidateIdentity::OAuth(Some(shared.to_owned())),
            (
                UsageProvider::Kimi
                | UsageProvider::Zai
                | UsageProvider::MiniMax
                | UsageProvider::Mimo
                | UsageProvider::OpenRouter,
                None,
            ) => CandidateIdentity::api_key(&Secret::new(format!("row-{index}-{}", row.label))),
            _ => CandidateIdentity::OAuth(Some(format!("row-subject-{index}-{}", row.label))),
        };
        let id = db.add_with_snapshot(
            row.provider,
            row.label,
            &identity,
            &SecretBytes::new(vec![1]),
            row.fetched_at,
            &row.snapshot,
        )?;
        if let Some((kind, message, failed_at)) = row.failure {
            let version = db.version(id)?;
            db.record_failure(id, version, failed_at, kind, None, message)?;
        }
        accounts.push(ProfileAccount {
            id,
            provider: row.provider,
            label: row.label.to_owned(),
            snapshot: row.snapshot,
        });
    }
    Ok(accounts)
}

pub fn pin_usage_view_clock(clock: &FakeClock) {
    let behind = USAGE_VIEW_NOW.saturating_sub(clock.now().0);
    clock.advance(Duration::from_secs(u64::try_from(behind).unwrap_or(0)));
    clock.set_utc_offset(USAGE_VIEW_OFFSET);
}
