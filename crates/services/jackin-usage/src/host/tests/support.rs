// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn open_runtime(dir: &Path) -> HostUsageRuntime {
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(HostRuntimeConfig::under_data_dir(dir))
        .expect("open");
    runtime
}

pub(super) fn codex_fixture_view() -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("Codex".to_owned()),
        account: FocusedAccountHeader {
            provider_label: "OpenAI / Codex".to_owned(),
            account_label: "codex@example.com".to_owned(),
            username: None,
            plan_label: Some("Pro 20x".to_owned()),
            credential_origin: Some("OAuth · ~/.codex/auth.json".to_owned()),
        },
        buckets: vec![
            QuotaBucketView {
                label: "Session".to_owned(),
                used_label: Some("63% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(37),
                reset_label: Some("Resets in 2h".to_owned()),
                resets_at: Some(1_700_000_000),
                status_slot: Some(StatusSlot::Session),
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
                used_money: None,
                limit_money: None,
                severity: UsageSeverity::Normal,
            },
            QuotaBucketView {
                label: "Weekly".to_owned(),
                used_label: Some("40% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(60),
                reset_label: Some("Resets in 3d".to_owned()),
                resets_at: Some(1_700_200_000),
                status_slot: Some(StatusSlot::Weekly),
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
                used_money: None,
                limit_money: None,
                severity: UsageSeverity::Normal,
            },
        ],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_699_000_000,
        updated_label: "just now".to_owned(),
        status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
        tabs: Vec::new(),
        last_error: None,
    }
}

pub(super) fn canonical_discovered_account(
    surface: HostSurfaceId,
    account_label: &str,
) -> DiscoveredAccountDescriptor {
    let identity = CanonicalAccountIdentity {
        surface,
        subject: CanonicalAccountSubject::ProviderStableHandle(account_label.to_owned()),
    };
    DiscoveredAccountDescriptor {
        surface_id: surface.id().to_owned(),
        account_key: identity.account_key(),
        account_label: account_label.to_owned(),
        provenance: vec!["workspace sample".to_owned()],
        source_ids: vec!["source-0001".to_owned()],
        identity,
    }
}

pub(super) fn inject_remaining(runtime: &mut HostUsageRuntime, surface_id: &str, remaining: u8) {
    inject_remaining_at(runtime, surface_id, remaining, None);
}

pub(super) fn inject_remaining_at(
    runtime: &mut HostUsageRuntime,
    surface_id: &str,
    remaining: u8,
    resets_at: Option<i64>,
) {
    let mut view = FocusedUsageView::unavailable("seed", 1);
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.status_bar_label = format!("{remaining}% left");
    // Amp glance is Daily; all other Desktop surfaces use Weekly (SB-20/21).
    let slot = if surface_id == "amp" {
        StatusSlot::Daily
    } else {
        StatusSlot::Weekly
    };
    let label = if surface_id == "amp" {
        "Daily"
    } else {
        "Weekly"
    };
    view.buckets = vec![QuotaBucketView {
        label: label.to_owned(),
        used_label: Some(format!("{}% used", 100u8.saturating_sub(remaining))),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(remaining),
        reset_label: None,
        resets_at,
        status_slot: Some(slot),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];
    runtime.inject_snapshot(surface_id, view).expect("inject");
}

pub(super) fn inject_dual_remaining(
    runtime: &mut HostUsageRuntime,
    surface_id: &str,
    session_remaining: u8,
    weekly_remaining: u8,
) {
    let mut view = FocusedUsageView::unavailable("seed", 1);
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.status_bar_label = format!("{session_remaining}% left");
    view.buckets = vec![
        QuotaBucketView {
            label: "Session".to_owned(),
            used_label: Some(format!("{}% used", 100u8.saturating_sub(session_remaining))),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(session_remaining),
            reset_label: Some("Resets in 5h".to_owned()),
            resets_at: None,
            status_slot: Some(StatusSlot::Session),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        },
        QuotaBucketView {
            label: "Weekly".to_owned(),
            used_label: Some(format!("{}% used", 100u8.saturating_sub(weekly_remaining))),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(weekly_remaining),
            reset_label: Some("Resets in 2d".to_owned()),
            resets_at: None,
            status_slot: Some(StatusSlot::Weekly),
            pace_label: Some("10% in reserve".to_owned()),
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        },
    ];
    runtime
        .inject_snapshot(surface_id, view)
        .expect("inject dual");
}

pub(super) fn seeded_personal_account() -> FocusedUsageView {
    let mut account = FocusedUsageView::unavailable("seed", 1);
    account.status = UsageSnapshotStatus::Fresh;
    account.source = UsageSource::ProviderApi;
    account.confidence = UsageConfidence::Authoritative;
    account.account.provider_label = "Anthropic / Claude".to_owned();
    account.account.account_label = "personal@example.com".to_owned();
    account.account.plan_label = Some("Max".to_owned());
    account.status_bar_label = "50% left".to_owned();
    account.buckets = vec![QuotaBucketView {
        label: "Session".to_owned(),
        used_label: Some("50% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(50),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Session),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];
    account
}

pub(super) fn glance_weekly_bucket(remaining: u8) -> QuotaBucketView {
    QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(remaining),
        reset_label: Some("Resets in 3d".to_owned()),
        resets_at: Some(1_700_200_000),
        status_slot: Some(StatusSlot::Weekly),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

pub(super) fn glance_daily_bucket(remaining: u8) -> QuotaBucketView {
    let mut bucket = glance_weekly_bucket(remaining);
    bucket.label = "Amp Free".to_owned();
    bucket.status_slot = Some(StatusSlot::Daily);
    bucket.reset_label = Some("Resets daily".to_owned());
    bucket.resets_at = None;
    bucket
}

pub(super) fn glance_view(
    provider_label: &str,
    origin: Option<&str>,
    buckets: Vec<QuotaBucketView>,
    status: UsageSnapshotStatus,
) -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: None,
        focused_provider: Some(provider_label.to_owned()),
        account: FocusedAccountHeader {
            provider_label: provider_label.to_owned(),
            account_label: "user@example.com".to_owned(),
            username: None,
            plan_label: None,
            credential_origin: origin.map(str::to_owned),
        },
        buckets,
        status,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_699_000_000,
        updated_label: "just now".to_owned(),
        status_bar_label: String::new(),
        tabs: Vec::new(),
        last_error: None,
    }
}

pub(super) struct BatchCountingExecutor {
    pub(super) calls: AtomicUsize,
}

impl crate::coordinator::UsageProviderExecutor for BatchCountingExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> crate::coordinator::ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        crate::coordinator::ProviderProbeOutcome::success(codex_fixture_view())
    }
}

pub(super) fn batch_capability(account_id: &str, surface_id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: account_id.to_owned(),
        surface_id: surface_id.to_owned(),
    }
}

pub(super) fn batch_broker() -> (
    tempfile::TempDir,
    UsageBrokerClient,
    Arc<BatchCountingExecutor>,
) {
    let temp = tempfile::tempdir().expect("tempdir");
    let executor = Arc::new(BatchCountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn crate::coordinator::UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_path_buf()),
        broker_executor,
    )
    .expect("broker");
    (temp, client, executor)
}

pub(super) fn join_batch(
    client: &UsageBrokerClient,
    batch: &[(
        UsageAccountCapability,
        Result<UsageGenerationView, UsageCoordinationError>,
    )],
) {
    for (_, result) in batch {
        let view = result.as_ref().expect("batch row ok");
        client
            .join(
                view.capability.clone(),
                view.generation,
                Duration::from_secs(5),
            )
            .expect("join");
    }
}

pub(super) struct BatchRateLimitedExecutor {
    pub(super) calls: AtomicUsize,
}

impl crate::coordinator::UsageProviderExecutor for BatchRateLimitedExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> crate::coordinator::ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        crate::coordinator::ProviderProbeOutcome::Failure {
            kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::RateLimited,
            message: "usage provider rate limit is active".to_owned(),
            retry_at_epoch: Some(chrono::Utc::now().timestamp() + 3_600),
        }
    }
}

pub(super) fn overlong_broker_data_dir(temp: &tempfile::TempDir) -> PathBuf {
    let suffix_len = Path::new("usage-broker/run/usage-broker.sock")
        .as_os_str()
        .len()
        + 1;
    let base_len = temp.path().as_os_str().len() + 1;
    let padding = "p".repeat(UNIX_SOCKET_PATH_LIMIT.saturating_sub(base_len + suffix_len) + 8);
    temp.path().join(padding)
}

pub(super) fn full_broker_socket_path(data_dir: &Path) -> PathBuf {
    data_dir
        .join("usage-broker")
        .join("run")
        .join("usage-broker.sock")
}
