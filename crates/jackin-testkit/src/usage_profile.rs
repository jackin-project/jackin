// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Secret, SecretBytes};
use jackin_usage::accounts::{
    AccountRegistry, CandidateIdentity, ConnectionError, CredentialVault, UsageProvider,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

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
