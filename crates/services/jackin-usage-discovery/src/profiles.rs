// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Profile credential readers.

use crate::ProfileCredentialMaterial;
use std::collections::{BTreeMap, BTreeSet};

use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) enum ProfileReadOutcome {
    Bytes(Vec<u8>),
    Missing,
    Denied,
    ConsentRequired,
}

pub(crate) trait ProfileCredentialReader {
    fn read(&self, path: &Path) -> ProfileReadOutcome;
    fn exists(&self, path: &Path) -> bool;
    fn read_claude_keychain(&self, scope: &jackin_core::ClaudeKeychainScope) -> ProfileReadOutcome;
    /// Presence-only probe for the Antigravity Keychain grant singleton.
    /// `Bytes` is always empty and never carries the grant: the CLI owns the
    /// secret, discovery only learns whether it exists.
    fn read_antigravity_keychain(&self) -> ProfileReadOutcome;
}

pub(crate) struct CachingProfileCredentialReader<'a> {
    inner: &'a dyn ProfileCredentialReader,
    exists: std::cell::RefCell<BTreeMap<PathBuf, bool>>,
    files: std::cell::RefCell<BTreeMap<PathBuf, ProfileReadOutcome>>,
    keychain: std::cell::RefCell<BTreeMap<String, ProfileReadOutcome>>,
    antigravity_grant: std::cell::RefCell<Option<ProfileReadOutcome>>,
}

impl<'a> CachingProfileCredentialReader<'a> {
    pub(crate) fn new(inner: &'a dyn ProfileCredentialReader) -> Self {
        Self {
            inner,
            exists: std::cell::RefCell::new(BTreeMap::new()),
            files: std::cell::RefCell::new(BTreeMap::new()),
            keychain: std::cell::RefCell::new(BTreeMap::new()),
            antigravity_grant: std::cell::RefCell::new(None),
        }
    }
}

impl ProfileCredentialReader for CachingProfileCredentialReader<'_> {
    fn read(&self, path: &Path) -> ProfileReadOutcome {
        if let Some(outcome) = self.files.borrow().get(path).cloned() {
            return outcome;
        }
        let outcome = self.inner.read(path);
        self.files
            .borrow_mut()
            .insert(path.to_path_buf(), outcome.clone());
        outcome
    }

    fn exists(&self, path: &Path) -> bool {
        if let Some(exists) = self.exists.borrow().get(path).copied() {
            return exists;
        }
        let exists = self.inner.exists(path);
        self.exists.borrow_mut().insert(path.to_path_buf(), exists);
        exists
    }

    fn read_claude_keychain(&self, scope: &jackin_core::ClaudeKeychainScope) -> ProfileReadOutcome {
        if let Some(outcome) = self.keychain.borrow().get(&scope.service).cloned() {
            return outcome;
        }
        let outcome = self.inner.read_claude_keychain(scope);
        self.keychain
            .borrow_mut()
            .insert(scope.service.clone(), outcome.clone());
        outcome
    }

    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        if let Some(outcome) = self.antigravity_grant.borrow().clone() {
            return outcome;
        }
        let outcome = self.inner.read_antigravity_keychain();
        *self.antigravity_grant.borrow_mut() = Some(outcome.clone());
        outcome
    }
}

pub(crate) struct SystemProfileCredentialReader;

impl ProfileCredentialReader for SystemProfileCredentialReader {
    fn read(&self, path: &Path) -> ProfileReadOutcome {
        match std::fs::read(path) {
            Ok(bytes) => ProfileReadOutcome::Bytes(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ProfileReadOutcome::Missing
            }
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                ProfileReadOutcome::Denied
            }
            Err(_) => ProfileReadOutcome::Missing,
        }
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_claude_keychain(&self, scope: &jackin_core::ClaudeKeychainScope) -> ProfileReadOutcome {
        match jackin_usage_provider_claude::read_claude_keychain_item(
            &scope.service,
            jackin_usage_provider_claude::ClaudeKeychainInteractionPolicy::Unattended,
        ) {
            #[cfg(any(target_os = "macos", test))]
            jackin_usage_provider_claude::ClaudeKeychainRead::Payload { json } => {
                ProfileReadOutcome::Bytes(json.into_bytes())
            }
            jackin_usage_provider_claude::ClaudeKeychainRead::Denied => ProfileReadOutcome::Denied,
            jackin_usage_provider_claude::ClaudeKeychainRead::Missing => {
                ProfileReadOutcome::Missing
            }
            jackin_usage_provider_claude::ClaudeKeychainRead::ConsentRequired => {
                ProfileReadOutcome::ConsentRequired
            }
        }
    }

    fn read_antigravity_keychain(&self) -> ProfileReadOutcome {
        #[cfg(target_os = "macos")]
        {
            use security_framework::item::{ItemClass, ItemSearchOptions};

            // This profile query can run outside the shipped broker binary
            // (for example through a library discovery caller), so establish
            // its own no-UI scope before invoking Security.framework.
            let Ok(_unattended_keychain_guard) =
                jackin_usage_provider_claude::unattended_keychain_guard()
            else {
                return ProfileReadOutcome::ConsentRequired;
            };

            // Reference-only search: no `load_data`, so the grant payload is
            // never read into this process — presence is the whole answer.
            let mut options = ItemSearchOptions::new();
            options
                .class(ItemClass::generic_password())
                .service(jackin_usage_provider_antigravity::ANTIGRAVITY_KEYCHAIN_SERVICE)
                .limit(1);
            match options.search() {
                Ok(results) if !results.is_empty() => ProfileReadOutcome::Bytes(Vec::new()),
                Ok(_) => ProfileReadOutcome::Missing,
                Err(error) => match jackin_usage_provider_claude::classify_claude_keychain_status(
                    error.code(),
                ) {
                    // Unreachable: the classifier only emits Denied/Missing.
                    // Fail closed to absence either way.
                    jackin_usage_provider_claude::ClaudeKeychainRead::Payload { .. } => {
                        ProfileReadOutcome::Missing
                    }
                    jackin_usage_provider_claude::ClaudeKeychainRead::Denied => {
                        ProfileReadOutcome::Denied
                    }
                    jackin_usage_provider_claude::ClaudeKeychainRead::Missing => {
                        ProfileReadOutcome::Missing
                    }
                    jackin_usage_provider_claude::ClaudeKeychainRead::ConsentRequired => {
                        ProfileReadOutcome::ConsentRequired
                    }
                },
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            ProfileReadOutcome::Missing
        }
    }
}

pub(crate) enum ProfileValidation {
    Authenticated {
        provider_id: Option<String>,
        account_label: Option<String>,
        material: Option<Box<ProfileCredentialMaterial>>,
    },
    Anonymous(Option<Box<ProfileCredentialMaterial>>),
    Missing,
    Denied,
    ConsentRequired,
    Malformed,
}

pub(crate) struct AccountAccumulator {
    pub(crate) label: String,
    pub(crate) provenance: BTreeSet<String>,
    pub(crate) source_ids: BTreeSet<String>,
}
