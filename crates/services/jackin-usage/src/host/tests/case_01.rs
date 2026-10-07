// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn canonical_projection_uses_current_membership_provider_names_and_rust_ranks() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let mut zulu = codex_fixture_view();
    zulu.account.account_label = "zulu@example.test".to_owned();
    zulu.status = UsageSnapshotStatus::Stale;
    zulu.buckets
        .iter_mut()
        .for_each(|bucket| bucket.status = UsageSnapshotStatus::Stale);
    let mut alpha = codex_fixture_view();
    alpha.account.account_label = "Alpha@example.test".to_owned();
    alpha.buckets[0].severity = UsageSeverity::Danger;
    let zulu_account = canonical_discovered_account(HostSurfaceId::Codex, "zulu@example.test");
    let alpha_account = canonical_discovered_account(HostSurfaceId::Codex, "Alpha@example.test");
    runtime.discovered_views.insert(
        (HostSurfaceId::Codex, zulu_account.account_key.clone()),
        zulu,
    );
    runtime.discovered_views.insert(
        (HostSurfaceId::Codex, alpha_account.account_key.clone()),
        alpha,
    );
    runtime.discovery = Some(ValidatedUsageDiscovery {
        config_generation: Some("config-generation".to_owned()),
        accounts: vec![zulu_account, alpha_account],
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: Vec::new(),
    });

    let projection = runtime.canonical_projection("en").expect("projection");
    let repeated = runtime
        .canonical_projection("en")
        .expect("repeat projection");
    assert_eq!(repeated, projection, "reads must not republish generations");
    assert_eq!(projection.providers.len(), 1);
    assert_eq!(projection.providers[0].display_name, "OpenAI");
    assert_eq!(
        projection.providers[0]
            .accounts
            .iter()
            .map(|account| account.display_label.as_str())
            .collect::<Vec<_>>(),
        vec!["Alpha@example.test", "zulu@example.test"]
    );
    assert_eq!(projection.providers[0].accounts[0].rank, 0);
    assert_eq!(projection.providers[0].accounts[1].rank, 1);
    assert_eq!(projection.providers[0].accounts[0].windows[0].rank, 0);
    assert_eq!(
        projection.providers[0].accounts[1].freshness.phase,
        jackin_protocol::usage_broker::UsageFreshnessPhaseV1::Stale
    );
    assert_eq!(projection.providers[0].accounts[1].windows.len(), 2);

    let selected = UsageDestination::Account {
        provider_id: "openai".to_owned(),
        canonical_account_id: projection.providers[0].accounts[1]
            .canonical_account_id
            .clone(),
    };
    assert_eq!(
        normalize_destination(&projection, &selected).destination,
        selected
    );
    let removed = UsageDestination::Account {
        provider_id: "openai".to_owned(),
        canonical_account_id: "removed".to_owned(),
    };
    assert_eq!(
        normalize_destination(&projection, &removed),
        NormalizedUsageDestination {
            destination: UsageDestination::Overview,
            notice: Some("Selected account is no longer available.".to_owned()),
        }
    );
}

#[test]
fn canonical_projection_keeps_unresolved_capability_out_of_account_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime.discovery = Some(ValidatedUsageDiscovery {
        config_generation: Some("unresolved-generation".to_owned()),
        accounts: Vec::new(),
        diagnostics: Vec::new(),
        candidates: vec![UsageSourceCandidateDescriptor {
            surface_id: "codex".to_owned(),
            credential_kind: UsageCredentialKind::ForwardedCapability,
            source_id: "source-0001".to_owned(),
            capability_id: "opaque-capability".to_owned(),
            provenance: vec!["workspace sample".to_owned()],
        }],
        bindings: vec![discovery::ValidatedCredentialBinding {
            surface: HostSurfaceId::Codex,
            identity: None,
            source_id: "source-0001".to_owned(),
            capability_id: "opaque-capability".to_owned(),
            credential_revision: "credential-revision".to_owned(),
            provenance: std::collections::BTreeSet::from(["workspace sample".to_owned()]),
            source: discovery::ValidatedCredentialSource::Capability,
        }],
    });

    let projection = runtime.canonical_projection("und").expect("projection");
    assert_eq!(projection.providers.len(), 1);
    assert!(projection.providers[0].accounts.is_empty());
    assert_eq!(projection.unresolved.len(), 1);
    assert_eq!(projection.unresolved[0].provider_id, "openai");
}

#[test]
fn canonical_projection_provider_order_is_settled_and_not_agent_named() {
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
fn canonical_projection_alias_transition_is_atomic_idempotent_and_fail_closed() {
    let mut graph = accounts::CanonicalIdentityGraph::default();
    let first = CanonicalAccountIdentity {
        surface: HostSurfaceId::Codex,
        subject: CanonicalAccountSubject::ProviderId("organization-a".to_owned()),
    };
    let conflicting = CanonicalAccountIdentity {
        surface: HostSurfaceId::Codex,
        subject: CanonicalAccountSubject::ProviderId("organization-b".to_owned()),
    };
    let canonical_id = graph
        .resolve_alias("capability-a", &first)
        .expect("first alias");
    assert_eq!(
        graph
            .resolve_alias("capability-a", &first)
            .expect("alias replay"),
        canonical_id
    );
    assert_eq!(
        graph
            .resolve_alias("capability-a", &conflicting)
            .expect_err("conflicting alias"),
        "canonical account alias collision"
    );
    assert_eq!(
        graph
            .resolve_alias("capability-a", &first)
            .expect("failed transaction preserves alias"),
        canonical_id
    );
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
fn fixture_snapshot_matches_capsule_view_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let fixture = codex_fixture_view();
    runtime
        .inject_snapshot("codex", fixture.clone())
        .expect("inject");
    let view = runtime.snapshot("codex").expect("snapshot");
    assert_eq!(view.status_bar_label, fixture.status_bar_label);
    assert_eq!(view.buckets.len(), fixture.buckets.len());
    assert_eq!(
        view.buckets[0].remaining_percent,
        fixture.buckets[0].remaining_percent
    );
    assert_eq!(view.buckets[0].resets_at, fixture.buckets[0].resets_at);
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.account.account_label, "codex@example.com");
    assert_eq!(
        runtime.status_bar_label("codex").expect("label"),
        Some(fixture.status_bar_label)
    );
}

#[test]
fn unavailable_and_refreshing_never_invent_percent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    // No inject → refreshing (focused agent path with empty cache).
    let refreshing = runtime.snapshot("claude").expect("snapshot");
    assert_eq!(refreshing.status_bar_label, "refreshing");
    assert!(
        refreshing
            .buckets
            .iter()
            .all(|bucket| bucket.remaining_percent.is_none()),
        "refreshing must not invent remaining_percent"
    );

    let unavailable = FocusedUsageView::unavailable("missing credentials", 42);
    runtime
        .inject_snapshot("claude", unavailable)
        .expect("inject");
    let view = runtime.snapshot("claude").expect("snapshot");
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert!(view.buckets.is_empty());
    assert_eq!(view.status_bar_label, "usage unavailable");
    assert!(
        !view.status_bar_label.chars().any(|c| c.is_ascii_digit()),
        "unavailable headline must not invent numbers"
    );
}

#[test]
fn snapshot_surfaces_discovery_diagnostic_instead_of_refreshing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = HostUsageRuntime::new();
    // Logged-out claude (malformed) + missing kimi: both must read as an
    // honest needs-login view, never the bare `refreshing` placeholder.
    let discovery = ValidatedUsageDiscovery {
        config_generation: None,
        accounts: Vec::new(),
        diagnostics: vec![
            UsageDiscoveryDiagnostic {
                surface_id: Some("claude".to_owned()),
                scope_label: "account claude".to_owned(),
                issue: UsageDiscoveryIssue::CredentialMalformed,
            },
            UsageDiscoveryDiagnostic {
                surface_id: Some("kimi".to_owned()),
                scope_label: "account kimi".to_owned(),
                issue: UsageDiscoveryIssue::CredentialMissing,
            },
        ],
        candidates: Vec::new(),
        bindings: Vec::new(),
    };
    runtime
        .open_with_validated_discovery(HostRuntimeConfig::under_data_dir(dir.path()), discovery)
        .expect("open");
    for surface in ["claude", "kimi"] {
        let view = runtime.snapshot(surface).expect("snapshot");
        assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
        assert!(!view.is_refreshing_placeholder());
        let error = view.last_error.as_deref().expect("diagnostic error");
        assert!(
            error.contains("log in"),
            "diagnostic must name the login action: {error}"
        );
    }
    // A surface with no diagnostic keeps the genuine cold placeholder.
    let cold = runtime.snapshot("codex").expect("snapshot");
    assert!(cold.is_refreshing_placeholder());
}

#[test]
fn selected_account_route_is_unselected_without_persisted_choice() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let catalog = runtime
        .materialize_account_catalog()
        .expect("account catalog");

    let (route, view) = runtime.selected_route_and_view_for_catalog(&catalog, HostSurfaceId::Codex);

    assert_eq!(route, HostSelectedAccountRoute::Unselected);
    assert!(view.is_some_and(|view| view.is_refreshing_placeholder()));
}

#[test]
fn selected_account_route_resolves_during_cold_placeholder_without_sibling_fallback() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = "persisted-codex-key";
    let selected = HashMap::from([("codex".to_owned(), key.to_owned())]);
    accounts::save_selected_accounts(&selected_accounts_path(dir.path()), &selected)
        .expect("seed persisted selection");
    let mut runtime = open_runtime(dir.path());

    let projection = runtime.desktop_projection(3).expect("cold projection");
    let provider = projection
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "codex")
        .expect("persisted selection keeps provider visible");

    assert_eq!(
        provider.selected_account_route,
        HostSelectedAccountRoute::Resolving {
            account_key: key.to_owned(),
        }
    );
    assert!(provider.selected_usage.is_refreshing_placeholder());
    assert!(provider.group.accounts.is_empty());
}

#[test]
fn selected_account_routes_remain_scoped_to_their_provider() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let codex_view = codex_fixture_view();
    let codex = canonical_discovered_account(HostSurfaceId::Codex, "codex@example.com");
    let mut claude_view = codex_fixture_view();
    claude_view.focused_agent = Some("claude".to_owned());
    claude_view.focused_provider = Some("Claude".to_owned());
    claude_view.account.provider_label = "Anthropic / Claude".to_owned();
    claude_view.account.account_label = "claude@example.com".to_owned();
    let claude = canonical_discovered_account(HostSurfaceId::Claude, "claude@example.com");
    runtime.discovered_views.insert(
        (HostSurfaceId::Codex, codex.account_key.clone()),
        codex_view,
    );
    runtime.discovered_views.insert(
        (HostSurfaceId::Claude, claude.account_key.clone()),
        claude_view,
    );
    runtime.discovery = Some(ValidatedUsageDiscovery {
        config_generation: Some("two-provider-generation".to_owned()),
        accounts: vec![codex.clone(), claude.clone()],
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: Vec::new(),
    });
    runtime
        .set_selected_account("codex", &codex.account_key)
        .expect("select exact Codex account");
    runtime
        .selected_accounts
        .insert("claude".to_owned(), "removed-claude-account".to_owned());

    let projection = runtime
        .desktop_projection(3)
        .expect("two-provider projection");
    let codex_projection = projection
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "codex")
        .expect("Codex projection");
    let claude_projection = projection
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "claude")
        .expect("Claude projection");

    assert_eq!(
        codex_projection.selected_account_route,
        HostSelectedAccountRoute::Available {
            account_key: codex.account_key,
        }
    );
    assert_eq!(
        claude_projection.selected_account_route,
        HostSelectedAccountRoute::Unavailable {
            account_key: "removed-claude-account".to_owned(),
            notice: SELECTED_ACCOUNT_UNAVAILABLE_NOTICE,
        }
    );
    assert_eq!(
        claude_projection.selected_usage.last_error.as_deref(),
        Some(SELECTED_ACCOUNT_UNAVAILABLE_NOTICE)
    );
    assert!(
        claude_projection
            .group
            .accounts
            .iter()
            .all(|account| !account.selected)
    );
}
