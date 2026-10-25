// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::*;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

struct ScopeRecordingExecutor {
    expected: UsageCredentialScope,
    observed: Mutex<Vec<UsageCredentialScope>>,
    calls: AtomicUsize,
}

impl UsageProviderExecutor for ScopeRecordingExecutor {
    fn authorize_credential_scope(
        &self,
        _capability: &UsageAccountCapability,
        scope: &UsageCredentialScope,
    ) -> Result<(), UsageCoordinationError> {
        self.observed.lock().unwrap().push(scope.clone());
        if scope == &self.expected {
            Ok(())
        } else {
            Err(UsageCoordinationError {
                kind: UsageCoordinationErrorKind::Unauthorized,
                message: "saved credential scope did not match the staged launch proof".to_owned(),
            })
        }
    }

    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ProviderProbeOutcome::success(quota_view())
    }
}

fn saved_scope() -> UsageCredentialScope {
    UsageCredentialScope {
        sources: BTreeSet::from([UsageCredentialSourceProof {
            account_id: "z-account".to_owned(),
            surface_id: "claude".to_owned(),
            key: "ANTHROPIC_API_KEY".to_owned(),
            source: UsageCredentialSourceIdentity::HostEnv {
                name: "ANTHROPIC_API_KEY".to_owned(),
            },
            material_fingerprint: usage_credential_material_fingerprint(
                "synthetic-saved-proof-test-material",
            ),
        }]),
    }
}

#[tokio::test]
async fn saved_proof_keeps_capability_order_and_dispatches_with_exact_scope() {
    let temp = tempfile::tempdir().unwrap();
    let scope = saved_scope();
    let capabilities = vec![capability("z-account"), capability("a-account")];
    let executor = Arc::new(ScopeRecordingExecutor {
        expected: scope.clone(),
        observed: Mutex::new(Vec::new()),
        calls: AtomicUsize::new(0),
    });
    let broker_executor: Arc<dyn UsageProviderExecutor> =
        Arc::<ScopeRecordingExecutor>::clone(&executor);
    let broker = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().join("data")),
        broker_executor,
    )
    .unwrap();
    let paths = JackinPaths::resolve_with_env(temp.path(), None, None);
    let ensure_calls = Arc::new(AtomicUsize::new(0));
    let ensure_calls_from_closure = Arc::clone(&ensure_calls);
    let prepared = prepare_for_saved_proof_with_ensure(
        &paths,
        scope.clone(),
        capabilities.clone(),
        move |_paths| {
            ensure_calls_from_closure.fetch_add(1, Ordering::SeqCst);
            Ok(broker.clone())
        },
    )
    .await
    .unwrap();
    assert_eq!(ensure_calls.load(Ordering::SeqCst), 1);

    let (saved_scope, saved_capabilities) = prepared.saved_proof();
    assert_eq!(saved_scope, scope);
    assert_eq!(saved_capabilities, capabilities);

    let allowlist = UsageCapabilitySet::new(saved_capabilities.clone());
    let response = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            capability: saved_capabilities[0].clone(),
            observed_generation: 0,
            force: true,
        },
        prepared.broker.clone(),
        allowlist.clone(),
        saved_scope.clone(),
    )
    .await;
    let UsageBrokerResponse::State { state } = response else {
        panic!("saved capability refresh was not dispatched");
    };
    let terminal = dispatch(
        UsageBrokerOperation::JoinForCapability {
            capability: saved_capabilities[0].clone(),
            generation: state.generation,
            timeout_ms: 2_000,
        },
        prepared.broker.clone(),
        allowlist.clone(),
        saved_scope.clone(),
    )
    .await;
    let UsageBrokerResponse::State { state } = terminal else {
        panic!("saved generation did not complete");
    };
    assert_eq!(state.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let observed_scopes = executor.observed.lock().unwrap().clone();
    assert!(!observed_scopes.is_empty());
    assert!(
        observed_scopes
            .iter()
            .all(|observed| observed == &saved_scope)
    );
    let allowed_authorizations = observed_scopes.len();

    let denied_added_capability = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            capability: capability("new-account"),
            observed_generation: 0,
            force: true,
        },
        prepared.broker.clone(),
        allowlist.clone(),
        saved_scope.clone(),
    )
    .await;
    assert!(matches!(
        denied_added_capability,
        UsageBrokerResponse::Error { error }
            if error.kind == UsageCoordinationErrorKind::Unauthorized
    ));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        executor.observed.lock().unwrap().len(),
        allowed_authorizations
    );

    let changed_scope = changed_scope_material(&saved_scope);
    let denied_changed_source = dispatch(
        UsageBrokerOperation::RefreshForCapability {
            capability: saved_capabilities[0].clone(),
            observed_generation: 0,
            force: true,
        },
        prepared.broker,
        allowlist,
        changed_scope.clone(),
    )
    .await;
    assert!(matches!(
        denied_changed_source,
        UsageBrokerResponse::Error { error }
            if error.kind == UsageCoordinationErrorKind::Unauthorized
    ));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let observed_scopes = executor.observed.lock().unwrap().clone();
    assert_eq!(observed_scopes.len(), allowed_authorizations + 1);
    assert_eq!(observed_scopes.last(), Some(&changed_scope));
}

fn changed_scope_material(scope: &UsageCredentialScope) -> UsageCredentialScope {
    let mut source = scope.sources.iter().next().unwrap().clone();
    source.key = "OTHER_API_KEY".to_owned();
    source.source = UsageCredentialSourceIdentity::HostEnv {
        name: "OTHER_API_KEY".to_owned(),
    };
    source.material_fingerprint =
        usage_credential_material_fingerprint("different-synthetic-material");
    UsageCredentialScope {
        sources: BTreeSet::from([source]),
    }
}

#[tokio::test]
async fn test_layout_saved_proof_is_attach_only_and_releases_unused_lease() {
    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let prepared = prepare_for_saved_proof(&paths, saved_scope(), vec![capability("z-account")])
        .await
        .unwrap();
    assert!(prepared.capabilities.is_empty());
    assert!(prepared.credential_scope.sources.is_empty());
    assert!(!paths.data_dir.exists());

    let (_lock_file, lease, observer) = lock_pair(temp.path());
    let guard = start_apple_tunnel_with_lease("fixture", prepared, lease).unwrap();
    assert!(guard.task.is_none());
    observer.try_lock().unwrap();
    observer.unlock().unwrap();
}

#[tokio::test]
async fn canceled_shutdown_waiter_does_not_release_lease_before_child_reap() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("allow-child-exit");
    let (_lock_file, lease, observer) = lock_pair(temp.path());
    let (controls, reached, proceed) = pending_wait_controls();
    let broker = UsageBrokerConfig::for_data_dir(temp.path().join("broker-data")).client();
    let guard = start_tunnel_process_with_lease_for_test(
        wait_for_marker_request(&marker),
        broker,
        vec![capability("saved-account")],
        saved_scope(),
        lease,
        controls,
    )
    .unwrap();

    wait_for_file(&child_started_marker(&marker)).await;
    let waiter = guard.shutdown_and_wait();
    drop(waiter);
    wait_for_gate(reached).await;
    assert_child_waiting_for_marker(&marker);
    assert_lease_is_busy(&observer);
    fs::write(&marker, b"exit").unwrap();
    wait_for_file(&child_exited_marker(&marker)).await;
    assert_lease_is_busy(&observer);
    proceed.send(()).unwrap();
    wait_until_lease_is_free(&observer).await;
}

#[tokio::test]
async fn dropping_guard_keeps_lease_with_owner_until_live_child_is_reaped() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("allow-child-exit");
    let (_lock_file, lease, observer) = lock_pair(temp.path());
    let (controls, reached, proceed) = pending_wait_controls();
    let broker = UsageBrokerConfig::for_data_dir(temp.path().join("broker-data")).client();
    let guard = start_tunnel_process_with_lease_for_test(
        wait_for_marker_request(&marker),
        broker,
        vec![capability("saved-account")],
        saved_scope(),
        lease,
        controls,
    )
    .unwrap();

    wait_for_file(&child_started_marker(&marker)).await;
    drop(guard);
    wait_for_gate(reached).await;
    assert_child_waiting_for_marker(&marker);
    assert_lease_is_busy(&observer);
    fs::write(&marker, b"exit").unwrap();
    wait_for_file(&child_exited_marker(&marker)).await;
    assert_lease_is_busy(&observer);
    proceed.send(()).unwrap();
    wait_until_lease_is_free(&observer).await;
}

#[tokio::test]
async fn canceling_a_polled_shutdown_waiter_does_not_cancel_cleanup_owner() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("allow-child-exit");
    let (_lock_file, lease, observer) = lock_pair(temp.path());
    let (controls, reached, proceed) = pending_wait_controls();
    let broker = UsageBrokerConfig::for_data_dir(temp.path().join("broker-data")).client();
    let guard = start_tunnel_process_with_lease_for_test(
        wait_for_marker_request(&marker),
        broker,
        vec![capability("saved-account")],
        saved_scope(),
        lease,
        controls,
    )
    .unwrap();
    wait_for_file(&child_started_marker(&marker)).await;
    let (polled_tx, polled_rx) = oneshot::channel();
    let mut completion = Box::pin(guard.shutdown_and_wait());
    let mut polled_tx = Some(polled_tx);
    let waiter = tokio::spawn(std::future::poll_fn(move |context| {
        let result = completion.as_mut().poll(context);
        if let Some(polled) = polled_tx.take().filter(|_| result.is_pending()) {
            let _sent = polled.send(());
        }
        result
    }));
    wait_for_gate(reached).await;
    polled_rx.await.unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert_child_waiting_for_marker(&marker);
    assert_lease_is_busy(&observer);

    fs::write(&marker, b"exit").unwrap();
    wait_for_file(&child_exited_marker(&marker)).await;
    assert_lease_is_busy(&observer);
    proceed.send(()).unwrap();
    wait_until_lease_is_free(&observer).await;
}

#[tokio::test]
async fn spawn_failure_releases_lease_before_any_child_exists() {
    let temp = tempfile::tempdir().unwrap();
    let (_lock_file, lease, observer) = lock_pair(temp.path());
    let request = jackin_process::ExecRequest::new(
        temp.path().join("missing-command"),
        std::iter::empty::<std::ffi::OsString>(),
    )
    .stdin_mode(jackin_process::StdioMode::Capture)
    .stdout_mode(jackin_process::StdioMode::Capture)
    .stderr_mode(jackin_process::StdioMode::Inherit);
    let broker = UsageBrokerConfig::for_data_dir(temp.path().join("broker-data")).client();

    let Err(_error) = start_tunnel_process_with_lease_for_test(
        request,
        broker,
        vec![capability("saved-account")],
        saved_scope(),
        lease,
        LeaseTunnelTestControls::default(),
    ) else {
        panic!("missing executable unexpectedly spawned");
    };
    observer.try_lock().unwrap();
    observer.unlock().unwrap();
}

#[tokio::test]
async fn wait_error_reports_unconfirmed_but_keeps_lease_until_a_later_wait_succeeds() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("allow-child-exit");
    let (_lock_file, lease, observer) = lock_pair(temp.path());
    let (reached_tx, reached_rx) = oneshot::channel();
    let (proceed_tx, proceed_rx) = oneshot::channel();
    let controls = LeaseTunnelTestControls {
        faults: BTreeSet::from([LeaseTunnelTestFault::FailFirstWait]),
        after_wait_error_gate: Some(LeaseTunnelTestGate {
            reached: Some(reached_tx),
            proceed: Some(proceed_rx),
        }),
        ..LeaseTunnelTestControls::default()
    };
    let broker = UsageBrokerConfig::for_data_dir(temp.path().join("broker-data")).client();
    let guard = start_tunnel_process_with_lease_for_test(
        wait_for_marker_request(&marker),
        broker,
        vec![capability("saved-account")],
        saved_scope(),
        lease,
        controls,
    )
    .unwrap();
    wait_for_file(&child_started_marker(&marker)).await;

    let wait_result = guard.shutdown_and_wait();
    let waiter = tokio::spawn(wait_result);
    tokio::time::timeout(std::time::Duration::from_secs(3), reached_rx)
        .await
        .unwrap()
        .unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
        .await
        .unwrap()
        .unwrap();
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("LeaseCleanupUnconfirmed")
    );
    assert_child_waiting_for_marker(&marker);
    assert_lease_is_busy(&observer);

    fs::write(&marker, b"exit").unwrap();
    wait_for_file(&child_exited_marker(&marker)).await;
    assert_lease_is_busy(&observer);
    proceed_tx.send(()).unwrap();
    wait_until_lease_is_free(&observer).await;
}

#[tokio::test]
async fn kill_error_is_not_unconfirmed_when_wait_later_proves_reap() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("allow-child-exit");
    let (_lock_file, lease, observer) = lock_pair(temp.path());
    let (mut controls, reached, proceed) = pending_wait_controls();
    controls.faults = BTreeSet::from([
        LeaseTunnelTestFault::ForceWaitTimeout,
        LeaseTunnelTestFault::FailStartKillOnce,
    ]);
    let broker = UsageBrokerConfig::for_data_dir(temp.path().join("broker-data")).client();
    let guard = start_tunnel_process_with_lease_for_test(
        wait_for_marker_request(&marker),
        broker,
        vec![capability("saved-account")],
        saved_scope(),
        lease,
        controls,
    )
    .unwrap();
    wait_for_file(&child_started_marker(&marker)).await;

    let waiter = tokio::spawn(guard.shutdown_and_wait());
    wait_for_gate(reached).await;
    assert_child_waiting_for_marker(&marker);
    assert_lease_is_busy(&observer);
    fs::write(&marker, b"exit").unwrap();
    wait_for_file(&child_exited_marker(&marker)).await;
    assert_lease_is_busy(&observer);
    proceed.send(()).unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(3), waiter)
        .await
        .unwrap()
        .unwrap();
    match result {
        Ok(()) => {}
        Err(error) => panic!("a later successful wait should confirm reaping: {error}"),
    }
    wait_until_lease_is_free(&observer).await;
}

#[tokio::test]
async fn successful_kill_followed_by_wait_error_retains_lease_until_wait_confirms_reap() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("allow-child-exit");
    let (_lock_file, lease, observer) = lock_pair(temp.path());
    let (reached_tx, reached_rx) = oneshot::channel();
    let (proceed_tx, proceed_rx) = oneshot::channel();
    let controls = LeaseTunnelTestControls {
        faults: BTreeSet::from([
            LeaseTunnelTestFault::ForceWaitTimeout,
            LeaseTunnelTestFault::FailFirstWait,
        ]),
        after_wait_error_gate: Some(LeaseTunnelTestGate {
            reached: Some(reached_tx),
            proceed: Some(proceed_rx),
        }),
        ..LeaseTunnelTestControls::default()
    };
    let broker = UsageBrokerConfig::for_data_dir(temp.path().join("broker-data")).client();
    let guard = start_tunnel_process_with_lease_for_test(
        wait_for_marker_request(&marker),
        broker,
        vec![capability("saved-account")],
        saved_scope(),
        lease,
        controls,
    )
    .unwrap();
    wait_for_file(&child_started_marker(&marker)).await;

    let waiter = tokio::spawn(guard.shutdown_and_wait());
    tokio::time::timeout(std::time::Duration::from_secs(3), reached_rx)
        .await
        .unwrap()
        .unwrap();
    assert_lease_is_busy(&observer);
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
        .await
        .unwrap()
        .unwrap();
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("LeaseCleanupUnconfirmed")
    );
    assert_lease_is_busy(&observer);

    proceed_tx.send(()).unwrap();
    wait_until_lease_is_free(&observer).await;
}

#[tokio::test]
async fn post_spawn_pipe_error_keeps_lease_with_cleanup_owner_until_reap() {
    let temp = tempfile::tempdir().unwrap();
    let marker = temp.path().join("allow-child-exit");
    let (_lock_file, lease, observer) = lock_pair(temp.path());
    let (reached_tx, reached_rx) = oneshot::channel();
    let (proceed_tx, proceed_rx) = oneshot::channel();
    let controls = LeaseTunnelTestControls {
        faults: BTreeSet::from([LeaseTunnelTestFault::MissingStdout]),
        cleanup_gate: Some(LeaseTunnelTestGate {
            reached: Some(reached_tx),
            proceed: Some(proceed_rx),
        }),
        ..LeaseTunnelTestControls::default()
    };
    let broker = UsageBrokerConfig::for_data_dir(temp.path().join("broker-data")).client();
    let result = start_tunnel_process_with_lease_for_test(
        wait_for_marker_request(&marker),
        broker,
        vec![capability("saved-account")],
        saved_scope(),
        lease,
        controls,
    );
    let Err(_error) = result else {
        panic!("missing child stdout unexpectedly succeeded");
    };
    wait_for_file(&child_started_marker(&marker)).await;
    tokio::time::timeout(std::time::Duration::from_secs(3), reached_rx)
        .await
        .unwrap()
        .unwrap();
    assert_child_waiting_for_marker(&marker);
    assert_lease_is_busy(&observer);

    fs::write(&marker, b"exit").unwrap();
    wait_for_file(&child_exited_marker(&marker)).await;
    assert_lease_is_busy(&observer);
    proceed_tx.send(()).unwrap();
    wait_until_lease_is_free(&observer).await;
}

fn wait_for_marker_request(marker: &Path) -> jackin_process::ExecRequest {
    let started = child_started_marker(marker);
    let exited = child_exited_marker(marker);
    let args = vec![
        "-c".into(),
        "printf started > \"$1\"; while [ ! -e \"$2\" ]; do sleep 0.01; done; printf exited > \"$3\"".into(),
        "usage-relay-test".into(),
        started.as_os_str().to_owned(),
        marker.as_os_str().to_owned(),
        exited.as_os_str().to_owned(),
    ];
    jackin_process::ExecRequest::new("sh", args)
        .stdin_mode(jackin_process::StdioMode::Capture)
        .stdout_mode(jackin_process::StdioMode::Capture)
        .stderr_mode(jackin_process::StdioMode::Inherit)
}

fn child_started_marker(marker: &Path) -> PathBuf {
    marker.with_extension("started")
}

fn child_exited_marker(marker: &Path) -> PathBuf {
    marker.with_extension("exited")
}

fn assert_child_waiting_for_marker(marker: &Path) {
    assert!(child_started_marker(marker).exists());
    assert!(!child_exited_marker(marker).exists());
}

async fn wait_for_file(path: &Path) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while !path.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("child marker was not observed before timeout");
}

fn lock_pair(directory: &Path) -> (tempfile::NamedTempFile, File, File) {
    let lock_file = tempfile::NamedTempFile::new_in(directory).unwrap();
    let lease = lock_file.reopen().unwrap();
    lease.try_lock().unwrap();
    let observer = lock_file.reopen().unwrap();
    (lock_file, lease, observer)
}

fn pending_wait_controls() -> (
    LeaseTunnelTestControls,
    oneshot::Receiver<()>,
    oneshot::Sender<()>,
) {
    let (reached_tx, reached_rx) = oneshot::channel();
    let (proceed_tx, proceed_rx) = oneshot::channel();
    (
        LeaseTunnelTestControls {
            wait_pending_gate: Some(LeaseTunnelTestGate {
                reached: Some(reached_tx),
                proceed: Some(proceed_rx),
            }),
            ..LeaseTunnelTestControls::default()
        },
        reached_rx,
        proceed_tx,
    )
}

async fn wait_for_gate(reached: oneshot::Receiver<()>) {
    tokio::time::timeout(std::time::Duration::from_secs(3), reached)
        .await
        .expect("cleanup owner did not reach its test gate")
        .expect("cleanup owner stopped before reaching its test gate");
}

fn assert_lease_is_busy(observer: &File) {
    assert!(matches!(
        observer.try_lock(),
        Err(fs::TryLockError::WouldBlock)
    ));
}

async fn wait_until_lease_is_free(observer: &File) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match observer.try_lock() {
                Ok(()) => {
                    observer.unlock().unwrap();
                    return;
                }
                Err(fs::TryLockError::WouldBlock) => {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                Err(fs::TryLockError::Error(error)) => {
                    panic!("checking relay lock failed: {error}")
                }
            }
        }
    })
    .await
    .expect("relay cleanup owner did not release the confirmed lease");
}
