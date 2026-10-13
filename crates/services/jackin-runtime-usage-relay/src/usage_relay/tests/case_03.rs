// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn usage_relay_stdio_dispatch_scopes_exact_capability() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete;
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().join("data")),
        broker_executor,
    )
    .unwrap();
    let allowed = capability("allowed");
    let allowlist = UsageCapabilitySet::new([allowed.clone()]);

    let denied_response = dispatch(
        UsageBrokerOperation::Refresh {
            capability: capability("denied"),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        allowlist.clone(),
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied_response else {
        panic!("denied capability returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    let denied_capability = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            capability: capability("denied"),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        allowlist.clone(),
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied_capability else {
        panic!("denied capability returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    let refresh = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            capability: allowed.clone(),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        allowlist.clone(),
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::State { state } = refresh else {
        panic!("allowed capability returned error");
    };
    let terminal = dispatch(
        UsageBrokerOperation::JoinForCapability {
            capability: allowed,
            generation: state.generation,
            timeout_ms: 2_000,
        },
        broker,
        allowlist,
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::State { state } = terminal else {
        panic!("allowed generation join returned error");
    };
    assert_eq!(state.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn usage_relay_dispatch_denies_host_only_operations() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete;
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().join("data")),
        broker_executor,
    )
    .unwrap();
    let allowlist = UsageCapabilitySet::new([capability("allowed")]);

    for operation in [
        UsageBrokerOperation::CurrentProjectionForSurface,
        UsageBrokerOperation::ResolveRelayCapabilities {
            scope_label: "workspace fixture role reviewer".to_owned(),
            forwarded_sources: jackin_protocol::usage_broker::UsageRelayForwardedSourcesV1::default(
            ),
        },
    ] {
        let denied = dispatch(
            operation,
            broker.clone(),
            allowlist.clone(),
            UsageCredentialScope::default(),
        )
        .await;
        let UsageBrokerResponse::Error { error } = denied else {
            panic!("host-only operation returned a result");
        };
        assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn empty_capabilities_do_not_start_a_tunnel_child() {
    let temp = tempfile::tempdir().unwrap();
    let broker = UsageBrokerConfig::for_data_dir(temp.path().join("data")).client();
    let guard = start_apple_tunnel(
        "fixture",
        PreparedUsageRelay {
            broker,
            capabilities: vec![],
            canonical_launch_usage_capabilities: CanonicalLaunchUsageCapabilities::default(),
            credential_scope: UsageCredentialScope::default(),
        },
    )
    .unwrap();
    assert!(guard.task.is_none());
}

#[tokio::test]
async fn s2_relay_dispatch_admits_exactly_abc() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete;
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().join("data")),
        broker_executor,
    )
    .unwrap();
    let allowlist = UsageCapabilitySet::new([
        capability("acc-a"),
        capability("acc-b"),
        capability("acc-c"),
    ]);

    // Forged D across every operation shape: denied, zero provider calls.
    for operation in [
        UsageBrokerOperation::Current {
            capability: capability("acc-d"),
        },
        UsageBrokerOperation::Refresh {
            capability: capability("acc-d"),
            observed_generation: 0,
            force: true,
        },
        UsageBrokerOperation::Join {
            capability: capability("acc-d"),
            generation: 1,
            timeout_ms: 50,
        },
        UsageBrokerOperation::CurrentForCapability {
            capability: capability("acc-d"),
        },
        UsageBrokerOperation::JoinForCapability {
            capability: capability("acc-d"),
            generation: 1,
            timeout_ms: 50,
        },
    ] {
        let denied = dispatch(
            operation,
            broker.clone(),
            allowlist.clone(),
            UsageCredentialScope::default(),
        )
        .await;
        let UsageBrokerResponse::Error { error } = denied else {
            panic!("forged acc-d returned state");
        };
        assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    }
    // Same-surface sibling: same provider surface, non-launched account.
    let denied = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            capability: capability("acc-a-evil"),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        allowlist.clone(),
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied else {
        panic!("same-surface forgery returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    // Empty scope denies even a well-formed, otherwise-known capability.
    let empty = UsageCapabilitySet::new([]);
    let denied = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            capability: capability("acc-a"),
            observed_generation: 0,
            force: true,
        },
        broker.clone(),
        empty,
        UsageCredentialScope::default(),
    )
    .await;
    let UsageBrokerResponse::Error { error } = denied else {
        panic!("empty scope returned state");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);

    // Each admitted account refreshes and completes independently.
    for id in ["acc-a", "acc-b", "acc-c"] {
        let refresh = dispatch(
            UsageBrokerOperation::RefreshForCapability {
                capability: capability(id),
                observed_generation: 0,
                force: true,
            },
            broker.clone(),
            allowlist.clone(),
            UsageCredentialScope::default(),
        )
        .await;
        let UsageBrokerResponse::State { state } = refresh else {
            panic!("admitted {id} returned error");
        };
        let terminal = dispatch(
            UsageBrokerOperation::JoinForCapability {
                capability: capability(id),
                generation: state.generation,
                timeout_ms: 2_000,
            },
            broker.clone(),
            allowlist.clone(),
            UsageCredentialScope::default(),
        )
        .await;
        let UsageBrokerResponse::State { state } = terminal else {
            panic!("admitted {id} join returned error");
        };
        assert_eq!(
            state.phase,
            UsageRefreshPhase::Completed,
            "{id} did not complete"
        );
        assert_eq!(state.capability.account_id, id);
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 3);
}
