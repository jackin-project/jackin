// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::usage::{estimate_caption, provider_display_label};
use jackin_core::Agent;
use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSeverity,
    UsageSnapshotStatus, UsageSource,
};
use jackin_protocol::usage_broker::{
    UsageAccountCapability, UsageCoordinationError, UsageCoordinationErrorKind,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::broker::{UNIX_SOCKET_PATH_LIMIT, short_socket_alias};

fn codex_fixture_view() -> FocusedUsageView {
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
        buckets: vec![QuotaBucketView {
            label: "Weekly".to_owned(),
            used_label: Some("40% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(60),
            reset_label: Some("Resets in 3d".to_owned()),
            resets_at: Some(1_700_200_000),
            status_slot: Some(jackin_protocol::control::StatusSlot::Weekly),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        }],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_699_000_000,
        updated_label: "just now".to_owned(),
        status_bar_label: "Codex Weekly: 40% used · 60% left".to_owned(),
        tabs: Vec::new(),
        last_error: None,
    }
}

#[test]
fn host_surfaces_cover_agent_all_plus_routed_providers() {
    let agent_ids: HashSet<_> = Agent::ALL
        .iter()
        .map(|agent| HostSurfaceId::from_agent(*agent).id())
        .collect();
    for id in [
        "claude", "codex", "amp", "kimi", "opencode", "grok", "google", "cursor", "meta",
    ] {
        assert!(agent_ids.contains(id), "missing agent surface {id}");
    }
    assert!(HostSurfaceId::from_id("zai").is_some());
    assert!(HostSurfaceId::from_id("minimax").is_some());
    assert!(HostSurfaceId::from_id("openrouter").is_some());
    assert_eq!(HostSurfaceId::ALL.len(), 12);
}

#[test]
fn host_surface_provider_order_is_settled_and_not_agent_named() {
    assert_eq!(
        HostSurfaceId::ALL
            .iter()
            .map(|surface| surface.label())
            .collect::<Vec<_>>(),
        vec![
            "OpenAI",
            "Anthropic",
            "Amp",
            "xAI",
            "Z.AI",
            "Kimi",
            "MiniMax",
            "OpenCode",
            "Google",
            "Cursor",
            "Meta",
            "OpenRouter"
        ]
    );
}

#[test]
fn canonical_identity_domain_separates_evidence_and_normalizes_stable_handles() {
    use crate::host::accounts::{CanonicalAccountIdentity, CanonicalAccountSubject};

    let provider_id = CanonicalAccountIdentity {
        surface: HostSurfaceId::Codex,
        subject: CanonicalAccountSubject::ProviderId("Same@Example.Test".to_owned()),
    };
    let stable_handle = CanonicalAccountIdentity {
        surface: HostSurfaceId::Codex,
        subject: CanonicalAccountSubject::ProviderStableHandle(" Person@Example.Test ".to_owned()),
    };
    let normalized_stable_handle = CanonicalAccountIdentity {
        surface: HostSurfaceId::Codex,
        subject: CanonicalAccountSubject::ProviderStableHandle("person@example.test".to_owned()),
    };
    assert_ne!(
        provider_id.account_key(),
        stable_handle.account_key(),
        "routing keys must retain the identity evidence kind"
    );
    assert_eq!(
        stable_handle.account_key(),
        normalized_stable_handle.account_key()
    );

    let first_source =
        CanonicalAccountIdentity::source_capability(HostSurfaceId::Codex, "opaque-a");
    let second_source =
        CanonicalAccountIdentity::source_capability(HostSurfaceId::Codex, "opaque-b");
    assert_ne!(first_source.account_key(), second_source.account_key());
    assert!(!first_source.account_key().contains("opaque-a"));
}

#[test]
fn provider_display_label_cases() {
    assert_eq!(provider_display_label("Codex"), "OpenAI");
    assert_eq!(provider_display_label("OpenAI / Codex"), "OpenAI");
    assert_eq!(provider_display_label("Claude"), "Anthropic");
    assert_eq!(provider_display_label("Anthropic / Claude"), "Anthropic");
    assert_eq!(provider_display_label("Grok Build"), "xAI");
    assert_eq!(provider_display_label("xAI / Grok"), "xAI");
    assert_eq!(provider_display_label("GLM / Z.AI"), "Z.AI");
    assert_eq!(provider_display_label("Amp"), "Amp");
}

#[test]
fn estimate_caption_variants() {
    let mut view = FocusedUsageView::unavailable("x", 1);
    view.confidence = UsageConfidence::Authoritative;
    view.source = UsageSource::ProviderApi;
    assert_eq!(estimate_caption(&view), None);

    view.confidence = UsageConfidence::Estimated;
    assert_eq!(
        estimate_caption(&view).as_deref(),
        Some("Estimated from token usage · not a subscription bill")
    );

    view.confidence = UsageConfidence::Authoritative;
    view.source = UsageSource::LocalLogs;
    assert_eq!(
        estimate_caption(&view).as_deref(),
        Some("Estimated from token usage · not a subscription bill")
    );

    view.source = UsageSource::Cli;
    view.confidence = UsageConfidence::PresenceOnly;
    assert_eq!(estimate_caption(&view), None);
}

#[test]
fn canon_alias_table_never_uses_probe_routing_as_ownership() {
    assert_eq!(
        HostSurfaceId::from_provider_alias("OpenAI / Codex"),
        Some(HostSurfaceId::Codex)
    );
    assert_eq!(
        HostSurfaceId::from_provider_alias("Anthropic"),
        Some(HostSurfaceId::Claude)
    );
    assert_eq!(
        HostSurfaceId::from_provider_alias("xAI / Grok"),
        Some(HostSurfaceId::Grok)
    );
    assert_eq!(
        HostSurfaceId::from_provider_alias("GLM / Z.AI"),
        Some(HostSurfaceId::Zai)
    );
    assert_eq!(
        HostSurfaceId::from_provider_alias("MiniMax"),
        Some(HostSurfaceId::Minimax)
    );
    assert_eq!(
        HostSurfaceId::from_provider_alias("Moonshot"),
        Some(HostSurfaceId::Kimi)
    );
    assert_eq!(HostSurfaceId::from_provider_alias("OpenAI Z.AI"), None);
    assert_eq!(HostSurfaceId::Zai.agent_slug(), "codex");
    assert_eq!(HostSurfaceId::Minimax.agent_slug(), "codex");
}

#[test]
fn credential_matrix_lists_all_host_surfaces() {
    let rows = host_credential_root_matrix();
    let surfaces: HashSet<_> = rows.iter().map(|row| row.surface).collect();
    for surface in HostSurfaceId::ALL {
        assert!(
            surfaces.contains(surface.id()),
            "matrix missing {}",
            surface.id()
        );
    }
}

struct BatchCountingExecutor {
    calls: AtomicUsize,
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

fn batch_capability(account_id: &str, surface_id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: account_id.to_owned(),
        surface_id: surface_id.to_owned(),
    }
}

fn batch_broker() -> (
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

fn join_batch(
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

#[test]
fn request_usage_batch_dedups_capabilities_and_reports_per_account() {
    let (_temp, client, executor) = batch_broker();
    let first = batch_capability("abc123", "codex");
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

    let reused = request_usage_batch(&client, [second, first], false);
    assert!(reused.iter().all(|(_, result)| result.is_ok()));
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);

    let forced = request_usage_batch(&client, [batch_capability("abc123", "codex")], true);
    assert_eq!(forced.len(), 1);
    assert_eq!(forced[0].1.as_ref().expect("forced ok").generation, 2);
    join_batch(&client, &forced);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 3);
}

#[test]
fn request_usage_batch_rejects_claude_without_active_monitor_mapping() {
    let (_temp, client, executor) = batch_broker();
    let batch = request_usage_batch(
        &client,
        [batch_capability("claude-account", "claude")],
        true,
    );

    assert_eq!(batch.len(), 1);
    let error = batch[0]
        .1
        .as_ref()
        .expect_err("Claude refresh is unauthorized");
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unauthorized);
    assert_eq!(
        error.message,
        "Claude collection requires an active opted-in mapped monitor"
    );
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
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
    assert_eq!(error.kind, UsageCoordinationErrorKind::Unavailable);
}

struct BatchRateLimitedExecutor {
    calls: AtomicUsize,
}

impl crate::coordinator::UsageProviderExecutor for BatchRateLimitedExecutor {
    fn probe(
        &self,
        _capability: &UsageAccountCapability,
        _generation: u64,
    ) -> crate::coordinator::ProviderProbeOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        crate::coordinator::ProviderProbeOutcome::Failure {
            kind: UsageCoordinationErrorKind::RateLimited,
            message: "usage provider rate limit is active".to_owned(),
            retry_at_epoch: Some(chrono::Utc::now().timestamp() + 3_600),
        }
    }
}

#[test]
fn request_usage_batch_forced_refresh_still_honors_retry_after() {
    let temp = tempfile::tempdir().expect("tempdir");
    let executor = Arc::new(BatchRateLimitedExecutor {
        calls: AtomicUsize::new(0),
    });
    let concrete_executor = Arc::clone(&executor);
    let broker_executor: Arc<dyn crate::coordinator::UsageProviderExecutor> = concrete_executor;
    let client = ensure_usage_broker_with_executor(
        UsageBrokerConfig::for_data_dir(temp.path().to_path_buf()),
        broker_executor,
    )
    .expect("broker");
    let capability = batch_capability("abc123", "codex");

    let first = request_usage_batch(&client, [capability.clone()], true);
    assert_eq!(first[0].1.as_ref().expect("first ok").generation, 1);
    client
        .join(capability.clone(), 1, Duration::from_secs(5))
        .expect("join");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);

    let forced = request_usage_batch(&client, [capability], true);
    assert_eq!(forced[0].1.as_ref().expect("forced ok").generation, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

/// Data directory whose broker socket path exceeds the platform `sun_path`
/// limit, mirroring deep test tempdirs and long `$HOME` layouts.
fn overlong_broker_data_dir(temp: &tempfile::TempDir) -> PathBuf {
    let suffix_len = Path::new("usage-broker/run/usage-broker.sock")
        .as_os_str()
        .len()
        + 1;
    let base_len = temp.path().as_os_str().len() + 1;
    let padding = "p".repeat(UNIX_SOCKET_PATH_LIMIT.saturating_sub(base_len + suffix_len) + 8);
    temp.path().join(padding)
}

fn full_broker_socket_path(data_dir: &Path) -> PathBuf {
    data_dir
        .join("usage-broker")
        .join("run")
        .join("usage-broker.sock")
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
    let broker_executor: Arc<dyn crate::coordinator::UsageProviderExecutor> = concrete_executor;
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
