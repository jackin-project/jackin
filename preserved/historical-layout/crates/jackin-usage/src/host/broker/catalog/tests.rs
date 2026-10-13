// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

fn empty_discovery(revision: &str) -> ValidatedUsageDiscovery {
    ValidatedUsageDiscovery {
        config_generation: Some(revision.to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: Vec::new(),
    }
}

fn populated_discovery(revision: &str) -> ValidatedUsageDiscovery {
    use crate::host::discovery::{ValidatedCredentialBinding, ValidatedCredentialSource};
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

    ValidatedUsageDiscovery {
        config_generation: Some(revision.to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![ValidatedCredentialBinding {
            surface: HostSurfaceId::Claude,
            identity: Some(CanonicalAccountIdentity {
                surface: HostSurfaceId::Claude,
                subject: CanonicalAccountSubject::ProviderStableHandle(
                    "fixture-account".to_owned(),
                ),
            }),
            capability_id: "fixture-capability".to_owned(),
            credential_revision: "fixture-credential".to_owned(),
            provenance: BTreeSet::default(),
            source: ValidatedCredentialSource::Capability,
        }],
    }
}

#[test]
fn transient_empty_scan_uses_a_fresh_confirmation_scan() {
    let scans = AtomicUsize::new(0);
    let mut results = VecDeque::from([
        empty_discovery("transient-empty"),
        populated_discovery("confirmed-populated"),
    ]);

    let discovery = discover_confirming_empty(
        || -> Result<ValidatedUsageDiscovery, UsageCoordinationError> {
            scans.fetch_add(1, Ordering::SeqCst);
            Ok(results.pop_front().expect("scripted scan"))
        },
    )
    .unwrap();

    assert_eq!(scans.load(Ordering::SeqCst), 2);
    assert_eq!(
        discovery.config_generation.as_deref(),
        Some("confirmed-populated")
    );
    assert_eq!(super::super::usage_catalog_entries(&discovery).len(), 1);
}

#[test]
fn guard_failure_prevents_catalog_scan() {
    let scans = AtomicUsize::new(0);
    let error = with_unattended_guard::<(), ()>(
        || Err(crate::usage::ClaudeKeychainPolicyError::StateUnavailable),
        || {
            scans.fetch_add(1, Ordering::SeqCst);
            Err(discovery_unavailable())
        },
    )
    .unwrap_err();

    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(scans.load(Ordering::SeqCst), 0);
}

#[test]
fn injected_guard_allows_catalog_scan_without_native_keychain_access() {
    struct FakeGuard;

    let scans = AtomicUsize::new(0);
    let _error = with_unattended_guard::<FakeGuard, ()>(
        || Ok(FakeGuard),
        || {
            scans.fetch_add(1, Ordering::SeqCst);
            Err(discovery_unavailable())
        },
    )
    .unwrap_err();

    assert_eq!(scans.load(Ordering::SeqCst), 1);
}
