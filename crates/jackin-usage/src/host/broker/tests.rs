// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex, mpsc};
use std::thread;

use crate::coordinator::{
    AccountStateEnvelope, AccountStateStore, ClockSample, FileAccountStateStore,
    FileProjectionStateStore, MonotonicClock, ProjectionAlias, ProjectionStateEnvelope,
    ProviderProbeOutcome, UsageCoordinator, UsageCoordinatorConfig, UsageProviderExecutor,
};
use crate::host::discovery::{ProviderCredentialSourceMaterial, ValidatedCredentialSource};
use crate::host::{HostSurfaceId, OpaqueCredentialHandle};
use jackin_config::AppConfig;
use jackin_core::{UsageCredentialEnvName, WorkspaceName};
use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};
use jackin_protocol::usage_broker::{
    USAGE_BROKER_PROTOCOL_VERSION, UsageAccountV1, UsageBrokerOperation, UsageBrokerRequest,
    UsageBrokerResponse, UsageCatalogEntry, UsageCredentialScope, UsageCredentialSourceIdentity,
    UsageCredentialSourceProof, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIdentityKindV1,
    UsageLifecycleV1, UsageLimitWindowV1, UsageMembershipStateV1, UsagePercent,
    UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1, UsageProviderV1,
    UsageQuotaStateV1, UsageRefreshPhase, UsageRelayForwardedSourcesV1, UsageUnresolvedV1,
    UsageWindowCategoryV1, usage_credential_material_fingerprint,
};
use jackin_protocol::usage_monitor::{
    MonitorAccountBinding, MonitorAccountBindingInput, MonitorConfig, MonitorOperation,
    MonitorProvider, MonitorPurpose, MonitorReply, MonitorScope,
};
use zeroize::Zeroizing;

use super::*;
use crate::host::{ForwardedUsageAccount, ProviderCredentialEnvResolution};

struct CountingExecutor {
    calls: AtomicUsize,
}

struct RetryRecordingResolver {
    manual_retries: Arc<AtomicUsize>,
}

impl Default for RetryRecordingResolver {
    fn default() -> Self {
        Self {
            manual_retries: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl ProviderCredentialEnvResolver for RetryRecordingResolver {
    fn begin_manual_retry(&self) {
        self.manual_retries.fetch_add(1, Ordering::SeqCst);
    }

    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        _keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        Vec::new()
    }
}

struct NoopCredentialResolver;

impl ProviderCredentialEnvResolver for NoopCredentialResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        _keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        Vec::new()
    }
}

#[derive(Default)]
struct CountingCredentialResolver {
    calls: AtomicUsize,
}

impl ProviderCredentialEnvResolver for CountingCredentialResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        _keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Vec::new()
    }
}

impl UsageProviderExecutor for CountingExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ProviderProbeOutcome::success(quota_view())
    }
}

struct FakeBrokerClock {
    sample: Mutex<ClockSample>,
}

impl FakeBrokerClock {
    fn new(epoch: i64) -> Self {
        Self {
            sample: Mutex::new(ClockSample::anchored(epoch, Duration::ZERO)),
        }
    }

    fn set_epoch(&self, epoch: i64) {
        self.sample.lock().unwrap().wall_epoch =
            Duration::from_secs(u64::try_from(epoch.max(0)).unwrap_or(u64::MAX));
    }

    fn epoch(&self) -> i64 {
        self.sample.lock().unwrap().floor_epoch()
    }

    fn advance(&self, duration: Duration) {
        let mut sample = self.sample.lock().unwrap();
        sample.monotonic = sample.monotonic.saturating_add(duration);
        sample.wall_epoch = sample.wall_epoch.saturating_add(duration);
    }
}

impl MonotonicClock for FakeBrokerClock {
    fn now(&self) -> Duration {
        self.sample.lock().unwrap().monotonic
    }

    fn sample(&self, _fallback_epoch: i64) -> ClockSample {
        *self.sample.lock().unwrap()
    }
}

/// One-request localhost server for exercising the real shared bearer HTTP
/// parser without capturing request headers or contacting a provider.
struct Fake429Server {
    url: String,
    request_started: mpsc::Receiver<Result<(), String>>,
    release_response: mpsc::Sender<()>,
    request_count: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}

struct Fake429ResponseGuard(Option<mpsc::Sender<()>>);

impl Fake429ResponseGuard {
    fn new(release_response: mpsc::Sender<()>) -> Self {
        Self(Some(release_response))
    }

    fn release(&mut self) {
        if let Some(release_response) = self.0.take() {
            let _send_result = release_response.send(());
        }
    }
}

impl Drop for Fake429ResponseGuard {
    fn drop(&mut self) {
        self.release();
    }
}

impl Fake429Server {
    fn start(retry_after: Option<String>) -> Self {
        const ACCEPT_TIMEOUT: Duration = Duration::from_secs(10);

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind local fake provider");
        listener
            .set_nonblocking(true)
            .expect("make local fake provider cancellable");
        let url = format!("http://{}/test/usage", listener.local_addr().unwrap());
        let (request_started_tx, request_started) = mpsc::channel();
        let (release_response, response_released) = mpsc::channel();
        let request_count = Arc::new(AtomicUsize::new(0));
        let worker_request_count = Arc::clone(&request_count);
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let accept_deadline = Instant::now() + ACCEPT_TIMEOUT;
            let (mut stream, _) = loop {
                if worker_stop.load(Ordering::SeqCst) {
                    let _send_result = request_started_tx.send(Err(
                        "fake provider stopped before accepting the request".to_owned(),
                    ));
                    return;
                }
                if Instant::now() >= accept_deadline {
                    let _send_result = request_started_tx.send(Err(
                        "timed out accepting the localhost fake provider request".to_owned(),
                    ));
                    return;
                }
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::park_timeout(
                            Duration::from_millis(5)
                                .min(accept_deadline.saturating_duration_since(Instant::now())),
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => {
                        let _send_result = request_started_tx.send(Err(format!(
                            "accepting the localhost fake provider request failed: {error}"
                        )));
                        return;
                    }
                }
            };
            if let Err(error) = drain_http_request_headers(&mut stream, &worker_stop) {
                let _send_result = request_started_tx.send(Err(error));
                return;
            }
            worker_request_count.fetch_add(1, Ordering::SeqCst);
            if request_started_tx.send(Ok(())).is_err()
                || response_released
                    .recv_timeout(Duration::from_secs(15))
                    .is_err()
            {
                return;
            }
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("bound the localhost fake provider response write");
            let retry_after = retry_after
                .as_deref()
                .map_or_else(String::new, |value| format!("Retry-After: {value}\r\n"));
            let response = format!(
                "HTTP/1.1 429 Too Many Requests\r\n{retry_after}Content-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream
                .write_all(response.as_bytes())
                .expect("send the localhost fake provider response");
        });
        Self {
            url,
            request_started,
            release_response,
            request_count,
            stop,
            worker: Some(worker),
        }
    }

    fn wait_for_request(&self) {
        match self.request_started.recv_timeout(Duration::from_secs(23)) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                panic!("localhost fake provider did not receive a complete request: {error}")
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                panic!(
                    "timed out waiting for the broker probe to reach the localhost fake provider"
                )
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                panic!("localhost fake provider worker exited without reporting its request result")
            }
        }
    }

    fn release(&self) {
        let _send_result = self.release_response.send(());
    }

    fn request_count(&self) -> usize {
        self.request_count.load(Ordering::SeqCst)
    }

    fn join(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.join().expect("localhost fake provider should exit");
        }
    }
}

impl Drop for Fake429Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.release();
        self.join();
    }
}

/// Consume headers bytewise so the fixture never retains Authorization or
/// other request values.
fn drain_http_request_headers(stream: &mut TcpStream, stop: &AtomicBool) -> Result<(), String> {
    const DEADLINE: Duration = Duration::from_secs(12);
    const READ_SLICE: Duration = Duration::from_millis(100);
    const MAX_HEADER_BYTES: usize = 64 * 1024;

    let deadline = Instant::now() + DEADLINE;
    let mut ending = [0_u8; 4];
    let mut request_line_prefix = [0_u8; 4];
    for byte_count in 0..MAX_HEADER_BYTES {
        if stop.load(Ordering::SeqCst) {
            return Err("fake provider stopped while reading request headers".to_owned());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("timed out reading localhost fake provider request headers".to_owned());
        }
        stream
            .set_read_timeout(Some(remaining.min(READ_SLICE)))
            .map_err(|error| format!("setting fake provider read timeout failed: {error}"))?;
        let mut byte = [0_u8; 1];
        match stream.read(&mut byte) {
            Ok(0) => {
                return Err("peer closed before completing HTTP request headers".to_owned());
            }
            Ok(_) => {
                if byte_count < request_line_prefix.len() {
                    request_line_prefix[byte_count] = byte[0];
                }
                ending.rotate_left(1);
                ending[3] = byte[0];
                if ending == *b"\r\n\r\n" {
                    return if request_line_prefix == *b"GET " {
                        Ok(())
                    } else {
                        Err("localhost fake provider received a non-GET request line".to_owned())
                    };
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => {
                return Err(format!(
                    "reading fake provider request headers failed: {error}"
                ));
            }
        }
    }
    Err(format!(
        "localhost fake provider request headers exceeded {MAX_HEADER_BYTES} bytes"
    ))
}

struct FakeHttp429Executor {
    calls: AtomicUsize,
    url: String,
    clock: Arc<FakeBrokerClock>,
}

impl UsageProviderExecutor for FakeHttp429Executor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match crate::usage::get_json_bearer::<serde_json::Value>(
            jackin_telemetry::schema::enums::ProviderName::Anthropic,
            "/test/usage",
            "test usage",
            &self.url,
            "fake-test-token",
            &[],
        ) {
            Err(crate::usage::ProviderHttpError::HttpStatus {
                status,
                retry_after_seconds,
                response_received_at_epoch,
                ..
            }) => {
                let response_epoch =
                    response_received_at_epoch.expect("HTTP error carries its response timestamp");
                self.clock.set_epoch(response_epoch);
                let retry_at_epoch = retry_after_seconds.map(|seconds| {
                    response_epoch.saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX))
                });
                provider_probe_outcome_with_metadata(
                    quota_view(),
                    (status == 429).then_some(crate::usage::ProviderRateLimit { retry_at_epoch }),
                    Some(crate::usage::ProviderFailureMetadata {
                        kind: crate::usage::ProviderErrorKind::HttpStatus,
                        http_status: Some(status),
                    }),
                )
            }
            Err(error) => panic!("fake provider should return a typed HTTP status: {error:?}"),
            Ok(_) => panic!("fake provider must not return a successful response"),
        }
    }
}

fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "abc123".to_owned(),
        surface_id: "claude".to_owned(),
    }
}

fn non_claude_capability() -> UsageAccountCapability {
    // Generic broker lifecycle tests exercise shared queue/cache behavior
    // without implicitly authorizing the Claude collector path.
    UsageAccountCapability {
        account_id: "abc123".to_owned(),
        surface_id: "amp".to_owned(),
    }
}

fn second_capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "def456".to_owned(),
        surface_id: "codex".to_owned(),
    }
}

fn quota_view() -> FocusedUsageView {
    let mut view = FocusedUsageView::unavailable("claude", chrono::Utc::now().timestamp());
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.account.provider_label = "Claude".to_owned();
    view.account.account_label = "account@example.test".to_owned();
    view.buckets = vec![QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(75),
        reset_label: None,
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];
    view
}

fn env_material(source_name: &str, material: &str) -> ProviderCredentialSourceMaterial {
    ProviderCredentialSourceMaterial {
        source: UsageCredentialSourceIdentity::HostEnv {
            name: source_name.to_owned(),
        },
        material_fingerprint: usage_credential_material_fingerprint(material),
    }
}

fn env_scope(
    account_id: &str,
    surface_id: &str,
    key: &str,
    material: &ProviderCredentialSourceMaterial,
) -> UsageCredentialScope {
    UsageCredentialScope {
        sources: BTreeSet::from([UsageCredentialSourceProof {
            account_id: account_id.to_owned(),
            surface_id: surface_id.to_owned(),
            key: key.to_owned(),
            source: material.source.clone(),
            material_fingerprint: material.material_fingerprint.clone(),
        }]),
    }
}

#[test]
fn launch_scope_fails_closed_on_rotation_repoint_and_mixed_agent_source() {
    let capability = UsageAccountCapability {
        account_id: "shared-account".to_owned(),
        surface_id: "amp".to_owned(),
    };
    let staged = env_material("JACKIN_AGENT_A_KEY", "S1");
    let binding = ValidatedCredentialBinding {
        surface: HostSurfaceId::Amp,
        identity: None,
        capability_id: "capability-a".to_owned(),
        credential_revision: "credential-revision-a".to_owned(),
        provenance: BTreeSet::from(["account shared-account".to_owned()]),
        source: ValidatedCredentialSource::Env {
            handle: OpaqueCredentialHandle::new("handle-a"),
            key: "AMP_API_KEY".to_owned(),
            dispatch_key: "AMP_API_KEY".to_owned(),
            launch_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            material: Some(staged.clone()),
        },
    };
    let executor = DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::from([(capability.clone(), vec![binding])])),
        validated_catalog: Mutex::new(None),
        scope: UsageDiscoveryScope::HostDesktop {
            config_root: PathBuf::new(),
            operator_home: PathBuf::new(),
        },
        resolver: Arc::new(NoopCredentialResolver),
        monitor_store: None,
        collector_service: None,
        collector_liveness: None,
        claude_collector: None,
        probe_budget: Duration::from_secs(1),
    };
    let staged_scope = env_scope("shared-account", "amp", "AMP_API_KEY", &staged);
    executor
        .authorize_credential_scope(&capability, &staged_scope)
        .expect("staged source should authorize");

    let rotated = env_material("JACKIN_AGENT_A_KEY", "S2");
    executor
        .bindings
        .lock()
        .unwrap()
        .get_mut(&capability)
        .unwrap()
        .first_mut()
        .unwrap()
        .source = ValidatedCredentialSource::Env {
        handle: OpaqueCredentialHandle::new("handle-a-rotated"),
        key: "AMP_API_KEY".to_owned(),
        dispatch_key: "AMP_API_KEY".to_owned(),
        launch_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
        material: Some(rotated),
    };
    let error = executor
        .authorize_credential_scope(&capability, &staged_scope)
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);

    let repointed = env_material("JACKIN_AGENT_B_KEY", "S1");
    executor
        .bindings
        .lock()
        .unwrap()
        .get_mut(&capability)
        .unwrap()
        .first_mut()
        .unwrap()
        .source = ValidatedCredentialSource::Env {
        handle: OpaqueCredentialHandle::new("handle-b-repointed"),
        key: "AMP_API_KEY".to_owned(),
        dispatch_key: "AMP_API_KEY".to_owned(),
        launch_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
        material: Some(repointed.clone()),
    };
    let error = executor
        .authorize_credential_scope(&capability, &staged_scope)
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);

    let mixed_scope = UsageCredentialScope {
        sources: staged_scope
            .sources
            .iter()
            .cloned()
            .chain(env_scope("shared-account", "amp", "AMP_API_KEY", &repointed).sources)
            .collect(),
    };
    let error = executor
        .authorize_credential_scope(&capability, &mixed_scope)
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
}

#[test]
fn launch_scope_accepts_provider_native_zhipu_alias_for_canonical_zai_binding() {
    let capability = UsageAccountCapability {
        account_id: "zhipu-account".to_owned(),
        surface_id: "zai".to_owned(),
    };
    let staged = env_material("ZAI_HOST_SECRET", "S1");
    let executor = DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::from([(
            capability.clone(),
            vec![ValidatedCredentialBinding {
                surface: HostSurfaceId::Zai,
                identity: None,
                capability_id: "capability-zai".to_owned(),
                credential_revision: "credential-revision-zai".to_owned(),
                provenance: BTreeSet::from(["account zhipu-account".to_owned()]),
                source: ValidatedCredentialSource::Env {
                    handle: OpaqueCredentialHandle::new("handle-zai"),
                    key: "ZAI_API_KEY".to_owned(),
                    dispatch_key: "ZAI_API_KEY".to_owned(),
                    launch_keys: BTreeSet::from(["ZHIPU_API_KEY".to_owned()]),
                    material: Some(staged.clone()),
                },
            }],
        )])),
        validated_catalog: Mutex::new(None),
        scope: UsageDiscoveryScope::HostDesktop {
            config_root: PathBuf::new(),
            operator_home: PathBuf::new(),
        },
        resolver: Arc::new(NoopCredentialResolver),
        monitor_store: None,
        collector_service: None,
        collector_liveness: None,
        claude_collector: None,
        probe_budget: Duration::from_secs(1),
    };

    for key in ["ZAI_API_KEY", "ZHIPU_API_KEY", "Z_AI_API_KEY"] {
        let scope = env_scope("zhipu-account", "zai", key, &staged);
        executor
            .authorize_credential_scope(&capability, &scope)
            .expect("Z.AI alias with exact source material should authorize");
    }
    let wrong_material = env_material("ZAI_HOST_SECRET", "different-secret");
    let rejected = env_scope("zhipu-account", "zai", "Z_AI_API_KEY", &wrong_material);
    assert!(
        executor
            .authorize_credential_scope(&capability, &rejected)
            .is_err()
    );
}

#[test]
fn discovery_provider_error_text_cannot_set_rate_limit_or_retry_deadline() {
    for text in [
        "provider HTTP 401 Unauthorized",
        "provider HTTP 403 Forbidden",
        "provider HTTP 429; Retry-After: 97",
        "transport failed while contacting port 429",
    ] {
        let mut view = quota_view();
        view.status = UsageSnapshotStatus::Stale;
        view.last_error = Some(text.to_owned());

        let ProviderProbeOutcome::Failure {
            kind,
            message,
            retry_at_epoch,
        } = provider_probe_outcome(view)
        else {
            panic!("provider view must not publish as success");
        };
        assert_eq!(kind, UsageCoordinationErrorKind::ProviderUnavailable);
        assert_eq!(message, text);
        assert_eq!(retry_at_epoch, None);
    }
}

#[test]
fn discovery_typed_rate_limit_reaches_broker_without_text_parsing() {
    let mut view = quota_view();
    view.status = UsageSnapshotStatus::Stale;
    view.last_error = Some("transport message mentions HTTP 429".to_owned());

    let ProviderProbeOutcome::Failure {
        kind,
        message,
        retry_at_epoch,
    } = provider_probe_outcome_with_rate_limit(
        view,
        Some(crate::usage::ProviderRateLimit {
            retry_at_epoch: Some(1_700_000_037),
        }),
    )
    else {
        panic!("typed rate limit must be a broker failure");
    };
    assert_eq!(kind, UsageCoordinationErrorKind::RateLimited);
    assert_eq!(message, "usage provider rate limit is active");
    assert_eq!(retry_at_epoch, Some(1_700_000_037));
}

#[test]
fn provider_failure_metadata_controls_broker_retry_classification() {
    use crate::usage::{ProviderErrorKind, ProviderFailureMetadata};

    let mut view = quota_view();
    view.status = UsageSnapshotStatus::Stale;
    view.last_error = Some("request failed with text mentioning HTTP 401 and 403".to_owned());
    for (metadata, expected) in [
        (
            ProviderFailureMetadata {
                kind: ProviderErrorKind::HttpStatus,
                http_status: Some(401),
            },
            UsageCoordinationErrorKind::NeedsSecret,
        ),
        (
            ProviderFailureMetadata {
                kind: ProviderErrorKind::HttpStatus,
                http_status: Some(403),
            },
            UsageCoordinationErrorKind::Unauthorized,
        ),
        (
            ProviderFailureMetadata {
                kind: ProviderErrorKind::HttpStatus,
                http_status: Some(429),
            },
            UsageCoordinationErrorKind::RateLimited,
        ),
        (
            ProviderFailureMetadata {
                kind: ProviderErrorKind::Timeout,
                http_status: None,
            },
            UsageCoordinationErrorKind::ProviderTimeout,
        ),
    ] {
        let ProviderProbeOutcome::Failure {
            kind,
            retry_at_epoch,
            ..
        } = provider_probe_outcome_with_metadata(view.clone(), None, Some(metadata))
        else {
            panic!("typed provider failure must remain a broker failure");
        };
        assert_eq!(kind, expected);
        assert_eq!(retry_at_epoch, None);
    }
}

struct TypedRateLimitResolver;

impl ProviderCredentialEnvResolver for TypedRateLimitResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        _keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        Vec::new()
    }

    fn refresh_provider_credential(
        &self,
        _surface: HostSurfaceId,
        _key: &str,
        _handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialRefreshOutcome {
        ProviderCredentialRefreshOutcome::Snapshot {
            view: Box::new(quota_view()),
            rate_limit: Some(crate::usage::ProviderRateLimit {
                retry_at_epoch: Some(1_700_000_037),
            }),
            failure_metadata: None,
        }
    }
}

#[test]
fn refresh_binding_outcome_carries_typed_rate_limit_into_broker() {
    let binding = ValidatedCredentialBinding {
        surface: HostSurfaceId::Claude,
        identity: None,
        capability_id: "capability-typed-rate-limit".to_owned(),
        credential_revision: "credential-revision-typed-rate-limit".to_owned(),
        provenance: BTreeSet::new(),
        source: ValidatedCredentialSource::Env {
            handle: OpaqueCredentialHandle::new("typed-rate-limit-handle"),
            key: "CLAUDE_API_KEY".to_owned(),
            dispatch_key: "CLAUDE_API_KEY".to_owned(),
            launch_keys: BTreeSet::from(["CLAUDE_API_KEY".to_owned()]),
            material: Some(env_material("CLAUDE_API_KEY", "fixture-secret")),
        },
    };

    let outcome = refresh_binding_outcome(&binding, &TypedRateLimitResolver);
    let ProviderProbeOutcome::Failure {
        kind,
        retry_at_epoch,
        ..
    } = outcome
    else {
        panic!("typed rate limit must remain a broker failure");
    };
    assert_eq!(kind, UsageCoordinationErrorKind::RateLimited);
    assert_eq!(retry_at_epoch, Some(1_700_000_037));
}

#[test]
fn discovery_provider_stale_and_error_views_are_retryable_failures() {
    for status in [UsageSnapshotStatus::Stale, UsageSnapshotStatus::Error] {
        let mut view = quota_view();
        view.status = status;
        view.last_error = None;
        let ProviderProbeOutcome::Failure {
            kind,
            retry_at_epoch,
            ..
        } = provider_probe_outcome(view)
        else {
            panic!("{status:?} provider view must not publish as success");
        };
        assert_eq!(kind, UsageCoordinationErrorKind::ProviderUnavailable);
        assert_eq!(retry_at_epoch, None);
    }
}

#[test]
fn discovery_provider_unsupported_views_remain_publishable_unsupported() {
    let mut view = quota_view();
    view.status = UsageSnapshotStatus::Unsupported;
    assert!(matches!(
        provider_probe_outcome(view),
        ProviderProbeOutcome::Success(_)
    ));
}

#[test]
fn background_rediscovery_does_not_start_manual_retry_or_admit_mismatch() {
    let resolver = RetryRecordingResolver::default();
    let scope = UsageDiscoveryScope::Capsule {
        forwarded_accounts: vec![ForwardedUsageAccount {
            surface_id: "claude".to_owned(),
            capability_id: "different-capability".to_owned(),
            account_label: Some("other@example.test".to_owned()),
        }],
    };
    let (binding, refreshed) = rediscover_bindings(&scope, &resolver, &capability());

    assert!(binding.is_none());
    assert!(refreshed.is_none());
    assert_eq!(resolver.manual_retries.load(Ordering::SeqCst), 0);
}

#[test]
fn discovery_executor_rejects_catalog_that_does_not_match_current_scope() {
    let manual_retries = Arc::new(AtomicUsize::new(0));
    let resolver: Arc<dyn ProviderCredentialEnvResolver> = Arc::new(RetryRecordingResolver {
        manual_retries: Arc::clone(&manual_retries),
    });
    let executor = DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::new()),
        validated_catalog: Mutex::new(None),
        scope: UsageDiscoveryScope::Capsule {
            forwarded_accounts: Vec::new(),
        },
        resolver: Arc::clone(&resolver),
        monitor_store: None,
        collector_service: None,
        collector_liveness: None,
        claude_collector: None,
        probe_budget: Duration::from_secs(1),
    };
    let error = executor
        .validate_catalog(&[UsageCatalogEntry {
            capability: capability(),
            revision: "mismatched".to_owned(),
        }])
        .expect_err("mismatched catalog must fail closed");

    assert_eq!(
        error.kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(manual_retries.load(Ordering::SeqCst), 0);
}

#[test]
fn foreground_discovery_executor_rejects_non_claude_before_credentials_or_collector() {
    let temp = tempfile::tempdir().unwrap();
    let config_root = temp.path().join("config");
    let mut config = AppConfig::default();
    config.accounts.insert(
        "fixture-amp".to_owned(),
        jackin_config::AccountConfig {
            enabled: true,
            name: "fixture-amp".to_owned(),
            provider: jackin_config::AiProvider::Amp,
            credential: jackin_config::AccountCredential::ApiKey {
                value: jackin_config::EnvValue::Plain("fixture-only-value".to_owned()),
                base_url: None,
                model: None,
            },
        },
    );
    fs::create_dir_all(&config_root).unwrap();
    fs::write(
        config_root.join("config.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();

    let resolver = Arc::new(CountingCredentialResolver::default());
    let collector_calls = Arc::new(AtomicUsize::new(0));
    let executor = DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::new()),
        validated_catalog: Mutex::new(None),
        scope: UsageDiscoveryScope::HostDesktop {
            config_root,
            operator_home: temp.path().join("home"),
        },
        resolver: Arc::<CountingCredentialResolver>::clone(&resolver),
        monitor_store: None,
        collector_service: Some("fixture-claude-service".to_owned()),
        collector_liveness: None,
        claude_collector: Some({
            let collector_calls = Arc::clone(&collector_calls);
            Arc::new(move |_, _| {
                collector_calls.fetch_add(1, Ordering::SeqCst);
                ProviderProbeOutcome::success(quota_view())
            })
        }),
        probe_budget: Duration::from_secs(1),
    };

    let ProviderProbeOutcome::Failure { kind, .. } = executor.probe(&non_claude_capability(), 1)
    else {
        panic!("foreground Claude executor must reject a non-Claude capability");
    };
    assert_eq!(kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    assert_eq!(collector_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn discovery_executor_rejects_unselected_catalog_rows_without_credential_or_provider_access() {
    let resolver = Arc::new(CountingCredentialResolver::default());
    let fake_collector_calls = Arc::new(AtomicUsize::new(0));
    let scope = UsageDiscoveryScope::Capsule {
        forwarded_accounts: vec![ForwardedUsageAccount {
            surface_id: "amp".to_owned(),
            capability_id: "selected-amp-source".to_owned(),
            account_label: Some("selected fixture account".to_owned()),
        }],
    };
    let executor = DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::new()),
        validated_catalog: Mutex::new(None),
        scope: scope.clone(),
        resolver: Arc::<CountingCredentialResolver>::clone(&resolver),
        monitor_store: None,
        collector_service: None,
        collector_liveness: None,
        claude_collector: Some({
            let fake_collector_calls = Arc::clone(&fake_collector_calls);
            Arc::new(move |_, _| {
                fake_collector_calls.fetch_add(1, Ordering::SeqCst);
                ProviderProbeOutcome::success(quota_view())
            })
        }),
        probe_budget: Duration::from_secs(1),
    };
    let discovery = rediscover_discovery(&scope, resolver.as_ref())
        .expect("forwarded fixture discovery is available");
    let mut entries = usage_catalog_entries(&discovery);
    assert_eq!(entries.len(), 1, "fixture has exactly one selected row");
    entries.push(UsageCatalogEntry {
        capability: UsageAccountCapability {
            surface_id: "amp".to_owned(),
            account_id: "unselected-canonical-row".to_owned(),
        },
        revision: "unselected-revision".to_owned(),
    });

    for error in [
        executor
            .validate_catalog(&entries)
            .expect_err("unselected row must fail full catalog validation"),
        executor
            .validate_catalog_revision("empty", &entries)
            .expect_err("unselected row must fail revision validation"),
        executor
            .reconcile_catalog_revision("empty", &entries)
            .expect_err("unselected row must fail reconciliation"),
    ] {
        assert_eq!(
            error.kind,
            UsageCoordinationErrorKind::CatalogRevisionConflict
        );
    }
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    assert_eq!(fake_collector_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn discovery_provider_failures_carry_each_gap_reason() {
    // Every gap kind keeps its failure category but renders the collector's
    // specific message instead of the generic fallback. Payloads below are
    // the collectors' real gap strings.
    for (status, kind, gap) in [
        (
            UsageSnapshotStatus::Error,
            UsageCoordinationErrorKind::ProviderUnavailable,
            "Grok billing requires an authenticated profile",
        ),
        (
            UsageSnapshotStatus::Unavailable,
            UsageCoordinationErrorKind::ProviderUnavailable,
            "Grok billing requires an authenticated profile",
        ),
        (
            UsageSnapshotStatus::Stale,
            UsageCoordinationErrorKind::ProviderUnavailable,
            "Grok billing requires an authenticated profile",
        ),
        (
            UsageSnapshotStatus::NeedsSecret,
            UsageCoordinationErrorKind::NeedsSecret,
            "Gemini auth not available to Capsule",
        ),
        (
            UsageSnapshotStatus::NeedsLogin,
            UsageCoordinationErrorKind::NeedsSecret,
            "Grok auth not available to Capsule",
        ),
    ] {
        let mut view = quota_view();
        view.status = status;
        view.last_error = Some(gap.to_owned());
        let ProviderProbeOutcome::Failure {
            kind: actual,
            message,
            ..
        } = provider_probe_outcome(view)
        else {
            panic!("{status:?} provider view must not publish as success");
        };
        assert_eq!(actual, kind);
        assert_eq!(message, gap);
    }
}

#[test]
fn discovery_provider_failures_without_reason_keep_generic_fallback() {
    for (status, kind, fallback) in [
        (
            UsageSnapshotStatus::Error,
            UsageCoordinationErrorKind::ProviderUnavailable,
            "usage provider quota is unavailable",
        ),
        (
            UsageSnapshotStatus::NeedsSecret,
            UsageCoordinationErrorKind::NeedsSecret,
            "usage provider credentials require operator action",
        ),
    ] {
        for last_error in [None, Some(String::new()), Some("   ".to_owned())] {
            let mut view = quota_view();
            view.status = status;
            view.last_error = last_error;
            let ProviderProbeOutcome::Failure {
                kind: actual,
                message,
                ..
            } = provider_probe_outcome(view)
            else {
                panic!("{status:?} provider view must not publish as success");
            };
            assert_eq!(actual, kind);
            assert_eq!(message, fallback);
        }
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "One table-style forwarding matrix: six source fixtures share one discovery setup; splitting would duplicate the binding fixtures per case."
)]
fn forwarded_scope_selects_only_accounts_backed_by_forwarded_sources() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

    let profile_identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Amp,
        subject: CanonicalAccountSubject::ProviderStableHandle("profile@example.test".to_owned()),
    };
    let env_identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Amp,
        subject: CanonicalAccountSubject::ProviderStableHandle("env@example.test".to_owned()),
    };
    let env_material = ProviderCredentialSourceMaterial {
        source: UsageCredentialSourceIdentity::HostEnv {
            name: "AMP_API_KEY".to_owned(),
        },
        material_fingerprint: usage_credential_material_fingerprint("env-secret"),
    };
    let scope = "workspace sample role test";
    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("generation".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![
            ValidatedCredentialBinding {
                surface: HostSurfaceId::Amp,
                identity: Some(profile_identity),
                capability_id: "profile-capability".to_owned(),
                credential_revision: "profile-revision".to_owned(),
                provenance: BTreeSet::from([
                    scope.to_owned(),
                    "account account-profile".to_owned(),
                ]),
                source: ValidatedCredentialSource::Profile(
                    super::super::discovery::ProfileCredentialMaterial::Amp {
                        key: "profile-secret".to_owned(),
                    },
                ),
            },
            ValidatedCredentialBinding {
                surface: HostSurfaceId::Amp,
                identity: Some(env_identity),
                capability_id: "env-capability".to_owned(),
                credential_revision: "env-revision".to_owned(),
                provenance: BTreeSet::from([scope.to_owned(), "account account-env".to_owned()]),
                source: ValidatedCredentialSource::Env {
                    handle: OpaqueCredentialHandle::new("env-handle"),
                    key: "AMP_API_KEY".to_owned(),
                    dispatch_key: "AMP_API_KEY".to_owned(),
                    launch_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
                    material: Some(env_material.clone()),
                },
            },
        ],
    };
    let profile_capability = capability_for_binding(
        &discovery.bindings[0],
        discovery.config_generation.as_deref(),
    );
    let env_capability = capability_for_binding(
        &discovery.bindings[1],
        discovery.config_generation.as_deref(),
    );

    let profile_only = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::new(),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::new(),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert_eq!(profile_only, vec![profile_capability.clone()]);

    let env_only = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::new(),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::new(),
            env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert_eq!(env_only, vec![env_capability.clone()]);

    let selected_profile = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-profile".to_owned()]),
            selected_account_surfaces: BTreeMap::from([(
                "account-profile".to_owned(),
                "amp".to_owned(),
            )]),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::new(),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert_eq!(selected_profile, vec![profile_capability.clone()]);

    let selected_env_sources = ForwardedUsageSources {
        selected_account_ids: BTreeSet::from(["account-env".to_owned()]),
        selected_account_surfaces: BTreeMap::from([("account-env".to_owned(), "amp".to_owned())]),
        profile_surface_ids: BTreeSet::new(),
        env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
        credential_scope: UsageCredentialScope {
            sources: BTreeSet::from([UsageCredentialSourceProof {
                account_id: "account-env".to_owned(),
                surface_id: "amp".to_owned(),
                key: "AMP_API_KEY".to_owned(),
                source: env_material.source.clone(),
                material_fingerprint: env_material.material_fingerprint.clone(),
            }]),
        },
    };
    let selected_env = forwarded_usage_capabilities(&discovery, scope, &selected_env_sources);
    assert_eq!(selected_env, vec![env_capability.clone()]);
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "account-env",
            "amp",
            Some(&selected_env_sources),
        ),
        Some(env_capability.clone())
    );

    let wrong_env_source = ForwardedUsageSources {
        credential_scope: UsageCredentialScope::default(),
        ..selected_env_sources.clone()
    };
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "account-env",
            "amp",
            Some(&wrong_env_source),
        ),
        None,
        "selected account and provider surface need matching source proof"
    );

    let wrong_surface = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-profile".to_owned()]),
            selected_account_surfaces: BTreeMap::from([(
                "account-profile".to_owned(),
                "codex".to_owned(),
            )]),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::new(),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert!(
        wrong_surface.is_empty(),
        "account proof must bind both configured account and provider surface"
    );

    let wrong_account = forwarded_usage_capabilities(
        &discovery,
        scope,
        &ForwardedUsageSources {
            selected_account_ids: BTreeSet::from(["account-does-not-exist".to_owned()]),
            selected_account_surfaces: BTreeMap::new(),
            profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
            env_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            credential_scope: UsageCredentialScope::default(),
        },
    );
    assert!(wrong_account.is_empty());

    assert_eq!(
        usage_capability_for_selected_account(&discovery, "account-profile", "amp"),
        Some(profile_capability.clone())
    );
    assert_eq!(
        usage_capability_for_selected_account(&discovery, "account-profile", "claude"),
        None
    );

    let publication = publication_identity_metadata(&discovery);
    assert_eq!(
        publication[&profile_capability].identity_kind,
        UsageIdentityKindV1::ProviderStableHandle
    );
    assert_eq!(publication[&profile_capability].provenance_count, 2);
}

#[test]
fn selected_routes_require_exact_source_proofs_and_same_identity() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

    let identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Zai,
        subject: CanonicalAccountSubject::ProviderStableHandle("zai-account".to_owned()),
    };
    let material = env_material("ZAI_HOST_SECRET", "zai-secret");
    let binding = |key: &str, handle: &str| ValidatedCredentialBinding {
        surface: HostSurfaceId::Zai,
        identity: Some(identity.clone()),
        capability_id: format!("capability-{handle}"),
        credential_revision: format!("revision-{handle}"),
        provenance: BTreeSet::from(["account zai".to_owned()]),
        source: ValidatedCredentialSource::Env {
            handle: OpaqueCredentialHandle::new(handle),
            key: "ZAI_API_KEY".to_owned(),
            dispatch_key: "ZAI_API_KEY".to_owned(),
            launch_keys: BTreeSet::from([key.to_owned()]),
            material: Some(material.clone()),
        },
    };
    let discovery = ValidatedUsageDiscovery {
        config_generation: None,
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![
            binding("ZHIPU_API_KEY", "zhipu"),
            binding("ZAI_API_KEY", "zai"),
        ],
    };
    let staged = ForwardedUsageSources {
        selected_account_ids: BTreeSet::from(["zai".to_owned()]),
        selected_account_surfaces: BTreeMap::from([("zai".to_owned(), "zai".to_owned())]),
        profile_surface_ids: BTreeSet::new(),
        env_keys: BTreeSet::from(["ZHIPU_API_KEY".to_owned(), "ZAI_API_KEY".to_owned()]),
        credential_scope: UsageCredentialScope {
            sources: BTreeSet::from([
                UsageCredentialSourceProof {
                    account_id: "zai".to_owned(),
                    surface_id: "zai".to_owned(),
                    key: "Z_AI_API_KEY".to_owned(),
                    source: material.source.clone(),
                    material_fingerprint: material.material_fingerprint.clone(),
                },
                UsageCredentialSourceProof {
                    account_id: "zai".to_owned(),
                    surface_id: "zai".to_owned(),
                    key: "ZAI_API_KEY".to_owned(),
                    source: material.source.clone(),
                    material_fingerprint: material.material_fingerprint.clone(),
                },
            ]),
        },
    };
    let capability =
        usage_capability_for_selected_account_with_sources(&discovery, "zai", "zai", Some(&staged));
    assert!(
        capability.is_some(),
        "same identity may combine route proofs"
    );

    let wrong_material = ForwardedUsageSources {
        credential_scope: env_scope(
            "zai",
            "zai",
            "ZAI_API_KEY",
            &env_material("ZAI_HOST_SECRET", "other"),
        ),
        ..staged.clone()
    };
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "zai",
            "zai",
            Some(&wrong_material),
        ),
        None
    );

    let wrong_account = ForwardedUsageSources {
        credential_scope: env_scope("other", "zai", "ZAI_API_KEY", &material),
        ..staged.clone()
    };
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "zai",
            "zai",
            Some(&wrong_account),
        ),
        None
    );

    let wrong_source = ForwardedUsageSources {
        credential_scope: env_scope(
            "zai",
            "zai",
            "ZAI_API_KEY",
            &env_material("OTHER_HOST_SECRET", "zai-secret"),
        ),
        ..staged.clone()
    };
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &discovery,
            "zai",
            "zai",
            Some(&wrong_source),
        ),
        None
    );

    let different_identity = ValidatedUsageDiscovery {
        bindings: vec![
            discovery.bindings[0].clone(),
            ValidatedCredentialBinding {
                identity: Some(CanonicalAccountIdentity {
                    surface: HostSurfaceId::Zai,
                    subject: CanonicalAccountSubject::ProviderStableHandle(
                        "different-zai-account".to_owned(),
                    ),
                }),
                ..discovery.bindings[1].clone()
            },
        ],
        ..discovery
    };
    assert_eq!(
        usage_capability_for_selected_account_with_sources(
            &different_identity,
            "zai",
            "zai",
            Some(&staged),
        ),
        None,
        "different provider identities must not collapse into one route"
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "The table-driven integration case shares one real broker, store, and restart flow across all Retry-After header variants."
)]
fn local_http_429_retry_after_survives_broker_restart_and_catalog_rotation() {
    #[derive(Clone, Copy)]
    enum ProviderDeadline {
        None,
        Offset(u64),
        Absolute(i64),
    }

    let date = SystemTime::now() + Duration::from_hours(1);
    let date_epoch = i64::try_from(
        date.duration_since(UNIX_EPOCH)
            .expect("future HTTP-date is after the epoch")
            .as_secs(),
    )
    .unwrap();
    let cases = [
        (
            "delta-seconds",
            Some("600".to_owned()),
            ProviderDeadline::Offset(600),
        ),
        (
            "http-date",
            Some(httpdate::fmt_http_date(date)),
            ProviderDeadline::Absolute(date_epoch),
        ),
        ("absent", None, ProviderDeadline::None),
        (
            "invalid",
            Some("not-a-delay".to_owned()),
            ProviderDeadline::None,
        ),
        ("zero", Some("0".to_owned()), ProviderDeadline::Offset(0)),
    ];

    for (case_name, retry_after, provider_deadline) in cases {
        let temp = tempfile::tempdir().expect("isolated rate-limit state directory");
        let capability = capability();
        let store = Arc::new(FileAccountStateStore::at(temp.path().join("accounts")));
        let initial_epoch = i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the epoch")
                .as_secs(),
        )
        .unwrap();
        let mut server = Fake429Server::start(retry_after);
        let clock = Arc::new(FakeBrokerClock::new(initial_epoch));
        let executor = Arc::new(FakeHttp429Executor {
            calls: AtomicUsize::new(0),
            url: server.url.clone(),
            clock: Arc::clone(&clock),
        });
        #[expect(
            clippy::clone_on_ref_ptr,
            reason = "coerce the concrete local fake executor to the broker port"
        )]
        let provider_executor: Arc<dyn UsageProviderExecutor> = executor.clone();
        #[expect(
            clippy::clone_on_ref_ptr,
            reason = "coerce the fake clock to the coordinator clock port"
        )]
        let clock_port: Arc<dyn MonotonicClock> = clock.clone();
        let coordinator = UsageCoordinator::start_with_clock(
            provider_executor,
            Arc::<FileAccountStateStore>::clone(&store),
            UsageCoordinatorConfig::default(),
            Some(BTreeMap::from([(
                capability.clone(),
                "credential-revision-a".to_owned(),
            )])),
            None,
            clock_port,
        );
        // Declared after the coordinator so unwinding always releases the
        // blocked local response before the coordinator joins its worker.
        let mut response_guard = Fake429ResponseGuard::new(server.release_response.clone());

        let queued = coordinator
            .request_refresh(&capability, 0, true, initial_epoch)
            .expect("first forced refresh is admitted");
        server.wait_for_request();
        assert_eq!(
            server.request_count(),
            1,
            "{case_name}: one HTTP request starts"
        );
        let joined = coordinator
            .request_refresh(&capability, queued.generation, true, initial_epoch)
            .expect("a force request joins the active generation");
        assert_eq!(joined.generation, queued.generation, "{case_name}");
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1, "{case_name}");

        response_guard.release();
        let failed = coordinator
            .join_generation(
                &capability,
                queued.generation,
                Duration::from_secs(5),
                clock.epoch(),
            )
            .expect("typed HTTP 429 reaches a terminal broker generation");
        server.join();
        let response_epoch = clock.epoch();
        assert_eq!(failed.phase, UsageRefreshPhase::Failed, "{case_name}");
        assert_eq!(
            failed.error.as_ref().map(|error| error.kind),
            Some(UsageCoordinationErrorKind::RateLimited),
            "{case_name}: 429 remains typed across the broker seam"
        );
        assert_eq!(
            server.request_count(),
            1,
            "{case_name}: no duplicate HTTP call"
        );

        let local_floor = response_epoch.saturating_add(300);
        let parsed_provider_deadline = match provider_deadline {
            ProviderDeadline::None => None,
            ProviderDeadline::Offset(seconds) => {
                Some(response_epoch.saturating_add(i64::try_from(seconds).unwrap_or(i64::MAX)))
            }
            ProviderDeadline::Absolute(epoch) => Some(epoch),
        };
        let expected_deadline =
            parsed_provider_deadline.map_or(local_floor, |deadline| deadline.max(local_floor));
        assert_eq!(
            failed.retry_at_epoch,
            Some(expected_deadline),
            "{case_name}"
        );

        coordinator
            .reconcile_catalog(
                [UsageCatalogEntry {
                    capability: capability.clone(),
                    revision: "credential-revision-b".to_owned(),
                }],
                response_epoch,
            )
            .expect("same account catalog rotation succeeds");
        let reset = coordinator
            .current(&capability, response_epoch)
            .expect("rotated account remains visible");
        assert_eq!(reset.phase, UsageRefreshPhase::Idle, "{case_name}");
        let durable = store
            .load(&capability, response_epoch)
            .expect("read durable rate-limit state")
            .expect("rate-limit state survives catalog rotation");
        assert_eq!(
            durable.rate_limit_deadline_epoch,
            Some(expected_deadline),
            "{case_name}: persisted provider cooldown"
        );
        assert_eq!(
            durable.retry_deadline_epoch,
            Some(expected_deadline),
            "{case_name}: persisted retry cooldown"
        );
        if matches!(provider_deadline, ProviderDeadline::None)
            || matches!(provider_deadline, ProviderDeadline::Offset(0))
        {
            assert_eq!(
                expected_deadline, local_floor,
                "{case_name}: positive floor"
            );
            assert!(
                durable.retry_deadline_epoch.unwrap()
                    >= durable
                        .provider_invoked_at_epoch
                        .unwrap()
                        .saturating_add(300),
                "{case_name}: missing, invalid, or zero Retry-After still gets the Claude floor"
            );
        }
        drop(coordinator);

        let restart_clock = Arc::new(FakeBrokerClock::new(response_epoch));
        let restart_executor = Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        });
        #[expect(
            clippy::clone_on_ref_ptr,
            reason = "coerce the post-restart executor to the broker port"
        )]
        let restart_provider: Arc<dyn UsageProviderExecutor> = restart_executor.clone();
        #[expect(
            clippy::clone_on_ref_ptr,
            reason = "coerce the restarted fake clock to the coordinator clock port"
        )]
        let restart_clock_port: Arc<dyn MonotonicClock> = restart_clock.clone();
        let restarted = UsageCoordinator::start_with_clock(
            restart_provider,
            Arc::<FileAccountStateStore>::clone(&store),
            UsageCoordinatorConfig::default(),
            Some(BTreeMap::from([(
                capability.clone(),
                "credential-revision-b".to_owned(),
            )])),
            None,
            restart_clock_port,
        );
        let restored = restarted
            .current(&capability, response_epoch)
            .expect("restarted coordinator restores the account cooldown");
        assert_eq!(
            restored.retry_at_epoch,
            Some(expected_deadline),
            "{case_name}"
        );

        let remaining = u64::try_from(expected_deadline - response_epoch)
            .expect("all cases have a future enforced deadline");
        assert!(remaining > 0, "{case_name}");
        restart_clock.advance(Duration::from_secs(remaining - 1));
        let before_deadline = restart_clock.epoch();
        assert!(
            restarted.poll_due(before_deadline).is_empty(),
            "{case_name}: polling cannot bypass the persisted deadline"
        );
        let early = restarted
            .request_refresh(&capability, restored.generation, true, before_deadline)
            .expect("forced refresh before the retry deadline is suppressed");
        assert_eq!(early.generation, restored.generation, "{case_name}");
        assert_eq!(
            restart_executor.calls.load(Ordering::SeqCst),
            0,
            "{case_name}"
        );

        restart_clock.advance(Duration::from_secs(1));
        let at_deadline = restart_clock.epoch();
        assert_eq!(at_deadline, expected_deadline, "{case_name}");
        let retry = restarted
            .request_refresh(&capability, restored.generation, true, at_deadline)
            .expect("refresh is admitted at the exact persisted deadline");
        assert_eq!(retry.generation, restored.generation + 1, "{case_name}");
        assert_eq!(
            restarted
                .join_generation(
                    &capability,
                    retry.generation,
                    Duration::from_secs(5),
                    at_deadline,
                )
                .expect("retry generation completes")
                .phase,
            UsageRefreshPhase::Completed,
            "{case_name}"
        );
        assert_eq!(
            restart_executor.calls.load(Ordering::SeqCst),
            1,
            "{case_name}"
        );
        assert_eq!(
            server.request_count(),
            1,
            "{case_name}: restart uses fake executor"
        );
    }
}

#[test]
fn broker_migrates_v2_projection_before_exposure_and_preserves_durable_history() {
    let temp = tempfile::tempdir().expect("isolated broker state directory");
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let now_epoch = 1_000;

    let claude_capability = UsageAccountCapability {
        account_id: "claude-account".to_owned(),
        surface_id: "claude".to_owned(),
    };
    let codex_capability = UsageAccountCapability {
        account_id: "codex-account".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let account_store = FileAccountStateStore::under_data_dir(&config.data_dir);
    let history_before = seed_legacy_claude_history(&account_store, &claude_capability, now_epoch);
    assert_eq!(
        history_before
            .terminal_result
            .as_ref()
            .and_then(|view| view.buckets.first())
            .and_then(|bucket| bucket.remaining_percent),
        Some(97),
        "the durable history fixture retains its original 97% remaining value"
    );

    let legacy_envelope =
        legacy_v2_projection_envelope(claude_capability.clone(), codex_capability);
    let projection_path = config.data_dir.join(BROKER_DIR).join("projection.json");
    fs::create_dir_all(projection_path.parent().unwrap()).unwrap();
    fs::write(
        &projection_path,
        serde_json::to_vec(&legacy_envelope).expect("serialize schema-v2 fixture"),
    )
    .expect("write schema-v2 fixture");

    let projection_store = FileProjectionStateStore::under_data_dir(&config.data_dir);
    assert_eq!(
        projection_store.load(),
        Err(
            crate::coordinator::StateStoreError::SchemaMigrationRequired {
                found: u64::from(ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION),
                current: ProjectionStateEnvelope::SCHEMA_VERSION,
            }
        )
    );
    assert!(
        projection_path.exists(),
        "a passive read must leave the broker migration input intact"
    );

    let loaded = load_projection(&config).expect("migrate durable projection before startup");
    let visible = loaded
        .projection
        .lock()
        .expect("lock migrated projection")
        .clone();
    assert_eq!(
        visible
            .providers
            .iter()
            .map(|provider| provider.provider_id.as_str())
            .collect::<Vec<_>>(),
        ["openai", "anthropic"]
    );
    assert_eq!(
        visible
            .providers
            .iter()
            .map(|provider| provider.rank)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(visible.projection_id, "legacy-instance:7");
    assert_eq!(visible.generated_at_epoch, 980);
    assert_eq!(visible.broker_generation, 7);
    assert_eq!(visible.discovery_revision, "legacy-catalog-revision");
    assert_eq!(visible.providers[0].display_name, "OpenAI");
    assert_eq!(visible.providers[1].display_name, "Anthropic");
    assert_eq!(
        visible.providers[0].accounts[0].canonical_account_id,
        "codex-account"
    );
    assert_eq!(visible.providers[0].accounts[0].display_label, "codex");
    assert_eq!(
        visible.providers[1].accounts[0].display_label,
        "historic Claude identity"
    );
    assert_eq!(
        visible.providers[0].accounts[0].identity_kind,
        UsageIdentityKindV1::UnverifiedHandle
    );
    assert_eq!(visible.providers[0].accounts[0].provenance_count, 3);
    assert_eq!(
        visible.providers[0].accounts[0].freshness,
        freshness_for_legacy_projection()
    );
    assert_eq!(
        visible.providers[0].accounts[0].windows[0].remaining_raw_percent,
        Some(72)
    );
    assert_eq!(
        visible.providers[0].accounts[0].windows[0].reset_at_epoch,
        Some(2_200)
    );
    assert_eq!(visible.unresolved[0].provider_id, "openai");
    assert_eq!(visible.unresolved[1].provider_id, "anthropic");

    let migrated_envelope = FileProjectionStateStore::under_data_dir(&config.data_dir)
        .load()
        .expect("read only the new schema after startup migration")
        .expect("migration writes a current envelope before returning");
    assert_eq!(
        migrated_envelope.schema_version,
        ProjectionStateEnvelope::SCHEMA_VERSION
    );
    assert_eq!(migrated_envelope.catalog, legacy_envelope.catalog);
    assert_eq!(migrated_envelope.aliases, legacy_envelope.aliases);
    assert_eq!(migrated_envelope.retry_deadline_epoch, Some(1_150));
    assert_eq!(migrated_envelope.success_deadline_epoch, Some(1_300));
    assert_eq!(migrated_envelope.broker_instance_id, "legacy-instance");
    assert_eq!(
        account_store
            .load(&claude_capability, now_epoch)
            .expect("coordinator history remains readable"),
        Some(history_before)
    );
}

fn seed_legacy_claude_history(
    account_store: &FileAccountStateStore,
    capability: &UsageAccountCapability,
    now_epoch: i64,
) -> AccountStateEnvelope {
    let mut account_history = AccountStateEnvelope::idle(capability.clone());
    account_history.generation = 7;
    account_history.phase = UsageRefreshPhase::Failed;
    let mut historical_result = quota_view();
    historical_result.fetched_at_epoch = 970;
    historical_result.account.account_label = "historical account".to_owned();
    for bucket in &mut historical_result.buckets {
        bucket.remaining_percent = Some(97);
    }
    historical_result.status_bar_label = "Claude Weekly: 97% left".to_owned();
    account_history.terminal_result = Some(historical_result);
    account_history.last_good = account_history.terminal_result.clone();
    account_history.started_at_epoch = Some(960);
    account_history.provider_invoked_at_epoch = Some(961);
    account_history.completed_at_epoch = Some(970);
    account_history.rate_limit_deadline_epoch = Some(1_100);
    account_history.retry_deadline_epoch = Some(1_120);
    account_history.success_deadline_epoch = Some(1_300);
    account_history.consecutive_failures = 2;
    account_store
        .store(&account_history, now_epoch)
        .expect("persist existing coordinator history");
    account_store
        .load(capability, now_epoch)
        .expect("read coordinator history before migration")
        .expect("coordinator history exists")
}

fn legacy_v2_projection_envelope(
    claude_capability: UsageAccountCapability,
    codex_capability: UsageAccountCapability,
) -> ProjectionStateEnvelope {
    let freshness = UsageFreshnessV1 {
        generation: 7,
        phase: UsageFreshnessPhaseV1::Stale,
        last_good_at_epoch: Some(970),
        retry_at_epoch: Some(1_120),
        is_stale: true,
    };
    let codex_account = UsageAccountV1 {
        canonical_account_id: "codex-account".to_owned(),
        identity_kind: UsageIdentityKindV1::ProviderStableHandle,
        rank: 0,
        display_label: "codex".to_owned(),
        plan_label: None,
        status_label: None,
        lifecycle: UsageLifecycleV1::Available,
        freshness: freshness.clone(),
        provenance_count: 3,
        windows: vec![UsageLimitWindowV1 {
            window_id: "codex-account:0".to_owned(),
            rank: 0,
            category: UsageWindowCategoryV1::Session,
            label: "Session".to_owned(),
            value_label: "72% left".to_owned(),
            reset_label: "in 20 minutes".to_owned(),
            remaining_percent: Some(UsagePercent::clamp_raw(72)),
            remaining_raw_percent: Some(72),
            used_percent: None,
            used_raw_percent: None,
            reset_at_epoch: Some(2_200),
            quota_state: UsageQuotaStateV1::Available,
            pace_label: None,
            runs_out_label: None,
        }],
        metric_groups: Vec::new(),
        credential_expires_at_epoch: None,
        issues: Vec::new(),
    };
    let claude_account = UsageAccountV1 {
        canonical_account_id: "claude-account".to_owned(),
        identity_kind: UsageIdentityKindV1::ProviderAccountId,
        rank: 0,
        display_label: "historic Claude identity".to_owned(),
        plan_label: None,
        status_label: None,
        lifecycle: UsageLifecycleV1::Available,
        freshness: freshness.clone(),
        provenance_count: 2,
        windows: Vec::new(),
        metric_groups: Vec::new(),
        credential_expires_at_epoch: None,
        issues: Vec::new(),
    };
    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "legacy-instance:7".to_owned(),
        generated_at_epoch: 980,
        discovery_revision: "legacy-catalog-revision".to_owned(),
        broker_instance_id: "legacy-instance".to_owned(),
        broker_generation: 7,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        // Schema v2 used raw surface IDs and sorted them lexically, so Claude
        // preceded Codex here. The canonical host order is Codex then Claude.
        providers: vec![
            UsageProviderV1 {
                provider_id: "claude".to_owned(),
                display_name: "claude".to_owned(),
                rank: 0,
                membership_state: UsageMembershipStateV1::Current,
                freshness: freshness.clone(),
                accounts: vec![claude_account],
                issues: Vec::new(),
            },
            UsageProviderV1 {
                provider_id: "codex".to_owned(),
                display_name: "codex".to_owned(),
                rank: 1,
                membership_state: UsageMembershipStateV1::Current,
                freshness,
                accounts: vec![codex_account],
                issues: Vec::new(),
            },
        ],
        unresolved: vec![
            UsageUnresolvedV1 {
                provider_id: "claude".to_owned(),
                capability_id: "claude-candidate".to_owned(),
                configuration_count: 1,
                state: UsageLifecycleV1::NeedsSecret,
                issues: Vec::new(),
            },
            UsageUnresolvedV1 {
                provider_id: "codex".to_owned(),
                capability_id: "codex-candidate".to_owned(),
                configuration_count: 2,
                state: UsageLifecycleV1::NeedsSecret,
                issues: Vec::new(),
            },
        ],
        issues: Vec::new(),
    };
    projection
        .validate()
        .expect("fixture matches schema-v2 row ranks");

    ProjectionStateEnvelope {
        schema_version: ProjectionStateEnvelope::MIGRATABLE_SCHEMA_VERSION,
        projection,
        aliases: vec![ProjectionAlias {
            capability_id: "legacy-capability".to_owned(),
            canonical_account_id: "claude-account".to_owned(),
        }],
        catalog_revision: "legacy-catalog-revision".to_owned(),
        catalog: vec![
            UsageCatalogEntry {
                capability: claude_capability,
                revision: "claude-revision".to_owned(),
            },
            UsageCatalogEntry {
                capability: codex_capability,
                revision: "codex-revision".to_owned(),
            },
        ],
        retry_deadline_epoch: Some(1_150),
        success_deadline_epoch: Some(1_300),
        broker_instance_id: "legacy-instance".to_owned(),
    }
}

fn freshness_for_legacy_projection() -> UsageFreshnessV1 {
    UsageFreshnessV1 {
        generation: 7,
        phase: UsageFreshnessPhaseV1::Stale,
        last_good_at_epoch: Some(970),
        retry_at_epoch: Some(1_120),
        is_stale: true,
    }
}

#[test]
fn canonical_provider_projection_restores_internal_capability_identity_metadata() {
    let freshness = UsageFreshnessV1 {
        generation: 4,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(1_800_000_000),
        retry_at_epoch: None,
        is_stale: false,
    };
    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "fixture:4".to_owned(),
        generated_at_epoch: 1_800_000_000,
        discovery_revision: "fixture-catalog".to_owned(),
        broker_instance_id: "fixture".to_owned(),
        broker_generation: 4,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "openai".to_owned(),
            display_name: "OpenAI".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness: freshness.clone(),
            accounts: vec![UsageAccountV1 {
                canonical_account_id: "stable-handle".to_owned(),
                identity_kind: UsageIdentityKindV1::ProviderStableHandle,
                rank: 0,
                display_label: "work@example.test".to_owned(),
                plan_label: None,
                status_label: None,
                lifecycle: UsageLifecycleV1::Available,
                freshness,
                provenance_count: 3,
                windows: Vec::new(),
                metric_groups: Vec::new(),
                credential_expires_at_epoch: None,
                issues: Vec::new(),
            }],
            issues: Vec::new(),
        }],
        unresolved: Vec::new(),
        issues: Vec::new(),
    };

    let metadata = projection_identity_metadata(&projection);
    let capability = UsageAccountCapability {
        account_id: "stable-handle".to_owned(),
        surface_id: "codex".to_owned(),
    };

    assert_eq!(
        metadata.get(&capability),
        Some(&publish::AccountIdentityMetadata {
            identity_kind: UsageIdentityKindV1::ProviderStableHandle,
            provenance_count: 3,
        })
    );
    assert!(!metadata.contains_key(&UsageAccountCapability {
        surface_id: "openai".to_owned(),
        ..capability
    }));
}

#[test]
fn broker_dispatch_publishes_identity_from_the_accepted_discovery_generation() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject};

    let temp = tempfile::tempdir().expect("isolated broker state directory");
    let binding = ValidatedCredentialBinding {
        surface: HostSurfaceId::Amp,
        identity: Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Amp,
            subject: CanonicalAccountSubject::ProviderStableHandle("amp-team".to_owned()),
        }),
        capability_id: "amp-capability".to_owned(),
        credential_revision: "amp-revision".to_owned(),
        provenance: BTreeSet::from(["account work".to_owned(), "workspace example".to_owned()]),
        source: ValidatedCredentialSource::Profile(
            super::super::discovery::ProfileCredentialMaterial::Amp {
                key: "fixture-credential".to_owned(),
            },
        ),
    };
    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("accepted-generation".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![binding],
    };
    let capability = usage_catalog_entries(&discovery)[0].capability.clone();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        executor,
        Arc::new(FileAccountStateStore::under_data_dir(temp.path())),
        UsageCoordinatorConfig::default(),
        Vec::<UsageCatalogEntry>::new(),
    ));
    let projection = Arc::new(Mutex::new(empty_projection("dispatch-metadata")));
    let publisher = publish::ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::clone(&projection),
        FileProjectionStateStore::under_data_dir(temp.path()),
    );
    let catalog_refresh = catalog::BrokerCatalogRefresh::new(
        UsageDiscoveryScope::Capsule {
            forwarded_accounts: Vec::new(),
        },
        Arc::new(NoopCredentialResolver),
    )
    .with_test_discovery(discovery);
    let monitor_store = monitor::MonitorStore::open(temp.path()).unwrap();
    let forwarded_sources = UsageRelayForwardedSourcesV1 {
        selected_account_ids: BTreeSet::new(),
        selected_account_surfaces: BTreeMap::new(),
        profile_surface_ids: BTreeSet::from(["amp".to_owned()]),
        env_keys: BTreeSet::new(),
        credential_scope: UsageCredentialScope::default(),
    };
    let request = UsageBrokerRequest {
        protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
        build_id: "dispatch-metadata-test".to_owned(),
        operation: UsageBrokerOperation::ResolveRelayCapabilities {
            scope_label: "account work".to_owned(),
            forwarded_sources,
        },
        launch_credential_scope: None,
    };
    let response = dispatch_ops::dispatch(
        &coordinator,
        request,
        "dispatch-metadata-test",
        &publisher,
        &monitor_store,
        &AtomicBool::new(false),
        Some(&catalog_refresh),
    );
    let UsageBrokerResponse::RelayCapabilities { resolution } = response else {
        panic!("relay resolution should accept the fake discovery generation");
    };
    assert_eq!(resolution.capabilities, vec![capability.clone()]);

    publisher.observe(&capability);
    let now = chrono::Utc::now().timestamp();
    let generation = coordinator
        .request_refresh(&capability, 0, true, now)
        .expect("accepted catalog member is refreshable")
        .generation;
    coordinator
        .join_generation(&capability, generation, Duration::from_secs(2), now + 1)
        .expect("fake provider generation completes");
    assert!(publisher.publish_due(now + 2));
    let published = publisher.current_projection().unwrap();
    let account = published
        .providers
        .iter()
        .find(|provider| provider.provider_id == "amp")
        .and_then(|provider| {
            provider
                .accounts
                .iter()
                .find(|account| account.canonical_account_id == capability.account_id)
        })
        .expect("dispatched discovery account reaches projection");
    assert_eq!(
        account.identity_kind,
        UsageIdentityKindV1::ProviderStableHandle
    );
    assert_eq!(account.provenance_count, 2);
}

#[test]
fn grouped_broker_authorization_accepts_sibling_proofs_and_rejects_conflicts() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

    let identity = CanonicalAccountIdentity {
        surface: HostSurfaceId::Zai,
        subject: CanonicalAccountSubject::ProviderStableHandle("zai-account".to_owned()),
    };
    let material = env_material("ZAI_HOST_SECRET", "zai-secret");
    let binding = |handle: &str,
                   identity: Option<CanonicalAccountIdentity>,
                   material: &ProviderCredentialSourceMaterial| {
        ValidatedCredentialBinding {
            surface: HostSurfaceId::Zai,
            identity,
            capability_id: format!("capability-{handle}"),
            credential_revision: format!("revision-{handle}"),
            provenance: BTreeSet::from(["account zai".to_owned()]),
            source: ValidatedCredentialSource::Env {
                handle: OpaqueCredentialHandle::new(handle),
                key: "ZAI_API_KEY".to_owned(),
                dispatch_key: "ZAI_API_KEY".to_owned(),
                launch_keys: BTreeSet::from(["ZHIPU_API_KEY".to_owned(), "ZAI_API_KEY".to_owned()]),
                material: Some(material.clone()),
            },
        }
    };
    let bindings = vec![binding("zai", Some(identity.clone()), &material)];
    let valid = UsageCredentialScope {
        sources: BTreeSet::from([
            UsageCredentialSourceProof {
                account_id: "zai".to_owned(),
                surface_id: "zai".to_owned(),
                key: "ZHIPU_API_KEY".to_owned(),
                source: material.source.clone(),
                material_fingerprint: material.material_fingerprint.clone(),
            },
            UsageCredentialSourceProof {
                account_id: "zai".to_owned(),
                surface_id: "zai".to_owned(),
                key: "ZAI_API_KEY".to_owned(),
                source: material.source.clone(),
                material_fingerprint: material.material_fingerprint.clone(),
            },
        ]),
    };
    assert!(authorize_credential_binding_group(&bindings, "zai", &valid).is_some());

    let mut unrelated = valid.clone();
    unrelated.sources.insert(UsageCredentialSourceProof {
        account_id: "other-account".to_owned(),
        surface_id: "zai".to_owned(),
        key: "Z_AI_API_KEY".to_owned(),
        source: material.source.clone(),
        material_fingerprint: material.material_fingerprint.clone(),
    });
    assert!(authorize_credential_binding_group(&bindings, "zai", &unrelated).is_some());

    let conflicting_material = env_material("ZAI_HOST_SECRET", "different-secret");
    let mut conflict = valid.clone();
    conflict.sources.insert(UsageCredentialSourceProof {
        account_id: "zai".to_owned(),
        surface_id: "zai".to_owned(),
        key: "ZHIPU_API_KEY".to_owned(),
        source: conflicting_material.source.clone(),
        material_fingerprint: conflicting_material.material_fingerprint.clone(),
    });
    assert!(authorize_credential_binding_group(&bindings, "zai", &conflict).is_none());

    let duplicate_conflict = vec![
        bindings[0].clone(),
        binding(
            "zhipu-other",
            Some(CanonicalAccountIdentity {
                surface: HostSurfaceId::Zai,
                subject: CanonicalAccountSubject::ProviderStableHandle("zai-account".to_owned()),
            }),
            &conflicting_material,
        ),
    ];
    assert!(
        authorize_credential_binding_group(
            &duplicate_conflict,
            "zai",
            &env_scope("zai", "zai", "ZHIPU_API_KEY", &material),
        )
        .is_none()
    );
}

#[derive(Default)]
struct RecordingRefreshResolver {
    calls: Mutex<Vec<(String, OpaqueCredentialHandle)>>,
}

impl ProviderCredentialEnvResolver for RecordingRefreshResolver {
    fn resolve_provider_credentials(
        &self,
        _config: &AppConfig,
        _workspace: Option<&WorkspaceName>,
        _role: Option<&str>,
        _keys: &[UsageCredentialEnvName],
    ) -> Vec<ProviderCredentialEnvResolution> {
        Vec::new()
    }

    fn refresh_provider_credential(
        &self,
        _surface: HostSurfaceId,
        key: &str,
        handle: &OpaqueCredentialHandle,
    ) -> ProviderCredentialRefreshOutcome {
        self.calls
            .lock()
            .unwrap()
            .push((key.to_owned(), handle.clone()));
        ProviderCredentialRefreshOutcome::Snapshot {
            view: Box::new(quota_view()),
            rate_limit: None,
            failure_metadata: None,
        }
    }
}

#[test]
fn scoped_probe_refreshes_exact_binding_selected_by_later_sibling_proof() {
    let capability = UsageAccountCapability {
        account_id: "zai".to_owned(),
        surface_id: "zai".to_owned(),
    };
    let material_a = env_material("SOURCE_A", "secret-a");
    let material_b = env_material("SOURCE_B", "secret-b");
    let binding = |handle: &str, key: &str, material: &ProviderCredentialSourceMaterial| {
        ValidatedCredentialBinding {
            surface: HostSurfaceId::Zai,
            identity: None,
            capability_id: "capability-zai".to_owned(),
            credential_revision: format!("revision-{handle}"),
            provenance: BTreeSet::from(["account zai".to_owned()]),
            source: ValidatedCredentialSource::Env {
                handle: OpaqueCredentialHandle::new(handle),
                key: "ZAI_API_KEY".to_owned(),
                dispatch_key: "ZAI_API_KEY".to_owned(),
                launch_keys: BTreeSet::from([key.to_owned()]),
                material: Some(material.clone()),
            },
        }
    };
    let resolver = Arc::new(RecordingRefreshResolver::default());
    let cloned_resolver = Arc::clone(&resolver);
    let resolver_for_executor: Arc<dyn ProviderCredentialEnvResolver> = cloned_resolver;
    let executor = DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::from([(
            capability.clone(),
            vec![
                binding("handle-a", "ZAI_API_KEY", &material_a),
                binding("handle-b", "ZHIPU_API_KEY", &material_b),
            ],
        )])),
        validated_catalog: Mutex::new(None),
        scope: UsageDiscoveryScope::Capsule {
            forwarded_accounts: Vec::new(),
        },
        resolver: resolver_for_executor,
        monitor_store: None,
        collector_service: None,
        collector_liveness: None,
        claude_collector: None,
        probe_budget: Duration::from_secs(1),
    };
    let scope = env_scope("zai", "zai", "ZHIPU_API_KEY", &material_b);
    assert!(matches!(
        probe_with_scope(&executor, &capability, Some(&scope)),
        ProviderProbeOutcome::Success(_)
    ));
    assert_eq!(
        resolver.calls.lock().unwrap().as_slice(),
        &[(
            "ZAI_API_KEY".to_owned(),
            OpaqueCredentialHandle::new("handle-b"),
        )]
    );
}

#[test]
fn mixed_profile_and_env_group_fails_closed_without_changing_pure_profile() {
    let material = env_material("AMP_SOURCE", "amp-secret");
    let profile = ValidatedCredentialBinding {
        surface: HostSurfaceId::Amp,
        identity: None,
        capability_id: "capability".to_owned(),
        credential_revision: "profile-revision".to_owned(),
        provenance: BTreeSet::from(["account shared".to_owned()]),
        source: ValidatedCredentialSource::Profile(
            super::super::discovery::ProfileCredentialMaterial::Amp {
                key: "profile-secret".to_owned(),
            },
        ),
    };
    let env = ValidatedCredentialBinding {
        surface: HostSurfaceId::Amp,
        identity: None,
        capability_id: "capability".to_owned(),
        credential_revision: "env-revision".to_owned(),
        provenance: BTreeSet::from(["account shared".to_owned()]),
        source: ValidatedCredentialSource::Env {
            handle: OpaqueCredentialHandle::new("env-handle"),
            key: "AMP_API_KEY".to_owned(),
            dispatch_key: "AMP_API_KEY".to_owned(),
            launch_keys: BTreeSet::from(["AMP_API_KEY".to_owned()]),
            material: Some(material.clone()),
        },
    };
    let bindings = vec![profile.clone(), env];
    let scope = env_scope("shared", "amp", "AMP_API_KEY", &material);
    assert!(authorize_credential_binding_group(&bindings, "amp", &scope).is_none());
    assert!(authorize_credential_binding_group(&[profile], "amp", &scope).is_some());
}

#[test]
fn capability_identity_keeps_distinct_and_anonymous_sources_separate() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

    let make = |identity: Option<CanonicalAccountIdentity>,
                capability_id: &str,
                handle: &str|
     -> ValidatedCredentialBinding {
        ValidatedCredentialBinding {
            surface: HostSurfaceId::Zai,
            identity,
            capability_id: capability_id.to_owned(),
            credential_revision: "revision".to_owned(),
            provenance: BTreeSet::from(["account zai".to_owned()]),
            source: ValidatedCredentialSource::Env {
                handle: OpaqueCredentialHandle::new(handle),
                key: "ZAI_API_KEY".to_owned(),
                dispatch_key: "ZAI_API_KEY".to_owned(),
                launch_keys: BTreeSet::from(["ZHIPU_API_KEY".to_owned()]),
                material: Some(env_material("ZAI_HOST_SECRET", "zai-secret")),
            },
        }
    };
    let first = make(
        Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Zai,
            subject: CanonicalAccountSubject::ProviderStableHandle("first".to_owned()),
        }),
        "first",
        "handle-first",
    );
    let second = make(
        Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Zai,
            subject: CanonicalAccountSubject::ProviderStableHandle("second".to_owned()),
        }),
        "second",
        "handle-second",
    );
    assert_ne!(
        capability_for_binding(&first, None),
        capability_for_binding(&second, None)
    );

    let anonymous_first = make(None, "anonymous-first", "handle-anonymous-first");
    let anonymous_second = make(None, "anonymous-second", "handle-anonymous-second");
    assert_ne!(
        capability_for_binding(&anonymous_first, None),
        capability_for_binding(&anonymous_second, None),
        "anonymous bindings remain source-specific"
    );

    let source_capability = make(
        Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Zai,
            subject: CanonicalAccountSubject::SourceCapability("local-source".to_owned()),
        }),
        "local-source",
        "handle-local-source",
    );
    let provider_id = make(
        Some(CanonicalAccountIdentity {
            surface: HostSurfaceId::Zai,
            subject: CanonicalAccountSubject::ProviderId("provider-account".to_owned()),
        }),
        "provider-account",
        "handle-provider-account",
    );
    let discovery = ValidatedUsageDiscovery {
        config_generation: None,
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![
            first.clone(),
            source_capability.clone(),
            anonymous_first.clone(),
            provider_id.clone(),
        ],
    };
    let metadata = publication_identity_metadata(&discovery);
    assert_eq!(
        metadata[&capability_for_binding(&first, None)].identity_kind,
        UsageIdentityKindV1::ProviderStableHandle
    );
    assert_eq!(
        metadata[&capability_for_binding(&source_capability, None)].identity_kind,
        UsageIdentityKindV1::LocalSourceHandle
    );
    assert_eq!(
        metadata[&capability_for_binding(&anonymous_first, None)].identity_kind,
        UsageIdentityKindV1::UnverifiedHandle
    );
    assert_eq!(
        metadata[&capability_for_binding(&provider_id, None)].identity_kind,
        UsageIdentityKindV1::ProviderAccountId
    );
}

#[test]
fn broker_catalog_match_requires_full_revision_and_entry_revisions() {
    use crate::host::{CanonicalAccountIdentity, CanonicalAccountSubject, HostSurfaceId};

    let discovery = ValidatedUsageDiscovery {
        config_generation: Some("generation-current".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: vec![ValidatedCredentialBinding {
            surface: HostSurfaceId::Claude,
            identity: Some(CanonicalAccountIdentity {
                surface: HostSurfaceId::Claude,
                subject: CanonicalAccountSubject::ProviderId("provider-account".to_owned()),
            }),
            capability_id: "capability-0001".to_owned(),
            credential_revision: "credential-revision-a".to_owned(),
            provenance: BTreeSet::from(["account work".to_owned()]),
            source: ValidatedCredentialSource::Capability,
        }],
    };
    let entries = usage_catalog_entries(&discovery);
    ensure_catalog_matches(&discovery, "generation-current", &entries).unwrap();

    let mut changed_entries = entries.clone();
    changed_entries[0].revision.push_str("-changed");
    assert_eq!(
        ensure_catalog_matches(&discovery, "generation-current", &changed_entries)
            .unwrap_err()
            .kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
    assert_eq!(
        ensure_catalog_matches(&discovery, "generation-old", &entries)
            .unwrap_err()
            .kind,
        UsageCoordinationErrorKind::CatalogRevisionConflict
    );
}

#[test]
fn usage_broker_twenty_clients_join_one_generation_and_probe() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        broker_executor,
    )
    .unwrap();
    let barrier = Arc::new(Barrier::new(20));
    let mut clients = Vec::new();
    for _ in 0..20 {
        let client = client.clone();
        let barrier = Arc::clone(&barrier);
        clients.push(thread::spawn(move || {
            barrier.wait();
            client.refresh(non_claude_capability(), 0, true).unwrap()
        }));
    }
    let generations = clients
        .into_iter()
        .map(|client| client.join().unwrap())
        .collect::<Vec<_>>();
    assert!(generations.iter().all(|state| state.generation == 1));
    let terminal = client
        .join(non_claude_capability(), 1, Duration::from_secs(2))
        .unwrap();
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn usage_broker_handshake_mismatch_fails_before_provider_dispatch() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(config.clone(), broker_executor).unwrap();
    let incompatible = UsageBrokerClient::at(client.socket_path, "other-build".to_owned());
    let error = incompatible.refresh(capability(), 0, true).unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::ProtocolMismatch);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn broker_client_scoped_operation_requires_relay_and_never_probes() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        broker_executor,
    )
    .unwrap();

    let error = client
        .current_for_capability(UsageAccountCapability {
            account_id: "account-a".to_owned(),
            surface_id: "claude".to_owned(),
        })
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn usage_broker_recovers_stale_guard_with_private_permissions() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let run_dir = secure_run_directory(&config.data_dir).unwrap();
    let leader = run_dir.join(BROKER_LEADER);
    let mut stale_lease = BrokerLease::new(&config.build_id);
    stale_lease.renewed_at_epoch -= i64::try_from(config.lease_duration.as_secs()).unwrap() + 1;
    fs::write(&leader, serde_json::to_vec(&stale_lease).unwrap()).unwrap();
    fs::set_permissions(&leader, fs::Permissions::from_mode(0o600)).unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });

    let client = ensure_usage_broker_with_executor(config.clone(), executor).unwrap();
    assert!(connect_probe(&client));
    assert_eq!(fs::metadata(run_dir).unwrap().mode() & 0o777, 0o700);
    assert_eq!(
        fs::metadata(config.socket_path()).unwrap().mode() & 0o777,
        0o600
    );
    assert_eq!(fs::metadata(leader).unwrap().mode() & 0o777, 0o600);
}

#[test]
fn broker_startup_failure_cleans_lease_and_socket_before_returning() {
    let temp = tempfile::tempdir().unwrap();
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let projection = temp.path().join(BROKER_DIR).join("projection.json");
    fs::create_dir_all(projection.parent().unwrap()).unwrap();
    fs::create_dir(&projection).unwrap();

    let error = ensure_usage_broker_with_executor(
        config.clone(),
        Arc::new(CountingExecutor {
            calls: AtomicUsize::new(0),
        }),
    )
    .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);

    let run_dir = temp.path().join(BROKER_DIR).join(BROKER_RUN_DIR);
    assert!(!run_dir.join(BROKER_LEADER).exists());
    assert!(!config.socket_path().exists());
}

#[test]
fn live_lease_owner_blocks_expired_takeover_and_can_renew_after_waking() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lease");
    let duration = Duration::from_secs(30);
    let started_at = 1_700_000_000;
    let woke_at = started_at + i64::try_from(duration.as_secs()).unwrap() + 1;
    let mut owner = claim_leader_at(&path, "build", duration, started_at)
        .unwrap()
        .expect("first claimant owns the lifetime lock");

    assert!(
        claim_leader_at(&path, "build", duration, woke_at)
            .unwrap()
            .is_none(),
        "expired wall time cannot replace a live owner's locked lease"
    );
    assert!(renew_lease_at(&path, &mut owner, woke_at));
    let renewed: BrokerLease = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(renewed.instance_id, owner.lease.instance_id);
    assert_eq!(renewed.renewed_at_epoch, woke_at);
    assert!(
        claim_leader_at(
            &path,
            "build",
            duration,
            woke_at + i64::try_from(duration.as_secs()).unwrap() + 1,
        )
        .unwrap()
        .is_none(),
        "the lifetime lock continues to protect an owner past the next expiry"
    );
}

#[test]
fn lease_renewal_does_not_regress_timestamp_when_clock_moves_backward() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lease");
    let duration = Duration::from_secs(30);
    let started_at = 1_700_000_000;
    let earlier_time = started_at - 10;
    let mut owner = claim_leader_at(&path, "build", duration, started_at)
        .unwrap()
        .expect("first claimant owns the lifetime lock");

    assert!(renew_lease_at(&path, &mut owner, earlier_time));
    let renewed: BrokerLease = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(renewed.renewed_at_epoch, started_at);
    assert_eq!(owner.lease.renewed_at_epoch, started_at);
}

#[test]
fn lease_renewal_rejects_tampered_owner_pid() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lease");
    let mut owner = claim_leader_at(&path, "build", Duration::from_secs(30), 1_700_000_000)
        .unwrap()
        .expect("first claimant owns the lifetime lock");
    let mut tampered = owner.lease.clone();
    tampered.process_id ^= 1;
    write_lease(&mut owner.file, &tampered).unwrap();

    assert!(!renew_lease_at(&path, &mut owner, 1_700_000_010));
    let persisted: BrokerLease = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(persisted.process_id, tampered.process_id);
}

#[test]
fn lease_renewal_rejects_tampered_owner_timestamp() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lease");
    let mut owner = claim_leader_at(&path, "build", Duration::from_secs(30), 1_700_000_000)
        .unwrap()
        .expect("first claimant owns the lifetime lock");
    let mut tampered = owner.lease.clone();
    tampered.renewed_at_epoch += 1;
    write_lease(&mut owner.file, &tampered).unwrap();

    assert!(!renew_lease_at(&path, &mut owner, 1_700_000_010));
    let persisted: BrokerLease = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(persisted.renewed_at_epoch, tampered.renewed_at_epoch);
}

#[test]
fn expired_lease_is_reclaimed_after_dead_owner_releases_lifetime_lock() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lease");
    let duration = Duration::from_secs(30);
    let started_at = 1_700_000_000;
    let expired_at = started_at + i64::try_from(duration.as_secs()).unwrap() + 1;
    let owner = claim_leader_at(&path, "build", duration, started_at)
        .unwrap()
        .expect("first claimant owns the lifetime lock");
    let original_instance = owner.lease.instance_id.clone();

    assert!(
        claim_leader_at(&path, "build", duration, expired_at)
            .unwrap()
            .is_none(),
        "a live owner's lock wins over its expired payload"
    );
    drop(owner);

    assert!(
        claim_leader_at(&path, "other-build", duration, expired_at)
            .unwrap()
            .is_none(),
        "an incompatible build cannot reclaim the expired payload"
    );
    let replacement = claim_leader_at(&path, "build", duration, expired_at)
        .unwrap()
        .expect("dead owner's expired lease can be reclaimed");
    assert_ne!(replacement.lease.instance_id, original_instance);
    assert!(replacement.stale_lease_reclaimed);
}

#[test]
fn broker_refuses_legacy_pid_lease_without_mutating_it() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lease");
    let legacy_pid = b"2147483647\n";
    fs::write(&path, legacy_pid).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    assert!(
        claim_leader(&path, "build", Duration::from_secs(30))
            .unwrap()
            .is_none(),
        "unknown legacy lease formats cannot authorize takeover"
    );
    assert_eq!(fs::read(&path).unwrap(), legacy_pid);
}

#[test]
fn stale_lease_descriptor_cannot_renew_or_clean_successor_files() {
    let temp = tempfile::tempdir().unwrap();
    let lease_path = temp.path().join("lease");
    let socket_path = temp.path().join("socket");
    let mut stale = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("first claimant owns the lease");
    fs::write(&socket_path, b"successor socket").unwrap();

    fs::remove_file(&lease_path).unwrap();
    let successor = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("successor owns the replacement lease");
    let successor_id = successor.lease.instance_id.clone();

    assert!(!renew_lease(&lease_path, &mut stale));
    assert!(!cleanup_owned_files(
        &lease_path,
        &socket_path,
        None,
        &mut stale
    ));
    let current: BrokerLease = serde_json::from_slice(&fs::read(&lease_path).unwrap()).unwrap();
    assert_eq!(current.instance_id, successor_id);
    assert!(socket_path.exists());
}

#[test]
fn renamed_stale_lease_descriptor_cannot_unlink_successor_lease_or_socket() {
    let temp = tempfile::tempdir().unwrap();
    let lease_path = temp.path().join("lease");
    let renamed_lease_path = temp.path().join("renamed-lease");
    let socket_path = temp.path().join("socket");
    let mut stale = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("first claimant owns the lease");

    fs::rename(&lease_path, &renamed_lease_path).unwrap();
    let successor = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("successor claims a new path inode");
    let successor_id = successor.lease.instance_id.clone();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let socket_metadata = fs::symlink_metadata(&socket_path).unwrap();
    let socket_identity = BrokerSocketIdentity {
        device: socket_metadata.dev(),
        inode: socket_metadata.ino(),
    };

    assert!(!renew_lease_at(&lease_path, &mut stale, 1_700_000_100));
    assert!(!cleanup_owned_files(
        &lease_path,
        &socket_path,
        Some(socket_identity),
        &mut stale,
    ));
    let current: BrokerLease = serde_json::from_slice(&fs::read(&lease_path).unwrap()).unwrap();
    assert_eq!(current.instance_id, successor_id);
    assert!(renamed_lease_path.exists());
    let after_socket = fs::symlink_metadata(&socket_path).unwrap();
    assert_eq!(
        (after_socket.dev(), after_socket.ino()),
        (socket_identity.device, socket_identity.inode),
    );
    drop(listener);
}

#[test]
fn usage_broker_rejects_symlinked_run_tree_without_mutating_target() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("data");
    let target = temp.path().join("target");
    fs::create_dir(&data_dir).unwrap();
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
    symlink(&target, data_dir.join(BROKER_DIR)).unwrap();
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });

    let result =
        ensure_usage_broker_with_executor(UsageBrokerConfig::for_data_dir(data_dir), executor);
    result.unwrap_err();
    assert_eq!(fs::metadata(target).unwrap().mode() & 0o777, 0o755);
}

struct HeldExecutor {
    started: mpsc::SyncSender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl UsageProviderExecutor for HeldExecutor {
    fn probe(&self, _: &UsageAccountCapability, _: u64) -> ProviderProbeOutcome {
        self.started.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        ProviderProbeOutcome::success(quota_view())
    }
}

#[test]
fn saturated_join_waiters_do_not_block_refresh_or_current() {
    let temp = tempfile::tempdir().unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(HeldExecutor {
        started: started_tx,
        release: Mutex::new(release_rx),
    });
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let client = ensure_usage_broker_with_executor(config.clone(), executor).unwrap();
    let active = client.refresh(non_claude_capability(), 0, true).unwrap();
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let mut waiters = Vec::new();
    for _ in 0..BROKER_CONNECTION_WORKERS * 2 {
        let mut stream = UnixStream::connect(config.socket_path()).unwrap();
        let request = UsageBrokerRequest {
            protocol_version: USAGE_BROKER_PROTOCOL_VERSION.to_owned(),
            build_id: config.build_id.clone(),
            operation: UsageBrokerOperation::Join {
                capability: non_claude_capability(),
                generation: active.generation,
                timeout_ms: 10_000,
            },
            launch_credential_scope: None,
        };
        let mut bytes = serde_json::to_vec(&request).unwrap();
        bytes.push(b'\n');
        stream.write_all(&bytes).unwrap();
        waiters.push(stream);
    }
    let (response_tx, response_rx) = mpsc::sync_channel(1);
    let control = client.clone();
    let request = thread::spawn(move || {
        let started = Instant::now();
        let short_wait = control.join(
            non_claude_capability(),
            active.generation,
            Duration::from_millis(1),
        );
        let elapsed = started.elapsed();
        let result = control
            .refresh(non_claude_capability(), 0, true)
            .and_then(|_| control.current(non_claude_capability()));
        response_tx.send((short_wait, elapsed, result)).unwrap();
    });
    let response = response_rx.recv_timeout(Duration::from_secs(2));
    // Always release the provider before asserting, so a failed regression
    // cannot strand fixture threads or turn cleanup into another timeout.
    release_tx.send(()).unwrap();
    request.join().unwrap();
    let (short_wait, elapsed, response) =
        response.expect("long polls starved a short wait or control requests");
    assert_eq!(
        short_wait.unwrap_err().kind,
        UsageCoordinationErrorKind::WaitTimeout
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "short join queued behind unrelated long polls"
    );
    let response = response.unwrap();
    assert_eq!(response.generation, active.generation);
    assert!(response.phase.is_active());
    for mut waiter in waiters {
        let response: UsageBrokerResponse = read_frame(&mut waiter).unwrap();
        assert!(
            matches!(response, UsageBrokerResponse::State { state } if state.phase == UsageRefreshPhase::Completed)
        );
    }
}

#[test]
fn stalled_response_reader_does_not_hold_worker_shutdown() {
    let (mut server, client) = UnixStream::pair().unwrap();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let bytes = vec![b'x'; 8 * 1024 * 1024];
        write_with_deadline(&mut server, &bytes, Duration::from_millis(50));
        done_tx.send(()).unwrap();
    });
    let finished = done_rx.recv_timeout(Duration::from_secs(1));
    // Even the failing implementation can be joined once the peer closes.
    drop(client);
    worker.join().unwrap();
    assert!(
        finished.is_ok(),
        "stalled reader prevented bounded worker shutdown"
    );
}

#[test]
fn subscribe_all_dedups_reuses_fresh_and_forces_only_on_demand() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        broker_executor,
    )
    .unwrap();

    // Due-on-open with a duplicated capability issues one request per account.
    let opened = client.subscribe_all([
        non_claude_capability(),
        second_capability(),
        non_claude_capability(),
    ]);
    assert_eq!(opened.len(), 2);
    assert!(opened.iter().all(|(_, result)| result.is_ok()));
    assert_eq!(
        client.subscriptions(),
        vec![non_claude_capability(), second_capability()]
    );
    for (_, result) in &opened {
        let view = result.as_ref().unwrap();
        client
            .join(
                view.capability.clone(),
                view.generation,
                Duration::from_secs(5),
            )
            .unwrap();
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    // Still-fresh observations are reused; nothing new is forced.
    let reopened = client.subscribe_all([non_claude_capability(), second_capability()]);
    assert!(reopened.iter().all(|(_, result)| result.is_ok()));
    let heartbeat = client.refresh_due(false);
    assert_eq!(heartbeat.len(), 2);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    // An explicit operator refresh bypasses the success cooldown exactly once.
    let forced = client.refresh_due(true);
    assert!(forced.iter().all(|(_, result)| result.is_ok()));
    for (_, result) in &forced {
        let view = result.as_ref().unwrap();
        assert_eq!(view.generation, 2);
        client
            .join(
                view.capability.clone(),
                view.generation,
                Duration::from_secs(5),
            )
            .unwrap();
    }
    assert_eq!(executor.calls.load(Ordering::SeqCst), 4);
}

#[test]
fn unsubscribe_releases_local_interest_without_cancelling_shared_work() {
    let temp = tempfile::tempdir().unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(HeldExecutor {
        started: started_tx,
        release: Mutex::new(release_rx),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();

    let opened = client.subscribe(non_claude_capability()).unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    // Prompt unsubscribe performs no broker I/O and leaves the broker-owned
    // generation untouched.
    assert!(client.unsubscribe(&non_claude_capability()));
    assert!(!client.unsubscribe(&non_claude_capability()));
    assert!(client.subscriptions().is_empty());
    let active = client.current(non_claude_capability()).unwrap();
    assert_eq!(active.generation, opened.generation);
    assert!(active.phase.is_active());

    // Another client awaiting the same generation still observes terminal.
    release_tx.send(()).unwrap();
    let waiter = client.clone();
    let terminal = waiter
        .join(
            non_claude_capability(),
            opened.generation,
            Duration::from_secs(5),
        )
        .unwrap();
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    assert!(terminal.snapshot.is_some());
}

#[test]
fn client_clone_forks_subscription_set() {
    let temp = tempfile::tempdir().unwrap();
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();
    client.subscribe(non_claude_capability()).unwrap();
    let fork = client.clone();

    assert!(fork.unsubscribe(&non_claude_capability()));
    assert_eq!(fork.subscriptions(), Vec::new());
    assert_eq!(client.subscriptions(), vec![non_claude_capability()]);
    assert_eq!(
        client.observed_generation(&non_claude_capability()),
        Some(1)
    );

    client.unsubscribe_all();
    assert!(client.subscriptions().is_empty());
}

struct StallOneExecutor {
    slow: UsageAccountCapability,
    release: Mutex<mpsc::Receiver<()>>,
}

impl UsageProviderExecutor for StallOneExecutor {
    fn probe(&self, capability: &UsageAccountCapability, _generation: u64) -> ProviderProbeOutcome {
        if *capability == self.slow {
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(15))
                .unwrap();
        }
        ProviderProbeOutcome::success(quota_view())
    }
}

#[test]
fn healthy_accounts_publish_while_one_account_stalls() {
    let temp = tempfile::tempdir().unwrap();
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(StallOneExecutor {
        slow: second_capability(),
        release: Mutex::new(release_rx),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();

    let before = client.current_projection().unwrap();
    let opened = client.subscribe_all([non_claude_capability(), second_capability()]);
    assert!(opened.iter().all(|(_, result)| result.is_ok()));
    let fast = opened
        .iter()
        .find(|(item, _)| *item == non_claude_capability())
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .clone();
    client
        .join(
            non_claude_capability(),
            fast.generation,
            Duration::from_secs(5),
        )
        .unwrap();

    // The healthy account is published with data while the stalled account
    // keeps its refreshing state; the catalog revision never changes.
    let partial = client.current_projection().unwrap();
    partial.validate().unwrap();
    assert_eq!(partial.discovery_revision, before.discovery_revision);
    assert!(partial.broker_generation > before.broker_generation);
    assert_eq!(
        partial.refresh_state,
        UsageProjectionRefreshStateV1::Refreshing
    );
    let providers = partial
        .providers
        .iter()
        .map(|provider| provider.provider_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(providers, vec!["openai", "amp"]);
    let fast_account = partial
        .providers
        .iter()
        .flat_map(|provider| provider.accounts.iter())
        .find(|account| account.canonical_account_id == "abc123")
        .unwrap();
    assert_eq!(fast_account.freshness.phase, UsageFreshnessPhaseV1::Current);
    assert!(!fast_account.windows.is_empty());
    let slow_account = partial
        .providers
        .iter()
        .flat_map(|provider| provider.accounts.iter())
        .find(|account| account.canonical_account_id == "def456")
        .unwrap();
    assert_eq!(
        slow_account.freshness.phase,
        UsageFreshnessPhaseV1::Refreshing
    );
    assert!(slow_account.windows.is_empty());

    release_tx.send(()).unwrap();
    let slow = opened
        .iter()
        .find(|(item, _)| *item == second_capability())
        .unwrap()
        .1
        .as_ref()
        .unwrap()
        .clone();
    client
        .join(second_capability(), slow.generation, Duration::from_secs(5))
        .unwrap();
    let settled = client.current_projection().unwrap();
    settled.validate().unwrap();
    assert_eq!(settled.discovery_revision, before.discovery_revision);
    assert!(settled.broker_generation > partial.broker_generation);
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert!(
        settled
            .providers
            .iter()
            .flat_map(|provider| &provider.accounts)
            .all(|account| account.freshness.phase == UsageFreshnessPhaseV1::Current)
    );
}

#[test]
fn projection_refresh_runs_due_checks_and_join_settles() {
    let temp = tempfile::tempdir().unwrap();
    let executor = Arc::new(CountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        broker_executor,
    )
    .unwrap();
    client.subscribe(non_claude_capability()).unwrap();
    client
        .join(non_claude_capability(), 1, Duration::from_secs(5))
        .unwrap();
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    // A non-forced projection refresh reuses the still-fresh observation.
    let reused = client.request_refresh(None, false).unwrap();
    reused.validate().unwrap();
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert!(
        reused
            .providers
            .iter()
            .flat_map(|provider| &provider.accounts)
            .any(|account| account.canonical_account_id == "abc123")
    );

    // A forced projection refresh starts one new generation and the join
    // observes it settle without cancelling broker ownership.
    //
    // Join returns a superseding publication immediately by design, and
    // every intermediate publish mints a fresh publication id, so a single
    // join can observe a still-Refreshing snapshot under load. Chase the
    // chain until Idle or the deadline, like any correct caller must.
    let refreshing = client.request_refresh(None, true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut target = refreshing.projection_id.clone();
    let settled = loop {
        let observed = client
            .join_publication(target.clone(), Duration::from_secs(5))
            .unwrap();
        if observed.refresh_state == UsageProjectionRefreshStateV1::Idle
            || Instant::now() >= deadline
        {
            break observed;
        }
        target = observed.projection_id.clone();
    };
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    // A superseded or unknown publication id returns the latest publication.
    let latest = client
        .join_publication("usage-broker:unknown".to_owned(), Duration::from_secs(5))
        .unwrap();
    assert_eq!(latest.projection_id, settled.projection_id);
}

#[test]
fn join_publication_timeout_leaves_broker_ownership_intact() {
    let temp = tempfile::tempdir().unwrap();
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(HeldExecutor {
        started: started_tx,
        release: Mutex::new(release_rx),
    });
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_owned()),
        executor,
    )
    .unwrap();
    client.subscribe(non_claude_capability()).unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Wait for a quiesced refreshing publication: once the id is stable
    // across a ticker interval, no publish can interleave with the join below
    // until the probe is released.
    let refreshing = loop {
        let first = client.current_projection().unwrap();
        thread::park_timeout(Duration::from_millis(250));
        let second = client.current_projection().unwrap();
        if first.projection_id == second.projection_id
            && second.refresh_state == UsageProjectionRefreshStateV1::Refreshing
        {
            break second;
        }
    };

    let error = client
        .join_publication(refreshing.projection_id.clone(), Duration::from_millis(50))
        .unwrap_err();
    assert_eq!(error.kind, UsageCoordinationErrorKind::WaitTimeout);

    // The timed-out join cancelled nothing: releasing the probe still settles
    // the same account generation into a newer publication.
    release_tx.send(()).unwrap();
    let settled = client
        .join_publication(refreshing.projection_id, Duration::from_secs(5))
        .unwrap();
    assert_eq!(settled.refresh_state, UsageProjectionRefreshStateV1::Idle);
    assert!(settled.broker_generation > refreshing.broker_generation);
}

#[test]
fn probe_budget_returns_fast_and_expires_without_waiting() {
    let fast = probe::run_probe_with_budget(Duration::from_secs(5), || 7_u32).unwrap();
    assert_eq!(fast, 7);

    let started = Instant::now();
    let expired = probe::run_probe_with_budget(Duration::from_millis(20), || {
        thread::park_timeout(Duration::from_secs(30));
        7_u32
    });
    assert_eq!(expired, Err(probe::ProbeBudgetExpired));
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "budget expiry waited for the probe"
    );

    let timeout = probe::probe_timeout_outcome();
    let ProviderProbeOutcome::Failure {
        kind,
        message,
        retry_at_epoch,
    } = timeout
    else {
        panic!("budget expiry must report failure, never empty success");
    };
    assert_eq!(kind, UsageCoordinationErrorKind::ProviderTimeout);
    assert!(!message.is_empty());
    assert_eq!(retry_at_epoch, None);
}

#[test]
fn probe_budget_propagates_worker_panic_to_coordinator_classification() {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        probe::run_probe_with_budget(Duration::from_secs(5), || {
            panic!("adapter panic must reach the coordinator")
        })
    }));
    assert!(
        outcome.is_err(),
        "worker panic must propagate to the caller"
    );
}

struct FakeForegroundState {
    bootstrap_calls: AtomicUsize,
    guard_calls: AtomicUsize,
    credential_drops: AtomicUsize,
    guard_drops: AtomicUsize,
    credential_active: AtomicBool,
    credential_generation: AtomicU64,
    guard_active: AtomicBool,
    cached_secret: Mutex<Option<Zeroizing<String>>>,
    call_order: Mutex<Vec<&'static str>>,
}

impl Default for FakeForegroundState {
    fn default() -> Self {
        Self {
            bootstrap_calls: AtomicUsize::new(0),
            guard_calls: AtomicUsize::new(0),
            credential_drops: AtomicUsize::new(0),
            guard_drops: AtomicUsize::new(0),
            credential_active: AtomicBool::new(false),
            credential_generation: AtomicU64::new(0),
            guard_active: AtomicBool::new(false),
            cached_secret: Mutex::new(None),
            call_order: Mutex::new(Vec::new()),
        }
    }
}

struct FakeForegroundCredentialLease(Arc<FakeForegroundState>);

impl service::ForegroundCredentialLease for FakeForegroundCredentialLease {
    fn generation(&self) -> u64 {
        let generation = self.0.credential_generation.load(Ordering::SeqCst);
        assert_ne!(generation, 0, "fake lease must carry its test generation");
        generation
    }
}

impl Drop for FakeForegroundCredentialLease {
    fn drop(&mut self) {
        self.0
            .cached_secret
            .lock()
            .expect("fake cache mutex")
            .take();
        self.0.credential_active.store(false, Ordering::SeqCst);
        self.0.credential_drops.fetch_add(1, Ordering::SeqCst);
    }
}

struct FakeForegroundKeychainGuard(Arc<FakeForegroundState>);

impl Drop for FakeForegroundKeychainGuard {
    fn drop(&mut self) {
        self.0.guard_active.store(false, Ordering::SeqCst);
        self.0.guard_drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn host_desktop_scope(root: &Path) -> UsageDiscoveryScope {
    UsageDiscoveryScope::HostDesktop {
        config_root: root.to_owned(),
        operator_home: root.to_owned(),
    }
}

struct CodexSeedExecutor;

impl UsageProviderExecutor for CodexSeedExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        let mut view = quota_view();
        view.focused_provider = Some("codex".to_owned());
        view.account.provider_label = "Codex".to_owned();
        view.account.account_label = "isolated-codex-fixture".to_owned();
        ProviderProbeOutcome::success(view)
    }
}

fn seed_prior_codex_projection(data_dir: &Path) -> UsageAccountCapability {
    let now_epoch = chrono::Utc::now().timestamp();
    let capability = second_capability();
    let entry = UsageCatalogEntry {
        capability: capability.clone(),
        revision: "prior-codex-source-revision".to_owned(),
    };
    let coordinator = Arc::new(UsageCoordinator::with_catalog(
        Arc::new(CodexSeedExecutor),
        Arc::new(FileAccountStateStore::under_data_dir(data_dir)),
        UsageCoordinatorConfig::default(),
        [entry.clone()],
    ));
    let queued = coordinator
        .request_refresh(&capability, 0, true, now_epoch)
        .expect("seed prior Codex account state");
    let settled = coordinator
        .join_generation(
            &capability,
            queued.generation,
            Duration::from_secs(5),
            now_epoch,
        )
        .expect("settle prior Codex account state");
    assert_eq!(settled.phase, UsageRefreshPhase::Completed);

    let publisher = publish::ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::new(Mutex::new(empty_projection("prior-catalog"))),
        FileProjectionStateStore::under_data_dir(data_dir),
    )
    .with_catalog([entry]);
    publisher.observe(&capability);
    assert!(publisher.publish_due(now_epoch));
    let prior_envelope = FileProjectionStateStore::under_data_dir(data_dir)
        .load()
        .expect("read seeded projection")
        .expect("seeded projection exists");
    assert_eq!(
        prior_envelope.catalog,
        vec![UsageCatalogEntry {
            capability: capability.clone(),
            revision: "prior-codex-source-revision".to_owned(),
        }]
    );
    let account = FileAccountStateStore::under_data_dir(data_dir)
        .load(&capability, now_epoch)
        .expect("read seeded account state")
        .expect("seeded account state exists");
    assert!(
        account
            .success_deadline_epoch
            .is_some_and(|deadline| deadline > now_epoch)
    );
    drop(coordinator);
    capability
}

#[test]
fn foreground_fake_bootstrap_holds_the_same_zeroizing_lease_through_catalog_ready_and_service() {
    use super::service::{
        ForegroundBootstrapOutcome, run_usage_broker_foreground_bootstrap_with_for_test,
    };

    let temp = tempfile::tempdir().expect("isolated foreground data directory");
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let client = config.client();
    let removed_sibling = seed_prior_codex_projection(&config.data_dir);
    let service = "isolated-claude-keychain-service";
    let state = Arc::new(FakeForegroundState::default());
    let (ready_sender, ready_receiver) = mpsc::channel();
    let worker_state = Arc::clone(&state);
    let worker_config = config.clone();
    let worker_scope = host_desktop_scope(temp.path());
    let service_name = service.to_owned();
    let service_thread = thread::spawn(move || {
        let expected_service = service_name.clone();
        run_usage_broker_foreground_bootstrap_with_for_test(
            worker_config,
            worker_scope,
            &service_name,
            {
                let state = Arc::clone(&worker_state);
                move |selected_service| {
                    assert_eq!(selected_service, expected_service);
                    state.bootstrap_calls.fetch_add(1, Ordering::SeqCst);
                    state
                        .call_order
                        .lock()
                        .expect("fake call-order mutex")
                        .push("bootstrap");
                    *state.cached_secret.lock().expect("fake cache mutex") =
                        Some(Zeroizing::new("test-only in-process credential".to_owned()));
                    state.credential_active.store(true, Ordering::SeqCst);
                    state.credential_generation.store(7, Ordering::SeqCst);
                    Ok(ForegroundBootstrapOutcome::Acquired(
                        FakeForegroundCredentialLease(state),
                    ))
                }
            },
            {
                let state = Arc::clone(&worker_state);
                move || {
                    assert!(state.credential_active.load(Ordering::SeqCst));
                    assert!(
                        state
                            .cached_secret
                            .lock()
                            .expect("fake cache mutex")
                            .is_some()
                    );
                    state.guard_calls.fetch_add(1, Ordering::SeqCst);
                    state
                        .call_order
                        .lock()
                        .expect("fake call-order mutex")
                        .push("guard");
                    state.guard_active.store(true, Ordering::SeqCst);
                    Ok(FakeForegroundKeychainGuard(state))
                }
            },
            move |ready| {
                assert!(worker_state.credential_active.load(Ordering::SeqCst));
                assert!(worker_state.guard_active.load(Ordering::SeqCst));
                let cache = worker_state.cached_secret.lock().expect("fake cache mutex");
                assert!(cache.is_some());
                drop(cache);
                ready_sender
                    .send(ready)
                    .expect("send secret-free ready metadata");
            },
        )
    });

    let ready = ready_receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("foreground broker reports readiness after catalog reconciliation");
    assert_foreground_ready_projection(&config, &client, service, &ready, &state, &removed_sibling);

    assert!(state.credential_active.load(Ordering::SeqCst));
    assert!(state.guard_active.load(Ordering::SeqCst));
    assert!(
        state
            .cached_secret
            .lock()
            .expect("fake cache mutex")
            .is_some()
    );
    assert_eq!(
        client
            .monitor(MonitorOperation::ServiceStop)
            .expect("stop isolated foreground broker"),
        MonitorReply::ServiceStopped
    );
    let outcome = service_thread
        .join()
        .expect("join foreground broker thread")
        .expect("foreground service exits cleanly");
    assert!(state.credential_active.load(Ordering::SeqCst));
    assert!(!state.guard_active.load(Ordering::SeqCst));
    assert!(
        state
            .cached_secret
            .lock()
            .expect("fake cache mutex")
            .is_some()
    );
    assert_eq!(state.guard_drops.load(Ordering::SeqCst), 1);
    let ForegroundBootstrapOutcome::Acquired(credential_lease) = outcome else {
        panic!("foreground service returns the acquired process-local lease");
    };
    drop(credential_lease);
    assert!(!state.credential_active.load(Ordering::SeqCst));
    assert!(
        state
            .cached_secret
            .lock()
            .expect("fake cache mutex")
            .is_none()
    );
    assert_eq!(state.credential_drops.load(Ordering::SeqCst), 1);
}

fn assert_foreground_ready_projection(
    config: &UsageBrokerConfig,
    client: &UsageBrokerClient,
    service: &str,
    ready: &UsageBrokerForegroundReady,
    state: &FakeForegroundState,
    removed_sibling: &UsageAccountCapability,
) {
    assert_eq!(
        ready.capability,
        claude_usage_capability_for_service(service)
    );
    assert_eq!(ready.binding_scope, "claude_keychain_service");
    assert_ne!(ready.binding_scope, service);
    let serialized_ready = serde_json::to_string(&(
        &ready.capability.surface_id,
        &ready.capability.account_id,
        &ready.binding_scope,
    ))
    .expect("serialize ready metadata fields");
    assert!(!serialized_ready.contains(service));
    assert_eq!(state.bootstrap_calls.load(Ordering::SeqCst), 1);
    assert_eq!(state.guard_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *state.call_order.lock().expect("fake call-order mutex"),
        ["bootstrap", "guard"]
    );
    let MonitorReply::ServiceStatus { status } = client
        .monitor(MonitorOperation::ServiceStatus)
        .expect("read foreground service mode")
    else {
        panic!("expected service status reply");
    };
    assert!(status.running);
    assert_eq!(
        status.experimental_collector_source,
        Some(ready.capability.account_id.clone())
    );

    let ready_envelope = FileProjectionStateStore::under_data_dir(&config.data_dir)
        .load()
        .expect("read foreground projection")
        .expect("foreground projection exists");
    assert_eq!(
        ready_envelope.catalog,
        vec![UsageCatalogEntry {
            capability: ready.capability.clone(),
            revision: jackin_core::account_key_hash(
                "usage-catalog-entry-v3",
                &ready.capability.account_id,
            ),
        }]
    );
    let revoked_projection_sibling = ready_envelope
        .projection
        .providers
        .iter()
        .find(|provider| {
            provider.provider_id == publish::canonical_provider_id(&removed_sibling.surface_id)
        })
        .and_then(|provider| {
            provider
                .accounts
                .iter()
                .find(|account| account.canonical_account_id == removed_sibling.account_id)
        })
        .expect("removed non-Claude sibling remains as a projection tombstone");
    assert_eq!(
        revoked_projection_sibling.status_label.as_deref(),
        Some("removed")
    );
    assert_eq!(
        revoked_projection_sibling.lifecycle,
        UsageLifecycleV1::Unavailable
    );
    assert!(revoked_projection_sibling.windows.is_empty());
    let revoked_account_state = FileAccountStateStore::under_data_dir(&config.data_dir)
        .load(removed_sibling, chrono::Utc::now().timestamp())
        .expect("read revoked sibling cooldown tombstone")
        .expect("cooldown tombstone remains durable after removal");
    assert_eq!(revoked_account_state.phase, UsageRefreshPhase::Idle);
    assert!(revoked_account_state.terminal_result.is_none());
    assert!(revoked_account_state.last_good.is_none());
    assert!(revoked_account_state.success_deadline_epoch.is_some());
}

#[test]
fn foreground_broker_conflict_precedes_fake_credential_or_guard_access() {
    use super::service::{
        ForegroundBootstrapOutcome, run_usage_broker_foreground_bootstrap_with_for_test,
    };

    let temp = tempfile::tempdir().expect("isolated conflict data directory");
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let run_dir = secure_run_directory(&config.data_dir).expect("create private run directory");
    let leader_path = run_dir.join(BROKER_LEADER);
    let _owner = claim_leader(&leader_path, &config.build_id, config.lease_duration)
        .expect("claim test broker lease")
        .expect("first test lease owner");
    let bootstrap_calls = Arc::new(AtomicUsize::new(0));
    let guard_calls = Arc::new(AtomicUsize::new(0));

    let result = run_usage_broker_foreground_bootstrap_with_for_test(
        config,
        host_desktop_scope(temp.path()),
        "conflicted-service",
        {
            let calls = Arc::clone(&bootstrap_calls);
            move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(ForegroundBootstrapOutcome::<FakeForegroundCredentialLease>::Missing)
            }
        },
        {
            let calls = Arc::clone(&guard_calls);
            move || {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        },
        |_| {},
    );
    let Err(error) = result else {
        panic!("a second foreground service cannot bootstrap against the held lease");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::BrokerConflict);
    assert_eq!(bootstrap_calls.load(Ordering::SeqCst), 0);
    assert_eq!(guard_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn malformed_foreground_bootstrap_reads_once_and_never_starts_collection() {
    use super::service::{
        ForegroundBootstrapOutcome, run_usage_broker_foreground_bootstrap_with_for_test,
    };

    let temp = tempfile::tempdir().expect("isolated malformed data directory");
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let diagnostic = crate::usage::diagnose_claude_profile_payload(
        br#"{"claudeAiOauth":{"accessToken":false}}"#,
    );
    let bootstrap_calls = Arc::new(AtomicUsize::new(0));
    let guard_calls = Arc::new(AtomicUsize::new(0));
    let ready_calls = Arc::new(AtomicUsize::new(0));

    let result = run_usage_broker_foreground_bootstrap_with_for_test(
        config,
        host_desktop_scope(temp.path()),
        "fixture-malformed-service",
        {
            let calls = Arc::clone(&bootstrap_calls);
            move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(
                    ForegroundBootstrapOutcome::<FakeForegroundCredentialLease>::Malformed(
                        diagnostic,
                    ),
                )
            }
        },
        {
            let calls = Arc::clone(&guard_calls);
            move || {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        },
        {
            let calls = Arc::clone(&ready_calls);
            move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
            }
        },
    );

    assert!(matches!(
        result.expect("malformed payload exits after the single bootstrap"),
        ForegroundBootstrapOutcome::Malformed(_)
    ));
    assert_eq!(bootstrap_calls.load(Ordering::SeqCst), 1);
    assert_eq!(guard_calls.load(Ordering::SeqCst), 0);
    assert_eq!(ready_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn foreground_orphan_socket_conflicts_before_credentials_and_stays_untouched() {
    use super::service::{
        ForegroundBootstrapOutcome, run_usage_broker_foreground_bootstrap_with_for_test,
    };

    let temp = tempfile::tempdir().expect("isolated orphan-socket data directory");
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let run_dir = secure_run_directory(&config.data_dir).expect("create private run directory");
    let leader_path = run_dir.join(BROKER_LEADER);
    let socket_path = config.socket_path();
    let listener = UnixListener::bind(&socket_path).expect("bind unrecognized orphan socket");
    let before = fs::symlink_metadata(&socket_path).expect("inspect orphan socket");
    let before_identity = (before.dev(), before.ino());

    let bootstrap_calls = Arc::new(AtomicUsize::new(0));
    let guard_calls = Arc::new(AtomicUsize::new(0));
    let ready_calls = Arc::new(AtomicUsize::new(0));
    let result = run_usage_broker_foreground_bootstrap_with_for_test(
        config,
        host_desktop_scope(temp.path()),
        "orphan-socket-service",
        {
            let calls = Arc::clone(&bootstrap_calls);
            move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(ForegroundBootstrapOutcome::<FakeForegroundCredentialLease>::Missing)
            }
        },
        {
            let calls = Arc::clone(&guard_calls);
            move || {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        },
        {
            let calls = Arc::clone(&ready_calls);
            move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
            }
        },
    );

    let Err(error) = result else {
        panic!("an unrecognized socket must block foreground startup");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::BrokerConflict);
    assert_eq!(bootstrap_calls.load(Ordering::SeqCst), 0);
    assert_eq!(guard_calls.load(Ordering::SeqCst), 0);
    assert_eq!(ready_calls.load(Ordering::SeqCst), 0);
    assert!(!leader_path.exists(), "failed claim must release its lease");
    let after = fs::symlink_metadata(&socket_path).expect("orphan socket remains");
    assert!(after.file_type().is_socket());
    assert_eq!((after.dev(), after.ino()), before_identity);
    drop(listener);
}

#[test]
fn foreground_reclaims_socket_from_expired_lease_before_credentials() {
    use super::service::{
        ForegroundBootstrapOutcome, run_usage_broker_foreground_bootstrap_with_for_test,
    };

    let temp = tempfile::tempdir().expect("isolated stale-socket data directory");
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let run_dir = secure_run_directory(&config.data_dir).expect("create private run directory");
    let leader_path = run_dir.join(BROKER_LEADER);
    let socket_path = config.socket_path();
    drop(UnixListener::bind(&socket_path).expect("bind prior broker socket"));

    let mut stale = BrokerLease::new(&config.build_id);
    stale.renewed_at_epoch -= i64::try_from(config.lease_duration.as_secs()).unwrap() + 1;
    fs::write(&leader_path, serde_json::to_vec(&stale).unwrap()).unwrap();
    fs::set_permissions(&leader_path, fs::Permissions::from_mode(0o600)).unwrap();

    let bootstrap_calls = Arc::new(AtomicUsize::new(0));
    let result = run_usage_broker_foreground_bootstrap_with_for_test(
        config,
        host_desktop_scope(temp.path()),
        "expired-socket-service",
        {
            let calls = Arc::clone(&bootstrap_calls);
            move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(ForegroundBootstrapOutcome::<FakeForegroundCredentialLease>::Missing)
            }
        },
        || Ok(()),
        |_| {},
    );

    assert!(matches!(
        result.expect("missing credential exits after startup claim"),
        ForegroundBootstrapOutcome::Missing
    ));
    assert_eq!(bootstrap_calls.load(Ordering::SeqCst), 1);
    assert!(!socket_path.exists(), "expired lease socket is reclaimed");
    assert!(!leader_path.exists(), "failed bootstrap releases its lease");
}

#[test]
fn foreground_expired_lease_keeps_responsive_unrecognized_socket() {
    use super::service::{
        ForegroundBootstrapOutcome, run_usage_broker_foreground_bootstrap_with_for_test,
    };

    let temp = tempfile::tempdir().expect("isolated responsive-socket data directory");
    let config = UsageBrokerConfig::for_data_dir(temp.path().to_owned());
    let run_dir = secure_run_directory(&config.data_dir).expect("create private run directory");
    let leader_path = run_dir.join(BROKER_LEADER);
    let socket_path = config.socket_path();
    let listener = UnixListener::bind(&socket_path).expect("bind responsive unrecognized socket");
    let before = fs::symlink_metadata(&socket_path).expect("inspect existing socket");
    let before_identity = (before.dev(), before.ino());

    let mut stale = BrokerLease::new(&config.build_id);
    stale.renewed_at_epoch -= i64::try_from(config.lease_duration.as_secs()).unwrap() + 1;
    fs::write(&leader_path, serde_json::to_vec(&stale).unwrap()).unwrap();
    fs::set_permissions(&leader_path, fs::Permissions::from_mode(0o600)).unwrap();

    let bootstrap_calls = Arc::new(AtomicUsize::new(0));
    let result = run_usage_broker_foreground_bootstrap_with_for_test(
        config,
        host_desktop_scope(temp.path()),
        "unrecognized-responsive-service",
        {
            let calls = Arc::clone(&bootstrap_calls);
            move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(ForegroundBootstrapOutcome::<FakeForegroundCredentialLease>::Missing)
            }
        },
        || Ok(()),
        |_| {},
    );

    let Err(error) = result else {
        panic!("a responsive unknown endpoint must block startup");
    };
    assert_eq!(error.kind, UsageCoordinationErrorKind::BrokerConflict);
    assert_eq!(bootstrap_calls.load(Ordering::SeqCst), 0);
    assert!(!leader_path.exists(), "failed claim releases its lease");
    let after = fs::symlink_metadata(&socket_path).expect("responsive socket remains");
    assert_eq!((after.dev(), after.ino()), before_identity);
    drop(listener);
}

#[test]
fn startup_cleanup_never_unlinks_an_unowned_or_replaced_socket() {
    let temp = tempfile::tempdir().unwrap();
    let lease_path = temp.path().join("lease");
    let socket_path = temp.path().join("socket");

    let mut owner = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("claim test broker lease");
    fs::write(&socket_path, b"unowned socket path").unwrap();
    assert!(cleanup_owned_files(
        &lease_path,
        &socket_path,
        None,
        &mut owner
    ));
    assert_eq!(fs::read(&socket_path).unwrap(), b"unowned socket path");
    assert!(!lease_path.exists());

    let lease_path = temp.path().join("replacement-lease");
    let socket_path = temp.path().join("replacement-socket");
    let listener = UnixListener::bind(&socket_path).unwrap();
    let metadata = fs::symlink_metadata(&socket_path).unwrap();
    let expected = BrokerSocketIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    };
    let mut owner = claim_leader(&lease_path, "build", Duration::from_secs(30))
        .unwrap()
        .expect("claim replacement test broker lease");
    fs::remove_file(&socket_path).unwrap();
    fs::write(&socket_path, b"replacement path").unwrap();
    assert!(cleanup_owned_files(
        &lease_path,
        &socket_path,
        Some(expected),
        &mut owner,
    ));
    assert_eq!(fs::read(&socket_path).unwrap(), b"replacement path");
    assert!(!lease_path.exists());
    drop(listener);
}

fn bind_and_start_experimental_observer(
    store: &monitor::MonitorStore,
    local_account_id: &str,
    provider_account_id: &str,
    now_epoch: i64,
) -> MonitorAccountBinding {
    let binding = match store
        .operate(
            MonitorOperation::BindAccount {
                binding: MonitorAccountBindingInput {
                    provider: MonitorProvider::Claude,
                    account_id: local_account_id.to_owned(),
                    provider_account_id: Some(provider_account_id.to_owned()),
                    experimental_collector_approved: true,
                    operator_label: "isolated-test-operator".to_owned(),
                    operator_confirmed: true,
                },
            },
            now_epoch,
        )
        .expect("approve isolated local source mapping")
    {
        MonitorReply::AccountBound { binding } => binding,
        other => panic!("expected account binding, got {other:?}"),
    };
    let reply = store
        .operate(
            MonitorOperation::Start {
                config: MonitorConfig {
                    provider: MonitorProvider::Claude,
                    purpose: MonitorPurpose::ObserveOnly,
                    scope: MonitorScope::BoundAccount {
                        binding_id: binding.binding_id.clone(),
                        binding_revision: binding.revision,
                        session_id: None,
                    },
                    goal_id: None,
                    expected_model: None,
                    policy_revision: None,
                    experimental_collector: true,
                },
                idempotency_key: format!("fixture-{local_account_id}"),
            },
            now_epoch,
        )
        .expect("start observe-only experimental monitor");
    assert!(matches!(reply, MonitorReply::Started { .. }));
    binding
}

struct RecordingDiscoveryExecutor {
    inner: Arc<DiscoveryProviderExecutor>,
    probes: Arc<Mutex<Vec<UsageAccountCapability>>>,
}

impl UsageProviderExecutor for RecordingDiscoveryExecutor {
    fn probe(&self, capability: &UsageAccountCapability, generation: u64) -> ProviderProbeOutcome {
        self.probes
            .lock()
            .expect("recording executor probe mutex")
            .push(capability.clone());
        self.inner.probe(capability, generation)
    }
}

#[test]
fn foreground_ticker_polls_only_current_selected_capability_from_approved_monitor_mappings() {
    use crate::coordinator::policy::UsageActivity;

    let temp = tempfile::tempdir().expect("isolated collector ticker data directory");
    let now_epoch = chrono::Utc::now().timestamp();
    let selected_service = "current-selected-claude-service";
    let stale_service = "other-canonical-claude-service";
    let selected = claude_usage_capability_for_service(selected_service);
    let stale = claude_usage_capability_for_service(stale_service);
    assert_ne!(selected, stale);

    let monitor_store =
        Arc::new(monitor::MonitorStore::open(temp.path()).expect("open isolated monitor store"));
    monitor_store.set_experimental_collector_source(Some(stale.account_id.clone()));
    bind_and_start_experimental_observer(
        &monitor_store,
        "local-stale-account",
        &stale.account_id,
        now_epoch,
    );
    monitor_store.set_experimental_collector_source(Some(selected.account_id.clone()));
    bind_and_start_experimental_observer(
        &monitor_store,
        "local-selected-account",
        &selected.account_id,
        now_epoch,
    );
    assert_eq!(
        monitor_store
            .collection_accounts()
            .into_iter()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([selected.account_id.clone()])
    );

    let fake_collector_calls = Arc::new(Mutex::new(Vec::new()));
    let probe_attempts = Arc::new(Mutex::new(Vec::new()));
    let (collector_sender, collector_receiver) = mpsc::channel();
    let collector_calls = Arc::clone(&fake_collector_calls);
    let executor = Arc::new(DiscoveryProviderExecutor {
        bindings: Mutex::new(BTreeMap::new()),
        validated_catalog: Mutex::new(None),
        scope: host_desktop_scope(temp.path()),
        resolver: Arc::new(NoopCredentialResolver),
        monitor_store: Some(Arc::clone(&monitor_store)),
        collector_service: Some(selected_service.to_owned()),
        collector_liveness: Some(Arc::new(crate::usage::ClaudeCollectorLiveness::new(()))),
        claude_collector: Some(Arc::new(move |capability, service| {
            collector_calls
                .lock()
                .expect("fake collector call mutex")
                .push((capability.clone(), service.to_owned()));
            collector_sender
                .send((capability.clone(), service.to_owned()))
                .expect("record authorized fake collection");
            ProviderProbeOutcome::success(quota_view())
        })),
        probe_budget: Duration::from_secs(1),
    });
    let selected_entry = UsageCatalogEntry {
        capability: selected.clone(),
        revision: "selected-foreground-source".to_owned(),
    };
    // The ticker's catalog is the selected foreground catalog. The coordinator
    // is intentionally unscoped here so the test detects any scheduler path
    // that forwards the second approved mapping past the publisher filter.
    let recording_executor: Arc<dyn UsageProviderExecutor> = Arc::new(RecordingDiscoveryExecutor {
        inner: executor,
        probes: Arc::clone(&probe_attempts),
    });
    let coordinator = Arc::new(UsageCoordinator::new(
        recording_executor,
        Arc::new(FileAccountStateStore::under_data_dir(temp.path())),
        UsageCoordinatorConfig::default(),
    ));
    let publisher = publish::ProjectionPublisher::new(
        Arc::clone(&coordinator),
        Arc::new(Mutex::new(empty_projection("foreground-ticker"))),
        FileProjectionStateStore::under_data_dir(temp.path()),
    )
    .with_catalog([selected_entry]);
    for capability in [&selected, &stale] {
        coordinator
            .set_activity(
                capability,
                UsageActivity::DirectInteraction,
                false,
                now_epoch,
            )
            .expect("make fixture account due for polling");
    }

    serve_loop::collect_due_for_active_monitors(
        &publisher,
        &coordinator,
        &monitor_store,
        now_epoch,
    );
    let (probed_capability, probed_service) = collector_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("selected approved monitor reaches fake collector");
    assert_eq!(probed_capability, selected);
    assert_eq!(probed_service, selected_service);

    let generation = coordinator
        .current(&selected, now_epoch)
        .expect("selected account generation")
        .generation;
    let settled = coordinator
        .join_generation(&selected, generation, Duration::from_secs(5), now_epoch)
        .expect("selected fake poll settles");
    assert_eq!(settled.phase, UsageRefreshPhase::Completed);
    assert_eq!(
        *fake_collector_calls
            .lock()
            .expect("fake collector call mutex"),
        vec![(selected.clone(), selected_service.to_owned())]
    );
    assert_eq!(
        *probe_attempts
            .lock()
            .expect("recording executor probe mutex"),
        vec![selected.clone()]
    );
    assert_eq!(
        coordinator
            .current(&stale, now_epoch)
            .expect("unselected state remains idle")
            .phase,
        UsageRefreshPhase::Idle
    );
    let unauthorized = probe_with_scope(
        &DiscoveryProviderExecutor {
            bindings: Mutex::new(BTreeMap::new()),
            validated_catalog: Mutex::new(None),
            scope: host_desktop_scope(temp.path()),
            resolver: Arc::new(NoopCredentialResolver),
            monitor_store: Some(monitor_store),
            collector_service: Some(selected_service.to_owned()),
            collector_liveness: Some(Arc::new(crate::usage::ClaudeCollectorLiveness::new(()))),
            claude_collector: Some(Arc::new(|_, _| {
                panic!("unselected canonical capability must not reach the fake collector")
            })),
            probe_budget: Duration::from_secs(1),
        },
        &stale,
        None,
    );
    let ProviderProbeOutcome::Failure { kind, .. } = unauthorized else {
        panic!("non-selected Claude capability must be unauthorized");
    };
    assert_eq!(kind, UsageCoordinationErrorKind::Unauthorized);
}
