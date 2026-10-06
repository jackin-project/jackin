// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn removed_last_selected_account_keeps_unavailable_provider_and_restores_exact_key() {
    for retain_cached_quota in [false, true] {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut runtime = open_runtime(dir.path());
        let view = codex_fixture_view();
        let account = canonical_discovered_account(HostSurfaceId::Codex, "codex@example.com");
        let key = account.account_key.clone();
        runtime
            .inject_snapshot("codex", view.clone())
            .expect("seed quota");
        runtime.discovery = Some(ValidatedUsageDiscovery {
            config_generation: Some("present".to_owned()),
            accounts: vec![account.clone()],
            diagnostics: Vec::new(),
            candidates: Vec::new(),
            bindings: Vec::new(),
        });
        runtime
            .set_selected_account("codex", &key)
            .expect("select account");
        runtime
            .discovery
            .as_mut()
            .expect("discovery")
            .accounts
            .clear();
        if !retain_cached_quota {
            runtime
                .inject_snapshot("codex", FocusedUsageView::unavailable("removed", 2))
                .expect("clear cache");
            runtime
                .discovery
                .as_mut()
                .expect("discovery")
                .accounts
                .clear();
        }
        let projection = runtime.desktop_projection(3).expect("removed projection");
        let provider = projection
            .providers
            .iter()
            .find(|provider| provider.group.surface_id == "codex")
            .expect("requested provider must remain visible");
        assert!(provider.group.accounts.is_empty());
        assert_eq!(
            provider.selected_account_route,
            HostSelectedAccountRoute::Unavailable {
                account_key: key.clone(),
                notice: SELECTED_ACCOUNT_UNAVAILABLE_NOTICE,
            }
        );
        assert_eq!(
            provider.selected_usage.status,
            UsageSnapshotStatus::Unavailable
        );
        assert!(
            provider.selected_usage.buckets.is_empty(),
            "removed account cannot show cached quota"
        );
        assert_eq!(
            provider.selected_usage.last_error.as_deref(),
            Some(SELECTED_ACCOUNT_UNAVAILABLE_NOTICE)
        );
        let empty = provider
            .group
            .empty_state
            .as_ref()
            .expect("explicit empty state");
        assert_eq!(empty.status_word, "unavailable");
        assert_eq!(
            empty.last_error.as_deref(),
            Some(SELECTED_ACCOUNT_UNAVAILABLE_NOTICE)
        );
        assert!(!empty.is_refreshing);
        assert!(
            projection
                .status_bar_glance_rows
                .iter()
                .all(|row| row.surface_id != "codex")
        );
        assert_eq!(
            accounts::load_selected_accounts(&accounts::selected_accounts_path(dir.path()))
                .get("codex"),
            Some(&key)
        );

        drop(runtime);
        let mut reopened = open_runtime(dir.path());
        reopened.discovery = Some(ValidatedUsageDiscovery {
            config_generation: Some("still-removed".to_owned()),
            accounts: Vec::new(),
            diagnostics: Vec::new(),
            candidates: Vec::new(),
            bindings: Vec::new(),
        });
        let unavailable = reopened
            .snapshot("codex")
            .expect("persisted unavailable selection");
        assert_eq!(unavailable.status, UsageSnapshotStatus::Unavailable);
        assert!(unavailable.buckets.is_empty());
        assert_eq!(
            unavailable.last_error.as_deref(),
            Some(SELECTED_ACCOUNT_UNAVAILABLE_NOTICE)
        );
        reopened.discovery = Some(ValidatedUsageDiscovery {
            config_generation: Some("returned".to_owned()),
            accounts: vec![account],
            diagnostics: Vec::new(),
            candidates: Vec::new(),
            bindings: Vec::new(),
        });
        reopened
            .inject_snapshot("codex", view)
            .expect("restore exact account quota");
        let restored = reopened
            .snapshot("codex")
            .expect("restored selected account");
        assert_eq!(restored.account.account_label, "codex@example.com");
        assert_eq!(restored.buckets.len(), 2);
        assert_eq!(reopened.selected_accounts.get("codex"), Some(&key));
    }
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
        subject: CanonicalAccountSubject::ProviderStableHandle("same@example.test".to_owned()),
    };
    assert_ne!(
        provider_id.canonical_id_v1(),
        stable_handle.canonical_id_v1()
    );
    assert_ne!(
        provider_id.account_key(),
        stable_handle.account_key(),
        "routing keys must retain the identity evidence kind"
    );

    let mut uppercase = FocusedUsageView::unavailable("seed", 1);
    uppercase.focused_agent = Some("codex".to_owned());
    uppercase.account.provider_label = "OpenAI".to_owned();
    uppercase.account.account_label = " Person@Example.Test ".to_owned();
    uppercase.confidence = UsageConfidence::Authoritative;
    let mut lowercase = uppercase.clone();
    lowercase.account.account_label = "person@example.test".to_owned();
    assert_eq!(
        canonical_account_id_for_view(&uppercase),
        canonical_account_id_for_view(&lowercase)
    );
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
fn canon_openai_account_never_appears_under_routed_providers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "codex",
            glance_view(
                "OpenAI / Codex",
                Some("OAuth"),
                vec![glance_weekly_bucket(66)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    assert_eq!(
        runtime
            .list_accounts(Some("codex"))
            .expect("codex accounts")
            .len(),
        1
    );
    assert!(
        runtime
            .list_accounts(Some("zai"))
            .expect("zai accounts")
            .is_empty()
    );
    assert!(
        runtime
            .list_accounts(Some("minimax"))
            .expect("minimax accounts")
            .is_empty()
    );
}

#[test]
fn canon_same_account_label_on_two_providers_remains_two_accounts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let email = "same@example.com";
    let mut codex = glance_view(
        "OpenAI / Codex",
        Some("OAuth"),
        vec![glance_weekly_bucket(66)],
        UsageSnapshotStatus::Fresh,
    );
    codex.account.account_label = email.to_owned();
    let mut claude = glance_view(
        "Anthropic / Claude",
        Some("OAuth"),
        vec![glance_weekly_bucket(77)],
        UsageSnapshotStatus::Fresh,
    );
    claude.account.account_label = email.to_owned();
    let codex_key = account_key_for_view(&codex).expect("Codex key");
    let claude_key = account_key_for_view(&claude).expect("Claude key");
    assert_ne!(codex_key, claude_key);
    runtime
        .inject_snapshot("codex", codex)
        .expect("inject Codex");
    runtime
        .inject_snapshot("claude", claude)
        .expect("inject Claude");
    let rows = runtime.list_accounts(None).expect("Desktop accounts");
    assert_eq!(
        rows.iter().filter(|row| row.account_label == email).count(),
        2
    );
}

#[test]
fn canon_presence_only_state_is_not_an_account() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let mut presence = glance_view(
        "Amp",
        Some("local Amp auth"),
        Vec::new(),
        UsageSnapshotStatus::Unavailable,
    );
    presence.account.account_label = "local Amp auth".to_owned();
    presence.confidence = UsageConfidence::PresenceOnly;
    runtime.inject_snapshot("amp", presence).expect("inject");
    assert!(
        runtime
            .list_accounts(Some("amp"))
            .expect("amp accounts")
            .is_empty()
    );
    let inventory = runtime.desktop_inventory().expect("inventory");
    let amp = inventory
        .groups
        .iter()
        .find(|group| group.surface_id == "amp")
        .expect("detected Amp state");
    assert!(amp.accounts.is_empty());
    assert_eq!(
        amp.empty_state
            .as_ref()
            .map(|state| state.status_word.as_str()),
        Some("unavailable")
    );
    assert_eq!(amp.plan_or_status_label, "unavailable");
}

#[test]
fn canon_sel_rejects_unknown_and_cross_surface_keys() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let view = glance_view(
        "OpenAI / Codex",
        Some("OAuth"),
        vec![glance_weekly_bucket(66)],
        UsageSnapshotStatus::Fresh,
    );
    let key = account_key_for_view(&view).expect("canonical key");
    runtime.inject_snapshot("codex", view).expect("inject");
    assert!(
        runtime
            .set_selected_account("codex", "sha256:unknown")
            .is_err()
    );
    assert!(runtime.set_selected_account("zai", &key).is_err());
    runtime
        .set_selected_account("codex", &key)
        .expect("same-surface selection");
    let rows = runtime.list_accounts(Some("codex")).expect("selected rows");
    assert_eq!(rows.iter().filter(|row| row.selected).count(), 1);
}

#[test]
fn canon_sel_stale_persisted_key_remains_explicitly_unavailable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut selected = HashMap::new();
    selected.insert("codex".to_owned(), "sha256:unknown".to_owned());
    accounts::save_selected_accounts(&accounts::selected_accounts_path(dir.path()), &selected)
        .expect("seed stale selection");

    let mut runtime = open_runtime(dir.path());
    let view = glance_view(
        "OpenAI / Codex",
        Some("OAuth"),
        vec![glance_weekly_bucket(66)],
        UsageSnapshotStatus::Fresh,
    );
    let key = account_key_for_view(&view).expect("canonical key");
    runtime.inject_snapshot("codex", view).expect("inject");
    let rows = runtime.list_accounts(Some("codex")).expect("accounts");
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].selected);
    assert_eq!(rows[0].account_key, key);
    let snapshot = runtime.snapshot("codex").expect("unavailable snapshot");
    assert_eq!(snapshot.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(
        snapshot.last_error.as_deref(),
        Some(SELECTED_ACCOUNT_UNAVAILABLE_NOTICE)
    );
    let persisted = accounts::load_selected_accounts(&accounts::selected_accounts_path(dir.path()));
    assert_eq!(persisted.get("codex"), Some(&"sha256:unknown".to_owned()));
}
