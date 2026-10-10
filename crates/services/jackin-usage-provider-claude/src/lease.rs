// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Process-local exact-source Claude credential lease.

use std::sync::{Arc, Mutex, OnceLock, atomic::AtomicU64};

use zeroize::{Zeroize, Zeroizing};

use crate::keychain::{
    ClaudeKeychainPolicyError, ClaudeKeychainRead, read_claude_keychain_item_for_foreground,
};
use crate::payload_diagnostic::{
    ClaudeCredentialPayloadDiagnostic, diagnose_claude_profile_payload,
};

/// Secret-free result of explicit foreground Claude Keychain bootstrap.
#[derive(Debug)]
pub enum ClaudeCredentialBootstrapOutcome {
    Missing,
    Denied,
    InteractionRequired,
    /// Rejected payload with bounded facts that exclude credential values.
    Malformed(ClaudeCredentialPayloadDiagnostic),
    Acquired(ClaudeCredentialLease),
}

struct CachedClaudeCredential {
    service: String,
    payload: Zeroizing<String>,
    generation: u64,
    unauthorized_reread_attempted: bool,
}

#[derive(Default)]
struct ClaudeCredentialCache {
    credential: Mutex<Option<CachedClaudeCredential>>,
}

impl ClaudeCredentialCache {
    fn service(&self) -> Option<String> {
        self.credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|cached| cached.service.clone())
    }

    fn payload(&self, service: &str, generation: Option<u64>) -> Option<Zeroizing<String>> {
        self.credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|cached| {
                cached.service == service
                    && generation.is_none_or(|generation| cached.generation == generation)
            })
            .map(|cached| cached.payload.clone())
    }

    fn store(&self, service: String, payload: Zeroizing<String>, generation: u64) {
        *self
            .credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(CachedClaudeCredential {
            service,
            payload,
            generation,
            unauthorized_reread_attempted: false,
        });
    }

    fn begin_unauthorized_reread(&self, service: &str, generation: u64) -> bool {
        let mut credential = self
            .credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(cached) = credential
            .as_mut()
            .filter(|cached| cached.service == service && cached.generation == generation)
        else {
            return false;
        };
        if cached.unauthorized_reread_attempted {
            return false;
        }
        cached.unauthorized_reread_attempted = true;
        true
    }

    fn replace_if_exact(&self, service: &str, generation: u64, payload: Zeroizing<String>) -> bool {
        let mut credential = self
            .credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(cached) = credential
            .as_mut()
            .filter(|cached| cached.service == service && cached.generation == generation)
        else {
            return false;
        };
        cached.payload = payload;
        true
    }

    fn clear_generation(&self, generation: u64) {
        let mut credential = self
            .credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if credential
            .as_ref()
            .is_some_and(|cached| cached.generation == generation)
        {
            *credential = None;
        }
    }

    #[cfg(test)]
    fn clear(&self) {
        *self
            .credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

fn credential_cache() -> &'static ClaudeCredentialCache {
    static CACHE: OnceLock<ClaudeCredentialCache> = OnceLock::new();
    CACHE.get_or_init(ClaudeCredentialCache::default)
}

fn bootstrap_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn next_generation() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub(crate) const MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_CLAUDE_KEYCHAIN_SERVICE_BYTES: usize = 512;

pub(crate) fn valid_claude_keychain_service(service: &str) -> bool {
    !service.trim().is_empty()
        && service.len() <= MAX_CLAUDE_KEYCHAIN_SERVICE_BYTES
        && !service.contains('\0')
}

fn valid_claude_keychain_payload(payload: &str) -> Result<(), ClaudeCredentialPayloadDiagnostic> {
    let bytes = payload.as_bytes();
    if bytes.len() > MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES {
        return Err(diagnose_claude_profile_payload(bytes));
    }
    if crate::credentials::parse_claude_keychain_profile(bytes)
        .is_some_and(|profile| profile.credential.is_some())
    {
        Ok(())
    } else {
        Err(diagnose_claude_profile_payload(bytes))
    }
}

/// Stable opaque partition for one exact Keychain service. It is independent
/// of tokens and account metadata, so credential rotation cannot remap consent.
#[must_use]
pub fn claude_source_capability_id_for_service(service: &str) -> String {
    let hashed = jackin_core::account_key_hash("claude-keychain-service-v1", service);
    hashed.strip_prefix("sha256:").unwrap_or(&hashed).to_owned()
}

struct ClaudeCredentialLeaseInner {
    service: String,
    source_capability_id: String,
    generation: u64,
}

impl Drop for ClaudeCredentialLeaseInner {
    fn drop(&mut self) {
        credential_cache().clear_generation(self.generation);
    }
}

/// Owner handle for the selected foreground Claude credential. It exposes
/// only a local source partition; raw service and credential payload stay private.
#[must_use = "hold the lease for the full foreground service lifetime"]
#[derive(Clone)]
pub struct ClaudeCredentialLease {
    inner: Arc<ClaudeCredentialLeaseInner>,
}

impl ClaudeCredentialLease {
    /// Opaque stable local ID for the exact selected Keychain service.
    #[must_use]
    pub fn source_capability_id(&self) -> &str {
        &self.inner.source_capability_id
    }

    pub(crate) fn service(&self) -> &str {
        &self.inner.service
    }

    pub(crate) fn generation(&self) -> u64 {
        self.inner.generation
    }
}

impl std::fmt::Debug for ClaudeCredentialLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ClaudeCredentialLease(REDACTED)")
    }
}

/// Acquire the only process-local Claude source lease after an attached
/// operator explicitly permits Keychain interaction. No payload leaves cache.
pub fn bootstrap_claude_credential(
    service: &str,
) -> Result<ClaudeCredentialBootstrapOutcome, ClaudeKeychainPolicyError> {
    bootstrap_claude_credential_with(service, crate::keychain::all_stdio_are_terminal(), || {
        read_claude_keychain_item_for_foreground(service)
    })
}

fn bootstrap_claude_credential_with(
    service: &str,
    all_stdio_are_terminal: bool,
    read_item: impl FnOnce() -> ClaudeKeychainRead,
) -> Result<ClaudeCredentialBootstrapOutcome, ClaudeKeychainPolicyError> {
    if !all_stdio_are_terminal {
        return Ok(ClaudeCredentialBootstrapOutcome::InteractionRequired);
    }
    if !valid_claude_keychain_service(service) {
        return Err(ClaudeKeychainPolicyError::InvalidService);
    }

    let _bootstrap = bootstrap_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if credential_cache().service().is_some() {
        return Err(ClaudeKeychainPolicyError::ScopeConflict);
    }

    match read_item() {
        ClaudeKeychainRead::Payload { json } => {
            if let Err(diagnostic) = valid_claude_keychain_payload(&json) {
                json.zeroize();
                return Ok(ClaudeCredentialBootstrapOutcome::Malformed(diagnostic));
            }
            let generation = next_generation();
            credential_cache().store(service.to_owned(), json, generation);
            Ok(ClaudeCredentialBootstrapOutcome::Acquired(
                ClaudeCredentialLease {
                    inner: Arc::new(ClaudeCredentialLeaseInner {
                        service: service.to_owned(),
                        source_capability_id: claude_source_capability_id_for_service(service),
                        generation,
                    }),
                },
            ))
        }
        ClaudeKeychainRead::Denied => Ok(ClaudeCredentialBootstrapOutcome::Denied),
        ClaudeKeychainRead::Missing => Ok(ClaudeCredentialBootstrapOutcome::Missing),
        ClaudeKeychainRead::ConsentRequired => {
            Ok(ClaudeCredentialBootstrapOutcome::InteractionRequired)
        }
    }
}

pub(crate) fn bootstrapped_claude_service() -> Option<String> {
    credential_cache().service()
}

pub(crate) fn cached_claude_keychain_payload(service: &str) -> Option<Zeroizing<String>> {
    credential_cache().payload(service, None)
}

pub(crate) fn cached_payload_for_lease(lease: &ClaudeCredentialLease) -> Option<Zeroizing<String>> {
    credential_cache().payload(lease.service(), Some(lease.generation()))
}

pub(crate) fn begin_unauthorized_reread(lease: &ClaudeCredentialLease) -> bool {
    credential_cache().begin_unauthorized_reread(lease.service(), lease.generation())
}

pub(crate) fn replace_if_exact(lease: &ClaudeCredentialLease, payload: Zeroizing<String>) -> bool {
    credential_cache().replace_if_exact(lease.service(), lease.generation(), payload)
}

#[cfg(test)]
pub(crate) fn clear_bootstrapped_claude_credential() {
    credential_cache().clear();
}

#[cfg(test)]
pub(crate) fn serialized_credential_cache_test() -> std::sync::MutexGuard<'static, ()> {
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
pub(crate) fn bootstrap_claude_credential_with_for_test(
    service: &str,
    all_stdio_are_terminal: bool,
    read_item: impl FnOnce() -> ClaudeKeychainRead,
) -> Result<ClaudeCredentialBootstrapOutcome, ClaudeKeychainPolicyError> {
    bootstrap_claude_credential_with(service, all_stdio_are_terminal, read_item)
}

#[cfg(test)]
mod tests;
