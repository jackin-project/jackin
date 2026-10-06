// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn write_isolated_config(config_root: &std::path::Path) {
    std::fs::create_dir_all(config_root).expect("config root");
    std::fs::write(
        config_root.join("config.toml"),
        format!(
            r#"version = "{}"

[claude]
auth_forward = "ignore"
[codex]
auth_forward = "ignore"
[amp]
auth_forward = "ignore"
[kimi]
auth_forward = "ignore"
[grok]
auth_forward = "ignore"
[opencode]
auth_forward = "ignore"
"#,
            jackin_config::CURRENT_CONFIG_VERSION
        ),
    )
    .expect("global config");
}

pub(super) fn open_bridge(dir: &std::path::Path) -> UsageMenuBarBridge {
    open_bridge_with_live(dir, false)
}

pub(super) fn open_bridge_with_live(
    dir: &std::path::Path,
    allow_live_probes: bool,
) -> UsageMenuBarBridge {
    let config_root = dir.join("config");
    if !config_root.join("config.toml").exists() {
        write_isolated_config(&config_root);
    }
    let bridge = UsageMenuBarBridge::create();
    bridge
        .open_runtime(OpenConfig {
            data_dir_override: Some(dir.display().to_string()),
            config_root_override: Some(config_root.display().to_string()),
            refresh_floor_secs: 120,
            enabled_surface_ids: vec!["codex".to_owned(), "claude".to_owned()],
            allow_live_probes,
        })
        .expect("open");
    bridge
}

pub(super) fn write_account_config(
    config_root: &std::path::Path,
    account_id: &str,
    provider: &str,
) {
    std::fs::create_dir_all(config_root).expect("config root");
    std::fs::write(
        config_root.join("config.toml"),
        format!(
            r#"version = "{}"

[accounts.{account_id}]
name = "{account_id}"
provider = "{provider}"

[accounts.{account_id}.credential]
type = "api_key"
value = "fixture-{account_id}-secret"
"#,
            jackin_config::CURRENT_CONFIG_VERSION
        ),
    )
    .expect("account config");
}

pub(super) struct BlockingBrokerExecutor {
    pub(super) calls: AtomicUsize,
    started: (Mutex<bool>, Condvar),
    released: (Mutex<bool>, Condvar),
}

impl BlockingBrokerExecutor {
    pub(super) fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            started: (Mutex::new(false), Condvar::new()),
            released: (Mutex::new(false), Condvar::new()),
        }
    }

    pub(super) fn wait_started(&self) {
        let (lock, changed) = &self.started;
        let started = lock.lock().unwrap();
        let (started, wait) = changed
            .wait_timeout_while(started, Duration::from_secs(2), |started| !*started)
            .unwrap();
        assert!(*started && !wait.timed_out());
    }

    pub(super) fn release(&self) {
        let (lock, changed) = &self.released;
        *lock.lock().unwrap() = true;
        changed.notify_all();
    }
}

impl UsageProviderExecutor for BlockingBrokerExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let (started, changed) = &self.started;
        *started.lock().unwrap() = true;
        changed.notify_all();
        let (released, changed) = &self.released;
        let released = released.lock().unwrap();
        drop(changed.wait_while(released, |released| !*released).unwrap());
        let mut view = FocusedUsageView::unavailable("fixture", 1);
        view.status = UsageSnapshotStatus::Fresh;
        view.source = UsageSource::ProviderApi;
        view.confidence = UsageConfidence::Authoritative;
        view.account.provider_label = "Anthropic / Claude".to_owned();
        view.account.account_label = "broker@example.test".to_owned();
        view.buckets = vec![QuotaBucketView {
            label: "Weekly".to_owned(),
            used_label: None,
            limit_label: None,
            remaining_percent: Some(64),
            reset_label: None,
            resets_at: None,
            status_slot: Some(StatusSlot::Weekly),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        }];
        ProviderProbeOutcome::success(view)
    }
}
