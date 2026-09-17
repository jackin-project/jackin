// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Read-only enumerators for third-party credential stores.
//!
//! Each submodule parses one store layout from caller-given paths and returns
//! the selected [`StoreCandidate`]s. Enumerators never write, never migrate,
//! never create lock or journal sidecars, and never scan for store locations:
//! discovery of *where* a store lives belongs to the caller.
//!
//! Canonical entry points, one per store:
//!
//! - [`opencode::enumerate_opencode_store`]: `auth.json` + `opencode.db`;
//! - [`omp::enumerate_omp_credentials`]: `agent.db` credentials;
//! - [`hermes::enumerate_hermes_store`]: `.hermes/` directory.
//!
//! Candidates carry secret values for import, so [`StoreCandidate`] redacts
//! them from [`std::fmt::Debug`] output. [`StoreError`] categories are
//! secret-free by construction.

pub(crate) mod hermes;
pub(crate) mod omp;
pub(crate) mod opencode;
pub(crate) mod sqlite;

use std::path::{Path, PathBuf};

/// Which third-party store produced a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoreKind {
    /// XDG data `opencode/auth.json`, provider-keyed entries.
    Opencode,
    /// `.omp/agent/agent.db`, `SQLite` credentials table.
    Omp,
    /// `.hermes/` directory: `config.yaml`, `profiles/`, `auth.json`.
    Hermes,
}

/// Credential shape selected from a store entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CredentialKind {
    /// Long-lived API key.
    ApiKey,
    /// Short-lived subscription token with optional refresh rotation.
    OAuth,
}

/// One selected credential from a third-party store.
///
/// The secret is always non-blank: entries without a usable value are skipped
/// during enumeration rather than surfaced as empty candidates.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct StoreCandidate {
    /// Store layout this candidate was parsed from.
    pub(crate) store: StoreKind,
    /// Provider key as named by the store (for example `"anthropic"`).
    pub(crate) provider: String,
    /// Named profile, when the store distinguishes profiles.
    pub(crate) profile: Option<String>,
    /// File evidencing the credential; discovery-style location handle.
    pub(crate) source: PathBuf,
    /// Shape of the selected secret.
    pub(crate) kind: CredentialKind,
    /// Store field that held the secret (for example `"key"`, `"access"`).
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

/// Stable, secret-free store inspection failure category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum StoreError {
    /// Source cannot be read as a regular file.
    #[error("credential store cannot be read")]
    Unreadable,
    /// Source exceeds the bounded credential read size.
    #[error("credential store exceeds the size limit")]
    TooLarge,
    /// Source is present but not parseable in its documented layout.
    #[error("credential store is malformed")]
    Malformed,
    /// Source parses but uses a layout this enumerator does not cover.
    #[error("credential store uses an unsupported layout: {0}")]
    Unsupported(&'static str),
}

/// Read a store file within `limit` bytes.
///
/// Returns `Ok(None)` when the path is absent so callers treat a missing
/// store as "no candidates". Directories and other unreadable sources fail.
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

/// Select the usable secret from one provider-keyed JSON entry.
///
/// Understands the shared `opencode` entry shapes: `{ "type": "api",
/// "key": ... }` and `{ "type": "oauth", "access": ..., "refresh": ... }`,
/// preferring `access` over `refresh`. Returns the credential kind, the field
/// name that held the secret, and the raw value. Blank or unshaped entries
/// yield `None` so callers skip them.
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

#[cfg(test)]
mod tests {
    use super::{CredentialKind, StoreCandidate, StoreKind, select_entry_secret};
    use std::path::PathBuf;

    #[test]
    fn debug_redacts_secret_value() {
        let candidate = StoreCandidate::new(
            StoreKind::Opencode,
            "anthropic".to_owned(),
            None,
            PathBuf::from("/tmp/auth.json"),
            CredentialKind::ApiKey,
            "key".to_owned(),
            "fixture-credential-001".to_owned(),
        );
        let rendered = format!("{candidate:?}");
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("fixture-credential-001"));
        assert!(rendered.contains("anthropic"));
        // The secret is stored (equality distinguishes it) but never shown.
        let other = StoreCandidate::new(
            StoreKind::Opencode,
            "anthropic".to_owned(),
            None,
            PathBuf::from("/tmp/auth.json"),
            CredentialKind::ApiKey,
            "key".to_owned(),
            "fixture-credential-002".to_owned(),
        );
        assert_ne!(candidate, other);
        assert_eq!(candidate, candidate.clone());
    }

    #[test]
    fn select_entry_secret_covers_shared_shapes() {
        let api: serde_json::Value = serde_json::from_str(r#"{"type":"api","key":"k"}"#).unwrap();
        assert_eq!(
            select_entry_secret(&api),
            Some((CredentialKind::ApiKey, "key", "k"))
        );
        let oauth: serde_json::Value =
            serde_json::from_str(r#"{"type":"oauth","access":"a","refresh":"r"}"#).unwrap();
        assert_eq!(
            select_entry_secret(&oauth),
            Some((CredentialKind::OAuth, "access", "a"))
        );
        let refresh_only: serde_json::Value =
            serde_json::from_str(r#"{"type":"oauth","refresh":"r"}"#).unwrap();
        assert_eq!(
            select_entry_secret(&refresh_only),
            Some((CredentialKind::OAuth, "refresh", "r"))
        );
        for raw in [
            r#"{"type":"api","key":"  "}"#,
            r#"{"type":"oauth"}"#,
            r#"{"type":"unknown","key":"k"}"#,
            r#"{"key":"k"}"#,
            r"[]",
        ] {
            let value: serde_json::Value = serde_json::from_str(raw).unwrap();
            assert_eq!(select_entry_secret(&value), None, "raw: {raw}");
        }
    }
}
