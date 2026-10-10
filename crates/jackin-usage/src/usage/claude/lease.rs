// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Process-local Claude credential lease for explicit operator bootstrap and
//! the experimental usage collector.

use std::sync::{Mutex, OnceLock, atomic::AtomicU64};

#[cfg(any(target_os = "macos", test))]
use zeroize::Zeroize;
use zeroize::Zeroizing;

use super::keychain::{
    ClaudeKeychainPolicyError, ClaudeKeychainRead, prepare_claude_keychain_auth,
};

/// Secret-free result of an explicit Claude Keychain bootstrap.
#[derive(Debug, PartialEq, Eq)]
pub enum ClaudeCredentialBootstrapOutcome {
    /// No matching Keychain payload was found.
    Missing,
    /// The operator denied access to the selected Keychain item.
    Denied,
    /// Operator interaction is required, including a missing all-stdio TTY.
    InteractionRequired,
    /// The selected Keychain payload was not valid bounded credential data.
    Malformed,
    /// The exact selected service is retained until this handle is dropped.
    Acquired(ClaudeCredentialLease),
}

struct CachedClaudeCredential {
    service: String,
    json: Zeroizing<String>,
    generation: u64,
    unauthorized_reread_attempted: bool,
}

/// Secret-free owner handle for the selected foreground Claude credential.
/// Its drop clears and zeroizes the matching process cache entry.
#[must_use = "hold the lease for the full foreground service lifetime"]
#[derive(PartialEq, Eq)]
pub struct ClaudeCredentialLease {
    service: String,
    generation: u64,
}

impl ClaudeCredentialLease {
    /// Exact Keychain service selected for this process-local lease.
    #[must_use]
    pub fn service(&self) -> &str {
        &self.service
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
}

impl std::fmt::Debug for ClaudeCredentialLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ClaudeCredentialLease(REDACTED)")
    }
}

impl Drop for ClaudeCredentialLease {
    fn drop(&mut self) {
        cached_credential().clear_generation(self.generation);
    }
}

#[derive(Default)]
struct ClaudeCredentialCache {
    credential: Mutex<Option<CachedClaudeCredential>>,
}

impl ClaudeCredentialCache {
    fn payload(&self, service: &str) -> Option<Zeroizing<String>> {
        self.credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|cached| cached.service == service)
            .map(|cached| cached.json.clone())
    }

    fn service(&self) -> Option<String> {
        self.credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|cached| cached.service.clone())
    }

    #[cfg(test)]
    fn generation_for(&self, service: &str) -> Option<u64> {
        self.credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .filter(|cached| cached.service == service)
            .map(|cached| cached.generation)
    }

    fn contains_generation(&self, service: &str, generation: u64) -> bool {
        self.credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|cached| cached.service == service && cached.generation == generation)
    }

    fn store(&self, service: String, json: Zeroizing<String>, generation: u64) {
        *self
            .credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(CachedClaudeCredential {
            service,
            json,
            generation,
            unauthorized_reread_attempted: false,
        });
    }

    fn begin_unauthorized_reread(&self, service: &str, generation: u64) -> bool {
        let mut credential = self
            .credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let cached = credential
            .as_mut()
            .filter(|cached| cached.service == service && cached.generation == generation);
        let Some(cached) = cached else {
            return false;
        };
        if cached.unauthorized_reread_attempted {
            return false;
        }
        cached.unauthorized_reread_attempted = true;
        true
    }

    fn replace_if_exact(&self, service: &str, generation: u64, json: Zeroizing<String>) -> bool {
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
        cached.json = json;
        true
    }

    fn clear(&self) {
        *self
            .credential
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
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
}

fn cached_credential() -> &'static ClaudeCredentialCache {
    static CACHE: OnceLock<ClaudeCredentialCache> = OnceLock::new();
    CACHE.get_or_init(ClaudeCredentialCache::default)
}

fn bootstrap_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn next_generation() -> u64 {
    static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
    NEXT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub(crate) const MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES: usize = 64 * 1024;
const MAX_CLAUDE_KEYCHAIN_SERVICE_BYTES: usize = 512;

pub(crate) fn valid_claude_keychain_service(service: &str) -> bool {
    !service.trim().is_empty()
        && service.len() <= MAX_CLAUDE_KEYCHAIN_SERVICE_BYTES
        && !service.contains('\0')
}

fn valid_claude_keychain_payload(json: &str) -> bool {
    json.len() <= MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES
        && super::parse_claude_profile_payload(json.as_bytes())
            .is_some_and(|profile| profile.credential.is_some())
}

/// Read one exact Keychain service from an attached operator terminal and
/// retain its payload only in a bounded, process-local zeroizing cache.
///
/// Call this in the foreground broker process before establishing its
/// lifetime unattended Keychain guard. It never returns or formats credential
/// material, and it does not authorize provider collection by itself.
pub fn bootstrap_claude_credential(
    service: &str,
) -> Result<ClaudeCredentialBootstrapOutcome, ClaudeKeychainPolicyError> {
    // Check all three streams before even inspecting or reusing the process
    // cache. Bootstrap always represents a deliberate attached-operator act.
    if !super::keychain::all_stdio_are_terminal() {
        return Ok(ClaudeCredentialBootstrapOutcome::InteractionRequired);
    }
    if !valid_claude_keychain_service(service) {
        return Err(ClaudeKeychainPolicyError::InvalidService);
    }

    let _bootstrap = bootstrap_lock()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    {
        if cached_credential().service().is_some() {
            return Err(ClaudeKeychainPolicyError::ScopeConflict);
        }
    }

    match prepare_claude_keychain_auth(service) {
        #[cfg(any(target_os = "macos", test))]
        ClaudeKeychainRead::Payload { mut json } => {
            if !valid_claude_keychain_payload(&json) {
                json.zeroize();
                return Ok(ClaudeCredentialBootstrapOutcome::Malformed);
            }
            let generation = next_generation();
            cached_credential().store(service.to_owned(), json, generation);
            Ok(ClaudeCredentialBootstrapOutcome::Acquired(
                ClaudeCredentialLease {
                    service: service.to_owned(),
                    generation,
                },
            ))
        }
        ClaudeKeychainRead::Missing => Ok(ClaudeCredentialBootstrapOutcome::Missing),
        ClaudeKeychainRead::Denied => Ok(ClaudeCredentialBootstrapOutcome::Denied),
        ClaudeKeychainRead::ConsentRequired => {
            Ok(ClaudeCredentialBootstrapOutcome::InteractionRequired)
        }
    }
}

/// Exact-source cache lookup. Returned secret material remains zeroizing and
/// is never exposed through a `Debug` or `Display` implementation.
pub(crate) fn cached_claude_keychain_payload(service: &str) -> Option<Zeroizing<String>> {
    cached_credential().payload(service)
}

/// The selected bootstrap source, if this process has one.
pub(crate) fn bootstrapped_claude_service() -> Option<String> {
    cached_credential().service()
}

/// Whether `service` belongs to the one selected process-local lease.
pub(crate) fn claude_service_is_bootstrapped(service: &str) -> bool {
    cached_credential()
        .service()
        .is_some_and(|cached| cached == service)
}

#[cfg(test)]
pub(crate) fn claude_credential_generation(service: &str) -> Option<u64> {
    cached_credential().generation_for(service)
}

pub(crate) fn claude_credential_generation_is_current(service: &str, generation: u64) -> bool {
    cached_credential().contains_generation(service, generation)
}

pub(crate) fn revoke_bootstrapped_claude_generation(generation: u64) {
    cached_credential().clear_generation(generation);
}

/// Replace payload only for the already selected exact service. Used by the
/// one bounded noninteractive reread after HTTP 401.
pub(crate) fn replace_bootstrapped_claude_payload(
    service: &str,
    generation: u64,
    json: Zeroizing<String>,
) -> bool {
    cached_credential().replace_if_exact(service, generation, json)
}

/// Claim the one process-lease 401 reread. Concurrent or later callers cannot
/// start another Keychain read, even if the first read failed or found no
/// changed credential.
pub(crate) fn begin_bootstrapped_claude_401_reread(service: &str, generation: u64) -> bool {
    cached_credential().begin_unauthorized_reread(service, generation)
}

#[cfg(test)]
pub(crate) fn clear_bootstrapped_claude_credential() {
    cached_credential().clear();
}

#[cfg(test)]
mod tests;
