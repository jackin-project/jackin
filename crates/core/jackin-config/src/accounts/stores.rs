// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(crate) mod hermes;
pub(crate) mod omp;
pub(crate) mod opencode;

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoreKind {
    Opencode,
    Hermes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CredentialKind {
    ApiKey,
    OAuth,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct StoreCandidate {
    pub(crate) store: StoreKind,
    pub(crate) provider: String,
    pub(crate) profile: Option<String>,
    pub(crate) source: PathBuf,
    pub(crate) kind: CredentialKind,
    pub(crate) field: String,
    secret: String,
}

impl StoreCandidate {
    pub(crate) fn new(
        store: StoreKind,
        provider: String,
        profile: Option<String>,
        source: PathBuf,
        kind: CredentialKind,
        field: String,
        secret: String,
    ) -> Self {
        Self {
            store,
            provider,
            profile,
            source,
            kind,
            field,
            secret,
        }
    }
}

impl std::fmt::Debug for StoreCandidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreCandidate")
            .field("store", &self.store)
            .field("provider", &self.provider)
            .field("profile", &self.profile)
            .field("source", &self.source)
            .field("kind", &self.kind)
            .field("field", &self.field)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum StoreError {
    #[error("credential store cannot be read")]
    Unreadable,
    #[error("credential store exceeds the size limit")]
    TooLarge,
    #[error("credential store is malformed")]
    Malformed,
    #[error("credential store uses an unsupported layout: {0}")]
    Unsupported(&'static str),
}

pub(crate) fn read_store_file(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, StoreError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err(StoreError::Unreadable),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StoreError::Unreadable),
    }
    let bytes = match crate::persist::read_bounded_file(path, limit.saturating_add(1)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StoreError::Unreadable),
    };
    if u64::try_from(bytes.len()).is_ok_and(|len| len > limit) {
        return Err(StoreError::TooLarge);
    }
    Ok(Some(bytes))
}

pub(crate) fn select_entry_secret(
    entry: &serde_json::Value,
) -> Option<(CredentialKind, &'static str, &str)> {
    let kind = entry.get("type").and_then(serde_json::Value::as_str)?;
    match kind {
        "api" => {
            let key = entry.get("key").and_then(serde_json::Value::as_str)?;
            (!key.trim().is_empty()).then_some((CredentialKind::ApiKey, "key", key))
        }
        "oauth" => {
            for field in ["access", "refresh"] {
                if let Some(token) = entry
                    .get(field)
                    .and_then(serde_json::Value::as_str)
                    .filter(|token| !token.trim().is_empty())
                {
                    return Some((CredentialKind::OAuth, field, token));
                }
            }
            None
        }
        _ => None,
    }
}
