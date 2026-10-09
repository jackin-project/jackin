// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::dispatch::retry_catalog_revision_conflict;

#[test]
fn broker_catalog_conflict_retries_run_a_fresh_attempt_before_success() {
    let attempts = AtomicUsize::new(0);
    let published = retry_catalog_revision_conflict(|| {
        let attempt = attempts.fetch_add(1, Ordering::SeqCst);
        if attempt == 0 {
            Err(UsageCoordinationError {
                kind: UsageCoordinationErrorKind::CatalogRevisionConflict,
                message: "scripted stale publication lease".to_owned(),
            })
        } else {
            Ok(attempt)
        }
    })
    .expect("fresh attempt should publish after a CAS conflict");

    assert_eq!(published, 1);
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        2,
        "each retry invokes the broker-owned discover-and-publish attempt again"
    );
}

#[test]
fn repeated_broker_catalog_conflicts_stop_at_the_configured_attempt_limit() {
    let attempts = AtomicUsize::new(0);
    let error = retry_catalog_revision_conflict::<()>(|| {
        attempts.fetch_add(1, Ordering::SeqCst);
        Err(UsageCoordinationError {
            kind: UsageCoordinationErrorKind::CatalogRevisionConflict,
            message: "scripted stale publication lease".to_owned(),
        })
    })
    .expect_err("bounded CAS retries must fail closed");

    assert_eq!(
        error.kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        BROKER_ACTIVATION_ATTEMPTS as usize
    );
}

#[test]
fn broker_catalog_retry_does_not_retry_non_conflict_failures() {
    let attempts = AtomicUsize::new(0);
    let error = retry_catalog_revision_conflict::<()>(|| {
        attempts.fetch_add(1, Ordering::SeqCst);
        Err(UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message: "scripted discovery failure".to_owned(),
        })
    })
    .expect_err("non-conflict failure must be propagated");

    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}
