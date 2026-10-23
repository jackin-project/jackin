// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::fs;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CHILD_MODE: &str = "JACKIN_SNAPSHOT_DURABLE_CHILD_MODE";
const CHILD_PATH: &str = "JACKIN_SNAPSHOT_DURABLE_CHILD_PATH";
const CHILD_TIMEOUT: Duration = Duration::from_secs(45);

pub(super) fn run_bounded_test_child(
    exact_test_name: &str,
    environment: &[(&str, &str)],
    completion_marker: &str,
) {
    let logs = tempfile::tempdir().expect("create bounded child log directory");
    let stdout_path = logs.path().join("stdout.log");
    let stderr_path = logs.path().join("stderr.log");
    let stdout = fs::File::create(&stdout_path).expect("create child stdout log");
    let stderr = fs::File::create(&stderr_path).expect("create child stderr log");
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .arg("--exact")
        .arg(exact_test_name)
        .arg("--nocapture")
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("start bounded test child");
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < CHILD_TIMEOUT => {
                std::thread::park_timeout(Duration::from_millis(20));
            }
            Ok(None) => {
                drop(child.kill());
                drop(child.wait());
                panic!(
                    "test child {exact_test_name} exceeded {CHILD_TIMEOUT:?}; stdout={} stderr={}",
                    read_log(&stdout_path),
                    read_log(&stderr_path)
                );
            }
            Err(error) => {
                drop(child.kill());
                drop(child.wait());
                panic!("poll test child {exact_test_name} failed: {error}");
            }
        }
    };
    let stdout_text = read_log(&stdout_path);
    let stderr_text = read_log(&stderr_path);
    assert!(
        status.success(),
        "test child {exact_test_name} failed with {status}; stdout={stdout_text} stderr={stderr_text}"
    );
    assert!(
        stdout_text.contains(completion_marker),
        "test child {exact_test_name} did not emit completion marker {completion_marker:?}; stdout={stdout_text} stderr={stderr_text}"
    );
}

fn read_log(path: &std::path::Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| format!("<unable to read child log: {error}>"))
}

pub(super) fn run_durable_store_child(mode: &str, path: &std::path::Path) {
    let path = path.to_str().expect("durable child database path is UTF-8");
    let marker = format!("JACKIN_DURABLE_CHILD_COMPLETE:{mode}");
    run_bounded_test_child(
        "tests::durable_process::account_capability_durable_child_entry",
        &[(CHILD_MODE, mode), (CHILD_PATH, path)],
        &marker,
    );
}

#[test]
fn account_capability_durable_child_entry() {
    let Ok(mode) = std::env::var(CHILD_MODE) else {
        return;
    };
    let path = std::env::var(CHILD_PATH).expect("durable snapshot child path");
    match mode.as_str() {
        "roundtrip_write" => write_roundtrip_fixture(std::path::Path::new(&path)),
        "roundtrip_read" => read_roundtrip_fixture(std::path::Path::new(&path)),
        "v5_migrate" => migrate_v5_fixture(std::path::Path::new(&path)),
        _ => panic!("unknown durable snapshot child mode {mode}"),
    }
    println!("JACKIN_DURABLE_CHILD_COMPLETE:{mode}");
}

fn write_roundtrip_fixture(path: &std::path::Path) {
    let now = 1_800_000_000;
    let first = capability_view("account-A", "codex", 17, now);
    let second = capability_view("account-B", "codex", 83, now);
    store_usage_snapshots(path, &[first, second]).expect("store two bound capability snapshots");
}

fn read_roundtrip_fixture(path: &std::path::Path) {
    let now = 1_800_000_060;
    let views = load_all_account_usage_views(path, now).expect("load durable capability views");
    assert_eq!(views.len(), 2);
    for expected in [
        capability_view("account-A", "codex", 17, 1_800_000_000),
        capability_view("account-B", "codex", 83, 1_800_000_000),
    ] {
        let expected_capability = expected
            .account
            .account_capability
            .as_ref()
            .expect("expected bound account selector");
        let view = views
            .iter()
            .map(|stored| &stored.view)
            .find(|view| {
                view.account
                    .account_capability
                    .as_ref()
                    .is_some_and(|capability| capability == expected_capability)
            })
            .expect("reloaded bound account selector");
        assert_eq!(view.account.account_label, expected.account.account_label);
        assert_eq!(
            view.account.account_capability,
            expected.account.account_capability
        );
        assert_eq!(view.buckets.len(), expected.buckets.len());
        assert_eq!(
            view.buckets, expected.buckets,
            "full typed quota survives reload"
        );
    }

    let identities = list_account_identities(path).expect("list durable capability identities");
    assert_eq!(identities.len(), 2);
    let production_views = views.iter().map(|stored| &stored.view).collect::<Vec<_>>();
    let tabs = jackin_usage_provider_core::provider_tabs(&production_views);
    assert_eq!(tabs.len(), 2);
    let mut tab_ids: Vec<_> = tabs.iter().map(|tab| tab.id.clone()).collect();
    tab_ids.sort_unstable();
    let mut expected_tab_ids = ["account-A", "account-B"].map(|account_id| {
        jackin_protocol::usage_broker::UsageAccountCapability {
            account_id: account_id.to_owned(),
            surface_id: "codex".to_owned(),
        }
        .opaque_identity_key()
    });
    expected_tab_ids.sort_unstable();
    assert_eq!(tab_ids, expected_tab_ids.to_vec());
    assert!(tabs.iter().all(|tab| tab.account_capability.is_some()));
}

fn migrate_v5_fixture(path: &std::path::Path) {
    let views = load_all_account_usage_views(path, 1_800_000_100)
        .expect("migrate and load v5 fixture through public API");
    assert_eq!(views.len(), 1);
    assert!(views[0].view.account.account_capability.is_none());
    assert_eq!(views[0].view.buckets.len(), 3);
    let mut expected_buckets = v5_typed_buckets();
    expected_buckets.sort_by(|left, right| left.label.cmp(&right.label));
    assert_eq!(views[0].view.buckets, expected_buckets);
    let identities = list_account_identities(path).expect("list legacy identity after migration");
    assert_eq!(identities.len(), 1);
    assert!(identities[0].account_capability.is_none());
    assert_eq!(schema_version(path).unwrap().as_deref(), Some("6"));
}

fn capability_view(
    account_id: &str,
    surface_id: &str,
    remaining: u8,
    fetched_at: i64,
) -> FocusedUsageView {
    let mut view = provider_usage_view(
        "Codex",
        "same@example.test",
        Some("Pro"),
        "Session",
        remaining,
        fetched_at,
    );
    view.account.account_capability = Some(jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: account_id.to_owned(),
        surface_id: surface_id.to_owned(),
    });
    if account_id == "account-A" {
        let bucket = &mut view.buckets[0];
        bucket.used_percent =
            Some(jackin_protocol::control::CanonicalUsedPercent::new(82.875).unwrap());
        bucket.used_percent_origin =
            Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported);
        bucket.remaining_percent =
            Some(jackin_protocol::control::CanonicalRemainingPercent::new(17.125).unwrap());
        bucket.remaining_percent_origin =
            Some(jackin_protocol::control::PercentObservationOrigin::ProviderReported);
        bucket.count_quota = Some(jackin_protocol::control::CountQuota::new(
            Some(u64::MAX),
            Some(u64::MAX),
            Some(0),
            jackin_protocol::control::CountQuotaUnit::Requests,
            jackin_protocol::control::CountQuotaPeriod::UtcDaily,
            jackin_protocol::control::CountQuotaProvenance::ProviderReported,
        ));
        bucket.provider_quota_state = Some(jackin_protocol::control::ProviderQuotaState::Exhausted);
        let mut spend_bucket = bucket.clone();
        spend_bucket.label = "Spend".to_owned();
        spend_bucket.used_money = Some(jackin_protocol::control::Money::new(i64::MIN, "USD", 4));
        spend_bucket.limit_money = Some(jackin_protocol::control::Money::new(i64::MAX, "USD", 7));
        spend_bucket.count_quota = None;
        spend_bucket.provider_quota_state = None;
        spend_bucket.used_percent = None;
        spend_bucket.used_percent_origin = None;
        spend_bucket.remaining_percent = None;
        spend_bucket.remaining_percent_origin = None;
        view.buckets.push(spend_bucket);
    }
    for bucket in &view.buckets {
        bucket
            .validate()
            .expect("durable capability fixture bucket is valid");
    }
    view
}
