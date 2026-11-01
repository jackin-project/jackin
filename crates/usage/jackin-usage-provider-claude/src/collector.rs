// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use zeroize::Zeroizing;

use jackin_protocol::control::FocusedUsageView;
use jackin_usage_provider_core::{ProviderFailureMetadata, ProviderHttpError, ProviderRateLimit};

use crate::keychain::{ClaudeKeychainRead, read_claude_keychain_item_uncached};

#[cfg(target_os = "macos")]
use super::lease::{MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES, replace_if_exact};
use super::{
    ClaudeCredentialLease, ClaudeOAuthUsageResponse, ClaudeResolved,
    credentials::parse_claude_keychain_profile,
    lease::{begin_unauthorized_reread, cached_payload_for_lease, valid_claude_keychain_service},
};
use super::{spend::fetch_claude_oauth_usage, wave::claude_resolved_view_from_result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeCollectionError {
    ConsentRevoked { provider_http_status: Option<u16> },
}

#[expect(
    clippy::type_complexity,
    reason = "the lease snapshot keeps the focused view beside optional rate-limit and failure metadata"
)]
pub fn experimental_claude_usage_snapshot_for_lease<C, A, M>(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    lease: &ClaudeCredentialLease,
    consent_is_current: C,
    admit_operation: A,
    commit_if_current: M,
) -> Result<
    Option<(
        FocusedUsageView,
        Option<ProviderRateLimit>,
        Option<ProviderFailureMetadata>,
    )>,
    ClaudeCollectionError,
>
where
    C: FnMut() -> bool,
    A: FnMut() -> bool,
    M: FnMut(&mut dyn FnMut() -> bool) -> Option<bool>,
{
    experimental_claude_usage_snapshot_for_lease_with(
        agent,
        provider,
        now,
        lease,
        fetch_claude_oauth_usage,
        read_claude_keychain_item_uncached,
        consent_is_current,
        admit_operation,
        commit_if_current,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Consent, admission, and commit are separate per-operation callbacks."
)]
#[expect(
    clippy::type_complexity,
    reason = "the lease snapshot keeps the focused view beside optional rate-limit and failure metadata"
)]
fn experimental_claude_usage_snapshot_for_lease_with<F, R, C, A, M>(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    lease: &ClaudeCredentialLease,
    fetch: F,
    reread: R,
    mut consent_is_current: C,
    admit_operation: A,
    commit_if_current: M,
) -> Result<
    Option<(
        FocusedUsageView,
        Option<ProviderRateLimit>,
        Option<ProviderFailureMetadata>,
    )>,
    ClaudeCollectionError,
>
where
    F: FnMut(&str) -> Result<ClaudeOAuthUsageResponse, ProviderHttpError>,
    R: FnOnce(&str) -> ClaudeKeychainRead,
    C: FnMut() -> bool,
    A: FnMut() -> bool,
    M: FnMut(&mut dyn FnMut() -> bool) -> Option<bool>,
{
    if !consent_is_current() {
        return Err(ClaudeCollectionError::ConsentRevoked {
            provider_http_status: None,
        });
    }
    if !valid_claude_keychain_service(lease.service()) {
        return Ok(None);
    }
    let Some(payload) = cached_payload_for_lease(lease) else {
        return Ok(None);
    };
    let Some(profile) = parse_claude_keychain_profile(payload.as_bytes()) else {
        return Ok(None);
    };
    let Some(credential) = profile.credential else {
        return Ok(None);
    };
    let mut resolved = ClaudeResolved::from_oauth_credentials(
        credential,
        profile.account_email,
        profile.organization_type,
        "OAuth · macOS Keychain".to_owned(),
        false,
    );
    let result = fetch_claude_with_one_401_reread(
        lease,
        &mut resolved,
        payload,
        fetch,
        reread,
        &mut consent_is_current,
        admit_operation,
        commit_if_current,
    );
    let result = match result {
        Ok(response) => Ok(response),
        Err(ClaudeFetchError::Provider(error)) => Err(error),
        Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status,
        }) => {
            return Err(ClaudeCollectionError::ConsentRevoked {
                provider_http_status,
            });
        }
    };
    let provider_http_status = match &result {
        Err(ProviderHttpError::HttpStatus { status, .. }) => Some(*status),
        Ok(_) | Err(_) => None,
    };
    let view = claude_resolved_view_from_result(agent, provider, now, resolved, result);
    if !consent_is_current() {
        return Err(ClaudeCollectionError::ConsentRevoked {
            provider_http_status,
        });
    }
    Ok(Some(view))
}

#[derive(Debug)]
enum ClaudeFetchError {
    Provider(ProviderHttpError),
    ConsentRevoked { provider_http_status: Option<u16> },
}

#[expect(
    clippy::too_many_arguments,
    reason = "Consent, admission, and commit are separate per-operation callbacks."
)]
fn fetch_claude_with_one_401_reread<F, R, C, A, M>(
    lease: &ClaudeCredentialLease,
    resolved: &mut ClaudeResolved,
    original_payload: Zeroizing<String>,
    mut fetch: F,
    reread: R,
    consent_is_current: &mut C,
    mut admit_operation: A,
    commit_if_current: M,
) -> Result<ClaudeOAuthUsageResponse, ClaudeFetchError>
where
    F: FnMut(&str) -> Result<ClaudeOAuthUsageResponse, ProviderHttpError>,
    R: FnOnce(&str) -> ClaudeKeychainRead,
    C: FnMut() -> bool,
    A: FnMut() -> bool,
    M: FnMut(&mut dyn FnMut() -> bool) -> Option<bool>,
{
    if !consent_is_current() {
        return Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: None,
        });
    }
    if !admit_operation() {
        return Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: None,
        });
    }
    let first = fetch(resolved.access_token()).map_err(ClaudeFetchError::Provider);
    if !matches!(
        &first,
        Err(ClaudeFetchError::Provider(ProviderHttpError::HttpStatus {
            status: 401,
            ..
        }))
    ) {
        if !consent_is_current() {
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: provider_http_status(&first),
            });
        }
        return first;
    }
    if !consent_is_current() {
        return Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401),
        });
    }
    if !admit_operation() {
        return Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401),
        });
    }
    if !begin_unauthorized_reread(lease) {
        return first_401_if_consent_current(first, consent_is_current);
    }
    let keychain_read = reread(lease.service());
    if !consent_is_current() {
        return Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401),
        });
    }
    #[cfg(target_os = "macos")]
    let Some(json) = payload_from_keychain_read(keychain_read) else {
        return first_401_if_consent_current(first, consent_is_current);
    };
    #[cfg(not(target_os = "macos"))]
    {
        drop((keychain_read, original_payload, commit_if_current));
        first_401_if_consent_current(first, consent_is_current)
    }
    #[cfg(target_os = "macos")]
    {
        let mut commit_if_current = commit_if_current;
        if json.len() > MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES {
            return first_401_if_consent_current(first, consent_is_current);
        }
        let Some(profile) = parse_claude_keychain_profile(json.as_bytes()) else {
            return first_401_if_consent_current(first, consent_is_current);
        };
        let Some(credential) = profile.credential else {
            return first_401_if_consent_current(first, consent_is_current);
        };
        if resolved
            .account_email
            .as_deref()
            .zip(profile.account_email.as_deref())
            .is_some_and(|(old, new)| old != new)
            || credential.access_token.as_str() == resolved.access_token()
        {
            return first_401_if_consent_current(first, consent_is_current);
        }
        if !consent_is_current() {
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: Some(401),
            });
        }
        if !admit_operation() {
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: Some(401),
            });
        }
        let access_token = credential.access_token;
        resolved.replace_access_token(access_token.clone());
        let retried = fetch(resolved.access_token()).map_err(ClaudeFetchError::Provider);
        if !consent_is_current() {
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: provider_http_status(&retried),
            });
        }
        let mut payload = Some(json);
        let mut replace_payload = || {
            payload
                .take()
                .is_some_and(|payload| replace_if_exact(lease, payload))
        };
        let Some(cache_updated) = commit_if_current(&mut replace_payload) else {
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: provider_http_status(&retried),
            });
        };
        if !consent_is_current() {
            if cache_updated {
                let _restored = replace_if_exact(lease, original_payload);
            }
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: provider_http_status(&retried),
            });
        }
        if !cache_updated {
            return retried;
        }
        retried
    }
}

fn provider_http_status(
    result: &Result<ClaudeOAuthUsageResponse, ClaudeFetchError>,
) -> Option<u16> {
    match result {
        Err(ClaudeFetchError::Provider(ProviderHttpError::HttpStatus { status, .. })) => {
            Some(*status)
        }
        Ok(_) | Err(ClaudeFetchError::Provider(_) | ClaudeFetchError::ConsentRevoked { .. }) => {
            None
        }
    }
}

fn first_401_if_consent_current<C>(
    first: Result<ClaudeOAuthUsageResponse, ClaudeFetchError>,
    consent_is_current: &mut C,
) -> Result<ClaudeOAuthUsageResponse, ClaudeFetchError>
where
    C: FnMut() -> bool,
{
    if consent_is_current() {
        first
    } else {
        Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401),
        })
    }
}

#[cfg(target_os = "macos")]
fn payload_from_keychain_read(read: ClaudeKeychainRead) -> Option<Zeroizing<String>> {
    match read {
        ClaudeKeychainRead::Payload { json } => Some(json),
        ClaudeKeychainRead::Denied
        | ClaudeKeychainRead::Missing
        | ClaudeKeychainRead::AccessUnavailable => None,
    }
}
