// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

// `Dialog` import removed: Dialog type lives in jackin-capsule; using it
// from jackin-usage tests would create a circular dep (Blocker 2 Option A).

use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageAccountIdentity,
    UsageCanonicalAccountSubject, UsageConfidence, UsageSnapshotStatus, UsageSource,
};

use super::*;

fn usage_view() -> FocusedUsageView {
    FocusedUsageView {
        canonical_identity: None,
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("OpenAI".to_owned()),
        account_identity: Some(UsageAccountIdentity {
            source_revision: None,
            account_id: "account-codex-alexey".to_owned(),
            surface_id: "surface-codex".to_owned(),
        }),
        account: FocusedAccountHeader {
            provider_label: "Codex".to_owned(),
            account_label: "alexey@example.com".to_owned(),
            username: None,
            plan_label: Some("Pro 20x".to_owned()),
            credential_origin: None,
        },
        buckets: vec![
            QuotaBucketView {
                count_quota: None,
                used_money: None,
                limit_money: None,
                remaining_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Session".to_owned(),
                used_label: Some("63% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(37),
                reset_label: Some("Resets in 1h".to_owned()),
                resets_at: None,
                status_slot: None,
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
            },
            QuotaBucketView {
                count_quota: None,
                used_money: None,
                limit_money: None,
                remaining_money: None,
                severity: jackin_protocol::control::UsageSeverity::default(),
                label: "Credits".to_owned(),
                used_label: None,
                limit_label: None,
                remaining_percent: None,
                reset_label: None,
                resets_at: None,
                status_slot: None,
                pace_label: Some("ACP billing unavailable".to_owned()),
                status: UsageSnapshotStatus::Unsupported,
            },
        ],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::Cli,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_781_185_560,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
        tabs: Vec::new(),
        last_error: None,
    }
}

fn provider_usage_view(
    provider: &str,
    account: &str,
    plan: Option<&str>,
    bucket: &str,
    remaining: u8,
    fetched_at_epoch: i64,
) -> FocusedUsageView {
    FocusedUsageView {
        canonical_identity: None,
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some(provider.to_owned()),
        account_identity: Some(provider_identity(provider, account)),
        account: FocusedAccountHeader {
            provider_label: provider.to_owned(),
            account_label: account.to_owned(),
            username: None,
            plan_label: plan.map(str::to_owned),
            credential_origin: None,
        },
        buckets: vec![QuotaBucketView {
            count_quota: None,
            used_money: None,
            limit_money: None,
            remaining_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: bucket.to_owned(),
            used_label: Some(format!("{}% used", 100_u8.saturating_sub(remaining))),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(remaining),
            reset_label: Some("Resets at 15:00 UTC".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("On pace".to_owned()),
            status: UsageSnapshotStatus::Fresh,
        }],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: format!("{bucket} {remaining}%"),
        tabs: Vec::new(),
        last_error: None,
    }
}

fn provider_identity(provider: &str, account: &str) -> UsageAccountIdentity {
    UsageAccountIdentity {
        source_revision: None,
        account_id: format!("account-{account}"),
        surface_id: format!("surface-{provider}"),
    }
}

#[test]
fn account_snapshot_rows_are_persisted_and_upserted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");

    store_usage_snapshot(&db, &usage_view()).expect("store first snapshot");
    let mut changed = usage_view();
    changed.buckets[0].remaining_percent = Some(25);
    changed.fetched_at_epoch += 60;
    store_usage_snapshot(&db, &changed).expect("store updated snapshot");

    let rows = stored_account_snapshots(&db).expect("read snapshots");
    assert_eq!(rows.len(), 2);
    let session = rows
        .iter()
        .find(|row| row.window_kind == "Session")
        .expect("session row");
    assert_eq!(session.provider, "Codex");
    assert!(session.account_key_hash.starts_with("sha256:"));
    assert_eq!(session.source, "cli");
    assert_eq!(session.confidence, "authoritative");
    assert_eq!(session.used_amount, Some(75));
    assert_eq!(session.used_unit.as_deref(), Some("percent"));
    assert_eq!(session.limit_amount, Some(100));
    assert_eq!(session.status, "fresh");
    assert_eq!(session.fetched_at, 1_781_185_620);
    assert_eq!(session.remaining_percent, Some(25));
    assert_eq!(session.used_label.as_deref(), Some("63% used"));
    assert_eq!(session.limit_label.as_deref(), Some("100%"));
    assert_eq!(session.plan_label.as_deref(), Some("Pro 20x"));
}

#[test]
fn canon_durable_reconstruction_pins_one_preferred_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let mut cli = usage_view();
    cli.source = UsageSource::Cli;
    cli.buckets = vec![QuotaBucketView {
        label: "Weekly".to_owned(),
        remaining_percent: Some(12),
        ..cli.buckets[0].clone()
    }];
    let mut provider_api = cli.clone();
    provider_api.source = UsageSource::ProviderApi;
    provider_api.buckets[0].remaining_percent = Some(88);
    store_usage_snapshot(&db, &cli).expect("store CLI");
    store_usage_snapshot(&db, &provider_api).expect("store provider API");

    let views =
        load_all_account_usage_views(&db, provider_api.fetched_at_epoch).expect("materialize once");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].view.source, UsageSource::ProviderApi);
    assert_eq!(views[0].view.buckets.len(), 1);
    assert_eq!(views[0].view.buckets[0].remaining_percent, Some(88));
}

#[test]
fn repeated_writes_reopen_owned_connections_and_revalidate_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");

    store_usage_snapshot(&db, &usage_view()).expect("store first snapshot");
    assert_eq!(
        connection_build_count(&db).expect("first build count"),
        1,
        "first write should build one connection"
    );

    let mut changed = usage_view();
    changed.fetched_at_epoch += 60;
    changed.buckets[0].remaining_percent = Some(21);
    store_usage_snapshot(&db, &changed).expect("store second snapshot");

    assert_eq!(
        connection_build_count(&db).expect("second build count"),
        2,
        "each write owns and releases a freshly validated connection"
    );
    let rows = stored_account_snapshots(&db).expect("read snapshots");
    let session = rows
        .iter()
        .find(|row| row.window_kind == "Session")
        .expect("session row");
    assert_eq!(session.remaining_percent, Some(21));
}

/// Display-derived rows have no recoverable canonical account identity.
#[test]
fn schema_migration_invalidates_display_derived_cache() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let path = db.to_str().expect("utf8 path").to_owned();
    block_on_store(async move {
        let conn = connect_local(&path).await?;
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta VALUES ('schema_version', '4');
             CREATE TABLE account_usage_snapshots (
                 account_key_hash TEXT NOT NULL
             );
             INSERT INTO account_usage_snapshots VALUES ('sha256:legacy');",
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok::<(), String>(())
    })
    .expect("seed v4 cache");
    store_usage_snapshot(&db, &usage_view()).expect("store after migration");
    let rows = stored_account_snapshots(&db).expect("read migrated rows");
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|row| row.account_key_hash != "sha256:legacy")
    );
    assert!(
        rows.iter()
            .all(|row| Some(&row.account_identity) == usage_view().account_identity.as_ref())
    );
    assert_eq!(
        schema_version(&db).expect("schema version"),
        Some("7".to_owned())
    );
}

#[test]
fn focused_usage_view_rebuilds_snapshot_from_account_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    let view = focused_usage_view(&db, Some("codex"), Some("Codex"), 1_781_185_590)
        .expect("read focused usage")
        .expect("stored usage view");

    assert_eq!(view.focused_agent.as_deref(), Some("codex"));
    assert_eq!(view.focused_provider.as_deref(), Some("OpenAI"));
    assert_eq!(view.account.provider_label, "Codex");
    assert_eq!(view.account.account_label, "alexey@example.com");
    assert_eq!(view.account.plan_label.as_deref(), Some("Pro 20x"));
    assert_eq!(view.buckets.len(), 2);
    assert_eq!(view.buckets[0].label, "Session");
    assert_eq!(view.buckets[0].remaining_percent, Some(37));
    assert_eq!(view.buckets[1].label, "Credits");
    // Restored buckets carry no status-bar slot: the headline is persisted as
    // `status_bar_label` and read directly, never recomputed from the restored
    // (untagged) buckets. Locks that contract so a future change recomputing
    // the headline from buckets — which would blank every cached headline —
    // fails loudly here.
    assert!(
        view.buckets
            .iter()
            .all(|bucket| bucket.status_slot.is_none())
    );
    assert_eq!(view.updated_label, "Updated now");
    assert_eq!(view.status_bar_label, "Codex Session: 63% used · 37% left");
}

#[test]
fn focused_usage_view_ticks_relative_updated_label_from_fetch_time() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    let view = focused_usage_view(&db, Some("codex"), Some("Codex"), 1_781_185_680)
        .expect("read focused usage")
        .expect("stored usage view");

    assert_eq!(view.updated_label, "Updated 2m ago");
}

#[test]
fn focused_usage_view_resolves_provider_from_agent_when_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let now = 1_781_185_680;
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Codex",
            "codex@example.com",
            Some("Pro 20x"),
            "Session",
            37,
            now,
        ),
    )
    .expect("store codex snapshot");
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Amp",
            "amp@example.com",
            Some("Amp Free"),
            "Amp Free",
            9,
            now,
        ),
    )
    .expect("store amp snapshot");

    let view = focused_usage_view(&db, Some("amp"), None, now)
        .expect("read focused usage")
        .expect("stored provider usage");

    assert_eq!(view.focused_agent.as_deref(), Some("amp"));
    assert_eq!(view.focused_provider.as_deref(), Some("Amp"));
    assert_eq!(view.account.provider_label, "Amp");
    assert_eq!(view.account.account_label, "amp@example.com");
    assert_eq!(view.buckets[0].label, "Amp Free");
    assert_eq!(view.buckets[0].remaining_percent, Some(9));
}

#[test]
fn focused_usage_view_without_resolved_provider_does_not_match_all() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    let view = focused_usage_view(&db, Some("unknown-agent"), None, 1_781_185_680)
        .expect("read focused usage");

    assert!(view.is_none());
}

#[test]
fn focused_usage_view_sorts_provider_buckets_canonically() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let now = 1_781_185_680;
    let mut view = provider_usage_view(
        "GLM / Z.AI",
        "zai@example.com",
        Some("Coding Pro"),
        "5-hour",
        100,
        now,
    );
    let base_bucket = view.buckets[0].clone();
    view.buckets.extend([
        QuotaBucketView {
            count_quota: None,
            used_money: None,
            limit_money: None,
            remaining_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "MCP".to_owned(),
            remaining_percent: Some(100),
            pace_label: Some("0 / 100 (100 remaining)".to_owned()),
            ..base_bucket.clone()
        },
        QuotaBucketView {
            count_quota: None,
            used_money: None,
            limit_money: None,
            remaining_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Tokens".to_owned(),
            remaining_percent: Some(99),
            ..base_bucket
        },
    ]);
    store_usage_snapshot(&db, &view).expect("store snapshot");

    let view = focused_usage_view(&db, Some("codex"), Some("Z.AI"), now)
        .expect("read focused usage")
        .expect("stored provider usage");

    assert_eq!(
        view.buckets
            .iter()
            .map(|bucket| bucket.label.as_str())
            .collect::<Vec<_>>(),
        vec!["5-hour", "Tokens", "MCP"]
    );
}

#[test]
fn all_provider_snapshots_round_trip_from_turso_to_usage_overlay_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let now = 1_781_185_680;
    let providers = [
        (
            "Codex",
            "OpenAI",
            "codex@example.com",
            Some("Pro 20x"),
            "Session",
            37,
        ),
        (
            "Claude",
            "Anthropic",
            "claude@example.com",
            Some("Max"),
            "Weekly",
            42,
        ),
        (
            "Amp",
            "Amp",
            "amp@example.com",
            Some("Amp Free"),
            "Amp Free",
            55,
        ),
        ("Grok Build", "xAI", "local Grok auth", None, "Credits", 61),
        (
            "GLM / Z.AI",
            "Z.AI",
            "zai@example.com",
            Some("GLM Coding"),
            "Tokens",
            72,
        ),
        (
            "Kimi",
            "Kimi",
            "kimi@example.com",
            Some("K2"),
            "5-hour rate limit",
            83,
        ),
        (
            "MiniMax",
            "MiniMax",
            "minimax@example.com",
            Some("MiniMax Pro"),
            "MiniMax Text Coding plan",
            94,
        ),
    ];

    for (provider, _tab_label, account, plan, bucket, remaining) in providers {
        store_usage_snapshot(
            &db,
            &provider_usage_view(provider, account, plan, bucket, remaining, now - 120),
        )
        .expect("store provider snapshot");
    }

    for (provider, tab_label, account, plan, bucket, remaining) in providers {
        let view = focused_usage_view(&db, Some("codex"), Some(tab_label), now)
            .expect("read focused usage")
            .expect("stored provider usage");
        assert_eq!(view.account.provider_label, provider);
        assert_eq!(view.account.account_label, account);
        assert_eq!(view.account.plan_label.as_deref(), plan);
        assert_eq!(view.buckets.len(), 1);
        assert_eq!(view.buckets[0].label, bucket);
        assert_eq!(view.buckets[0].remaining_percent, Some(remaining));
        assert_eq!(view.updated_label, "Updated 2m ago");
        assert_eq!(view.tabs.len(), 7);
        // One tab per stored account, keyed by the stable account id, sorted
        // by display label.
        let mut ids: Vec<&str> = view.tabs.iter().map(|tab| tab.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 7);
        assert_eq!(
            view.tabs
                .iter()
                .map(|tab| tab.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Amp · amp@example.com",
                "Claude · claude@example.com",
                "Codex · codex@example.com",
                "GLM / Z.AI · zai@example.com",
                "Grok Build · local Grok auth",
                "Kimi · kimi@example.com",
                "MiniMax · minimax@example.com",
            ]
        );
        let tab = view
            .tabs
            .iter()
            .find(|tab| tab.account_label == account)
            .expect("account tab");
        assert_eq!(
            tab.id,
            crate::usage::usage_account_tab_id(&provider_identity(provider, account))
        );

        // Dialog rendering assertion removed: Dialog type lives in jackin-capsule
        // and would create a circular dep (Blocker 2 Option A). The
        // bucket-row assertion that followed referenced `rows` from the
        // removed Dialog::new_usage(view).usage_state() expression; that
        // expression is now gone, so the rows variable is unreachable.
        let _unused = (view, account, plan, bucket, remaining, tab_label);
    }
}

#[test]
fn same_provider_accounts_keep_distinct_store_tabs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let now = 1_781_185_680;
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Claude",
            "a@example.com",
            Some("Max"),
            "Session",
            40,
            now - 120,
        ),
    )
    .expect("store claude-a snapshot");
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Claude",
            "b@example.com",
            Some("Max 20x"),
            "Session",
            60,
            now - 60,
        ),
    )
    .expect("store claude-b snapshot");
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Codex",
            "codex@example.com",
            Some("Pro 20x"),
            "Session",
            37,
            now - 30,
        ),
    )
    .expect("store codex snapshot");

    let view = focused_usage_view(&db, Some("codex"), Some("Anthropic"), now)
        .expect("read focused usage")
        .expect("stored provider usage");

    assert_eq!(view.tabs.len(), 3);
    let claude: Vec<_> = view
        .tabs
        .iter()
        .filter(|tab| tab.account_label == "a@example.com" || tab.account_label == "b@example.com")
        .collect();
    assert_eq!(claude.len(), 2);
    assert_ne!(claude[0].id, claude[1].id);
    assert_ne!(claude[0].label, claude[1].label);
    assert_eq!(
        claude[0].id,
        crate::usage::usage_account_tab_id(&provider_identity("Claude", &claude[0].account_label))
    );
    assert_eq!(
        claude[1].id,
        crate::usage::usage_account_tab_id(&provider_identity("Claude", &claude[1].account_label))
    );
}

#[test]
fn usage_snapshot_store_records_schema_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");

    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    assert_eq!(
        schema_version(&db).expect("schema version").as_deref(),
        Some("8")
    );
}

#[test]
fn canonical_accounts_with_same_display_label_remain_distinct() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let mut first = provider_usage_view("Codex", "shared@example.com", None, "Session", 20, 100);
    first
        .account_identity
        .as_mut()
        .expect("identity")
        .account_id = "account-a".to_owned();
    let mut second = first.clone();
    second
        .account_identity
        .as_mut()
        .expect("identity")
        .account_id = "account-b".to_owned();
    second.buckets[0].remaining_percent = Some(80);
    store_usage_snapshots(&db, &[first.clone(), second.clone()]).expect("store both accounts");
    let rows = stored_account_snapshots(&db).expect("read persisted accounts");
    let tabs = usage_provider_tabs_from_rows(&rows);
    assert_eq!(tabs.len(), 2);
    assert_eq!(tabs[0].label, tabs[1].label);
    assert_ne!(tabs[0].id, tabs[1].id);
    assert_eq!(
        list_account_identities(&db)
            .expect("list canonical accounts")
            .len(),
        2
    );
    let stored = load_all_account_usage_views(&db, 100).expect("load accounts");
    assert_eq!(stored.len(), 2);
    assert_ne!(stored[0].account_key_hash, stored[1].account_key_hash);
    for expected in [&first, &second] {
        let actual = stored
            .iter()
            .find(|row| row.view.account_identity == expected.account_identity)
            .expect("canonical account survived");
        assert_eq!(
            actual.view.buckets[0].remaining_percent,
            expected.buckets[0].remaining_percent
        );
        assert_eq!(actual.view.account.account_label, "shared@example.com");
        assert_eq!(
            actual.account_key_hash,
            crate::usage::usage_account_tab_id(
                expected.account_identity.as_ref().expect("identity")
            )
        );
    }
    let original_key =
        crate::usage::usage_account_tab_id(first.account_identity.as_ref().expect("identity"));
    first.account.account_label = "renamed@example.com".to_owned();
    first.account.provider_label = "OpenAI".to_owned();
    first.fetched_at_epoch += 1;
    store_usage_snapshot(&db, &first).expect("store renamed account");
    let stored = load_all_account_usage_views(&db, 101).expect("reload accounts");
    assert_eq!(stored.len(), 2);
    let renamed = stored
        .iter()
        .find(|row| row.account_key_hash == original_key)
        .expect("stable account key");
    assert_eq!(renamed.view.account.account_label, "renamed@example.com");
    assert_eq!(renamed.view.account.provider_label, "OpenAI");
}

#[test]
fn canonical_account_surfaces_keep_independent_usage_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let first = usage_view();
    let mut second = first.clone();
    second
        .account_identity
        .as_mut()
        .expect("identity")
        .surface_id = "surface-other".to_owned();
    second.buckets[0].remaining_percent = Some(90);
    store_usage_snapshots(&db, &[first.clone(), second.clone()]).expect("store both surfaces");
    let stored = load_all_account_usage_views(&db, 100).expect("load surfaces");
    assert_eq!(stored.len(), 2);
    for expected in [&first, &second] {
        let actual = stored
            .iter()
            .find(|row| row.view.account_identity == expected.account_identity)
            .expect("surface survived");
        let session = actual
            .view
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Session")
            .expect("session");
        assert_eq!(
            session.remaining_percent,
            expected.buckets[0].remaining_percent
        );
    }
}

#[test]
fn unbound_usage_views_do_not_create_display_derived_accounts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let mut view = usage_view();
    view.account_identity = None;
    store_usage_snapshot(&db, &view).expect("skip unbound view");
    assert!(
        load_all_account_usage_views(&db, 100)
            .expect("load cache")
            .is_empty()
    );
}

#[test]
fn shared_host_rows_preserve_snapshot_fields_and_canonical_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("accounts.db");
    let expected = AccountUsageSnapshotView {
        canonical_identity: None,
        account_identity: usage_view().account_identity,
        count_quota: None,
        used_money: None,
        limit_money: None,
        remaining_money: None,
        provider: "Codex".to_owned(),
        account_label: "shared@example.com".to_owned(),
        source: "provider_api".to_owned(),
        confidence: "authoritative".to_owned(),
        window_kind: "Session".to_owned(),
        used_amount: Some(23),
        used_unit: Some("percent".to_owned()),
        limit_amount: Some(100),
        limit_unit: Some("percent".to_owned()),
        resets_at: Some(200),
        fetched_at: 100,
        expires_at: Some(150),
        status: "fresh".to_owned(),
        last_error: Some("redacted diagnostic".to_owned()),
    };
    store_account_usage_snapshots(&db, std::slice::from_ref(&expected)).expect("store host row");
    assert_eq!(
        read_account_usage_snapshots(&db).expect("read host row"),
        vec![expected.clone()]
    );
    let views = load_all_account_usage_views(&db, 100).expect("read through shared focused cache");
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].view.account_identity, expected.account_identity);
    assert_eq!(views[0].view.buckets[0].label, expected.window_kind);
    assert_eq!(
        views[0].view.buckets[0].used_label.as_deref(),
        Some("23% used")
    );
    assert_eq!(
        views[0].view.buckets[0].limit_label.as_deref(),
        Some("100%")
    );
    assert_eq!(views[0].view.buckets[0].remaining_percent, Some(77));
    assert_eq!(views[0].view.updated_label, "Updated now");
    assert_eq!(views[0].view.status_bar_label, "usage cached");
    assert_eq!(views[0].view.last_error, expected.last_error);
}

#[test]
fn shared_host_read_does_not_create_missing_cache() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("missing").join("accounts.db");
    assert!(
        read_account_usage_snapshots(&db)
            .expect("read missing cache")
            .is_empty()
    );
    assert!(!db.parent().expect("parent").exists());
}

#[test]
fn shared_host_remaining_percentage_requires_valid_matching_quantities() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("accounts.db");
    let mut account = AccountUsageSnapshotView {
        canonical_identity: None,
        account_identity: usage_view().account_identity,
        count_quota: None,
        used_money: None,
        limit_money: None,
        remaining_money: None,
        provider: "Codex".to_owned(),
        account_label: "shared".to_owned(),
        source: "provider_api".to_owned(),
        confidence: "authoritative".to_owned(),
        window_kind: "Session".to_owned(),
        used_amount: Some(0),
        used_unit: Some("tokens".to_owned()),
        limit_amount: Some(100),
        limit_unit: Some("tokens".to_owned()),
        resets_at: None,
        fetched_at: 100,
        expires_at: None,
        status: "fresh".to_owned(),
        last_error: None,
    };
    for (used, limit, used_unit, limit_unit, expected) in [
        (
            Some(23),
            Some(100),
            Some("tokens"),
            Some("tokens"),
            Some(77),
        ),
        (Some(23), Some(0), Some("tokens"), Some("tokens"), None),
        (Some(23), Some(-100), Some("tokens"), Some("tokens"), None),
        (Some(-1), Some(100), Some("tokens"), Some("tokens"), None),
        (Some(23), Some(100), Some("tokens"), Some("requests"), None),
        (Some(23), Some(100), None, None, None),
        (Some(23), Some(100), Some(""), Some(""), None),
        (None, Some(100), Some("tokens"), Some("tokens"), None),
        (Some(23), None, Some("tokens"), Some("tokens"), None),
        (
            Some(i64::MAX),
            Some(i64::MAX),
            Some("tokens"),
            Some("tokens"),
            Some(0),
        ),
        (
            Some(i64::MAX),
            Some(1),
            Some("tokens"),
            Some("tokens"),
            Some(0),
        ),
        (
            Some(0),
            Some(i64::MAX),
            Some("tokens"),
            Some("tokens"),
            Some(100),
        ),
    ] {
        account.used_amount = used;
        account.limit_amount = limit;
        account.used_unit = used_unit.map(str::to_owned);
        account.limit_unit = limit_unit.map(str::to_owned);
        store_account_usage_snapshots(&db, std::slice::from_ref(&account))
            .expect("store quantities");
        let view = load_all_account_usage_views(&db, 100)
            .expect("load quantities")
            .remove(0)
            .view;
        assert_eq!(
            view.buckets[0].remaining_percent, expected,
            "{used:?}/{limit:?} {used_unit:?}/{limit_unit:?}"
        );
        assert_eq!(
            read_account_usage_snapshots(&db).expect("raw roundtrip"),
            vec![account.clone()]
        );
    }
}

fn request_count(
    used: Option<u64>,
    limit: Option<u64>,
    remaining: Option<u64>,
    period: jackin_protocol::control::CountQuotaPeriod,
) -> CountQuota {
    CountQuota {
        used,
        limit,
        remaining,
        unit: jackin_protocol::control::CountQuotaUnit::Requests,
        period,
        provenance: jackin_protocol::control::CountQuotaProvenance::ProviderReported,
    }
}

#[test]
fn typed_count_snapshots_preserve_literal_u64_values_and_stale_status() {
    use jackin_protocol::control::CountQuotaPeriod::{Unknown, UtcDaily};
    for (count, expected_used, expected_limit) in [
        (
            request_count(Some(0), Some(0), Some(0), UtcDaily),
            Some(0),
            Some(0),
        ),
        (request_count(None, None, None, Unknown), None, None),
        (
            request_count(Some(999), Some(1000), Some(1), UtcDaily),
            Some(999),
            Some(1000),
        ),
        (request_count(Some(17), None, None, Unknown), Some(17), None),
        (
            request_count(Some(u64::MAX), Some(u64::MAX), Some(u64::MAX), Unknown),
            None,
            None,
        ),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("counts.db");
        let mut view = provider_usage_view("OpenRouter", "count-account", None, "Requests", 0, 100);
        view.status = UsageSnapshotStatus::Stale;
        view.buckets[0].status = UsageSnapshotStatus::Stale;
        view.buckets[0].count_quota = Some(count.clone());
        // Conflicting labels and coarse geometry must never become raw amounts.
        view.buckets[0].used_label = Some("9999 imaginary dollars used".to_owned());
        view.buckets[0].limit_label = Some("9999 imaginary dollars".to_owned());
        let cached = HashMap::from([(
            "counts".to_owned(),
            crate::usage::CachedUsage { view: view.clone() },
        )]);
        let projected = crate::usage::account_snapshot_views_from_cache(&cached);
        assert_eq!(projected[0].count_quota, Some(count.clone()));
        assert_eq!(projected[0].used_amount, expected_used);
        assert_eq!(projected[0].limit_amount, expected_limit);
        assert_eq!(projected[0].account_identity, view.account_identity);
        assert_eq!(projected[0].status, "stale");
        store_usage_snapshot(&db, &view).expect("store count snapshot");
        let accounts = read_account_usage_snapshots(&db).expect("read count accounts");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].count_quota, Some(count.clone()));
        assert_eq!(accounts[0].account_identity, view.account_identity);
        assert_eq!(accounts[0].status, "stale");
        assert_eq!(accounts[0].used_amount, expected_used);
        assert_eq!(accounts[0].limit_amount, expected_limit);
        assert_eq!(
            accounts[0].used_unit.as_deref(),
            expected_used.map(|_| "requests")
        );
        assert_eq!(
            accounts[0].limit_unit.as_deref(),
            expected_limit.map(|_| "requests")
        );
        let restored = load_all_account_usage_views(&db, 200).expect("restore count buckets");
        assert_eq!(restored[0].view.buckets[0].count_quota, Some(count.clone()));
        assert_eq!(
            restored[0].view.buckets[0].remaining_percent,
            count.remaining_percent()
        );
        assert_eq!(restored[0].view.status, UsageSnapshotStatus::Stale);
        assert!(restored[0].view.buckets[0].used_money.is_none());
        assert!(restored[0].view.buckets[0].limit_money.is_none());
        store_account_usage_snapshots(&db, &accounts).expect("store host count accounts");
        assert_eq!(
            read_account_usage_snapshots(&db).expect("reload host counts"),
            accounts
        );
        let restored = load_all_account_usage_views(&db, 200).expect("restore host counts");
        assert_eq!(restored[0].view.buckets[0].count_quota, Some(count.clone()));
        assert_eq!(
            restored[0].view.buckets[0].remaining_percent,
            count.remaining_percent()
        );
    }
}

#[test]
fn canonical_v5_cache_migrates_on_write_without_losing_last_good_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("v5.db");
    let original = usage_view();
    create_uncached_canonical_fixture(&db, &original);
    let db_string = path_to_turso(&db).expect("database path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute_batch(
            "ALTER TABLE account_usage_snapshots DROP COLUMN source_revision; ALTER TABLE account_usage_snapshots DROP COLUMN monetary_quota_json; ALTER TABLE account_usage_snapshots DROP COLUMN canonical_identity_json;
             ALTER TABLE account_usage_snapshots DROP COLUMN count_quota_json;
            UPDATE _meta SET value = '5' WHERE key = 'schema_version';
            BEGIN;
            ALTER TABLE account_usage_snapshots ADD COLUMN count_quota_json TEXT;
            ROLLBACK;",
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("prepare v5 fixture");
    assert!(
        read_account_usage_snapshots(&db)
            .expect_err("explicit old-cache status")
            .contains("requires explicit refresh migration")
    );
    assert_eq!(
        schema_version(&db).expect("unchanged version").as_deref(),
        Some("5")
    );
    store_usage_snapshots(&db, &[]).expect("explicit write migrates v5");
    let accounts = read_account_usage_snapshots(&db).expect("retained canonical rows");
    assert_eq!(accounts.len(), 2);
    let session = accounts
        .iter()
        .find(|account| account.window_kind == "Session")
        .expect("session");
    assert_eq!(session.account_identity, original.account_identity);
    assert_eq!(session.used_amount, Some(63));
    assert_eq!(session.limit_amount, Some(100));
    assert_eq!(session.used_unit.as_deref(), Some("percent"));
    assert_eq!(session.count_quota, None);
    assert_eq!(session.canonical_identity, None);
    assert_eq!(
        fixture_meta(&db, "canonical_identity_migration_status").as_deref(),
        Some("refresh_required")
    );
    assert_eq!(
        schema_version(&db).expect("migrated version").as_deref(),
        Some("8")
    );
}

#[test]
fn canonical_v5_cache_with_staged_count_column_finishes_migration() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("interrupted-v5.db");
    let original = usage_view();
    create_uncached_canonical_fixture(&db, &original);
    let db_string = path_to_turso(&db).expect("database path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute(
            "UPDATE _meta SET value = '5' WHERE key = 'schema_version'",
            (),
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("stage existing column with old metadata");
    store_usage_snapshots(&db, &[]).expect("finish staged count migration");
    let accounts = read_account_usage_snapshots(&db).expect("read preserved accounts");
    assert_eq!(accounts.len(), 2);
    assert!(
        accounts
            .iter()
            .all(|account| account.account_identity == original.account_identity)
    );
    assert_eq!(
        schema_version(&db).expect("committed version").as_deref(),
        Some("8")
    );
}

fn create_uncached_canonical_fixture(db: &Path, original: &FocusedUsageView) {
    let db_string = path_to_turso(db).expect("database path");
    let rows = account_snapshot_rows(original);
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        initialize_schema(&conn).await?;
        upsert_account_snapshot_rows(&conn, rows).await
    })
    .expect("create uncached canonical fixture");
}

#[test]
fn logical_evidence_roundtrips_without_promoting_display_labels() {
    use jackin_protocol::control::UsageCanonicalAccountSubject::{
        ProviderId, ProviderStableHandle, SourceCapability,
    };
    for subject in [
        ProviderId("provider-id".to_owned()),
        ProviderStableHandle("authenticated-handle".to_owned()),
        SourceCapability("opaque-source".to_owned()),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("logical.db");
        let mut view = usage_view();
        view.canonical_identity = Some(UsageCanonicalAccountIdentity {
            surface_id: view
                .account_identity
                .as_ref()
                .expect("route")
                .surface_id
                .clone(),
            subject,
        });
        store_usage_snapshot(&db, &view).expect("store accepted logical evidence");
        let accounts = read_account_usage_snapshots(&db).expect("host proof");
        assert!(
            accounts
                .iter()
                .all(|account| account.canonical_identity == view.canonical_identity)
        );
        store_account_usage_snapshots(&db, &accounts).expect("host proof roundtrip");
        let restored = load_all_account_usage_views(&db, 100).expect("focused proof");
        assert_eq!(restored[0].view.canonical_identity, view.canonical_identity);
        assert_eq!(restored[0].view.account_identity, view.account_identity);
        assert_eq!(
            list_account_identities(&db).expect("identity inventory")[0].canonical_identity,
            view.canonical_identity
        );
        view.canonical_identity = None;
        view.fetched_at_epoch += 1;
        view.account.account_label = "provider-id".to_owned();
        store_usage_snapshot(&db, &view).expect("explicit unknown proof");
        assert!(
            load_all_account_usage_views(&db, 100).expect("unknown proof")[0]
                .view
                .canonical_identity
                .is_none()
        );
    }
}

#[test]
fn canonical_v6_cache_migration_preserves_counts_routes_and_raw_quantities() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("v6.db");
    let mut original = usage_view();
    original.buckets[0].count_quota = Some(request_count(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(1),
        jackin_protocol::control::CountQuotaPeriod::UtcDaily,
    ));
    create_uncached_canonical_fixture(&db, &original);
    let db_string = path_to_turso(&db).expect("database path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute_batch("ALTER TABLE account_usage_snapshots DROP COLUMN source_revision; ALTER TABLE account_usage_snapshots DROP COLUMN monetary_quota_json; ALTER TABLE account_usage_snapshots DROP COLUMN canonical_identity_json; UPDATE _meta SET value = '6' WHERE key = 'schema_version';")
            .await.map_err(|err| err.to_string())?;
        Ok(())
    }).expect("seed genuine v6 shape");
    assert!(
        read_account_usage_snapshots(&db)
            .expect_err("explicit obsolete status")
            .contains("requires explicit refresh migration")
    );
    assert_eq!(
        schema_version(&db).expect("unchanged version").as_deref(),
        Some("6")
    );
    store_usage_snapshots(&db, &[]).expect("upgrade v6 on explicit write");
    let accounts = read_account_usage_snapshots(&db).expect("preserved rows");
    assert_eq!(accounts.len(), 2);
    let session = accounts
        .iter()
        .find(|account| account.window_kind == "Session")
        .expect("session");
    assert_eq!(session.account_identity, original.account_identity);
    assert_eq!(session.count_quota, original.buckets[0].count_quota);
    assert_eq!(session.canonical_identity, None);
    assert_eq!(
        fixture_meta(&db, "canonical_identity_migration_status").as_deref(),
        Some("refresh_required")
    );
    assert_eq!(session.used_amount, None);
    assert_eq!(session.limit_amount, None);
    let restored = load_all_account_usage_views(&db, 100).expect("restored rows");
    assert_eq!(restored[0].view.canonical_identity, None);
    assert_eq!(restored[0].view.account_identity, original.account_identity);
    assert_eq!(
        restored[0]
            .view
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Session")
            .expect("count bucket")
            .count_quota,
        original.buckets[0].count_quota
    );
    assert_eq!(
        schema_version(&db).expect("current version").as_deref(),
        Some("8")
    );
}

#[test]
fn invalid_logical_evidence_is_rejected_without_overwriting_last_good_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("invalid-proof.db");
    let original = usage_view();
    store_usage_snapshot(&db, &original).expect("last good");
    for proof in [
        UsageCanonicalAccountIdentity {
            surface_id: "other-surface".to_owned(),
            subject: UsageCanonicalAccountSubject::ProviderId("valid-subject".to_owned()),
        },
        UsageCanonicalAccountIdentity {
            surface_id: original
                .account_identity
                .as_ref()
                .expect("route")
                .surface_id
                .clone(),
            subject: UsageCanonicalAccountSubject::ProviderStableHandle(" ".to_owned()),
        },
    ] {
        let mut invalid = original.clone();
        invalid.canonical_identity = Some(proof);
        invalid.buckets[0].remaining_percent = Some(0);
        assert!(
            store_usage_snapshot(&db, &invalid)
                .expect_err("invalid evidence")
                .contains("invalid canonical account evidence")
        );
        let restored = load_all_account_usage_views(&db, 100).expect("last good retained");
        assert_eq!(restored[0].view.canonical_identity, None);
        assert_eq!(
            restored[0]
                .view
                .buckets
                .iter()
                .find(|bucket| bucket.label == "Session")
                .expect("session")
                .remaining_percent,
            original.buckets[0].remaining_percent
        );
    }
}

#[test]
fn malformed_stored_logical_evidence_is_explicit_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("malformed-proof.db");
    create_uncached_canonical_fixture(&db, &usage_view());
    let db_string = path_to_turso(&db).expect("database path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute(
            "UPDATE account_usage_snapshots SET canonical_identity_json = '{bad json'",
            (),
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("corrupt private proof fixture");
    assert!(
        read_account_usage_snapshots(&db)
            .expect_err("explicit corrupt proof")
            .contains("decode canonical account evidence failed")
    );
    assert!(
        load_all_account_usage_views(&db, 100)
            .expect_err("reject corrupt focused proof")
            .contains("decode canonical account evidence failed")
    );
}

fn fixture_meta(db: &Path, key: &str) -> Option<String> {
    let db_string = path_to_turso(db).expect("fixture path");
    let key = key.to_owned();
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        let mut rows = conn
            .query("SELECT value FROM _meta WHERE key = ?1", [key])
            .await
            .map_err(|err| err.to_string())?;
        rows.next()
            .await
            .map_err(|err| err.to_string())?
            .map(|row| row_string(&row, 0, "metadata"))
            .transpose()
    })
    .expect("fixture metadata")
}

#[test]
fn canonical_schema_migration_rolls_back_rows_columns_and_status_together() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("migration-rollback.db");
    create_uncached_canonical_fixture(&db, &usage_view());
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute_batch(
            "ALTER TABLE account_usage_snapshots DROP COLUMN source_revision; ALTER TABLE account_usage_snapshots DROP COLUMN monetary_quota_json; ALTER TABLE account_usage_snapshots DROP COLUMN canonical_identity_json;
             ALTER TABLE account_usage_snapshots DROP COLUMN count_quota_json;
             UPDATE _meta SET value = '5' WHERE key = 'schema_version';
             CREATE TRIGGER reject_migration BEFORE UPDATE ON _meta WHEN NEW.key = 'schema_version' AND NEW.value = '8'
             BEGIN SELECT RAISE(ABORT, 'fixture migration blocked'); END;"
        ).await.map_err(|err| err.to_string())?;
        Ok(())
    }).expect("seed rollback fixture");
    assert!(store_usage_snapshots(&db, &[]).is_err());
    assert_eq!(fixture_meta(&db, "schema_version").as_deref(), Some("5"));
    assert_eq!(
        fixture_meta(&db, "canonical_identity_migration_status"),
        None
    );
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        let mut columns = conn
            .query("PRAGMA table_info(account_usage_snapshots)", ())
            .await
            .map_err(|err| err.to_string())?;
        while let Some(row) = columns.next().await.map_err(|err| err.to_string())? {
            let name = row_string(&row, 1, "column_name")?;
            assert_ne!(name, "canonical_identity_json");
            assert_ne!(name, "count_quota_json");
            assert_ne!(name, "monetary_quota_json");
        }
        drop(columns);
        let mut rows = conn
            .query("SELECT COUNT(*) FROM account_usage_snapshots", ())
            .await
            .map_err(|err| err.to_string())?;
        assert_eq!(
            row_i64(
                &rows
                    .next()
                    .await
                    .map_err(|err| err.to_string())?
                    .expect("count"),
                0,
                "rows"
            )?,
            2
        );
        Ok(())
    })
    .expect("rows and schema retained");
}

#[test]
fn unversioned_old_cache_reads_report_migration_without_hiding_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("unversioned.db");
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute_batch("CREATE TABLE account_usage_snapshots (account_label TEXT NOT NULL); INSERT INTO account_usage_snapshots VALUES ('legacy');")
            .await.map_err(|err| err.to_string())?;
        Ok(())
    }).expect("unversioned fixture");
    assert!(
        read_account_usage_snapshots(&db)
            .expect_err("migration status")
            .contains("requires explicit refresh migration")
    );
    store_usage_snapshots(&db, &[]).expect("explicit cache regeneration");
    assert_eq!(
        fixture_meta(&db, "usage_snapshot_migration_status").as_deref(),
        Some("display_derived_cache_invalidated")
    );
}

#[test]
fn failed_snapshot_commit_rolls_back_and_next_owned_operation_recovers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("commit-failure.db");
    let original = usage_view();
    store_usage_snapshot(&db, &original).expect("initial snapshot");
    let fixture_path = db.clone();
    block_on_store(async move {
        let conn = open_store(&fixture_path).await?;
        conn.execute_batch(
            "CREATE TABLE fixture_commit_parent (id INTEGER PRIMARY KEY);
             CREATE TABLE fixture_commit_child (
                 id INTEGER REFERENCES fixture_commit_parent(id) DEFERRABLE INITIALLY DEFERRED
             );
             CREATE TRIGGER fixture_commit_failure AFTER UPDATE ON account_usage_snapshots
             BEGIN INSERT INTO fixture_commit_child VALUES (1); END;",
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("deferred constraint fixture");
    let mut changed = original.clone();
    changed.fetched_at_epoch += 1;
    changed.buckets[0].remaining_percent = Some(12);
    assert!(
        store_usage_snapshot(&db, &changed)
            .expect_err("commit fails")
            .contains("commit telemetry snapshot transaction failed")
    );
    let fixture_path = db.clone();
    block_on_store(async move {
        let conn = open_store(&fixture_path).await?;
        let mut rows = conn
            .query("SELECT COUNT(*) FROM fixture_commit_child", ())
            .await
            .map_err(|err| err.to_string())?;
        assert_eq!(
            row_i64(
                &rows
                    .next()
                    .await
                    .map_err(|err| err.to_string())?
                    .expect("count"),
                0,
                "child_count"
            )?,
            0
        );
        drop(rows);
        conn.execute("DROP TRIGGER fixture_commit_failure", ())
            .await
            .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("failed transaction rolled back");
    let restored = load_all_account_usage_views(&db, 100).expect("last good snapshot");
    assert_eq!(restored[0].view.fetched_at_epoch, original.fetched_at_epoch);
    let builds_before_recovery =
        connection_build_count(&db).expect("completed operation connections");
    store_usage_snapshot(&db, &changed).expect("fresh owned operation after failed commit");
    assert_eq!(
        connection_build_count(&db).expect("new owned connection"),
        builds_before_recovery + 1
    );
    let restored = load_all_account_usage_views(&db, 100).expect("updated snapshot");
    assert_eq!(restored[0].view.fetched_at_epoch, changed.fetched_at_epoch);
}

#[test]
fn exact_monetary_snapshot_roundtrip_preserves_scale_and_signed_remaining() {
    for (used, limit, remaining, expected_percent) in [
        (
            Some(Money::new(1, "USD", 3)),
            Some(Money::new(100, "USD", 2)),
            Some(Money::new(999, "USD", 3)),
            Some(99),
        ),
        (
            Some(Money::new(0, "USD", 3)),
            Some(Money::new(0, "USD", 3)),
            Some(Money::new(0, "USD", 3)),
            None,
        ),
        (None, None, Some(Money::new(-1, "USD", 3)), None),
        (None, None, Some(Money::new(1, "credits", 7)), None),
        (
            Some(Money::new(i64::MAX, "USD", 0)),
            None,
            Some(Money::new(i64::MIN, "USD", 0)),
            None,
        ),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("money.db");
        let mut view = usage_view();
        view.buckets.truncate(1);
        let bucket = &mut view.buckets[0];
        bucket.count_quota = None;
        bucket.used_money = used.clone();
        bucket.limit_money = limit.clone();
        bucket.remaining_money = remaining.clone();
        bucket.used_label = Some("wrong fabricated $9999".to_owned());
        bucket.limit_label = Some("wrong fabricated $9999".to_owned());
        bucket.remaining_percent = None;
        bucket.status = UsageSnapshotStatus::Stale;
        view.status = UsageSnapshotStatus::Stale;
        let cached = HashMap::from([(
            "money".to_owned(),
            crate::usage::CachedUsage { view: view.clone() },
        )]);
        let projected = crate::usage::account_snapshot_views_from_cache(&cached);
        assert_eq!(projected[0].used_money, used);
        assert_eq!(projected[0].limit_money, limit);
        assert_eq!(projected[0].remaining_money, remaining);
        assert_eq!(projected[0].used_amount, None);
        assert_eq!(projected[0].used_unit, None);
        assert_eq!(projected[0].limit_amount, None);
        assert_eq!(projected[0].limit_unit, None);
        store_usage_snapshot(&db, &view).expect("persist exact money");
        let accounts = read_account_usage_snapshots(&db).expect("read typed account amounts");
        assert_eq!(accounts[0].used_money, used);
        assert_eq!(accounts[0].limit_money, limit);
        assert_eq!(accounts[0].remaining_money, remaining);
        assert_eq!(accounts[0].used_amount, None);
        assert_eq!(accounts[0].used_unit, None);
        assert_eq!(accounts[0].limit_amount, None);
        assert_eq!(accounts[0].limit_unit, None);
        let private = stored_account_snapshots(&db).expect("typed-only monetary SQL rows");
        assert_eq!(private[0].used_amount, None);
        assert_eq!(private[0].used_unit, None);
        assert_eq!(private[0].limit_amount, None);
        assert_eq!(private[0].limit_unit, None);
        assert_eq!(accounts[0].status, "stale");
        assert_eq!(accounts[0].account_identity, view.account_identity);
        let restored = load_all_account_usage_views(&db, 200).expect("restore typed bucket");
        assert_eq!(restored[0].view.buckets[0].used_money, used);
        assert_eq!(restored[0].view.buckets[0].limit_money, limit);
        assert_eq!(restored[0].view.buckets[0].remaining_money, remaining);
        assert_eq!(
            restored[0].view.buckets[0].remaining_percent,
            expected_percent
        );
        store_account_usage_snapshots(&db, &accounts).expect("host account roundtrip");
        assert_eq!(
            read_account_usage_snapshots(&db).expect("reread host rows"),
            accounts
        );
        let restored = load_all_account_usage_views(&db, 200).expect("restore host amounts");
        assert_eq!(restored[0].view.buckets[0].used_money, used);
        assert_eq!(restored[0].view.buckets[0].limit_money, limit);
        assert_eq!(restored[0].view.buckets[0].remaining_money, remaining);
        assert_eq!(
            restored[0].view.buckets[0].remaining_percent,
            expected_percent
        );
    }
}

#[test]
fn canonical_v7_money_migration_preserves_exact_count_and_identity_without_guessing_exponent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("v7.db");
    let mut original = usage_view();
    original.canonical_identity = Some(UsageCanonicalAccountIdentity {
        surface_id: original
            .account_identity
            .as_ref()
            .expect("route")
            .surface_id
            .clone(),
        subject: jackin_protocol::control::UsageCanonicalAccountSubject::ProviderId(
            "stable-money-owner".to_owned(),
        ),
    });
    original.buckets[1].count_quota = Some(request_count(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(1),
        jackin_protocol::control::CountQuotaPeriod::Unknown,
    ));
    create_uncached_canonical_fixture(&db, &original);
    let db_string = path_to_turso(&db).expect("path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute_batch("ALTER TABLE account_usage_snapshots DROP COLUMN source_revision; ALTER TABLE account_usage_snapshots DROP COLUMN monetary_quota_json;
            UPDATE _meta SET value = '7' WHERE key = 'schema_version';")
            .await.map_err(|err| err.to_string())?;
        Ok(())
    }).expect("prepare historical v6 cache");
    assert!(
        read_account_usage_snapshots(&db)
            .expect_err("explicit migration status")
            .contains("requires explicit refresh migration")
    );
    assert_eq!(
        schema_version(&db).expect("unchanged version").as_deref(),
        Some("7")
    );
    store_usage_snapshots(&db, &[]).expect("write migrates v7");
    let accounts = read_account_usage_snapshots(&db).expect("preserved rows");
    assert_eq!(accounts.len(), 2);
    assert!(
        accounts
            .iter()
            .all(|account| account.account_identity == original.account_identity)
    );
    assert!(
        accounts
            .iter()
            .all(|account| account.canonical_identity == original.canonical_identity)
    );
    assert_eq!(
        fixture_meta(&db, "canonical_identity_migration_status"),
        None
    );
    assert!(accounts.iter().all(|account| account.used_money.is_none()
        && account.limit_money.is_none()
        && account.remaining_money.is_none()));
    let counts = accounts
        .iter()
        .find(|account| account.window_kind == "Credits")
        .expect("count row");
    assert_eq!(counts.count_quota, original.buckets[1].count_quota);
    let percentage = accounts
        .iter()
        .find(|account| account.window_kind == "Session")
        .expect("percent row");
    assert_eq!(percentage.used_amount, Some(63));
    assert_eq!(percentage.limit_amount, Some(100));
}

#[test]
fn canonical_v7_staged_monetary_column_finishes_without_losing_exact_payload() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("staged-money.db");
    let mut original = usage_view();
    original.buckets[0].used_money = Some(Money::new(1, "USD", 3));
    original.buckets[0].remaining_money = Some(Money::new(-1, "USD", 3));
    create_uncached_canonical_fixture(&db, &original);
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute(
            "UPDATE _meta SET value = '7' WHERE key = 'schema_version'",
            (),
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("stage old metadata with monetary column");
    store_usage_snapshots(&db, &[]).expect("finish staged money migration");
    let restored = load_all_account_usage_views(&db, 100).expect("restore exact staged payload");
    let session = restored[0]
        .view
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Session")
        .expect("session");
    assert_eq!(session.used_money, Some(Money::new(1, "USD", 3)));
    assert_eq!(session.remaining_money, Some(Money::new(-1, "USD", 3)));
    assert_eq!(session.status, UsageSnapshotStatus::Fresh);
    assert_eq!(restored[0].view.last_error, None);
    assert_eq!(fixture_meta(&db, "schema_version").as_deref(), Some("8"));
}

#[test]
fn mixed_count_and_monetary_snapshots_reject_before_overwriting_last_good_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("mixed-domain.db");
    let original = usage_view();
    store_usage_snapshot(&db, &original).expect("store last good");
    let last_good = read_account_usage_snapshots(&db).expect("original rows");
    let mut invalid_view = original.clone();
    invalid_view.buckets[0].count_quota = Some(request_count(
        Some(9),
        Some(10),
        Some(1),
        jackin_protocol::control::CountQuotaPeriod::UtcDaily,
    ));
    invalid_view.buckets[0].remaining_money = Some(Money::new(1, "USD", 3));
    assert!(
        store_usage_snapshot(&db, &invalid_view)
            .expect_err("reject mixed focused domain")
            .contains("combines request counts")
    );
    assert_eq!(
        read_account_usage_snapshots(&db).expect("unchanged focused rows"),
        last_good
    );
    let mut invalid_accounts = last_good.clone();
    invalid_accounts[0].count_quota = invalid_view.buckets[0].count_quota;
    invalid_accounts[0].used_money = Some(Money::new(1, "USD", 3));
    assert!(
        store_account_usage_snapshots(&db, &invalid_accounts)
            .expect_err("reject mixed host domain")
            .contains("combines request counts")
    );
    assert_eq!(
        read_account_usage_snapshots(&db).expect("unchanged host rows"),
        last_good
    );
}

fn mixed_quota_fixture() -> FocusedUsageView {
    let mut view = usage_view();
    view.buckets[0].count_quota = Some(request_count(
        Some(9),
        Some(10),
        Some(1),
        jackin_protocol::control::CountQuotaPeriod::UtcDaily,
    ));
    view.buckets[0].remaining_money = Some(Money::new(1, "USD", 3));
    view
}

#[test]
fn invalid_snapshot_batches_do_not_create_database_or_parent_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("uncreated").join("invalid.db");
    let invalid = mixed_quota_fixture();
    assert!(store_usage_snapshots(&db, &[usage_view(), invalid.clone()]).is_err());
    assert!(!db.exists());
    assert!(!db.parent().expect("parent").exists());
    let cached = HashMap::from([(
        "invalid".to_owned(),
        crate::usage::CachedUsage { view: invalid },
    )]);
    let invalid_accounts = crate::usage::account_snapshot_views_from_cache(&cached);
    assert!(store_account_usage_snapshots(&db, &invalid_accounts).is_err());
    assert!(!db.exists());
    assert!(!db.parent().expect("parent").exists());
}

#[test]
fn invalid_batches_preserve_historical_v7_schema_until_valid_write() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("v7-preflight.db");
    create_uncached_canonical_fixture(&db, &usage_view());
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute_batch("ALTER TABLE account_usage_snapshots DROP COLUMN source_revision; ALTER TABLE account_usage_snapshots DROP COLUMN monetary_quota_json;
            UPDATE _meta SET value = '7' WHERE key = 'schema_version';")
            .await.map_err(|err| err.to_string())?;
        Ok(())
    }).expect("prepare historical v7");
    let invalid = mixed_quota_fixture();
    assert!(store_usage_snapshots(&db, &[usage_view(), invalid.clone()]).is_err());
    let cached = HashMap::from([(
        "invalid".to_owned(),
        crate::usage::CachedUsage { view: invalid },
    )]);
    assert!(
        store_account_usage_snapshots(
            &db,
            &crate::usage::account_snapshot_views_from_cache(&cached)
        )
        .is_err()
    );
    assert_eq!(fixture_meta(&db, "schema_version").as_deref(), Some("7"));
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        let mut columns = conn.query("PRAGMA table_info(account_usage_snapshots)", ())
            .await.map_err(|err| err.to_string())?;
        while let Some(row) = columns.next().await.map_err(|err| err.to_string())? {
            assert_ne!(row_string(&row, 1, "column_name")?, "monetary_quota_json");
        }
        drop(columns);
        let mut rows = conn.query("SELECT used_amount, limit_amount FROM account_usage_snapshots WHERE window_kind = 'Session'", ())
            .await.map_err(|err| err.to_string())?;
        let row = rows.next().await.map_err(|err| err.to_string())?.expect("last-good session");
        assert_eq!(row_i64(&row, 0, "used_amount")?, 63);
        assert_eq!(row_i64(&row, 1, "limit_amount")?, 100);
        Ok(())
    }).expect("schema and rows unchanged");
    store_usage_snapshots(&db, &[]).expect("valid write still migrates");
    assert_eq!(fixture_meta(&db, "schema_version").as_deref(), Some("8"));
    let accounts = read_account_usage_snapshots(&db).expect("retained rows after valid migration");
    assert_eq!(accounts.len(), 2);
    let session = accounts
        .iter()
        .find(|account| account.window_kind == "Session")
        .expect("session");
    assert_eq!(session.used_amount, Some(63));
    assert_eq!(session.limit_amount, Some(100));
    assert_eq!(session.used_money, None);
}

#[test]
fn invalid_monetary_batches_reject_without_creating_or_overwriting_cache() {
    for (used, limit, remaining) in [
        (Some(Money::new(-1, "USD", 3)), None, None),
        (None, Some(Money::new(-1, "USD", 3)), None),
        (
            Some(Money::new(1, "USD", 3)),
            Some(Money::new(1, "SGD", 3)),
            None,
        ),
        (
            None,
            Some(Money::new(1, "USD", 2)),
            Some(Money::new(-1, "SGD", 3)),
        ),
        (
            Some(Money::new(1, "USD", 3)),
            None,
            Some(Money::new(-1, "SGD", 3)),
        ),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("uncreated").join("invalid.db");
        let mut invalid = usage_view();
        invalid.buckets[0].used_money = used;
        invalid.buckets[0].limit_money = limit;
        invalid.buckets[0].remaining_money = remaining;
        let cached = HashMap::from([(
            "invalid".to_owned(),
            crate::usage::CachedUsage {
                view: invalid.clone(),
            },
        )]);
        let invalid_accounts = crate::usage::account_snapshot_views_from_cache(&cached);
        assert!(store_usage_snapshots(&missing, &[usage_view(), invalid.clone()]).is_err());
        assert!(store_account_usage_snapshots(&missing, &invalid_accounts).is_err());
        assert!(!missing.exists());
        assert!(!missing.parent().expect("parent").exists());
        let existing = dir.path().join("last-good.db");
        store_usage_snapshot(&existing, &usage_view()).expect("last good");
        let last_good = read_account_usage_snapshots(&existing).expect("original rows");
        assert!(store_usage_snapshot(&existing, &invalid).is_err());
        assert!(store_account_usage_snapshots(&existing, &invalid_accounts).is_err());
        assert_eq!(
            read_account_usage_snapshots(&existing).expect("unchanged rows"),
            last_good
        );
    }
}

#[test]
fn historical_v7_monetary_scale_loss_requires_refresh_without_harming_count_or_percent_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("historical-precision.db");
    let mut original = usage_view();
    original.buckets[1].count_quota = Some(request_count(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(1),
        jackin_protocol::control::CountQuotaPeriod::UtcDaily,
    ));
    original.buckets[1].status = UsageSnapshotStatus::Fresh;
    let mut historical = original.buckets[0].clone();
    historical.label = "Historical".to_owned();
    original.buckets.push(historical);
    original.status_bar_label = "old misleading $0.00 spent and exhausted".to_owned();
    original.canonical_identity = Some(UsageCanonicalAccountIdentity {
        surface_id: original
            .account_identity
            .as_ref()
            .expect("route")
            .surface_id
            .clone(),
        subject: jackin_protocol::control::UsageCanonicalAccountSubject::ProviderId(
            "stable-historical-owner".to_owned(),
        ),
    });
    create_uncached_canonical_fixture(&db, &original);
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        // V7 cannot distinguish a rounded-down fractional source from true zero.
        // Currency metadata is the evidence; these labels carry none.
        conn.execute_batch("UPDATE account_usage_snapshots SET used_amount = 0, used_unit = 'USD', limit_amount = 10000, limit_unit = 'USD', remaining_percent = 0, used_label = 'unknown original scale', limit_label = 'unknown original cap scale' WHERE window_kind = 'Historical';
            UPDATE account_usage_snapshots SET used_label = '$0.00 mislabeled percentage' WHERE window_kind = 'Session';
            ALTER TABLE account_usage_snapshots DROP COLUMN monetary_quota_json;
            UPDATE _meta SET value = '7' WHERE key = 'schema_version';")
            .await.map_err(|err| err.to_string())?;
        Ok(())
    }).expect("prepare literal scalar-only monetary history");
    store_usage_snapshots(&db, &[]).expect("preserve rows during v8 migration");
    let private = stored_account_snapshots(&db).expect("private preserved raw history");
    let raw = private
        .iter()
        .find(|row| row.window_kind == "Historical")
        .expect("raw history");
    assert_eq!(raw.used_amount, Some(0));
    assert_eq!(raw.limit_amount, Some(10000));
    assert_eq!(raw.used_unit.as_deref(), Some("USD"));
    assert_eq!(raw.remaining_percent, Some(0));
    assert_eq!(raw.used_label.as_deref(), Some("unknown original scale"));
    assert_eq!(raw.status, "fresh");
    let accounts = read_account_usage_snapshots(&db).expect("safe public projection");
    assert_eq!(accounts.len(), 3);
    assert!(
        accounts
            .iter()
            .all(|account| account.canonical_identity == original.canonical_identity)
    );
    let lost = accounts
        .iter()
        .find(|account| account.window_kind == "Historical")
        .expect("unknown precision");
    assert_eq!(lost.used_amount, None);
    assert_eq!(lost.limit_amount, None);
    assert_eq!(lost.used_unit, None);
    assert_eq!(lost.limit_unit, None);
    assert_eq!(lost.used_money, None);
    assert_eq!(lost.limit_money, None);
    assert_eq!(lost.remaining_money, None);
    assert_eq!(lost.status, "unavailable");
    assert!(
        lost.last_error
            .as_deref()
            .expect("refresh diagnostic")
            .contains("lacks exact decimal scale")
    );
    let count = accounts
        .iter()
        .find(|account| account.window_kind == "Credits")
        .expect("count row");
    assert_eq!(count.count_quota, original.buckets[1].count_quota);
    assert_eq!(count.status, "fresh");
    let percent = accounts
        .iter()
        .find(|account| account.window_kind == "Session")
        .expect("percentage row");
    assert_eq!(percent.used_amount, Some(63));
    assert_eq!(percent.used_unit.as_deref(), Some("percent"));
    assert_eq!(percent.status, "fresh");
    let restored = load_all_account_usage_views(&db, 100).expect("safe focused projection");
    let lost = restored[0]
        .view
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Historical")
        .expect("unknown bucket");
    assert_eq!(lost.used_label, None);
    assert_eq!(lost.limit_label, None);
    assert_eq!(lost.remaining_percent, None);
    assert_eq!(lost.status, UsageSnapshotStatus::Unavailable);
    assert!(
        restored[0]
            .view
            .last_error
            .as_deref()
            .expect("account diagnostic")
            .contains("provider refresh required")
    );
    assert!(!restored[0].view.status_bar_label.contains("$0.00 spent"));
    assert_eq!(restored[0].view.status, UsageSnapshotStatus::Fresh);
    let percent = restored[0]
        .view
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Session")
        .expect("percentage bucket");
    assert_eq!(percent.remaining_percent, Some(37));
    assert_eq!(
        percent.used_label.as_deref(),
        Some("$0.00 mislabeled percentage")
    );
    assert_eq!(percent.status, UsageSnapshotStatus::Fresh);
    store_account_usage_snapshots(&db, &accounts)
        .expect("resave safe account DTOs without destroying evidence");
    let preserved = stored_account_snapshots(&db).expect("private history after host write");
    assert_eq!(
        preserved
            .iter()
            .find(|row| row.window_kind == "Historical")
            .expect("historical row"),
        raw
    );
    store_usage_snapshot(&db, &restored[0].view)
        .expect("resave safe focused DTO without destroying evidence");
    let preserved = stored_account_snapshots(&db).expect("private history after focused write");
    assert_eq!(
        preserved
            .iter()
            .find(|row| row.window_kind == "Historical")
            .expect("historical row"),
        raw
    );
    original.buckets[2].used_money = Some(Money::new(1, "USD", 3));
    original.buckets[2].limit_money = Some(Money::new(100, "USD", 2));
    original.buckets[2].remaining_money = Some(Money::new(999, "USD", 3));
    original.status_bar_label = "refreshed exact quota".to_owned();
    store_usage_snapshot(&db, &original).expect("actual exact provider refresh");
    let restored = load_all_account_usage_views(&db, 100).expect("exact refreshed projection");
    let exact = restored[0]
        .view
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Historical")
        .expect("exact money");
    assert_eq!(exact.used_money, Some(Money::new(1, "USD", 3)));
    assert_eq!(exact.remaining_percent, Some(99));
    assert_eq!(exact.status, UsageSnapshotStatus::Fresh);
    assert_eq!(restored[0].view.last_error, None);
}

#[test]
fn accepted_source_revision_roundtrips_without_becoming_display_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("source-revision.db");
    let mut original = usage_view();
    original
        .account_identity
        .as_mut()
        .expect("route")
        .source_revision = Some("opaque-accepted-revision-a".to_owned());
    original.canonical_identity = Some(UsageCanonicalAccountIdentity {
        surface_id: original
            .account_identity
            .as_ref()
            .expect("route")
            .surface_id
            .clone(),
        subject: UsageCanonicalAccountSubject::SourceCapability("opaque-stable-source".to_owned()),
    });
    original.buckets[0].count_quota = Some(request_count(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(3),
        jackin_protocol::control::CountQuotaPeriod::UtcDaily,
    ));
    store_usage_snapshot(&db, &original).expect("write accepted revision");
    let raw = read_account_usage_snapshots(&db).expect("read accepted revision");
    assert!(
        raw.iter()
            .all(|row| row.account_identity == original.account_identity)
    );
    assert!(
        raw.iter()
            .all(|row| row.canonical_identity == original.canonical_identity)
    );
    store_account_usage_snapshots(&db, &raw).expect("host revision roundtrip");
    let restored = load_all_account_usage_views(&db, 100).expect("restored revision");
    assert_eq!(restored[0].view.account_identity, original.account_identity);
    assert_eq!(
        restored[0].view.canonical_identity,
        original.canonical_identity
    );
    let mut rotated = original.clone();
    rotated
        .account_identity
        .as_mut()
        .expect("route")
        .source_revision = Some("opaque-accepted-revision-b".to_owned());
    rotated.fetched_at_epoch += 1;
    store_usage_snapshot(&db, &rotated).expect("write accepted replacement revision");
    let restored = load_all_account_usage_views(&db, 100).expect("restored replacement revision");
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].view.account_identity, rotated.account_identity);
    assert_eq!(
        restored[0].view.canonical_identity,
        original.canonical_identity
    );
    assert_eq!(
        restored[0].account_key_hash,
        crate::usage::usage_account_tab_id(original.account_identity.as_ref().expect("route"))
    );
}

#[test]
fn staged_v8_revision_migration_preserves_exact_history_and_marks_unknown_authority() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("staged-v8-revision.db");
    let mut original = usage_view();
    original.canonical_identity = Some(UsageCanonicalAccountIdentity {
        surface_id: original
            .account_identity
            .as_ref()
            .expect("route")
            .surface_id
            .clone(),
        subject: UsageCanonicalAccountSubject::SourceCapability("opaque-stable-source".to_owned()),
    });
    original.buckets[0].count_quota = Some(request_count(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(0),
        jackin_protocol::control::CountQuotaPeriod::Unknown,
    ));
    original.buckets[1].used_money = Some(Money::new(17, "USD", 3));
    original.buckets[1].limit_money = Some(Money::new(19, "USD", 3));
    create_uncached_canonical_fixture(&db, &original);
    let before = read_account_usage_snapshots(&db).expect("private expected raw history");
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute(
            "ALTER TABLE account_usage_snapshots DROP COLUMN source_revision",
            (),
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("stage unreleased v8 without revision field");
    assert!(
        read_account_usage_snapshots(&db)
            .expect_err("explicit missing authority field")
            .contains("requires explicit refresh migration")
    );
    assert_eq!(fixture_meta(&db, "source_revision_migration_status"), None);
    store_usage_snapshots(&db, &[]).expect("explicit migration preserves typed history");
    let after = read_account_usage_snapshots(&db).expect("preserved raw history");
    assert_eq!(after, before);
    assert!(after.iter().all(|row| {
        row.account_identity
            .as_ref()
            .expect("route")
            .source_revision
            .is_none()
    }));
    assert_eq!(
        fixture_meta(&db, "source_revision_migration_status").as_deref(),
        Some("refresh_required")
    );
    assert_eq!(
        schema_version(&db).expect("final schema8").as_deref(),
        Some("8")
    );
}

#[test]
fn empty_source_revision_is_rejected_and_last_good_binding_retained() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("invalid-source-revision.db");
    let mut original = usage_view();
    original
        .account_identity
        .as_mut()
        .expect("route")
        .source_revision = Some("valid-opaque-revision".to_owned());
    store_usage_snapshot(&db, &original).expect("last good revision");
    let mut invalid = original.clone();
    invalid
        .account_identity
        .as_mut()
        .expect("route")
        .source_revision = Some(" ".to_owned());
    assert!(
        store_usage_snapshot(&db, &invalid)
            .expect_err("empty revision rejected")
            .contains("invalid usage snapshot source revision")
    );
    assert_eq!(
        load_all_account_usage_views(&db, 100).expect("last good revision")[0]
            .view
            .account_identity,
        original.account_identity
    );
}

#[test]
fn same_second_conflicting_authorities_cannot_launder_historical_buckets() {
    for (change_revision, change_proof) in [(true, true), (true, false), (false, true)] {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("authority-conflict.db");
        let mut old = provider_usage_view("Codex", "same-label", None, "ZZZ Historical", 1, 100);
        old.account_identity
            .as_mut()
            .expect("route")
            .source_revision = Some("accepted-old-revision".to_owned());
        old.canonical_identity = Some(UsageCanonicalAccountIdentity {
            surface_id: old
                .account_identity
                .as_ref()
                .expect("route")
                .surface_id
                .clone(),
            subject: UsageCanonicalAccountSubject::SourceCapability(
                "old-logical-source".to_owned(),
            ),
        });
        let mut current = old.clone();
        current.buckets[0].label = "AAA Current".to_owned();
        current.buckets[0].remaining_percent = Some(88);
        if change_revision {
            current
                .account_identity
                .as_mut()
                .expect("route")
                .source_revision = Some("accepted-current-revision".to_owned());
        }
        if change_proof {
            current
                .canonical_identity
                .as_mut()
                .expect("logical proof")
                .subject =
                UsageCanonicalAccountSubject::SourceCapability("current-logical-source".to_owned());
        }
        store_usage_snapshot(&db, &old).expect("historical authority");
        store_usage_snapshot(&db, &current).expect("current authority in same second");
        let raw =
            read_account_usage_snapshots(&db).expect("raw history remains individually attributed");
        assert_eq!(raw.len(), 2);
        for expected in [&old, &current] {
            let row = raw
                .iter()
                .find(|row| row.window_kind == expected.buckets[0].label)
                .expect("attributed window");
            assert_eq!(row.account_identity, expected.account_identity);
            assert_eq!(row.canonical_identity, expected.canonical_identity);
        }
        assert!(
            load_all_account_usage_views(&db, 100)
                .expect_err("no authority laundering")
                .contains("conflicting usage snapshot authority")
        );
        assert!(
            list_account_identities(&db)
                .expect_err("no mixed authority summary")
                .contains("conflicting usage snapshot authority")
        );
        // A later accepted fetch resolves current authority; old rows remain raw history.
        current.fetched_at_epoch += 1;
        store_usage_snapshot(&db, &current).expect("new authoritative fetch");
        let views = load_all_account_usage_views(&db, 101).expect("current authority resolved");
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].view.account_identity, current.account_identity);
        assert_eq!(views[0].view.canonical_identity, current.canonical_identity);
        assert_eq!(views[0].view.buckets.len(), 1);
        assert_eq!(views[0].view.buckets[0].label, "AAA Current");
        assert_eq!(views[0].view.buckets[0].remaining_percent, Some(88));
        let summary = list_account_identities(&db)
            .expect("current authority summary")
            .remove(0);
        assert_eq!(Some(summary.account_identity), current.account_identity);
        assert_eq!(summary.canonical_identity, current.canonical_identity);
        assert_eq!(summary.remaining_percent, Some(88));
        assert_eq!(
            read_account_usage_snapshots(&db)
                .expect("raw history retained")
                .len(),
            2
        );
    }
}

#[test]
fn forged_durable_quota_payloads_are_rejected_at_shared_read_boundary() {
    let count = request_count(
        Some(1),
        Some(2),
        Some(1),
        jackin_protocol::control::CountQuotaPeriod::Unknown,
    );
    for (used, limit, remaining, count, revision, expected_error) in [
        (
            Some(Money::new(-1, "USD", 3)),
            None,
            None,
            None,
            None,
            "monetary usage or cap is negative",
        ),
        (
            None,
            Some(Money::new(-1, "USD", 3)),
            None,
            None,
            None,
            "monetary usage or cap is negative",
        ),
        (
            Some(Money::new(1, "USD", 3)),
            Some(Money::new(2, "EUR", 3)),
            None,
            None,
            None,
            "combines currencies",
        ),
        (
            Some(Money::new(1, "USD", 3)),
            None,
            None,
            Some(count),
            None,
            "combines request counts with monetary amounts",
        ),
        (
            None,
            None,
            None,
            None,
            Some(" "),
            "invalid usage snapshot source revision",
        ),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("forged-durable-row.db");
        create_uncached_canonical_fixture(&db, &usage_view());
        let monetary_json = serde_json::to_string(&StoredMonetaryQuota {
            used,
            limit,
            remaining,
        })
        .expect("fixture monetary JSON");
        let count_json = count
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .expect("fixture count JSON");
        let revision = revision.map(str::to_owned);
        let db_string = path_to_turso(&db).expect("fixture path");
        block_on_store(async move {
            let conn = connect_local(&db_string).await?;
            conn.execute("UPDATE account_usage_snapshots SET monetary_quota_json = ?1, count_quota_json = ?2, source_revision = ?3 WHERE window_kind = 'Session'", params![monetary_json, count_json, revision])
                .await.map_err(|err| err.to_string())?;
            Ok(())
        }).expect("forge private persisted payload");
        assert!(
            read_account_usage_snapshots(&db)
                .expect_err("raw export rejects invalid durable representation")
                .contains(expected_error)
        );
        assert!(
            load_all_account_usage_views(&db, 100)
                .expect_err("focused read rejects invalid durable representation")
                .contains(expected_error)
        );
        assert!(
            stored_account_snapshots(&db)
                .expect_err("test read cannot bypass shared validation")
                .contains(expected_error)
        );
        assert!(
            list_account_identities(&db)
                .expect_err("summary read rejects invalid durable representation")
                .contains(expected_error)
        );
    }
}

#[test]
fn valid_durable_negative_remaining_preserves_literal_overage() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("durable-overage.db");
    create_uncached_canonical_fixture(&db, &usage_view());
    let monetary = StoredMonetaryQuota {
        used: Some(Money::new(11, "USD", 3)),
        limit: Some(Money::new(2, "USD", 3)),
        remaining: Some(Money::new(-9, "USD", 3)),
    };
    let json = serde_json::to_string(&monetary).expect("fixture overage JSON");
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute("UPDATE account_usage_snapshots SET monetary_quota_json = ?1 WHERE window_kind = 'Session'", [json])
            .await.map_err(|err| err.to_string())?;
        Ok(())
    }).expect("valid private overage payload");
    let raw = read_account_usage_snapshots(&db).expect("valid overage raw read");
    let session = raw
        .iter()
        .find(|row| row.window_kind == "Session")
        .expect("session");
    assert_eq!(session.used_money, monetary.used);
    assert_eq!(session.limit_money, monetary.limit);
    assert_eq!(session.remaining_money, monetary.remaining);
    let focused = load_all_account_usage_views(&db, 100).expect("valid overage focused read");
    let session = focused[0]
        .view
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Session")
        .expect("session");
    assert_eq!(session.remaining_money, monetary.remaining);
}

#[test]
fn invalid_revision_preflight_keeps_legacy_cache_bytes_and_rows_unchanged() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("legacy-invalid-revision.db");
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta VALUES ('schema_version', '4');
             CREATE TABLE account_usage_snapshots (account_label TEXT NOT NULL);
             INSERT INTO account_usage_snapshots VALUES ('legacy-row-preserved');",
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("private legacy fixture");
    let original_bytes = std::fs::read(&db).expect("private fixture bytes");
    let mut invalid = usage_view();
    invalid
        .account_identity
        .as_mut()
        .expect("route")
        .source_revision = Some(" ".to_owned());
    assert!(
        store_usage_snapshot(&db, &invalid)
            .expect_err("focused preflight")
            .contains("invalid usage snapshot source revision")
    );
    let cache = HashMap::from([(
        "invalid".to_owned(),
        crate::usage::CachedUsage { view: invalid },
    )]);
    let raw = crate::usage::account_snapshot_views_from_cache(&cache);
    assert!(!raw.is_empty());
    assert!(
        store_account_usage_snapshots(&db, &raw)
            .expect_err("host raw preflight")
            .contains("invalid usage snapshot source revision")
    );
    assert_eq!(
        std::fs::read(&db).expect("private fixture unchanged bytes"),
        original_bytes
    );
    assert_eq!(connection_build_count(&db).expect("no store open"), 0);
    assert_eq!(
        schema_version(&db).expect("unchanged schema").as_deref(),
        Some("4")
    );
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        let mut rows = conn
            .query("SELECT account_label FROM account_usage_snapshots", ())
            .await
            .map_err(|err| err.to_string())?;
        assert_eq!(
            row_string(
                &rows
                    .next()
                    .await
                    .map_err(|err| err.to_string())?
                    .expect("legacy row"),
                0,
                "label"
            )?,
            "legacy-row-preserved"
        );
        Ok(())
    })
    .expect("legacy row retained");
}

#[test]
fn zero_bucket_invalid_bound_header_cannot_migrate_legacy_cache() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("zero-bucket-legacy.db");
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta VALUES ('schema_version', '4');
             CREATE TABLE account_usage_snapshots (account_label TEXT NOT NULL);
             INSERT INTO account_usage_snapshots VALUES ('zero-bucket-legacy-row');",
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("private legacy fixture");
    let original_bytes = std::fs::read(&db).expect("private fixture bytes");
    let mut invalid = usage_view();
    invalid.buckets.clear();
    invalid
        .account_identity
        .as_mut()
        .expect("bound route")
        .source_revision = Some(" ".to_owned());
    assert!(
        store_usage_snapshot(&db, &invalid)
            .expect_err("zero-bucket authority validated before migration")
            .contains("invalid usage snapshot source revision")
    );
    assert_eq!(
        std::fs::read(&db).expect("unchanged fixture bytes"),
        original_bytes
    );
    assert_eq!(connection_build_count(&db).expect("no cache open"), 0);
    assert_eq!(fixture_meta(&db, "schema_version").as_deref(), Some("4"));
    // An actually empty input slice remains the explicit migration operation.
    store_usage_snapshots(&db, &[]).expect("authorized explicit migration");
    assert_eq!(
        schema_version(&db).expect("migrated version").as_deref(),
        Some("8")
    );
    assert_eq!(
        fixture_meta(&db, "usage_snapshot_migration_status").as_deref(),
        Some("display_derived_cache_invalidated")
    );
}

fn membership_scope(id: char) -> UsageMembershipScope {
    UsageMembershipScope {
        container_id: id.to_string().repeat(64),
        workspace_config_proof: "accepted-workspace-proof".to_owned(),
    }
}

fn membership_projection(issuer: &str, generation: u64) -> UsageProjectionV2 {
    UsageProjectionV2 {
        schema_version: jackin_protocol::usage_broker::UsageProjectionSchemaV2,
        projection_id: format!("{issuer}-{generation}"),
        generated_at_epoch: 100,
        discovery_revision: "accepted-discovery".to_owned(),
        broker_instance_id: issuer.to_owned(),
        broker_generation: generation,
        refresh_state: jackin_protocol::usage_broker::UsageProjectionRefreshStateV2::Idle,
        providers: Vec::new(),
        unresolved: Vec::new(),
        unresolved_grants: Vec::new(),
        issues: Vec::new(),
    }
}

fn current_membership(issuer: &str, generation: u64) -> UsageAccountMembershipV1 {
    UsageAccountMembershipV1::Current {
        projection: Box::new(membership_projection(issuer, generation)),
    }
}

#[test]
fn scoped_membership_states_preserve_history_and_independent_container_authority() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("membership.db");
    store_usage_snapshot(&db, &usage_view()).expect("separate historical quotas");
    let historical = read_account_usage_snapshots(&db).expect("historical baseline");
    let first = membership_scope('a');
    let second = membership_scope('b');
    store_usage_membership(&db, &first, &current_membership("issuer-a", 1))
        .expect("accepted empty membership");
    store_usage_membership(&db, &second, &current_membership("issuer-b", 1))
        .expect("independent scope");
    let states = read_usage_memberships(&db).expect("typed memberships");
    assert_eq!(states.len(), 2);
    assert!(
        matches!(&states[0].membership, UsageAccountMembershipV1::Current { projection } if projection.providers.is_empty())
    );
    store_usage_membership(&db, &first, &UsageAccountMembershipV1::Unavailable)
        .expect("unavailable removes only current authority");
    let states = read_usage_memberships(&db).expect("typed unavailable");
    assert!(matches!(
        states[0].membership,
        UsageAccountMembershipV1::Unavailable
    ));
    assert!(matches!(
        states[1].membership,
        UsageAccountMembershipV1::Current { .. }
    ));
    store_usage_membership(&db, &first, &current_membership("issuer-a", 1))
        .expect("identical accepted publication recovers transport availability");
    store_usage_membership(&db, &first, &UsageAccountMembershipV1::Revoked)
        .expect("explicit scoped revocation");
    assert!(
        store_usage_membership(&db, &first, &current_membership("issuer-a", 1))
            .expect_err("revoked stale authority cannot return")
            .contains("requires a newer accepted publication")
    );
    let states = read_usage_memberships(&db).expect("typed revoked");
    assert!(matches!(
        states[0].membership,
        UsageAccountMembershipV1::Revoked
    ));
    assert!(matches!(
        states[1].membership,
        UsageAccountMembershipV1::Current { .. }
    ));
    store_usage_membership(&db, &first, &current_membership("issuer-a", 2))
        .expect("newly accepted publication");
    assert_eq!(
        read_account_usage_snapshots(&db).expect("quota history retained"),
        historical
    );
}

#[test]
fn scoped_membership_rejects_stale_equivocating_and_retired_issuers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("issuer-membership.db");
    let scope = membership_scope('a');
    store_usage_membership(&db, &scope, &current_membership("first-issuer", 2))
        .expect("accepted publication");
    assert!(
        store_usage_membership(&db, &scope, &current_membership("first-issuer", 1))
            .expect_err("older generation")
            .contains("stale usage membership")
    );
    let mut conflicting = membership_projection("first-issuer", 2);
    conflicting.projection_id = "different-payload-at-same-generation".to_owned();
    assert!(
        store_usage_membership(
            &db,
            &scope,
            &UsageAccountMembershipV1::Current {
                projection: Box::new(conflicting)
            }
        )
        .expect_err("issuer equivocation")
        .contains("conflicting usage membership")
    );
    store_usage_membership(&db, &scope, &current_membership("replacement-issuer", 1))
        .expect("authenticated issuer replacement");
    assert!(
        store_usage_membership(&db, &scope, &current_membership("first-issuer", u64::MAX))
            .expect_err("retired issuer cannot return")
            .contains("retired usage membership issuer")
    );
    let states = read_usage_memberships(&db).expect("new authority retained");
    assert!(
        matches!(&states[0].membership, UsageAccountMembershipV1::Current { projection } if projection.broker_instance_id == "replacement-issuer")
    );
    let mut next_scope = scope.clone();
    next_scope.workspace_config_proof = "new-accepted-workspace-proof".to_owned();
    assert!(
        store_usage_membership(
            &db,
            &next_scope,
            &current_membership("replacement-issuer", 1)
        )
        .is_err()
    );
    store_usage_membership(
        &db,
        &next_scope,
        &current_membership("replacement-issuer", 2),
    )
    .expect("new admitted workspace proof with newer publication");
    let states = read_usage_memberships(&db).expect("one latest namespace per immutable container");
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].scope, next_scope);
}

#[test]
fn membership_write_does_not_migrate_or_delete_old_quota_cache() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("old-quota-membership.db");
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO _meta VALUES ('schema_version', '4');
            CREATE TABLE account_usage_snapshots (account_label TEXT NOT NULL);
            INSERT INTO account_usage_snapshots VALUES ('retained-old-history');",
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("private old quota fixture");
    assert!(
        read_usage_memberships(&db)
            .expect_err("old quota rows confer no membership")
            .contains("authenticated refresh required")
    );
    store_usage_membership(
        &db,
        &membership_scope('a'),
        &current_membership("accepted-issuer", 1),
    )
    .expect("separate authoritative membership");
    assert_eq!(fixture_meta(&db, "schema_version").as_deref(), Some("4"));
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        let mut rows = conn
            .query("SELECT account_label FROM account_usage_snapshots", ())
            .await
            .map_err(|err| err.to_string())?;
        assert_eq!(
            row_string(
                &rows
                    .next()
                    .await
                    .map_err(|err| err.to_string())?
                    .expect("historical row"),
                0,
                "label"
            )?,
            "retained-old-history"
        );
        Ok(())
    })
    .expect("old history unchanged");
}

#[test]
fn membership_invalid_scope_or_projection_fails_before_cache_creation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("not-created").join("membership.db");
    let mut scope = membership_scope('a');
    scope.container_id = "reusable-container-name".to_owned();
    assert!(store_usage_membership(&db, &scope, &current_membership("issuer", 1)).is_err());
    let scope = membership_scope('a');
    let mut projection = membership_projection("issuer", 1);
    projection.discovery_revision.clear();
    assert!(
        store_usage_membership(
            &db,
            &scope,
            &UsageAccountMembershipV1::Current {
                projection: Box::new(projection)
            }
        )
        .is_err()
    );
    assert!(!db.parent().expect("parent").exists());
    assert!(
        read_usage_memberships(&db)
            .expect("no recorded scopes")
            .is_empty()
    );
    assert!(!db.exists());
}

#[test]
fn scoped_membership_revocation_survives_intermediate_unavailability() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("sticky-revocation.db");
    let scope = membership_scope('a');
    let accepted = current_membership("issuer-a", 1);
    store_usage_membership(&db, &scope, &accepted).expect("accepted generation");
    store_usage_membership(&db, &scope, &UsageAccountMembershipV1::Revoked)
        .expect("persist independent revocation fence");
    store_usage_membership(&db, &scope, &UsageAccountMembershipV1::Unavailable)
        .expect("availability changes without clearing revocation");
    assert!(
        store_usage_membership(&db, &scope, &accepted)
            .expect_err("revoked generation cannot regain authority after unavailable")
            .contains("requires a newer accepted publication")
    );
    let rows = read_usage_memberships(&db).expect("explicit latest availability");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].membership, UsageAccountMembershipV1::Unavailable);
    store_usage_membership(&db, &scope, &current_membership("issuer-a", 2))
        .expect("newer accepted generation recovers authority");
    let rows = read_usage_memberships(&db).expect("recovered current authority");
    assert!(
        matches!(&rows[0].membership, UsageAccountMembershipV1::Current { projection } if projection.broker_generation == 2)
    );
}

#[test]
fn scoped_membership_accepted_scope_proof_survives_availability_scope_changes() {
    for intermediate in [
        UsageAccountMembershipV1::Unavailable,
        UsageAccountMembershipV1::Revoked,
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("sticky-accepted-proof.db");
        let original_scope = membership_scope('a');
        let mut new_scope = original_scope.clone();
        new_scope.workspace_config_proof = "new-current-workspace-proof".to_owned();
        let accepted = current_membership("issuer-a", 1);
        store_usage_membership(&db, &original_scope, &accepted)
            .expect("publication bound to original proof");
        store_usage_membership(&db, &new_scope, &intermediate)
            .expect("new scope without accepted replacement authority");
        store_usage_membership(&db, &new_scope, &UsageAccountMembershipV1::Unavailable)
            .expect("intermediate unavailability");
        assert!(
            store_usage_membership(&db, &new_scope, &accepted)
                .expect_err("old generation cannot acquire new configuration proof")
                .contains("conflicting usage membership publication")
        );
        let rows = read_usage_memberships(&db).expect("latest unavailable state preserved");
        assert_eq!(rows[0].scope, new_scope);
        assert_eq!(rows[0].membership, UsageAccountMembershipV1::Unavailable);
        store_usage_membership(&db, &new_scope, &current_membership("issuer-a", 2))
            .expect("new publication binds new configuration proof");
        let rows = read_usage_memberships(&db).expect("new accepted proof");
        assert_eq!(rows[0].scope, new_scope);
        assert!(
            matches!(&rows[0].membership, UsageAccountMembershipV1::Current { projection } if projection.broker_generation == 2)
        );
    }
}

#[test]
fn replaced_database_path_cannot_reuse_old_inode_or_initialized_schema() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("replaced.db");
    store_usage_snapshot(&db, &usage_view()).expect("original owned database");
    std::fs::remove_file(&db)
        .expect("replace private fixture inode after operation releases handles");
    let db_string = path_to_turso(&db).expect("fixture path");
    block_on_store(async move {
        let conn = connect_local(&db_string).await?;
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta VALUES ('schema_version', '4');
             CREATE TABLE account_usage_snapshots (account_label TEXT NOT NULL);
             INSERT INTO account_usage_snapshots VALUES ('replacement-old-schema');",
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("different private database at same path");
    let mut replacement = usage_view();
    replacement
        .account_identity
        .as_mut()
        .expect("route")
        .account_id = "replacement-account".to_owned();
    store_usage_snapshot(&db, &replacement)
        .expect("reopen replacement and validate its own schema");
    let rows = read_account_usage_snapshots(&db).expect("replacement database contents");
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|row| row.account_identity == replacement.account_identity)
    );
    assert_eq!(
        fixture_meta(&db, "usage_snapshot_migration_status").as_deref(),
        Some("display_derived_cache_invalidated")
    );
}

#[test]
fn aliases_share_owned_operation_custody_and_reader_observes_completed_writer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("alias-membership.db");
    let alias = dir.path().join(".").join("alias-membership.db");
    let scope = membership_scope('a');
    store_usage_membership(&db, &scope, &current_membership("owned-issuer", 1))
        .expect("original path publication");
    store_usage_membership(&alias, &scope, &current_membership("owned-issuer", 2))
        .expect("alias owned publication");
    let states = read_usage_memberships(&db).expect("strict reader after all writer handles close");
    assert!(
        matches!(&states[0].membership, UsageAccountMembershipV1::Current { projection } if projection.broker_generation == 2)
    );
    assert_eq!(
        states,
        read_usage_memberships(&alias).expect("same physical authority through alias")
    );
}

#[test]
fn membership_reader_and_writer_operations_make_progress_under_shared_custody() {
    use std::sync::{Arc, Barrier, mpsc};
    use std::time::Duration;
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("concurrent-membership.db");
    let scope = membership_scope('a');
    store_usage_membership(&db, &scope, &current_membership("progress-issuer", 1))
        .expect("initial authority");
    let barrier = Arc::new(Barrier::new(3));
    let (completed, receive) = mpsc::channel();
    let writer_path = db.clone();
    let writer_scope = scope.clone();
    let writer_barrier = Arc::clone(&barrier);
    let writer_completed = completed.clone();
    let writer = std::thread::spawn(move || {
        writer_barrier.wait();
        let result = (|| {
            for generation in 2..=5 {
                store_usage_membership(
                    &writer_path,
                    &writer_scope,
                    &current_membership("progress-issuer", generation),
                )?;
            }
            Ok::<_, String>(())
        })();
        writer_completed
            .send(result)
            .expect("report writer completion");
    });
    let reader_path = db.clone();
    let reader_barrier = Arc::clone(&barrier);
    let reader = std::thread::spawn(move || {
        reader_barrier.wait();
        let result = (|| {
            for _ in 0..4 {
                let states = read_usage_memberships(&reader_path)?;
                assert_eq!(states.len(), 1);
                assert!(
                    matches!(&states[0].membership, UsageAccountMembershipV1::Current { projection } if (1..=5).contains(&projection.broker_generation))
                );
            }
            Ok::<_, String>(())
        })();
        completed.send(result).expect("report reader completion");
    });
    barrier.wait();
    for _ in 0..2 {
        receive
            .recv_timeout(Duration::from_secs(30))
            .expect("both owned operations make bounded progress")
            .expect("owned operation succeeded");
    }
    writer.join().expect("writer thread");
    reader.join().expect("reader thread");
    let states = read_usage_memberships(&db).expect("completed writer state");
    assert!(
        matches!(&states[0].membership, UsageAccountMembershipV1::Current { projection } if projection.broker_generation == 5)
    );
}

#[test]
fn all_snapshot_reader_entrypoints_preserve_legacy_database_and_wal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("strict-legacy-reader.db");
    let db_string = path_to_turso(&db).expect("fixture path");
    let seed_path = db_string.clone();
    block_on_store(async move {
        let conn = connect_local(&seed_path).await?;
        conn.execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta VALUES ('schema_version', '4');
             CREATE TABLE account_usage_snapshots (account_label TEXT NOT NULL);
             INSERT INTO account_usage_snapshots VALUES ('strict-read-history');",
        )
        .await
        .map_err(|err| err.to_string())?;
        Ok(())
    })
    .expect("private old-schema fixture");
    let before = store_backend::source_bytes_for_test(&db_string);
    assert!(
        read_account_usage_snapshots(&db)
            .expect_err("wire snapshots do not migrate")
            .contains("explicit refresh migration")
    );
    assert!(
        load_all_account_usage_views(&db, 100)
            .expect_err("focused views do not migrate")
            .contains("explicit refresh migration")
    );
    assert!(
        load_account_usage_view(&db, "unknown-route", 100)
            .expect_err("single focused view does not migrate")
            .contains("explicit refresh migration")
    );
    assert!(
        list_account_identities(&db)
            .expect_err("identity inventory does not migrate")
            .contains("explicit refresh migration")
    );
    assert!(
        stored_account_snapshots(&db)
            .expect_err("test scalar reader does not migrate")
            .contains("explicit refresh migration")
    );
    assert_eq!(
        schema_version(&db)
            .expect("strict metadata oracle")
            .as_deref(),
        Some("4")
    );
    assert_eq!(store_backend::source_bytes_for_test(&db_string), before);
    assert_eq!(connection_build_count(&db).expect("zero writer opens"), 0);
}

#[test]
fn all_snapshot_reader_entrypoints_do_not_create_removed_or_missing_paths() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("missing-parent").join("missing.db");
    assert!(
        read_account_usage_snapshots(&db)
            .expect("missing raw cache")
            .is_empty()
    );
    assert!(
        load_all_account_usage_views(&db, 100)
            .expect("missing focused cache")
            .is_empty()
    );
    assert!(
        load_account_usage_view(&db, "missing-route", 100)
            .expect("missing account cache")
            .is_none()
    );
    assert!(
        list_account_identities(&db)
            .expect("missing identity cache")
            .is_empty()
    );
    assert!(schema_version(&db).expect("missing metadata").is_none());
    assert!(!db.parent().expect("parent").exists());
    let present = dir.path().join("removed.db");
    store_usage_snapshot(&present, &usage_view()).expect("private owned writer");
    std::fs::remove_file(&present).expect("remove private fixture after writer release");
    assert!(
        load_all_account_usage_views(&present, 100)
            .expect("removed cache is absent")
            .is_empty()
    );
    assert!(!present.exists());
}

#[test]
fn all_snapshot_reader_entrypoints_preserve_current_database_and_wal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("strict-current-reader.db");
    store_usage_snapshot(&db, &usage_view()).expect("private current fixture");
    let db_string = path_to_turso(&db).expect("fixture path");
    let before = store_backend::source_bytes_for_test(&db_string);
    let raw = read_account_usage_snapshots(&db).expect("strict current raw read");
    assert_eq!(raw.len(), 2);
    let views = load_all_account_usage_views(&db, 100).expect("strict focused read");
    assert_eq!(views.len(), 1);
    assert!(
        load_account_usage_view(&db, &views[0].account_key_hash, 100)
            .expect("strict single read")
            .is_some()
    );
    assert_eq!(
        list_account_identities(&db)
            .expect("strict identity read")
            .len(),
        1
    );
    assert_eq!(
        schema_version(&db)
            .expect("strict metadata read")
            .as_deref(),
        Some("8")
    );
    assert_eq!(store_backend::source_bytes_for_test(&db_string), before);
    assert_eq!(
        connection_build_count(&db).expect("only explicit writer opened"),
        1
    );
}
