// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn provider_glance_rows_accept_affirmative_origin_when_unsupported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "kimi",
            glance_view(
                "Kimi",
                Some("API key · env KIMI_AUTH_TOKEN"),
                Vec::new(),
                UsageSnapshotStatus::Unsupported,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let kimi = rows
        .iter()
        .find(|r| r.surface_id == "kimi")
        .expect("kimi detected");
    assert_eq!(kimi.bar_label, "–");
}

#[test]
fn provider_glance_rows_do_not_fallback_to_unrelated_slots() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let mut session = glance_weekly_bucket(80);
    session.status_slot = Some(StatusSlot::Session);
    let mut spend = glance_weekly_bucket(20);
    spend.status_slot = Some(StatusSlot::Spend);
    runtime
        .inject_snapshot(
            "codex",
            glance_view(
                "Codex",
                Some("OAuth · file"),
                vec![session, spend],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let codex = rows
        .iter()
        .find(|r| r.surface_id == "codex")
        .expect("codex row");
    assert_eq!(codex.bar_label, "–");
    assert_eq!(codex.glance_remaining_percent, None);
}

#[test]
fn provider_glance_rows_never_include_opencode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "opencode",
            glance_view(
                "OpenCode",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(90)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    assert!(rows.iter().all(|r| r.surface_id != "opencode"));
}

#[test]
fn provider_glance_rows_icon_keys_match_closed_desktop_domain() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "grok",
            glance_view(
                "Grok Build",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(33)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let grok = rows
        .iter()
        .find(|r| r.surface_id == "grok")
        .expect("grok row");
    assert_eq!(grok.icon_key, "grok");
    assert_eq!(grok.icon_key, grok.surface_id);
}

#[test]
fn provider_glance_rows_preserve_dimmed_last_known() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let mut stale = glance_view(
        "Codex",
        Some("OAuth · file"),
        vec![glance_weekly_bucket(45)],
        UsageSnapshotStatus::Stale,
    );
    stale.buckets[0].status = UsageSnapshotStatus::Stale;
    runtime.inject_snapshot("codex", stale).expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let codex = rows
        .iter()
        .find(|r| r.surface_id == "codex")
        .expect("codex row");
    assert_eq!(codex.bar_label, "45%");
    assert!(codex.dimmed);
}

#[test]
fn provider_glance_rows_marks_canonical_placeholder_refreshing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    // A first-ever placeholder without prior evidence is absent.
    runtime
        .inject_snapshot("codex", FocusedUsageView::refreshing(Some("Codex"), 0))
        .expect("inject");
    assert!(
        runtime
            .provider_glance_rows()
            .expect("rows")
            .iter()
            .all(|r| r.surface_id != "codex")
    );
    // Establish evidence, then replace with the placeholder → retained + refreshing.
    runtime
        .inject_snapshot(
            "codex",
            glance_view(
                "Codex",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(50)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    assert!(
        runtime
            .provider_glance_rows()
            .expect("rows")
            .iter()
            .any(|r| r.surface_id == "codex")
    );
    runtime
        .inject_snapshot("codex", FocusedUsageView::refreshing(Some("Codex"), 0))
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let codex = rows
        .iter()
        .find(|r| r.surface_id == "codex")
        .expect("retained");
    assert!(codex.is_refreshing);
}

#[test]
fn provider_glance_rows_redetect_new_credentials() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    assert!(runtime.provider_glance_rows().expect("rows").is_empty());
    runtime
        .inject_snapshot(
            "grok",
            glance_view(
                "Grok Build",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(33)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    assert!(
        runtime
            .provider_glance_rows()
            .expect("rows")
            .iter()
            .any(|r| r.surface_id == "grok")
    );
}

#[test]
fn disabled_probe_policy_skips_dispatch_and_is_never_due() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(HostRuntimeConfig {
            data_dir: dir.path().to_path_buf(),
            refresh_floor_secs: 60,
            enabled_surface_ids: Vec::new(),
            probe_policy: HostProbePolicy::Disabled,
            discovery_scope: UsageDiscoveryScope::Capsule {
                forwarded_accounts: Vec::new(),
            },
        })
        .expect("open");
    assert!(!runtime.live_probes_enabled());
    assert!(!runtime.refresh_due());
}

#[test]
fn request_usage_batch_dedups_capabilities_and_reports_per_account() {
    let (_temp, client, executor) = batch_broker();
    let first = batch_capability("abc123", "claude");
    let second = batch_capability("def456", "codex");

    let batch = request_usage_batch(
        &client,
        [first.clone(), second.clone(), first.clone()],
        false,
    );
    assert_eq!(batch.len(), 2);
    assert!(batch.iter().all(|(_, result)| result.is_ok()));
    join_batch(&client, &batch);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    // Still-fresh observations are reused; nothing new is dispatched.
    let reused = request_usage_batch(&client, [second, first], false);
    assert!(reused.iter().all(|(_, result)| result.is_ok()));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    // An explicit operator refresh bypasses the success cooldown exactly once.
    let forced = request_usage_batch(&client, [batch_capability("abc123", "claude")], true);
    assert_eq!(forced.len(), 1);
    assert_eq!(forced[0].1.as_ref().expect("forced ok").generation, 2);
    join_batch(&client, &forced);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 3);
}

#[test]
fn request_usage_batch_never_aborts_on_per_account_errors() {
    let missing = PathBuf::from("missing-batch-broker.sock");
    let client = UsageBrokerClient::at(missing, env!("CARGO_PKG_VERSION").to_owned());
    let batch = request_usage_batch(
        &client,
        [
            batch_capability("abc123", "claude"),
            batch_capability("abc123", "claude"),
        ],
        true,
    );
    assert_eq!(batch.len(), 1, "duplicates request once even on error");
    let error = batch[0].1.as_ref().unwrap_err();
    assert_eq!(
        error.kind,
        jackin_protocol::usage_broker::UsageCoordinationErrorKind::Unavailable
    );
}

#[test]
fn request_usage_batch_forced_refresh_still_honors_retry_after() {
    let temp = tempfile::tempdir().expect("tempdir");
    let executor = Arc::new(BatchRateLimitedExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn jackin_usage_coordinator::UsageProviderExecutor> =
        concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_path_buf()),
        broker_executor,
    )
    .expect("broker");
    let capability = batch_capability("abc123", "claude");

    let first = request_usage_batch(&client, [capability.clone()], true);
    assert_eq!(first[0].1.as_ref().expect("first ok").generation, 1);
    client
        .join(capability.clone(), 1, Duration::from_secs(5))
        .expect("join");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    // A forced manual refresh during the Retry-After window joins the same
    // generation instead of dispatching a duplicate probe.
    let forced = request_usage_batch(&client, [capability], true);
    assert_eq!(forced[0].1.as_ref().expect("forced ok").generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn broker_socket_alias_only_triggers_past_sun_path_limit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let short = full_broker_socket_path(temp.path());
    assert!(
        short.as_os_str().len() < UNIX_SOCKET_PATH_LIMIT,
        "fixture must fit the limit, got {}",
        short.display()
    );
    assert_eq!(short_socket_alias(&short), None);

    let data_dir = overlong_broker_data_dir(&temp);
    let full = full_broker_socket_path(&data_dir);
    assert!(
        full.as_os_str().len() >= UNIX_SOCKET_PATH_LIMIT,
        "fixture must exceed the limit, got {}",
        full.display()
    );
    let alias = short_socket_alias(&full).expect("over-long path needs an alias");
    assert!(
        alias.as_os_str().len() < UNIX_SOCKET_PATH_LIMIT,
        "alias must fit the limit, got {}",
        alias.display()
    );
    // Client and server rendezvous without shared state: same input, same
    // alias, and distinct data directories never share one.
    assert_eq!(short_socket_alias(&full), Some(alias.clone()));
    let sibling = full_broker_socket_path(&data_dir.join("sibling"));
    let sibling_alias = short_socket_alias(&sibling).expect("sibling needs an alias");
    assert_ne!(alias, sibling_alias);
}

#[test]
fn broker_serves_through_socket_alias_for_overlong_data_dir() {
    let temp = tempfile::tempdir().expect("tempdir");
    let data_dir = overlong_broker_data_dir(&temp);
    let executor = Arc::new(BatchCountingExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn jackin_usage_coordinator::UsageProviderExecutor> =
        concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(data_dir.clone()),
        broker_executor,
    )
    .expect("broker must bind through the alias");
    let alias = short_socket_alias(&full_broker_socket_path(&data_dir)).expect("alias");
    assert!(alias.exists(), "broker must listen on {}", alias.display());

    let batch = request_usage_batch(&client, [batch_capability("alias-account", "codex")], false);
    assert!(batch.iter().all(|(_, result)| result.is_ok()));
    join_batch(&client, &batch);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}
