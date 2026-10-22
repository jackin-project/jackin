// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn compact_count_uses_token_suffixes() {
    assert_eq!(compact_count(999), "999");
    assert_eq!(compact_count(1_500), "1.5K");
    assert_eq!(compact_count(2_000_000), "2.0M");
}

#[test]
fn provider_connector_exports_physical_attempts_without_endpoint_material() {
    use std::io::{Read as _, Write as _};

    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _read = stream.read(&mut request).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok")
            .unwrap();
    });
    let secret_route = "provider-secret-route?token=provider-secret-query";
    provider_http_client()
        .unwrap()
        .get(format!("http://{address}/{secret_route}"))
        .send()
        .unwrap();
    server.join().unwrap();

    let refused = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let refused_address = refused.local_addr().unwrap();
    drop(refused);
    provider_http_client()
        .unwrap()
        .get(format!("http://{refused_address}/{secret_route}"))
        .send()
        .unwrap_err();

    export.force_flush();
    let spans = export.finished_spans();
    assert_eq!(spans.len(), 2);
    assert!(
        spans
            .iter()
            .all(|span| span.name == jackin_telemetry::schema::spans::CONNECTION_ATTEMPT)
    );
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("provider"));
    assert!(export.contains_span_text("error"));
    assert!(export.contains_span_text("io_error"));
    for prohibited in [
        secret_route,
        "provider-secret-query",
        &address.to_string(),
        &refused_address.to_string(),
    ] {
        assert!(!export.contains_span_text(prohibited));
        assert!(!export.contains_log_text(prohibited));
    }
}

#[test]
fn provider_labels_resolve_all_account_refresh_surfaces() {
    assert_eq!(
        resolve_surface("codex", Some("Claude")),
        UsageSurface::Claude
    );
    assert_eq!(
        resolve_surface("claude", Some("Codex")),
        UsageSurface::Codex
    );
    assert_eq!(resolve_surface("codex", Some("Amp")), UsageSurface::Amp);
    assert_eq!(
        resolve_surface("claude", Some("Grok Build")),
        UsageSurface::Grok
    );
    assert_eq!(
        resolve_surface("codex", Some("GLM / Z.AI")),
        UsageSurface::Zai
    );
    assert_eq!(resolve_surface("codex", Some("Kimi")), UsageSurface::Kimi);
    assert_eq!(
        resolve_surface("codex", Some("MiniMax")),
        UsageSurface::Minimax
    );
    assert_eq!(
        resolve_surface("cursor", Some("Cursor")),
        UsageSurface::Cursor
    );
    assert_eq!(resolve_surface("cursor", None), UsageSurface::Cursor);
    assert_eq!(
        resolve_surface("gemini", Some("Google")),
        UsageSurface::Google
    );
    assert_eq!(
        resolve_surface("codex", Some("Gemini")),
        UsageSurface::Google
    );
    assert_eq!(resolve_surface("gemini", None), UsageSurface::Google);
    assert_eq!(
        resolve_surface("opencode", Some("OpenRouter")),
        UsageSurface::OpenRouter
    );
    assert_eq!(
        broker_surface_id("opencode", Some("OpenRouter")),
        Some("openrouter")
    );
    assert_eq!(resolve_surface("antigravity", None), UsageSurface::Google);
    for agent in ["muse", "omp", "hermes"] {
        assert_eq!(
            resolve_surface(agent, None),
            UsageSurface::Unsupported,
            "{agent} must stay unsupported"
        );
    }
}

#[test]
fn capability_matches_newly_wired_surfaces_only() {
    use jackin_protocol::usage_broker::UsageAccountCapability;

    let capability = |surface_id: &str| UsageAccountCapability {
        account_id: "account-test".to_owned(),
        surface_id: surface_id.to_owned(),
    };
    assert!(capability_matches_surface(
        "cursor",
        Some("Cursor"),
        &capability("cursor")
    ));
    assert!(capability_matches_surface(
        "gemini",
        Some("Google"),
        &capability("google")
    ));
    assert!(capability_matches_surface(
        "opencode",
        Some("OpenRouter"),
        &capability("openrouter")
    ));
    assert!(!capability_matches_surface(
        "cursor",
        Some("Cursor"),
        &capability("google")
    ));
    assert!(capability_matches_surface(
        "antigravity",
        None,
        &capability("google")
    ));
    assert!(!capability_matches_surface(
        "muse",
        Some("Muse"),
        &capability("meta")
    ));
}

#[test]
fn unpollable_snapshot_is_honest_unsupported() {
    let view = unpollable_snapshot("muse", Some("Meta"), 1_781_728_000);
    assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.confidence, UsageConfidence::None);
    assert_eq!(
        view.last_error.as_deref(),
        Some("usage polling not supported for this provider")
    );
    assert!(view.buckets.is_empty());
    assert!(view.account.account_label.is_empty());
    assert_eq!(view.focused_agent.as_deref(), Some("muse"));
}

#[test]
fn provider_tabs_emit_one_tab_per_account_keyed_by_stable_id() {
    let claude_stale = account_snapshot_view("Anthropic", "a@example.com", Some("Max"), 100);
    let claude_latest = account_snapshot_view("Anthropic", "a@example.com", Some("Max 20x"), 200);
    let codex = account_snapshot_view("OpenAI", "codex@example.com", Some("Pro 20x"), 150);

    let tabs = provider_tabs(&[&claude_stale, &claude_latest, &codex]);

    assert_eq!(tabs.len(), 2);
    assert_eq!(
        tabs.iter().map(|tab| &tab.label).collect::<Vec<_>>(),
        vec!["Anthropic · a@example.com", "OpenAI · codex@example.com"]
    );
    let claude = tabs
        .iter()
        .find(|tab| tab.account_label == "a@example.com")
        .expect("claude tab");
    assert_eq!(claude.plan_label.as_deref(), Some("Max 20x"));
    assert_eq!(
        claude.id,
        usage_account_tab_id("Anthropic", "a@example.com")
    );
    assert_eq!(
        tabs[1].id,
        usage_account_tab_id("OpenAI", "codex@example.com")
    );
    assert_ne!(tabs[0].id, tabs[1].id);
    assert!(tabs.iter().all(|tab| !tab.active));

    let cursor = account_snapshot_view("Cursor", "cursor@example.com", None, 100);
    let tabs = provider_tabs(&[&cursor]);
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0].label, "Cursor · cursor@example.com");
    assert!(provider_tabs(&[]).is_empty());

    let claude_b = account_snapshot_view("Anthropic", "b@example.com", None, 100);
    let tabs = provider_tabs(&[&claude_stale, &claude_b]);
    assert_eq!(
        tabs.iter().map(|tab| &tab.label).collect::<Vec<_>>(),
        vec!["Anthropic · a@example.com", "Anthropic · b@example.com"]
    );
    let unknown = account_snapshot_view("Anthropic", "", None, 100);
    let tabs = provider_tabs(&[&unknown]);
    assert_eq!(tabs[0].label, "Anthropic");
}

#[test]
fn enrich_provider_tabs_rebuilds_strip_from_snapshots() {
    let mut view = account_snapshot_view("OpenAI", "codex@example.com", Some("Pro 20x"), 123);
    view.tabs = vec![UsageProviderTab {
        id: "stale".to_owned(),
        label: "Stale".to_owned(),
        status_label: String::new(),
        account_label: String::new(),
        plan_label: None,
        source_label: None,
        active: true,
        account_capability: None,
    }];
    let claude = account_snapshot_view("Anthropic", "claude@example.com", Some("Max"), 120);

    let mut snapshots = HashMap::new();
    snapshots.insert(
        "Anthropic:account-1".to_owned(),
        CachedUsage { view: claude },
    );
    snapshots.insert(
        "OpenAI:account-2".to_owned(),
        CachedUsage { view: view.clone() },
    );

    enrich_provider_tabs(&mut view, &snapshots);

    assert_eq!(view.tabs.len(), 2);
    let codex = view
        .tabs
        .iter()
        .find(|tab| tab.label == "OpenAI · codex@example.com")
        .expect("codex tab");
    assert_eq!(codex.account_label, "codex@example.com");
    assert_eq!(codex.plan_label.as_deref(), Some("Pro 20x"));
    let claude = view
        .tabs
        .iter()
        .find(|tab| tab.label == "Anthropic · claude@example.com")
        .expect("claude tab");
    assert_eq!(claude.account_label, "claude@example.com");
    assert_eq!(claude.plan_label.as_deref(), Some("Max"));

    let mut view = account_snapshot_view("OpenAI", "codex@example.com", None, 123);
    enrich_provider_tabs(&mut view, &HashMap::new());
    assert!(view.tabs.is_empty());
}

#[test]
fn two_claude_accounts_and_codex_produce_three_tabs_with_distinct_ids() {
    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_test(
        "claude",
        Some("Anthropic"),
        account_snapshot_view("Anthropic", "a@example.com", Some("Max"), 100),
    );
    cache.insert_snapshot_for_test(
        "claude",
        Some("Anthropic"),
        account_snapshot_view("Anthropic", "b@example.com", Some("Max 20x"), 200),
    );
    cache.insert_snapshot_for_test(
        "codex",
        Some("OpenAI"),
        account_snapshot_view("OpenAI", "codex@example.com", Some("Pro 20x"), 150),
    );

    let snapshot = cache.focused_snapshot(Some("claude"), Some("Anthropic"));

    assert_eq!(snapshot.tabs.len(), 3);
    let mut ids: Vec<String> = snapshot.tabs.iter().map(|tab| tab.id.clone()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 3);
    let mut expected = vec![
        usage_account_tab_id("Anthropic", "a@example.com"),
        usage_account_tab_id("Anthropic", "b@example.com"),
        usage_account_tab_id("OpenAI", "codex@example.com"),
    ];
    expected.sort();
    assert_eq!(ids, expected);
    let active: Vec<&UsageProviderTab> = snapshot.tabs.iter().filter(|tab| tab.active).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(
        active[0].id,
        usage_account_tab_id("Anthropic", "b@example.com")
    );

    let id_a = usage_account_tab_id("Anthropic", "a@example.com");
    let mut selected = account_snapshot_view("Anthropic", "a@example.com", Some("Max"), 100);
    enrich_provider_tabs(&mut selected, &cache.snapshots);
    mark_active_tab(&mut selected);
    let active: Vec<&UsageProviderTab> = selected.tabs.iter().filter(|tab| tab.active).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, id_a);
    assert_eq!(active[0].account_label, "a@example.com");
    let mut labels: Vec<String> = selected.tabs.iter().map(|tab| tab.label.clone()).collect();
    labels.sort();
    labels.dedup();
    assert_eq!(labels.len(), 3);
}

#[test]
fn focused_snapshot_for_account_id_selects_exact_account() {
    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_test(
        "claude",
        Some("Anthropic"),
        account_snapshot_view("Anthropic", "a@example.com", Some("Max"), 100),
    );
    cache.insert_snapshot_for_test(
        "claude",
        Some("Anthropic"),
        account_snapshot_view("Anthropic", "b@example.com", Some("Max 20x"), 200),
    );
    let id_b = usage_account_tab_id("Anthropic", "b@example.com");

    let snapshot = cache
        .focused_snapshot_for_account_id(&id_b)
        .expect("snapshot for claude-b");
    assert_eq!(snapshot.account.account_label, "b@example.com");
    assert_eq!(snapshot.tabs.len(), 2);
    let active: Vec<&UsageProviderTab> = snapshot.tabs.iter().filter(|tab| tab.active).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, id_b);

    assert!(
        cache
            .focused_snapshot_for_account_id("sha256:unknown")
            .is_none()
    );
    assert!(cache.focused_snapshot_for_account_id("").is_none());
}
