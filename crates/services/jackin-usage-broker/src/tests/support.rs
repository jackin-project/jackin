// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

pub(super) struct PairedTestClock {
    state: Mutex<PairedTestClockState>,
}

struct PairedTestClockState {
    wall_epoch: i64,
    monotonic: Duration,
}

impl PairedTestClock {
    pub(super) fn at(wall_epoch: i64) -> Self {
        Self {
            state: Mutex::new(PairedTestClockState {
                wall_epoch,
                monotonic: Duration::ZERO,
            }),
        }
    }

    pub(super) fn wall_epoch(&self) -> i64 {
        self.state.lock().unwrap().wall_epoch
    }

    /// Advance wall and monotonic time by the same whole-second interval.
    pub(super) fn advance_to_epoch(&self, wall_epoch: i64) {
        let mut state = self.state.lock().unwrap();
        assert!(
            wall_epoch >= state.wall_epoch,
            "test clock cannot move backward"
        );
        let elapsed = u64::try_from(wall_epoch.saturating_sub(state.wall_epoch))
            .expect("nonnegative test-clock interval");
        state.wall_epoch = wall_epoch;
        state.monotonic = state.monotonic.saturating_add(Duration::from_secs(elapsed));
    }
}

impl MonotonicClock for PairedTestClock {
    fn now(&self) -> Duration {
        self.state.lock().unwrap().monotonic
    }

    fn sample(&self, _fallback_epoch: i64) -> ClockSample {
        let state = self.state.lock().unwrap();
        ClockSample::anchored(state.wall_epoch, state.monotonic)
    }
}

pub(super) struct CountingExecutor {
    pub(super) calls: AtomicUsize,
}

pub(super) struct RetryRecordingResolver {
    pub(super) manual_retries: Arc<AtomicUsize>,
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

pub(super) struct NoopCredentialResolver;

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

pub(super) fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "abc123".to_owned(),
        surface_id: "claude".to_owned(),
    }
}

pub(super) fn second_capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "def456".to_owned(),
        surface_id: "codex".to_owned(),
    }
}

pub(super) fn quota_view() -> FocusedUsageView {
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

pub(super) fn env_material(source_name: &str, material: &str) -> ProviderCredentialSourceMaterial {
    ProviderCredentialSourceMaterial {
        source: UsageCredentialSourceIdentity::HostEnv {
            name: source_name.to_owned(),
        },
        material_fingerprint: usage_credential_material_fingerprint(material),
    }
}

pub(super) fn env_scope(
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

pub(super) struct TypedRateLimitResolver;

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
            rate_limit: Some(jackin_usage_provider_core::ProviderRateLimit {
                retry_at_epoch: Some(1_700_000_037),
            }),
            provider_error: None,
        }
    }
}

#[derive(Default)]
pub(super) struct RecordingRefreshResolver {
    pub(super) calls: Mutex<Vec<(String, OpaqueCredentialHandle)>>,
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
            provider_error: None,
        }
    }
}

pub(super) struct HeldExecutor {
    pub(super) started: mpsc::SyncSender<()>,
    pub(super) release: Mutex<mpsc::Receiver<()>>,
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

pub(super) struct StallOneExecutor {
    pub(super) slow: UsageAccountCapability,
    pub(super) release: Mutex<mpsc::Receiver<()>>,
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
