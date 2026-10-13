// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn canon_sel_valid_historical_choice_survives_reopen() {
    use jackin_usage_snapshot_store::store_usage_snapshot;

    let dir = tempfile::tempdir().expect("tempdir");
    let view = glance_view(
        "OpenAI / Codex",
        Some("OAuth"),
        vec![glance_weekly_bucket(44)],
        UsageSnapshotStatus::Fresh,
    );
    let key = account_key_for_view(&view).expect("canonical key");
    store_usage_snapshot(&host_snapshot_store_path(dir.path()), &view).expect("store history");

    let mut first = open_runtime(dir.path());
    first
        .set_selected_account("codex", &key)
        .expect("select history explicitly");
    drop(first);

    let mut reopened = open_runtime(dir.path());
    let rows = reopened
        .list_accounts(Some("codex"))
        .expect("reopened accounts");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].selected);
    assert_eq!(rows[0].lifecycle, "historical");
}

#[test]
fn canon_amp_presence_does_not_promote_durable_history() {
    use jackin_usage_snapshot_store::store_usage_snapshot;

    let dir = tempfile::tempdir().expect("tempdir");
    let mut history = glance_view(
        "Amp",
        Some("OAuth"),
        vec![glance_daily_bucket(73)],
        UsageSnapshotStatus::Fresh,
    );
    history.account.account_label = "amp@example.com".to_owned();
    store_usage_snapshot(&host_snapshot_store_path(dir.path()), &history).expect("store history");

    let mut presence = glance_view(
        "Amp",
        Some("local Amp auth"),
        Vec::new(),
        UsageSnapshotStatus::Unavailable,
    );
    presence.account.account_label = "local Amp auth".to_owned();
    presence.confidence = UsageConfidence::PresenceOnly;
    let mut runtime = open_runtime(dir.path());
    runtime.inject_snapshot("amp", presence).expect("inject");
    let rows = runtime.list_accounts(Some("amp")).expect("accounts");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].account_label, "amp@example.com");
    assert_eq!(rows[0].lifecycle, "historical");
    assert!(!rows[0].selected, "history must not be selected implicitly");
}

#[test]
fn canon_each_account_retains_its_own_status_limit_and_error() {
    use jackin_usage_snapshot_store::store_usage_snapshot;

    let dir = tempfile::tempdir().expect("tempdir");
    let mut history = glance_view(
        "Anthropic / Claude",
        Some("OAuth"),
        vec![glance_weekly_bucket(11)],
        UsageSnapshotStatus::Stale,
    );
    history.account.account_label = "history@example.com".to_owned();
    history.buckets[0].status = UsageSnapshotStatus::Stale;
    history.last_error = Some("history unavailable".to_owned());
    store_usage_snapshot(&host_snapshot_store_path(dir.path()), &history).expect("store history");

    let mut current = glance_view(
        "Anthropic / Claude",
        Some("OAuth"),
        vec![glance_weekly_bucket(88)],
        UsageSnapshotStatus::Fresh,
    );
    current.account.account_label = "current@example.com".to_owned();
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot("claude", current)
        .expect("inject current");
    let rows = runtime.list_accounts(Some("claude")).expect("accounts");
    let history = rows
        .iter()
        .find(|row| row.account_label == "history@example.com")
        .expect("history row");
    let current = rows
        .iter()
        .find(|row| row.account_label == "current@example.com")
        .expect("current row");
    assert_eq!(history.remaining_percent, Some(11));
    assert_eq!(history.status_word, "stale");
    assert_eq!(history.last_error.as_deref(), Some("history unavailable"));
    assert_eq!(current.remaining_percent, Some(88));
    assert_eq!(current.status_word, "fresh");
    assert_eq!(current.last_error, None);
}

#[test]
fn canon_projection_ignores_removed_legacy_shared_snapshot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let shared = dir.path().join("usage-shared").join("snapshots");
    std::fs::create_dir_all(&shared).expect("shared dir");
    std::fs::write(shared.join("usage-broken.snapshot.json"), "not-json").expect("broken snapshot");
    let mut runtime = open_runtime(dir.path());
    runtime.desktop_inventory().expect("legacy tree ignored");
}

#[test]
fn canon_open_rejects_unknown_surface_and_resets_changed_profile() {
    let first = tempfile::tempdir().expect("first tempdir");
    let second = tempfile::tempdir().expect("second tempdir");
    let mut runtime = open_runtime(first.path());
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
    let mut invalid = HostRuntimeConfig::under_data_dir(second.path());
    invalid.enabled_surface_ids = vec!["typo".to_owned()];
    assert!(runtime.open(invalid).is_err());
    runtime
        .open(HostRuntimeConfig::under_data_dir(second.path()))
        .expect("reopen");
    assert!(
        runtime
            .list_accounts(Some("codex"))
            .expect("second profile")
            .is_empty()
    );
}

#[test]
fn canon_desktop_inventory_is_grouped_and_complete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "codex",
            glance_view(
                "OpenAI / Codex",
                Some("OAuth"),
                vec![glance_weekly_bucket(57)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    runtime
        .inject_snapshot(
            "opencode",
            glance_view(
                "OpenCode",
                Some("OAuth"),
                vec![glance_weekly_bucket(90)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let inventory = runtime.desktop_inventory().expect("inventory");
    assert_eq!(inventory.groups.len(), 1);
    let codex = &inventory.groups[0];
    assert_eq!(codex.surface_id, "codex");
    assert_eq!(codex.display_label, "OpenAI");
    assert_eq!(codex.fallback_glyph, "Cx");
    assert!(
        codex
            .usage_url
            .as_deref()
            .is_some_and(|url| url.contains("usage"))
    );
    assert_eq!(codex.accounts.len(), 1);
    let account = &codex.accounts[0];
    assert!(account.selected);
    assert_eq!(account.lifecycle, "current");
    assert_eq!(account.remaining_label, "57%");
    assert_eq!(account.headline, "57% left");
    assert_eq!(account.status_word, "fresh");
    assert_eq!(account.plan_or_status_label, "—");
    assert!(
        inventory
            .groups
            .iter()
            .all(|group| group.surface_id != "opencode")
    );
}

#[test]
fn provider_glance_rows_use_exact_seven_provider_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for id in ["codex", "claude", "amp", "grok", "zai", "kimi", "minimax"] {
        runtime
            .inject_snapshot(
                id,
                glance_view(
                    "P",
                    Some("OAuth · file"),
                    vec![glance_weekly_bucket(50)],
                    UsageSnapshotStatus::Fresh,
                ),
            )
            .expect("inject");
    }
    let rows = runtime.provider_glance_rows().expect("rows");
    let ids: Vec<_> = rows.iter().map(|r| r.surface_id.as_str()).collect();
    assert_eq!(
        ids,
        ["codex", "claude", "amp", "grok", "zai", "kimi", "minimax"]
    );
}

#[test]
fn provider_glance_rows_show_three_weekly_labels() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "codex",
            glance_view(
                "Codex",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(57)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    runtime
        .inject_snapshot(
            "claude",
            glance_view(
                "Claude",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(74)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    runtime
        .inject_snapshot(
            "zai",
            glance_view(
                "GLM / Z.AI",
                Some("API key · env ZAI_API_KEY"),
                vec![glance_weekly_bucket(31)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let bar = |id: &str| {
        rows.iter()
            .find(|r| r.surface_id == id)
            .map(|r| r.bar_label.clone())
    };
    assert_eq!(bar("codex").as_deref(), Some("57%"));
    assert_eq!(bar("claude").as_deref(), Some("74%"));
    assert_eq!(bar("zai").as_deref(), Some("31%"));
}

#[test]
fn provider_glance_rows_select_amp_daily() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "amp",
            glance_view(
                "Amp",
                Some("API key · env AMP_API_KEY"),
                vec![glance_daily_bucket(61)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let amp = rows
        .iter()
        .find(|r| r.surface_id == "amp")
        .expect("amp row");
    assert_eq!(amp.bar_label, "61%");
    assert_eq!(amp.glance_remaining_percent, Some(61));
}

#[test]
fn provider_glance_rows_show_dash_for_paid_only_amp() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    // Credit-bound bucket but no Daily slot → detected but no glance percent.
    let mut credits = glance_weekly_bucket(40);
    credits.label = "Individual credits".to_owned();
    credits.status_slot = None;
    credits.remaining_percent = None;
    credits.limit_label = Some("$9.86".to_owned());
    runtime
        .inject_snapshot(
            "amp",
            glance_view(
                "Amp",
                Some("API key · env AMP_API_KEY"),
                vec![credits],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let amp = rows
        .iter()
        .find(|r| r.surface_id == "amp")
        .expect("amp row");
    assert_eq!(amp.bar_label, "–");
    assert_eq!(amp.glance_remaining_percent, None);
}

#[test]
fn provider_glance_rows_show_dash_before_first_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "codex",
            glance_view(
                "Codex",
                Some("OAuth · file"),
                Vec::new(),
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
    assert_eq!(codex.headline, "–");
}

#[test]
fn provider_glance_rows_empty_without_credentials() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let rows = runtime.provider_glance_rows().expect("rows");
    assert!(rows.is_empty());
}

#[test]
fn provider_glance_rows_reject_negative_credential_placeholders() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "zai",
            glance_view(
                "GLM / Z.AI",
                Some("needs env ZAI_API_KEY"),
                Vec::new(),
                UsageSnapshotStatus::NeedsLogin,
            ),
        )
        .expect("inject");
    runtime
        .inject_snapshot(
            "kimi",
            glance_view(
                "Kimi",
                Some("needs Kimi auth"),
                Vec::new(),
                UsageSnapshotStatus::NeedsLogin,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    assert!(rows.is_empty());
}
