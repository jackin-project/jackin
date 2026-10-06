// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "e2e")]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use jackin_protocol::control::{
    FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity, UsageSnapshotStatus,
    UsageSource,
};
use jackin_protocol::usage_broker::{UsageAccountCapability, UsageCatalogEntry, UsageRefreshPhase};
use jackin_protocol::usage_broker::{UsageCoordinationError, UsageCoordinationErrorKind};
use jackin_usage::coordinator::{ProviderProbeOutcome, UsageProviderExecutor};
use jackin_usage::host::{UsageBrokerConfig, ensure_usage_broker_with_executor};

const CHILD_ENV: &str = "JACKIN_USAGE_BROKER_E2E_CHILD";
const ROOT_ENV: &str = "JACKIN_USAGE_BROKER_E2E_ROOT";
const MODE_ENV: &str = "JACKIN_USAGE_BROKER_E2E_MODE";
const EXPECTED_ENV: &str = "JACKIN_USAGE_BROKER_E2E_EXPECTED";
const CLIENTS: usize = 20;
static PROCESS_CALLS: AtomicUsize = AtomicUsize::new(0);

trait CoordinationResultExt<T> {
    fn test_result(self) -> Result<T>;
}

impl<T> CoordinationResultExt<T> for std::result::Result<T, UsageCoordinationError> {
    fn test_result(self) -> Result<T> {
        self.map_err(|error| anyhow::anyhow!("{:?}: {}", error.kind, error.message))
    }
}

struct FileCountingProvider {
    root: PathBuf,
    release: Option<PathBuf>,
}

impl UsageProviderExecutor for FileCountingProvider {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        let call = PROCESS_CALLS.fetch_add(1, Ordering::SeqCst);
        if fs::write(
            self.root
                .join(format!("provider-call-{}-{call}", std::process::id())),
            b"called\n",
        )
        .is_err()
        {
            return ProviderProbeOutcome::Failure {
                kind: UsageCoordinationErrorKind::ProviderUnavailable,
                message: "fake provider counter is unavailable".to_owned(),
                retry_at_epoch: None,
            };
        }
        if let Some(release) = &self.release {
            wait_until(Duration::from_secs(10), || release.exists());
        }
        std::thread::park_timeout(Duration::from_millis(100));
        ProviderProbeOutcome::success(quota_view())
    }
}

fn capability() -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: "shared-account".to_owned(),
        surface_id: "claude".to_owned(),
    }
}

fn quota_view() -> FocusedUsageView {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut view = FocusedUsageView::unavailable("claude", i64::try_from(now).unwrap_or(i64::MAX));
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.account.provider_label = "Claude".to_owned();
    view.account.account_label = "shared@example.test".to_owned();
    view.buckets = vec![QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(64),
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

#[test]
fn usage_broker_child() -> Result<()> {
    let Ok(child) = std::env::var(CHILD_ENV) else {
        return Ok(());
    };
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).context("missing broker E2E root")?);
    let mode = std::env::var(MODE_ENV).unwrap_or_else(|_| "standard".to_owned());
    let expected = std::env::var(EXPECTED_ENV)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(CLIENTS);
    if mode == "owner" {
        let executor: Arc<dyn UsageProviderExecutor> = Arc::new(FileCountingProvider {
            root: root.clone(),
            release: Some(root.join("release-owner")),
        });
        let client = ensure_usage_broker_with_executor(
            UsageBrokerConfig::for_data_dir(root.join("data")),
            executor,
        )
        .test_result()?;
        // Mirror production activation (ensure → reconcile with current
        // discovery): without this the owner persists an empty catalog, the
        // post-kill recovery leader loads it as a gated-empty admission set,
        // and every recovery refresh fails `CatalogRevoked`.
        client
            .reconcile_catalog(
                "e2e-catalog-1".to_owned(),
                vec![UsageCatalogEntry {
                    capability: capability(),
                    revision: "e2e-shared-account-1".to_owned(),
                }],
            )
            .test_result()?;
        let state = client.refresh(capability(), 0, true).test_result()?;
        assert_eq!(state.generation, 1);
        wait_until(Duration::from_secs(10), || {
            entries_with_prefix(&root, "provider-call-") == 1
        });
        fs::write(root.join("owner-active"), b"active\n")?;
        loop {
            std::thread::park_timeout(Duration::from_secs(1));
        }
    }
    fs::write(root.join(format!("ready-{child}")), b"ready\n")?;
    wait_until(Duration::from_secs(10), || root.join("go").exists());

    let executor: Arc<dyn UsageProviderExecutor> = Arc::new(FileCountingProvider {
        root: root.clone(),
        release: None,
    });
    // The broker lease is a process-independent authority: a killed owner's
    // lease stays valid for its full duration, so takeover — and therefore
    // this client's connect — legitimately fails with Unavailable until the
    // lease expires. Retry like a real desktop client instead of assuming
    // recovery is instantaneous.
    const ENSURE_DEADLINE: Duration = Duration::from_secs(90);
    let deadline = Instant::now() + ENSURE_DEADLINE;
    let client = loop {
        match ensure_usage_broker_with_executor(
            UsageBrokerConfig::for_data_dir(root.join("data")),
            Arc::clone(&executor),
        ) {
            Ok(client) => break client,
            Err(error)
                if error.kind == UsageCoordinationErrorKind::Unavailable
                    && Instant::now() < deadline =>
            {
                std::thread::park_timeout(Duration::from_millis(500));
            }
            Err(error) => return Err(error).test_result(),
        }
    };
    let state = client.refresh(capability(), 0, true).test_result()?;
    let terminal = client
        .join(capability(), state.generation, Duration::from_secs(5))
        .test_result()?;
    assert_eq!(terminal.generation, if mode == "recovery" { 2 } else { 1 });
    assert_eq!(terminal.phase, UsageRefreshPhase::Completed);
    fs::write(root.join(format!("done-{child}")), b"done\n")?;
    wait_until(Duration::from_secs(10), || {
        entries_with_prefix(&root, "done-") == expected
    });
    Ok(())
}

#[test]
fn usage_broker_twenty_host_processes_make_one_provider_call() -> Result<()> {
    assert_host_process_singleflight(CLIENTS)
}

#[test]
fn usage_broker_two_host_processes_make_one_provider_call() -> Result<()> {
    assert_host_process_singleflight(2)
}

fn assert_host_process_singleflight(clients: usize) -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path();
    let executable = std::env::current_exe()?;
    let mut children = Vec::new();
    for child in 0..clients {
        let envs: Vec<(std::ffi::OsString, std::ffi::OsString)> = vec![
            (CHILD_ENV.into(), child.to_string().into()),
            (ROOT_ENV.into(), root.as_os_str().to_owned()),
            (EXPECTED_ENV.into(), clients.to_string().into()),
        ];
        let request = jackin_process::ExecRequest::new(
            &executable,
            ["--exact", "usage_broker_child", "--nocapture"],
        )
        .envs(envs)
        .stdout_mode(jackin_process::StdioMode::Inherit)
        .stderr_mode(jackin_process::StdioMode::Inherit);
        children.push(jackin_process::spawn_sync(&request)?);
    }
    wait_until(Duration::from_secs(10), || {
        entries_with_prefix(root, "ready-") == clients
    });
    fs::write(root.join("go"), b"go\n")?;
    for mut child in children {
        assert!(child.wait()?.success());
    }
    assert_eq!(entries_with_prefix(root, "provider-call-"), 1);
    Ok(())
}

fn wait_until(timeout: Duration, condition: impl Fn() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(started.elapsed() < timeout, "timed out waiting for barrier");
        std::thread::park_timeout(Duration::from_millis(10));
    }
}

fn entries_with_prefix(root: &Path, prefix: &str) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
        .count()
}

#[path = "usage_broker_e2e/docker.rs"]
mod docker;
#[path = "usage_broker_e2e/recovery.rs"]
mod recovery;
